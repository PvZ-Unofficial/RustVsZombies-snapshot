//! DLL 注入 / 卸载 / 文件生命周期管理。
//!
//! 本模块包含：
//! - 将 DLL 注入目标进程（CreateRemoteThread + LoadLibraryW）
//! - 从目标进程卸载指定 DLL（模块枚举 + FreeLibrary）
//! - 封装完整的注入工作流（卸载旧 DLL → 清理孤儿 → 复制 → 注入）

use std::ffi::{CStr, OsString};
use std::fs;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::ptr;

use windows_sys::Win32::Foundation::{FALSE, FreeLibrary, STILL_ACTIVE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
};
use windows_sys::Win32::System::LibraryLoader::{
    DONT_RESOLVE_DLL_REFERENCES, GetModuleHandleA, GetProcAddress, LoadLibraryExW,
};
use windows_sys::Win32::System::ProcessStatus::{K32GetModuleInformation, MODULEINFO};
use windows_sys::Win32::System::Threading::{
    CreateRemoteThread, GetCurrentProcess, GetExitCodeThread, WaitForSingleObject,
};

use super::process::PvzProcess;
use super::{InjectorError, Result, own_handle, raw_handle};

type RemoteThreadStart = unsafe extern "system" fn(*mut core::ffi::c_void) -> u32;

fn inject_dll(process: &PvzProcess, dll_path: &Path) -> Result<()> {
    if !dll_path.exists() {
        return Err(InjectorError::DllNotFound(dll_path.to_path_buf()));
    }

    // Validate both lifecycle exports before LoadLibrary changes the remote process.
    let initialize_rva = local_export_rva(dll_path, c"rsvz_initialize")?;
    let request_unload_rva = local_export_rva(dll_path, c"rsvz_request_unload")?;
    let wide_path = path_to_wide_bytes(dll_path);
    let remote_mem = process.alloc_and_write(&wide_path)?;

    let load_library_addr = get_kernel32_proc_addr(c"LoadLibraryW")?;

    // SAFETY: `load_library_addr` comes from `GetProcAddress("LoadLibraryW")` in the local
    // process. The remote process is the same-bitness PvZ process and uses the same kernel32 export.
    let load_library_start = unsafe { std::mem::transmute::<usize, RemoteThreadStart>(load_library_addr) };

    let module_base = run_remote_thread(process.handle(), load_library_start, remote_mem.as_ptr().cast_const())?;
    if module_base == 0 {
        return Err(InjectorError::InjectionFailed(format!(
            "LoadLibraryW 返回 NULL，DLL 加载失败: {}",
            dll_path.display()
        )));
    }

    let initialized = match call_remote_rva(process.handle(), module_base as usize, initialize_rva) {
        Ok(initialized) => initialized,
        Err(error) => {
            return Err(rollback_loaded_dll(
                process.handle(),
                module_base as usize,
                dll_path,
                request_unload_rva,
                error,
            ));
        }
    };
    if initialized == 0 {
        let error = InjectorError::InjectionFailed(format!("rsvz_initialize 拒绝启动: {}", dll_path.display()));
        return Err(rollback_loaded_dll(
            process.handle(),
            module_base as usize,
            dll_path,
            request_unload_rva,
            error,
        ));
    }

    Ok(())
}

fn rollback_loaded_dll(
    process: windows_sys::Win32::Foundation::HANDLE, module_base: usize, dll_path: &Path, request_unload_rva: usize,
    error: InjectorError,
) -> InjectorError {
    let cleanup = call_remote_rva(process, module_base, request_unload_rva).and_then(|ready| {
        if ready == 0 {
            return Err(InjectorError::InjectionFailed("DLL 拒绝安全卸载".to_owned()));
        }
        free_remote_library(process, module_base, dll_path)
    });
    match cleanup {
        Ok(()) => error,
        Err(cleanup_error) => {
            InjectorError::InjectionFailed(format!("{error}; additionally failed to unload: {cleanup_error}"))
        }
    }
}

fn eject_dll(process: &PvzProcess, dll_name: &str) -> Result<bool> {
    let Some(module) = find_module(process.pid(), dll_name)? else {
        return Ok(false);
    };

    if !request_unload(process.handle(), module.base as usize, &module.path)? {
        return Err(InjectorError::InjectionFailed(format!(
            "旧 DLL 拒绝卸载，未调用 FreeLibrary: {}",
            module.path.display()
        )));
    }
    free_remote_library(process.handle(), module.base as usize, &module.path)?;
    Ok(true)
}

