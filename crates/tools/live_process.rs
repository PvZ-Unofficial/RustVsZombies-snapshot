//! File/process utilities for external Windows test tooling, never linked into a game.
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use windows_sys::Win32::Foundation::{CloseHandle, ERROR_NO_MORE_FILES, GetLastError, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW, TH32CS_SNAPMODULE, TH32CS_SNAPMODULE32,
};

pub struct Sandbox(pub PathBuf);

impl Sandbox {
    pub fn new(label: &str) -> Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("rsvz-{label}-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Only a directory created and owned by this guard can reach this cleanup.
        if let Err(error) = fs::remove_dir_all(&self.0) {
            eprintln!("temporary directory retained: {}: {error}", self.0.display());
        }
    }
}

pub fn copy_tree(source: &Path, destination: &Path, deadline: Instant) -> Result<()> {
    check_deadline(deadline)?;
    if !source.is_dir() {
        bail!("directory missing: {}", source.display());
    }
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        check_deadline(deadline)?;
        let entry = entry?;
        let kind = entry.file_type()?;
        // Do not traverse links or Windows junctions into unrelated directories.
        use std::os::windows::fs::MetadataExt;
        if entry.metadata()?.file_attributes() & 0x400 != 0 {
            bail!("reparse point is not a test fixture: {}", entry.path().display());
        }
        let target = destination.join(entry.file_name());
        if kind.is_dir() {
            copy_tree(&entry.path(), &target, deadline)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), target)?;
        } else {
            bail!("unsupported fixture entry: {}", entry.path().display());
        }
    }
    Ok(())
}

pub fn module_loaded(pid: u32, name: &std::ffi::OsStr) -> Result<bool> {
    // SAFETY: snapshot is owned here, entry size is initialized, and no remote pointers are dereferenced.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(std::io::Error::last_os_error()).context("module snapshot");
        }
        let result = (|| {
            let mut entry: MODULEENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of_val(&entry) as u32;
            let mut found = Module32FirstW(snapshot, &mut entry);
            while found != 0 {
                let end = entry
                    .szModule
                    .iter()
                    .position(|v| *v == 0)
                    .unwrap_or(entry.szModule.len());
                let actual = String::from_utf16_lossy(&entry.szModule[..end]);
                if actual.eq_ignore_ascii_case(&name.to_string_lossy()) {
                    return Ok(true);
                }
                found = Module32NextW(snapshot, &mut entry);
            }
            if GetLastError() != ERROR_NO_MORE_FILES {
                return Err(std::io::Error::last_os_error()).context("module enumeration");
            }
            Ok(false)
        })();
        CloseHandle(snapshot);
        result
    }
}

pub fn check_deadline(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        bail!("live run timed out");
    }
    Ok(())
}

pub fn poll_delay() {
    thread::sleep(Duration::from_millis(25));
}

pub struct ChildGuard {
    pub child: std::process::Child,
    pub terminate_on_drop: bool,
}

impl ChildGuard {
    // This source is compiled independently by both tooling crates; some callers only use ownership.
    #[allow(dead_code)]
    pub fn ensure_alive(&mut self) -> Result<()> {
        if let Some(status) = self.child.try_wait()? {
            bail!("owned game exited early: {status}");
        }
        Ok(())
    }
    pub fn terminate(&mut self) -> Result<()> {
        if self.child.try_wait()?.is_none() {
            self.child.kill()?;
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.child.try_wait()?.is_none() {
            check_deadline(deadline)?;
            poll_delay();
        }
        self.terminate_on_drop = false;
        Ok(())
    }
}

impl std::ops::Deref for ChildGuard {
    type Target = std::process::Child;
    fn deref(&self) -> &Self::Target {
        &self.child
    }
}
impl std::ops::DerefMut for ChildGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.terminate_on_drop {
            if let Err(error) = self.terminate() {
                eprintln!("owned child cleanup failed: {error:#}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copies_without_changing_template() {
        let source = Sandbox::new("copy-source").unwrap();
        let target = Sandbox::new("copy-target").unwrap();
        fs::write(source.0.join("test.dat"), b"original").unwrap();
        copy_tree(&source.0, &target.0, Instant::now() + Duration::from_secs(2)).unwrap();
        fs::write(target.0.join("test.dat"), b"changed").unwrap();
        assert_eq!(fs::read(source.0.join("test.dat")).unwrap(), b"original");
    }
    #[test]
    fn deadline_is_not_a_success() {
        assert!(check_deadline(Instant::now()).is_err());
    }
    #[test]
    fn nonexistent_process_is_not_an_unloaded_module() {
        assert!(module_loaded(u32::MAX, std::ffi::OsStr::new("test.dll")).is_err());
    }
    #[test]
    fn observes_a_loaded_module_without_claiming_unload() {
        assert!(module_loaded(std::process::id(), std::ffi::OsStr::new("kernel32.dll")).unwrap());
        assert!(!module_loaded(std::process::id(), std::ffi::OsStr::new("rsvz_missing_test_module.dll")).unwrap());
    }
    #[test]
    fn early_child_exit_is_detected_and_reaped() {
        let mut child = ChildGuard {
            child: std::process::Command::new("cmd.exe")
                .args(["/c", "exit", "7"])
                .spawn()
                .unwrap(),
            terminate_on_drop: true,
        };
        child.wait().unwrap();
        assert!(child.ensure_alive().is_err());
        child.terminate().unwrap();
        assert_eq!(child.try_wait().unwrap().unwrap().code(), Some(7));
    }
}
