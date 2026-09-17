//! A disposable 1051 process. All addresses below are for the hash-locked English EXE.
use std::ffi::OsStr;
use std::fs;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::System::Diagnostics::Debug::{FlushInstructionCache, ReadProcessMemory, WriteProcessMemory};
use windows_sys::Win32::System::Memory::{PAGE_EXECUTE_READWRITE, VirtualProtectEx};
use windows_sys::Win32::System::Threading::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::live_process::{check_deadline, poll_delay};

const HASH: &str = "f9669af338964787a3785a7895791297d599295b8bb669b0db49443f736a1322";
const ADDRESS: usize = 0x553b05;
const ORIGINAL: [u8; 6] = [0x0f, 0x84, 0xea, 0x01, 0, 0];
const ISOLATED: [u8; 6] = [0xe9, 0xeb, 0x01, 0, 0, 0x90];

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(Some(0)).collect()
}

pub fn verify_exe(exe: &Path) -> Result<()> {
    if format!("{:x}", Sha256::digest(fs::read(exe)?)) != HASH {
        bail!("unsupported 1051 EXE SHA-256");
    }
    Ok(())
}

pub fn ensure_no_game() -> Result<()> {
    let mut exists = false;
    // The native single-instance mutex can exist before the visible window is created.
    let name = wide(OsStr::new("PlantsVsZombiesMutex"));
    unsafe {
        let mutex = OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, 0, name.as_ptr());
        if !mutex.is_null() {
            exists = true;
            CloseHandle(mutex);
        } else if GetLastError() == ERROR_ACCESS_DENIED {
            exists = true;
        }
    }
    // SAFETY: callback only writes the bool passed for the synchronous enumeration duration.
    if unsafe { EnumWindows(Some(find_existing), (&mut exists as *mut bool) as isize) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if exists {
        bail!("an existing 1051 game is running; it was not modified");
    }
    Ok(())
}