pub(crate) fn unload_owned_module(process: &PvzProcess) -> Result<()> {
    let name = inject_dll_name(process.pid());
    eject_dll(process, &name)?;
    if find_module(process.pid(), &name)?.is_some() {
        return Err(InjectorError::InjectionFailed(
            "DLL remained loaded after safe unload".to_owned(),
        ));
    }
    println!("1051 DLL absent after safe unload: PID {}", process.pid());
    Ok(())
}

fn free_remote_library(
    process: windows_sys::Win32::Foundation::HANDLE, module_base: usize, dll_path: &Path,
) -> Result<()> {
    let free_library_addr = get_kernel32_proc_addr(c"FreeLibrary")?;
    // SAFETY: `free_library_addr` comes from `GetProcAddress("FreeLibrary")`; the parameter passed
    // to the remote thread is the module base returned by ToolHelp for that same process.
    let free_library_start = unsafe { std::mem::transmute::<usize, RemoteThreadStart>(free_library_addr) };
    if run_remote_thread(process, free_library_start, module_base as *const core::ffi::c_void)? == 0 {
        return Err(InjectorError::InjectionFailed(format!(
            "FreeLibrary 失败: {}",
            dll_path.display()
        )));
    }
    Ok(())
}

pub(crate) fn manage_dll(process: &PvzProcess, source_dll: impl AsRef<Path>) -> Result<()> {
    let source_dll = source_dll.as_ref();
    if !source_dll.exists() {
        return Err(InjectorError::DllNotFound(source_dll.to_path_buf()));
    }

    let inject_name = inject_dll_name(process.pid());
    let inject_path = inject_dll_path(process.pid());

    eject_dll(process, &inject_name)?;

    cleanup_orphaned_dlls();

    if let Some(parent) = inject_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source_dll, &inject_path)?;

    let abs_path = fs::canonicalize(&inject_path)?;
    inject_dll(process, &abs_path)?;

    println!("RustVsZombies 1051 后端注入成功");
    Ok(())
}

fn cleanup_orphaned_dlls() {
    let bin_dir = Path::new("./bin");
    let Ok(entries) = fs::read_dir(bin_dir) else {
        return;
    };

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        let Some(_pid) = parse_inject_dll_pid(&file_name) else {
            continue;
        };

        let path = entry.path();
        if fs::remove_file(&path).is_err() {
            eprintln!("警告: 无法删除可能仍在使用的注入 DLL: {}", path.display());
        }
    }
}

fn inject_dll_path(pid: u32) -> PathBuf {
    Path::new("bin").join(inject_dll_name(pid))
}

fn inject_dll_name(pid: u32) -> String {
    format!("rsvz_backend_1051_inject_{pid}.dll")
}

fn parse_inject_dll_pid(file_name: &str) -> Option<u32> {
    let stem = file_name.strip_suffix(".dll")?;
    let pid_str = stem.strip_prefix("rsvz_backend_1051_inject_")?;
    pid_str.parse().ok()
}

fn path_to_wide_bytes(path: &Path) -> Vec<u8> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

fn get_kernel32_proc_addr(func_name: &'static CStr) -> Result<usize> {
    // SAFETY: Both strings are nul-terminated C strings; the returned address is checked for null.
    unsafe {
        let kernel32 = GetModuleHandleA(c"Kernel32.dll".as_ptr().cast());
        if kernel32.is_null() {
            return Err(InjectorError::Windows(std::io::Error::last_os_error()));
        }
        let addr = GetProcAddress(kernel32, func_name.as_ptr().cast())
            .ok_or_else(|| InjectorError::Windows(std::io::Error::last_os_error()))?;
        Ok(addr as usize)
    }
}

fn run_remote_thread(
    process: windows_sys::Win32::Foundation::HANDLE, start: RemoteThreadStart, parameter: *const core::ffi::c_void,
) -> Result<u32> {
    // SAFETY: The caller supplies an open process handle, a remote-compatible entry point, and the
    // parameter expected by that entry point.
    let thread = unsafe { CreateRemoteThread(process, ptr::null(), 0, Some(start), parameter, 0, ptr::null_mut()) };
    // SAFETY: CreateRemoteThread returns ownership of any non-sentinel thread handle.
    let Some(thread) = (unsafe { own_handle(thread) }) else {
        return Err(InjectorError::RemoteThread(std::io::Error::last_os_error()));
    };
    // SAFETY: `thread` is an owned thread handle returned immediately above.
    let wait = unsafe { WaitForSingleObject(raw_handle(&thread), u32::MAX) };
    if wait != WAIT_OBJECT_0 {
        return Err(InjectorError::RemoteThreadWait(wait));
    }
    let mut exit_code = 0;
    // SAFETY: The thread remains open and `exit_code` is a valid out-parameter.
    if unsafe { GetExitCodeThread(raw_handle(&thread), &raw mut exit_code) } == FALSE {
        return Err(InjectorError::Windows(std::io::Error::last_os_error()));
    }
    if exit_code == STILL_ACTIVE as u32 {
        return Err(InjectorError::RemoteThreadStillActive);
    }
    Ok(exit_code)
}

