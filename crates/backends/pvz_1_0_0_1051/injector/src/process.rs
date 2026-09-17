//! PvZ 1.0.0.1051 目标进程管理：窗口枚举、选择、版本校验和 DLL 管理。

use std::mem;
use std::os::windows::io::OwnedHandle;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::Foundation::{FALSE, HANDLE, HWND, LPARAM, TRUE};
use windows_sys::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows_sys::Win32::System::Memory::{MEM_COMMIT, PAGE_READWRITE, VirtualAllocEx};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_ALL_ACCESS};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};
use windows_sys::core::BOOL;

use super::{InjectorError, RemoteAlloc, Result, ensure_supported_architecture, own_handle, raw_handle};

/// PvZ 英文原版窗口标题。
const PVZ_WINDOW_TITLE: &str = "Plants vs. Zombies";

/// 版本校验：英文原版 1.0.0.1051 在地址 0x4140c5 处的期望值。
const VERSION_CHECK_ADDR: usize = 0x4140c5;
const VERSION_CHECK_EXPECTED: u32 = 0x0019b337;

/// 内存校验：用于在窗口枚举时快速排除非原版。
const VERIFY_ADDR_1: usize = 0x45DC55;
const VERIFY_VAL_1: i32 = 300; // 普通植物血量
const VERIFY_ADDR_2: usize = 0x45E445;
const VERIFY_VAL_2: i32 = 4000; // 南瓜头血量

/// 游戏基址指针和 UI 状态偏移。
const GAME_BASE_ADDR: usize = 0x6a9ec0;
const GAME_UI_OFFSET: usize = 0x7fc;

/// 表示一个已选定并校验通过的 PvZ 1.0.0.1051 目标进程。
pub struct PvzProcess {
    handle: OwnedHandle,
    pid: u32,
}

impl PvzProcess {
    /// Cleanup is PID-addressed even if the window has already been closed/hidden.
    pub fn open_for_unload(pid: u32) -> Result<Self> {
        ensure_supported_architecture()?;
        let handle = open_process(pid)?;
        verify_version(raw_handle(&handle))?;
        Ok(Self { handle, pid })
    }
    pub fn find(pid: Option<u32>) -> Result<Self> {
        ensure_supported_architecture()?;
        let hwnds = enumerate_pvz_windows();
        if hwnds.is_empty() {
            return Err(InjectorError::NoPvzWindow);
        }
        let selected = match pid {
            Some(pid) => hwnds
                .iter()
                .copied()
                .find(|hwnd| window_pid(*hwnd) == pid)
                .ok_or(InjectorError::PvzProcessNotFound(pid))?,
            None if hwnds.len() == 1 => hwnds[0],
            None => {
                let mut pids = hwnds.iter().map(|hwnd| window_pid(*hwnd)).collect::<Vec<_>>();
                pids.sort_unstable();
                pids.dedup();
                return Err(InjectorError::MultiplePvzProcesses(pids));
            }
        };
        let pid = window_pid(selected);
        let handle = open_process(pid)?;

        verify_version(raw_handle(&handle))?;
        let game_ui = read_game_ui(raw_handle(&handle))?;
        println!(
            "选中 PvZ 窗口: PID: {pid}, HWND: {:#x}, GameUi: {game_ui}",
            selected as usize
        );

        Ok(Self { handle, pid })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    pub fn manage_dll(&self, source_dll: impl AsRef<Path>) -> Result<()> {
        super::inject::manage_dll(self, source_dll)
    }

    pub(crate) fn handle(&self) -> HANDLE {
        raw_handle(&self.handle)
    }

    pub(crate) fn alloc_and_write(&self, data: &[u8]) -> Result<RemoteAlloc> {
        let size = data.len();
        let handle = self.handle();
        // SAFETY: Requests a new writable allocation in the target process. The returned pointer is
        // checked for null before use.
        let remote_ptr = unsafe { VirtualAllocEx(handle, ptr::null(), size, MEM_COMMIT, PAGE_READWRITE) };
        if remote_ptr.is_null() {
            return Err(InjectorError::RemoteAlloc {
                pid: self.pid,
                size,
                source: std::io::Error::last_os_error(),
            });
        }

        // SAFETY: `remote_ptr` was returned by `VirtualAllocEx` for this process handle above.
        let guard = unsafe { RemoteAlloc::new(handle, remote_ptr) };

        // SAFETY: `remote_ptr` is the writable remote allocation above; `data` is a valid local
        // source buffer of `data.len()` bytes.
        unsafe {
            let ok = WriteProcessMemory(handle, remote_ptr, data.as_ptr().cast(), data.len(), ptr::null_mut());
            if ok == FALSE {
                return Err(InjectorError::RemoteWrite {
                    addr: remote_ptr as usize,
                    source: std::io::Error::last_os_error(),
                });
            }
        }

        Ok(guard)
    }
}

fn read_memory_at<T: Copy + Default>(handle: HANDLE, addr: usize) -> Result<T> {
    let mut value = T::default();
    // SAFETY: The caller supplies a remote address to read. `value` is a properly aligned local
    // output buffer large enough for `T`.
    unsafe {
        let ok = ReadProcessMemory(
            handle,
            addr as *const _,
            (&raw mut value).cast(),
            mem::size_of::<T>(),
            ptr::null_mut(),
        );
        if ok == FALSE {
            return Err(InjectorError::RemoteRead {
                addr,
                source: std::io::Error::last_os_error(),
            });
        }
    }
    Ok(value)
}

fn open_process(pid: u32) -> Result<OwnedHandle> {
    // SAFETY: `OpenProcess` validates the PID/access rights and reports errors, which are converted
    // to the crate error type.
    unsafe {
        let handle = OpenProcess(PROCESS_ALL_ACCESS, FALSE, pid);
        own_handle(handle).ok_or_else(|| InjectorError::ProcessOpen {
            pid,
            source: std::io::Error::last_os_error(),
        })
    }
}

pub(crate) fn window_pid(hwnd: HWND) -> u32 {
    let mut pid: u32 = 0;
    // SAFETY: `pid` is a valid out-parameter; invalid HWNDs simply leave PID as 0.
    unsafe {
        GetWindowThreadProcessId(hwnd, &raw mut pid);
    }
    pid
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    // SAFETY: `buf` is a valid writable UTF-16 buffer passed to the Win32 window text API.
    let len = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) } as usize;
    String::from_utf16_lossy(&buf[..len])
}

