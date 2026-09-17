//! PvZ 1.0.0.1051 DLL injector.

mod inject;
mod process;

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::process::{exit, id};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use clap::Parser;
use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Memory::{MEM_RELEASE, VirtualFreeEx};

use process::PvzProcess;

const CONFIG_PREFIX: &str = "rsvz-1051-script-";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Parser)]
#[command(name = "injector", version, about = "Inject a PvZ 1.0.0.1051 script DLL")]
struct Args {
    /// DLL to inject.
    #[arg(long, required_unless_present = "unload", conflicts_with = "unload")]
    dll: Option<PathBuf>,
    /// Request safe cleanup, then unload this PID's rsvz DLL.
    #[arg(long, requires = "pid")]
    unload: bool,
    /// Target PvZ process id. Required when multiple supported windows exist.
    #[arg(long)]
    pid: Option<u32>,
    /// Internal: complete opaque UTF-8 runner config JSON.
    #[arg(long, hide = true)]
    rsvz_run_config_json: Option<String>,
}

fn main() {
    if let Err(error) = run(Args::parse()) {
        eprintln!("注入失败: {error}");
        exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    if args.unload {
        let target = PvzProcess::open_for_unload(args.pid.expect("clap requires --pid"))?;
        return inject::unload_owned_module(&target);
    }
    let target_process = PvzProcess::find(args.pid)?;
    if let Some(json) = args.rsvz_run_config_json.as_deref() {
        let config = write_opaque_config(target_process.pid(), json.as_bytes())?;
        println!("script config: {}", config.display());
    }
    target_process.manage_dll(args.dll.as_ref().expect("clap requires --dll"))
}

#[derive(Debug, thiserror::Error)]
enum InjectorError {
    #[error("1051 injector must run as a 32-bit Windows process")]
    UnsupportedArchitecture,
    #[error("未找到可见的 PvZ 游戏窗口，请确认游戏已启动且窗口未最小化")]
    NoPvzWindow,
    #[error("检测到多个 PvZ 进程，请使用 --pid 选择: {0:?}")]
    MultiplePvzProcesses(Vec<u32>),
    #[error("未找到 PID {0} 对应的受支持 PvZ 窗口")]
    PvzProcessNotFound(u32),
    #[error("游戏版本不受支持 (期望英文原版 1.0.0.1051)，地址 {addr:#x} 处读到 {actual:#x}，期望 {expected:#x}")]
    InvalidVersion { addr: usize, expected: u32, actual: u32 },
    #[error("无法打开目标进程 (PID {pid}): {source}")]
    ProcessOpen { pid: u32, source: io::Error },
    #[error("在目标进程中分配内存失败 (PID {pid}, 大小 {size}): {source}")]
    RemoteAlloc { pid: u32, size: usize, source: io::Error },
    #[error("向目标进程写入内存失败 (地址 {addr:#x}): {source}")]
    RemoteWrite { addr: usize, source: io::Error },
    #[error("从目标进程读取内存失败 (地址 {addr:#x}): {source}")]
    RemoteRead { addr: usize, source: io::Error },
    #[error("创建远程线程失败: {0}")]
    RemoteThread(io::Error),
    #[error("等待远程线程失败，WaitForSingleObject 返回 {0:#x}")]
    RemoteThreadWait(u32),
    #[error("远程线程结束后仍报告 STILL_ACTIVE")]
    RemoteThreadStillActive,
    #[error("DLL 文件不存在: {0}")]
    DllNotFound(PathBuf),
    #[error("文件操作失败: {0}")]
    Io(#[from] io::Error),
    #[error("DLL 注入失败: {0}")]
    InjectionFailed(String),
    #[error("Windows API 错误: {0}")]
    Windows(io::Error),
}

type Result<T> = std::result::Result<T, InjectorError>;

struct RemoteAlloc {
    process: HANDLE,
    ptr: *mut core::ffi::c_void,
}

impl RemoteAlloc {
    /// # Safety
    ///
    /// `ptr` must be an allocation returned by `VirtualAllocEx` for `process`.
    unsafe fn new(process: HANDLE, ptr: *mut core::ffi::c_void) -> Self {
        Self { process, ptr }
    }

    fn as_ptr(&self) -> *mut core::ffi::c_void {
        self.ptr
    }
}

impl Drop for RemoteAlloc {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // SAFETY: this guard owns the allocation returned for this process handle.
            unsafe {
                if VirtualFreeEx(self.process, self.ptr, 0, MEM_RELEASE) == 0 {
                    eprintln!("警告: 释放远程内存失败: {}", io::Error::last_os_error());
                }
            }
        }
    }
}

unsafe fn own_handle(handle: HANDLE) -> Option<OwnedHandle> {
    // SAFETY: a non-sentinel Win32 handle is transferred to this owner exactly once.
    (!handle.is_null() && handle != INVALID_HANDLE_VALUE)
        .then(|| unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
}

fn raw_handle(handle: &OwnedHandle) -> HANDLE {
    handle.as_raw_handle() as HANDLE
}

fn ensure_supported_architecture() -> Result<()> {
    if cfg!(all(
        target_os = "windows",
        target_arch = "x86",
        target_pointer_width = "32"
    )) {
        Ok(())
    } else {
        Err(InjectorError::UnsupportedArchitecture)
    }
}

fn default_config_path(pid: u32) -> PathBuf {
    std::env::temp_dir().join(format!("{CONFIG_PREFIX}{pid}.json"))
}

fn write_opaque_config(pid: u32, json: &[u8]) -> Result<PathBuf> {
    let path = default_config_path(pid);
    remove_if_exists(&path)?;
    let temporary = path.with_extension(format!("tmp-{}", unique_temp_tag()));
    let write = (|| -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&temporary)?;
        file.write_all(json)?;
        file.flush()?;
        drop(file);
        if path.try_exists()? {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("1051 config path unexpectedly reappeared: {}", path.display()),
            ));
        }
        fs::rename(&temporary, &path)
    })();
    if let Err(error) = write {
        let _cleanup_result = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(path)
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn unique_temp_tag() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{sequence:x}", id(), now.as_nanos())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unloading_requires_explicit_pid_and_excludes_injection() {
        assert!(Args::try_parse_from(["injector", "--unload"]).is_err());
        assert!(Args::try_parse_from(["injector", "--unload", "--pid", "1234"]).is_ok());
        assert!(Args::try_parse_from(["injector", "--unload", "--pid", "1234", "--dll", "game.dll"]).is_err());
    }

    #[test]
    fn dll_path_is_required() {
        assert!(Args::try_parse_from(["injector"]).is_err());
        assert_eq!(
            Args::try_parse_from(["injector", "--dll", "runner.dll"])
                .expect("explicit DLL")
                .dll,
            Some(PathBuf::from("runner.dll"))
        );
    }

    #[test]
    fn config_path_is_pid_scoped_json() {
        assert_eq!(
            default_config_path(1234).file_name().and_then(|name| name.to_str()),
            Some("rsvz-1051-script-1234.json")
        );
    }

    #[test]
    fn opaque_config_is_published_without_interpreting_json() {
        let pid = 100_000 + id();
        let path = write_opaque_config(pid, br#"{"future":true}"#).expect("publish opaque config");
        assert_eq!(fs::read(&path).expect("read config"), br#"{"future":true}"#);
        fs::remove_file(path).expect("remove config");
    }

    #[test]
    fn architecture_gate_matches_the_process_bitness() {
        assert_eq!(
            ensure_supported_architecture().is_ok(),
            cfg!(all(
                target_os = "windows",
                target_arch = "x86",
                target_pointer_width = "32"
            ))
        );
    }
}
