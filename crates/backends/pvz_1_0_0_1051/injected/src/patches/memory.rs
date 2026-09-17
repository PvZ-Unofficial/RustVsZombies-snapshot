use std::ffi::c_void;
use std::ptr;

use crate::error::{Pvz1051Error, Result};

const PAGE_EXECUTE_READWRITE: u32 = 0x40;

pub(super) fn read_bytes(addr: usize, len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len);
    for offset in 0..len {
        // SAFETY: Callers supply PvZ 1.0.0.1051 patch addresses; volatile reads avoid eliding the
        // byte check before patch writes.
        bytes.push(unsafe { ptr::read_volatile((addr + offset) as *const u8) });
    }
    bytes
}

pub(super) fn write_bytes(addr: usize, bytes: &[u8]) -> Result<()> {
    with_writable_range(addr, bytes.len(), || {
        for (offset, byte) in bytes.iter().copied().enumerate() {
            // SAFETY: The writable guard changed protection for this exact patch range.
            unsafe { ptr::write_volatile((addr + offset) as *mut u8, byte) };
        }
    })
}

pub(crate) fn with_writable_range<T>(addr: usize, len: usize, write: impl FnOnce() -> T) -> Result<T> {
    let Some(_guard) = WritableCodeRange::acquire(addr, len) else {
        return Err(Pvz1051Error::Patch(format!(
            "failed to make patch range writable at 0x{addr:x}"
        )));
    };
    let result = write();
    flush_instruction_cache(addr, len);
    Ok(result)
}

pub(super) fn encode_rel32(opcode: u8, site: usize, target: usize, owner: &str) -> Result<[u8; 5]> {
    let next = site
        .checked_add(5)
        .ok_or_else(|| Pvz1051Error::Patch(format!("{owner} call-site address overflow")))?;
    let displacement = i32::try_from(target as i64 - next as i64)
        .map_err(|_error| Pvz1051Error::Patch(format!("{owner} is outside rel32 range")))?;
    let mut bytes = [opcode, 0, 0, 0, 0];
    bytes[1..].copy_from_slice(&displacement.to_le_bytes());
    Ok(bytes)
}

pub(super) trait PatchMemory {
    fn read_bytes(&mut self, addr: usize, len: usize) -> Vec<u8>;
    fn write_bytes(&mut self, addr: usize, bytes: &[u8]) -> Result<()>;
}

#[derive(Default)]
pub(super) struct RealPatchMemory;

impl PatchMemory for RealPatchMemory {
    fn read_bytes(&mut self, addr: usize, len: usize) -> Vec<u8> {
        read_bytes(addr, len)
    }

    fn write_bytes(&mut self, addr: usize, bytes: &[u8]) -> Result<()> {
        write_bytes(addr, bytes)
    }
}

/// Completes a failed transaction without losing a restoration failure.
pub(crate) fn finish_restoration<T>(operation: Result<T>, restoration: Result<()>, owner: &str) -> Result<T> {
    match restoration {
        Ok(()) => operation,
        Err(restore) => {
            crate::runtime::hook::fail_and_request_unload();
            Err(Pvz1051Error::Patch(match operation {
                Ok(_) => format!("{owner} restoration failed: {restore}"),
                Err(error) => format!("{owner} failed: {error}; restoration also failed: {restore}"),
            }))
        }
    }
}

/// Restores only bytes still owned by this hook; an already-restored site is harmless.
pub(super) fn restore_owned_bytes(address: usize, original: &[u8], installed: &[u8], owner: &str) -> Result<()> {
    let current = read_bytes(address, installed.len());
    if current == original {
        return Ok(());
    }
    if current != installed {
        return Err(Pvz1051Error::Patch(format!("{owner} changed while owned")));
    }
    write_bytes(address, original)
}

struct WritableCodeRange {
    addr: usize,
    len: usize,
    old_protect: u32,
}

impl WritableCodeRange {
    fn acquire(addr: usize, len: usize) -> Option<Self> {
        let mut old_protect = 0;
        // SAFETY: The caller supplies a process-local PvZ code/data range and the exact byte
        // length to patch; the old page protection is restored by Drop.
        let ok = unsafe { VirtualProtect(addr as *mut c_void, len, PAGE_EXECUTE_READWRITE, &raw mut old_protect) };
        (ok != 0).then_some(Self { addr, len, old_protect })
    }
}

impl Drop for WritableCodeRange {
    fn drop(&mut self) {
        let mut ignored = 0;
        // SAFETY: This restores the same range captured by `acquire`. Failure during teardown is
        // intentionally ignored so other restore paths can keep running.
        let _ok = unsafe { VirtualProtect(self.addr as *mut c_void, self.len, self.old_protect, &raw mut ignored) };
    }
}

fn flush_instruction_cache(addr: usize, len: usize) {
    // SAFETY: `GetCurrentProcess` returns a valid pseudo-handle in this process.
    let process = unsafe { GetCurrentProcess() };
    // SAFETY: The patched range is process-local and was just modified.
    let _ok = unsafe { FlushInstructionCache(process, addr as *const c_void, len) };
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualProtect(lpaddress: *mut c_void, dwsize: usize, flnewprotect: u32, lpfloldprotect: *mut u32) -> i32;

    fn FlushInstructionCache(hprocess: *mut c_void, lpbaseaddress: *const c_void, dwsize: usize) -> i32;

    fn GetCurrentProcess() -> *mut c_void;
}
