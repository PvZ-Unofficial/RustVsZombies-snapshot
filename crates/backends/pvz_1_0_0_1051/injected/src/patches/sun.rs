use std::cell::RefCell;

use crate::error::{Pvz1051Error, Result};

use super::memory::{encode_rel32, read_bytes, write_bytes};

const ADD_COIN: usize = 0x40cb10;
const CALLS: [(usize, [u8; 5]); 4] = [
    (0x413bec, [0xe8, 0x1f, 0x8f, 0xff, 0xff]),
    (0x45fae2, [0xe8, 0x29, 0xd0, 0xfa, 0xff]),
    (0x45fb1d, [0xe8, 0xee, 0xcf, 0xfa, 0xff]),
    (0x45fb44, [0xe8, 0xc7, 0xcf, 0xfa, 0xff]),
];

struct Guard {
    patched: [[u8; 5]; 4],
}

impl Guard {
    fn install() -> Result<Self> {
        let mut patched = [[0; 5]; 4];
        for (index, (site, original)) in CALLS.iter().copied().enumerate() {
            let current = read_bytes(site, 5);
            if current != original {
                return Err(Pvz1051Error::Patch(format!(
                    "sun-production call at 0x{site:x} expected {original:02x?}, found {current:02x?}"
                )));
            }
            patched[index] = encode_rel32(0xe8, site, direct_credit_shim as *const () as usize, "sun shim")?;
        }
        for (index, (site, _original)) in CALLS.iter().copied().enumerate() {
            if let Err(error) = write_bytes(site, &patched[index]) {
                let mut rollback_error = None;
                for (restore_site, restore_bytes) in CALLS[..index].iter().copied().rev() {
                    if let Err(error) = write_bytes(restore_site, &restore_bytes) {
                        rollback_error.get_or_insert(error);
                    }
                }
                if let Some(rollback) = rollback_error {
                    crate::runtime::hook::fail_and_request_unload();
                    return Err(Pvz1051Error::Patch(format!(
                        "sun hook install failed: {error}; rollback also failed: {rollback}"
                    )));
                }
                return Err(error);
            }
        }
        Ok(Self { patched })
    }

    fn restore(&mut self) -> Result<()> {
        let mut first_error = None;
        for (index, (site, original)) in CALLS.iter().copied().enumerate() {
            let current = read_bytes(site, 5);
            if current == original {
                continue;
            }
            if current != self.patched[index] {
                first_error.get_or_insert_with(|| {
                    Pvz1051Error::Patch(format!("sun-production call at 0x{site:x} changed while owned"))
                });
                continue;
            }
            if let Err(error) = write_bytes(site, &original) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _result = self.restore();
    }
}

thread_local! {
    static GUARD: RefCell<Option<Guard>> = const { RefCell::new(None) };
}

pub(crate) fn set_direct_credit(enabled: bool) -> Result<()> {
    GUARD.with(|guard| {
        let mut guard = guard.borrow_mut();
        if enabled {
            if guard.is_none() {
                *guard = Some(Guard::install()?);
            }
            Ok(())
        } else {
            let Some(current) = guard.as_mut() else {
                return Ok(());
            };
            current.restore()?;
            *guard = None;
            Ok(())
        }
    })
}

pub(crate) fn restore() -> Result<()> {
    set_direct_credit(false)
}

/// Replaces only the four verified natural/producer AddCoin calls. Non-sun
/// coin types at the shared producer call tail-jump to the original AddCoin.
#[unsafe(naked)]
unsafe extern "C" fn direct_credit_shim() {
    core::arch::naked_asm!(
        "cmp dword ptr [esp + 12], 4",
        "je 2f",
        "cmp dword ptr [esp + 12], 5",
        "je 3f",
        "cmp dword ptr [esp + 12], 6",
        "je 4f",
        "mov edx, {add_coin}",
        "jmp edx",
        "2:",
        "mov edx, 25",
        "jmp 5f",
        "3:",
        "mov edx, 15",
        "jmp 5f",
        "4:",
        "mov edx, 50",
        "5:",
        "mov eax, ecx",
        "mov ecx, edx",
        "mov edx, {add_sun}",
        "call edx",
        "xor eax, eax",
        "ret 16",
        add_coin = const ADD_COIN,
        add_sun = const 0x41b960,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_calls_all_target_add_coin() {
        for (site, original) in CALLS {
            assert_eq!(
                encode_rel32(0xe8, site, ADD_COIN, "sun shim").expect("original call"),
                original
            );
        }
    }
}