fn call_remote_export(
    process: windows_sys::Win32::Foundation::HANDLE, module_base: usize, dll_path: &Path, export: &'static CStr,
) -> Result<u32> {
    let rva = local_export_rva(dll_path, export)?;
    call_remote_rva(process, module_base, rva)
}

fn call_remote_rva(process: windows_sys::Win32::Foundation::HANDLE, module_base: usize, rva: usize) -> Result<u32> {
    let remote_address = module_base
        .checked_add(rva)
        .ok_or_else(|| InjectorError::InjectionFailed("远程 export 地址溢出".to_owned()))?;
    // SAFETY: `remote_address` is the loaded module base plus the RVA of a verified exported
    // `extern "system" fn(*mut c_void) -> u32`.
    let start = unsafe { std::mem::transmute::<usize, RemoteThreadStart>(remote_address) };
    run_remote_thread(process, start, ptr::null())
}

fn request_unload(
    process: windows_sys::Win32::Foundation::HANDLE, module_base: usize, dll_path: &Path,
) -> Result<bool> {
    call_remote_export(process, module_base, dll_path, c"rsvz_request_unload").map(|result| result != 0)
}

fn local_export_rva(dll_path: &Path, export: &'static CStr) -> Result<usize> {
    let wide = dll_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: The path is nul-terminated. `DONT_RESOLVE_DLL_REFERENCES` maps the image without
    // running DllMain or resolving imports, which is sufficient for export RVA lookup.
    let module = unsafe { LoadLibraryExW(wide.as_ptr(), ptr::null_mut(), DONT_RESOLVE_DLL_REFERENCES) };
    if module.is_null() {
        return Err(InjectorError::Windows(std::io::Error::last_os_error()));
    }
    let mut module_info = MODULEINFO {
        lpBaseOfDll: ptr::null_mut(),
        SizeOfImage: 0,
        EntryPoint: ptr::null_mut(),
    };
    // SAFETY: `module` is mapped in this process and `module_info` is a valid
    // output buffer with the required structure size.
    let module_info_ok = unsafe {
        K32GetModuleInformation(
            GetCurrentProcess(),
            module,
            &raw mut module_info,
            std::mem::size_of::<MODULEINFO>() as u32,
        )
    };
    // SAFETY: `module` is a mapped image and `export` is a nul-terminated symbol name.
    let address = unsafe { GetProcAddress(module, export.as_ptr().cast()) };
    let result = if module_info_ok == FALSE {
        Err(InjectorError::Windows(std::io::Error::last_os_error()))
    } else {
        address
            .ok_or_else(|| {
                InjectorError::InjectionFailed(format!(
                    "DLL 缺少导出 `{}`: {}",
                    export.to_string_lossy(),
                    dll_path.display()
                ))
            })
            .and_then(|address| {
                export_rva_within_image(
                    module_info.lpBaseOfDll as usize,
                    module_info.SizeOfImage as usize,
                    address as usize,
                )
                .ok_or_else(|| {
                    InjectorError::InjectionFailed(format!(
                        "DLL 导出 `{}` 不属于目标映像: {}",
                        export.to_string_lossy(),
                        dll_path.display()
                    ))
                })
            })
    };
    // SAFETY: `module` is the mapping acquired above and no function from it has been called.
    unsafe {
        let _freed = FreeLibrary(module);
    }
    result
}

fn export_rva_within_image(image_base: usize, image_size: usize, export_address: usize) -> Option<usize> {
    export_address.checked_sub(image_base).filter(|rva| *rva < image_size)
}

struct RemoteModule {
    base: *mut u8,
    path: PathBuf,
}

fn find_module(pid: u32, dll_name: &str) -> Result<Option<RemoteModule>> {
    // SAFETY: ToolHelp snapshots are process-id based and return either a valid snapshot handle or
    // an error, which is propagated here.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) };
    // SAFETY: CreateToolhelp32Snapshot returns ownership of any non-sentinel snapshot handle.
    let Some(snapshot) = (unsafe { own_handle(snapshot) }) else {
        return Err(InjectorError::Windows(std::io::Error::last_os_error()));
    };

    let mut entry = MODULEENTRY32W {
        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };

    let mut found = None;
    // SAFETY: `entry.dwSize` is initialized as required by ToolHelp, and `snapshot` remains open for
    // the whole enumeration loop.
    unsafe {
        if Module32FirstW(raw_handle(&snapshot), &raw mut entry) != FALSE {
            loop {
                let module_name = wide_array_to_os_string(&entry.szModule);
                let module_path = PathBuf::from(wide_array_to_os_string(&entry.szExePath));

                if module_name.to_string_lossy().eq_ignore_ascii_case(dll_name)
                    || module_path
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(dll_name))
                {
                    found = Some(RemoteModule {
                        base: entry.modBaseAddr,
                        path: module_path,
                    });
                    break;
                }

                if Module32NextW(raw_handle(&snapshot), &raw mut entry) == FALSE {
                    break;
                }
            }
        }
    }
    Ok(found)
}