unsafe extern "system" fn find_existing(hwnd: HWND, context: LPARAM) -> i32 {
    let mut pid = 0;
    // SAFETY: valid output pointer and a window supplied by EnumWindows.
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    // SAFETY: read-only handle; version signature reads never modify a candidate process.
    let handle = unsafe { OpenProcess(PROCESS_VM_READ | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if !handle.is_null() {
        if read::<u32>(handle, 0x4140c5).ok() == Some(0x0019b337) {
            unsafe {
                *(context as *mut bool) = true;
            }
        }
        unsafe {
            CloseHandle(handle);
        }
    }
    1
}

fn read<T: Copy + Default>(handle: HANDLE, address: usize) -> Result<T> {
    let mut value = T::default();
    let mut bytes = 0;
    // SAFETY: remote bytes are copied into an appropriately sized scalar/byte buffer, never dereferenced.
    if unsafe {
        ReadProcessMemory(
            handle,
            address as *const _,
            (&mut value as *mut T).cast(),
            std::mem::size_of::<T>(),
            &mut bytes,
        )
    } == 0
        || bytes != std::mem::size_of::<T>()
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(value)
}

pub struct OwnedGame {
    info: PROCESS_INFORMATION,
}

impl OwnedGame {
    pub fn launch(exe: &Path, directory: &Path) -> Result<Self> {
        verify_exe(exe)?;
        ensure_no_game()?;
        let exe_w = wide(exe.as_os_str());
        let cwd = wide(directory.as_os_str());
        let mut command = wide(OsStr::new(&format!(
            "\"{}\" -changedir=\"{}\"",
            exe.display(),
            directory.display()
        )));
        let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
        startup.cb = std::mem::size_of_val(&startup) as u32;
        let mut info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: all strings are nul terminated; output structures remain live for the call.
        if unsafe {
            CreateProcessW(
                exe_w.as_ptr(),
                command.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_SUSPENDED,
                std::ptr::null(),
                cwd.as_ptr(),
                &startup,
                &mut info,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let game = Self { info };
        game.isolate()?;
        // SAFETY: the main thread is still suspended and the validated startup branch was patched.
        if unsafe { ResumeThread(game.info.hThread) } == u32::MAX {
            return Err(std::io::Error::last_os_error().into());
        }
        eprintln!("owned 1051 PID: {}", game.pid());
        Ok(game)
    }

    fn isolate(&self) -> Result<()> {
        validate_patch(read::<[u8; 6]>(self.info.hProcess, ADDRESS)?)?;
        let mut old = 0;
        // SAFETY: changes only six validated instruction bytes in our still-suspended child.
        unsafe {
            if VirtualProtectEx(
                self.info.hProcess,
                ADDRESS as *const _,
                6,
                PAGE_EXECUTE_READWRITE,
                &mut old,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut bytes = 0;
            let written = WriteProcessMemory(
                self.info.hProcess,
                ADDRESS as *const _,
                ISOLATED.as_ptr().cast(),
                6,
                &mut bytes,
            );
            let mut ignored = 0;
            let restored = VirtualProtectEx(self.info.hProcess, ADDRESS as *const _, 6, old, &mut ignored);
            let flushed = FlushInstructionCache(self.info.hProcess, ADDRESS as *const _, 6);
            if written == 0 || bytes != 6 || restored == 0 || flushed == 0 {
                bail!("startup isolation patch failed");
            }
        }
        Ok(())
    }

    pub fn pid(&self) -> u32 {
        self.info.dwProcessId
    }
    pub fn alive(&self) -> Result<()> {
        // SAFETY: this guard owns the process handle until Drop.
        if unsafe { WaitForSingleObject(self.info.hProcess, 0) } != WAIT_TIMEOUT {
            bail!("1051 game exited early");
        }
        Ok(())
    }

    pub fn wait_ready(&self, deadline: Instant) -> Result<()> {
        loop {
            self.alive()?;
            check_deadline(deadline)?;
            let app = read::<u32>(self.info.hProcess, 0x6a9ec0).unwrap_or(0) as usize;
            if app != 0 {
                // LawnApp +0x76c/+0x82c; TitleScreen::Update reads +0xa1 (0x48e14d).
                let player = read::<u32>(self.info.hProcess, app + 0x82c).unwrap_or(0);
                let title = read::<u32>(self.info.hProcess, app + 0x76c).unwrap_or(0) as usize;
                let ui = read::<u32>(self.info.hProcess, app + 0x7fc).unwrap_or(u32::MAX);
                let loaded = if title == 0 {
                    (1..=7).contains(&ui)
                } else {
                    read::<u8>(self.info.hProcess, title + 0xa1).unwrap_or(0) == 1
                };
                if player != 0 && loaded {
                    return Ok(());
                }
            }
            poll_delay();
        }
    }

    pub fn terminate(&self) -> Result<()> {
        // SAFETY: only this guard's child may be terminated. Wait before freeing remote arguments.
        unsafe {
            if WaitForSingleObject(self.info.hProcess, 0) == WAIT_TIMEOUT
                && TerminateProcess(self.info.hProcess, 1) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
            if WaitForSingleObject(self.info.hProcess, 2000) != WAIT_OBJECT_0 {
                bail!("could not reap owned 1051 game");
            }
        }
        Ok(())
    }

    pub fn observe_alive(&self, deadline: Instant) -> Result<()> {
        let until = Instant::now() + Duration::from_millis(500);
        while Instant::now() < until {
            self.alive()?;
            check_deadline(deadline)?;
            poll_delay();
        }
        Ok(())
    }
}

impl Drop for OwnedGame {
    fn drop(&mut self) {
        if let Err(error) = self.terminate() {
            eprintln!("owned game cleanup failed: {error:#}");
        }
        // SAFETY: handles were returned by CreateProcessW and are owned exclusively here.
        unsafe {
            CloseHandle(self.info.hThread);
            CloseHandle(self.info.hProcess);
        }
    }
}

fn validate_patch(actual: [u8; 6]) -> Result<()> {
    if actual != ORIGINAL {
        bail!("1051 startup instruction mismatch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn refuses_wrong_instruction_and_targets_same_branch() {
        assert!(validate_patch(ORIGINAL).is_ok());
        assert!(validate_patch(ISOLATED).is_err());
        assert_eq!(
            ADDRESS + 5 + i32::from_le_bytes(ISOLATED[1..5].try_into().unwrap()) as usize,
            0x553cf5
        );
    }
    #[test]
    fn refuses_wrong_exe() {
        let dir = crate::live_process::Sandbox::new("wrong-exe").unwrap();
        let exe = dir.0.join("bad.exe");
        fs::write(&exe, b"not 1051").unwrap();
        assert!(verify_exe(&exe).is_err());
    }
    #[test]
    fn preflight_preserves_existing_single_instance_mutex() {
        let name = wide(OsStr::new("PlantsVsZombiesMutex"));
        // A real OS mutex models the native pre-window startup state without launching a game.
        let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        assert!(!mutex.is_null());
        assert!(ensure_no_game().is_err());
        unsafe {
            CloseHandle(mutex);
        }
    }
}