fn enumerate_pvz_windows() -> Vec<HWND> {
    let mut results = Vec::new();
    // SAFETY: The callback receives the address of `results` only for the duration of this call.
    unsafe {
        let _enum_result = EnumWindows(Some(enum_windows_callback), (&raw mut results) as isize);
    }
    results
}

unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `Vec<HWND>` pointer supplied by `enumerate_pvz_windows`.
    let results = unsafe { &mut *(lparam as *mut Vec<HWND>) };

    let pid = window_pid(hwnd);
    if pid == 0 {
        return TRUE;
    }

    // SAFETY: Querying visibility is valid for any HWND supplied by EnumWindows.
    if unsafe { IsWindowVisible(hwnd) } == FALSE {
        return TRUE;
    }

    if window_title(hwnd) != PVZ_WINDOW_TITLE {
        return TRUE;
    }

    // SAFETY: Best-effort validation of a candidate window's owning process.
    let Some(handle) = (unsafe { own_handle(OpenProcess(PROCESS_ALL_ACCESS, FALSE, pid)) }) else {
        return TRUE;
    };

    let handle = raw_handle(&handle);
    let is_pvz = read_memory_at::<i32>(handle, VERIFY_ADDR_1).is_ok_and(|v| v == VERIFY_VAL_1)
        && read_memory_at::<i32>(handle, VERIFY_ADDR_2).is_ok_and(|v| v == VERIFY_VAL_2);

    if is_pvz {
        results.push(hwnd);
    }

    TRUE
}

fn verify_version(handle: HANDLE) -> Result<()> {
    let actual = read_memory_at::<u32>(handle, VERSION_CHECK_ADDR)?;
    if actual != VERSION_CHECK_EXPECTED {
        return Err(InjectorError::InvalidVersion {
            addr: VERSION_CHECK_ADDR,
            expected: VERSION_CHECK_EXPECTED,
            actual,
        });
    }
    Ok(())
}

fn read_game_ui(handle: HANDLE) -> Result<i32> {
    let base = read_memory_at::<usize>(handle, GAME_BASE_ADDR)?;
    read_memory_at::<i32>(handle, base + GAME_UI_OFFSET)
}