fn wide_array_to_os_string<const N: usize>(value: &[u16; N]) -> OsString {
    let len = value.iter().position(|unit| *unit == 0).unwrap_or(value.len());
    OsString::from_wide(&value[..len])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_dll_path_format() {
        let path = inject_dll_path(12345);
        assert_eq!(
            path,
            PathBuf::from("bin/rsvz_backend_1051_inject_12345.dll"),
            "路径格式应为 bin/rsvz_backend_1051_inject_<pid>.dll"
        );
    }

    #[test]
    fn inject_dll_path_zero_pid() {
        let path = inject_dll_path(0);
        assert_eq!(path, PathBuf::from("bin/rsvz_backend_1051_inject_0.dll"));
    }

    #[test]
    fn parse_inject_dll_pid_valid() {
        assert_eq!(parse_inject_dll_pid("rsvz_backend_1051_inject_12345.dll"), Some(12345));
        assert_eq!(parse_inject_dll_pid("rsvz_backend_1051_inject_0.dll"), Some(0));
        assert_eq!(
            parse_inject_dll_pid("rsvz_backend_1051_inject_4294967295.dll"),
            Some(u32::MAX)
        );
    }

    #[test]
    fn parse_inject_dll_pid_invalid() {
        assert_eq!(parse_inject_dll_pid("libavz.dll"), None);
        assert_eq!(parse_inject_dll_pid("libavz_inject_12345.dll"), None);
        assert_eq!(parse_inject_dll_pid("rsvz_backend_1051_inject_.dll"), None);
        assert_eq!(parse_inject_dll_pid("rsvz_backend_1051_inject_abc.dll"), None);
        assert_eq!(parse_inject_dll_pid("rsvz_backend_1051_inject_123"), None);
        assert_eq!(parse_inject_dll_pid("random_file.txt"), None);
        assert_eq!(parse_inject_dll_pid(""), None);
    }

    #[test]
    fn path_to_wide_bytes_ascii() {
        let path = Path::new("C:\\test.dll");
        let bytes = path_to_wide_bytes(path);
        let expected_chars: Vec<u16> = "C:\\test.dll".encode_utf16().chain(std::iter::once(0u16)).collect();
        let expected_bytes: Vec<u8> = expected_chars.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(bytes, expected_bytes, "ASCII 路径应正确转换为 UTF-16 LE");
    }

    #[test]
    fn path_to_wide_bytes_unicode() {
        let path = Path::new("C:\\游戏\\test.dll");
        let bytes = path_to_wide_bytes(path);
        assert!(bytes.len() >= 2, "应至少包含 NUL 终止符的 2 字节");
        assert_eq!(&bytes[bytes.len() - 2..], &[0x00, 0x00], "末尾应为 UTF-16 NUL 终止符");
    }

    #[test]
    fn path_to_wide_bytes_nul_terminated() {
        let path = Path::new("a.dll");
        let bytes = path_to_wide_bytes(path);
        let wide: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        assert_eq!(*wide.last().unwrap(), 0u16, "转换后的 UTF-16 序列末尾应为 0");
    }

    #[test]
    fn export_rva_must_stay_inside_the_mapped_image() {
        assert_eq!(export_rva_within_image(0x1000, 0x2000, 0x1000), Some(0));
        assert_eq!(export_rva_within_image(0x1000, 0x2000, 0x2fff), Some(0x1fff));
        assert_eq!(export_rva_within_image(0x1000, 0x2000, 0x0fff), None);
        assert_eq!(export_rva_within_image(0x1000, 0x2000, 0x3000), None);
    }

    #[test]
    fn get_kernel32_proc_addr_load_library() {
        let addr = get_kernel32_proc_addr(c"LoadLibraryW").expect("应能获取 LoadLibraryW 地址");
        assert_ne!(addr, 0, "LoadLibraryW 地址不应为 0");
    }

    #[test]
    fn get_kernel32_proc_addr_free_library() {
        let addr = get_kernel32_proc_addr(c"FreeLibrary").expect("应能获取 FreeLibrary 地址");
        assert_ne!(addr, 0, "FreeLibrary 地址不应为 0");
    }
}
