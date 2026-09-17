use std::ptr::NonNull;

use crate::error::{Pvz1051Error, Result};
use crate::raw::layout as ptrs;

use super::memory::{encode_rel32, read_bytes, write_bytes};

const IMITATOR_MORPH_CALL_SITE: usize = 0x466b9d;
const BOARD_ADD_PLANT_TARGET: usize = 0x40d120;
const CALL_LEN: usize = 5;
const ORIGINAL_CALL: [u8; CALL_LEN] = [0xe8, 0x7e, 0x65, 0xfa, 0xff];

/// Owns the verified `Plant::ImitaterMorph` call-site patch.
pub(crate) struct ImitatorMorphHookGuard {
    patched_call: [u8; CALL_LEN],
    restored: bool,
}

impl ImitatorMorphHookGuard {
    pub(crate) fn install() -> Result<Self> {
        let current = read_bytes(IMITATOR_MORPH_CALL_SITE, CALL_LEN);
        if current != ORIGINAL_CALL {
            return Err(Pvz1051Error::Patch(format!(
                "imitator morph expected {ORIGINAL_CALL:02x?} at 0x{IMITATOR_MORPH_CALL_SITE:x}, found {current:02x?}"
            )));
        }
        let patched_call = encode_rel32(
            0xe8,
            IMITATOR_MORPH_CALL_SITE,
            imitator_morph_shim as *const () as usize,
            "imitator morph shim",
        )?;
        write_bytes(IMITATOR_MORPH_CALL_SITE, &patched_call)?;
        Ok(Self {
            patched_call,
            restored: false,
        })
    }

    pub(crate) fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        let current = read_bytes(IMITATOR_MORPH_CALL_SITE, CALL_LEN);
        if current == ORIGINAL_CALL {
            self.restored = true;
            return Ok(());
        }
        if current != self.patched_call {
            return Err(Pvz1051Error::Patch(format!(
                "imitator morph call at 0x{IMITATOR_MORPH_CALL_SITE:x} changed while owned: expected {:02x?}, found {current:02x?}",
                self.patched_call
            )));
        }
        write_bytes(IMITATOR_MORPH_CALL_SITE, &ORIGINAL_CALL)?;
        self.restored = true;
        Ok(())
    }
}

impl Drop for ImitatorMorphHookGuard {
    fn drop(&mut self) {
        let _restore = self.restore();
    }
}

/// Replays the original stack-cleaning `Board::AddPlant` call, records both full DataArray IDs,
/// and returns with the original `ret 0x10` stack effect.
///
/// # Safety
///
/// This function may only be reached from the verified PvZ 1.0.0.1051 call at `0x466B9D`.
/// At that point EAX is the row, ESI is the dying imitator `Plant*`, and the four native
/// `Board::AddPlant` arguments occupy the stack immediately after the return address.
#[unsafe(naked)]
unsafe extern "C" fn imitator_morph_shim() {
    core::arch::naked_asm!(
        // Duplicate the original four arguments in reverse push order. Board::AddPlant pops this
        // copy with `ret 0x10`; the shim pops the call-site copy with its own `ret 0x10`.
        "push dword ptr [esp + 16]",
        "push dword ptr [esp + 16]",
        "push dword ptr [esp + 16]",
        "push dword ptr [esp + 16]",
        "mov edx, {board_add_plant}",
        "call edx",
        // Preserve the complete native return state while Rust updates fixed thread-local data.
        "pushfd",
        "pushad",
        "mov eax, esp",
        "sub esp, 528",
        "and esp, -16",
        "mov [esp + 512], eax",
        "fxsave [esp]",
        "fninit",
        "mov dword ptr [esp + 516], 0x1f80",
        "ldmxcsr [esp + 516]",
        "cld",
        // PUSHAD saves the pre-PUSHAD ESP at +12. That value follows PUSHFD, so its +8 slot is
        // the original callsite's Board* argument; +4/+28 are saved ESI/EAX (old/new Plant*).
        "mov edx, [esp + 512]",
        "mov ecx, [edx + 12]",
        "push dword ptr [ecx + 8]",
        "push dword ptr [edx + 4]",
        "push dword ptr [edx + 28]",
        "call {record}",
        "add esp, 12",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret 16",
        board_add_plant = const BOARD_ADD_PLANT_TARGET,
        record = sym record_imitator_morph,
    )
}

extern "C" fn record_imitator_morph(
    new_plant: *mut ptrs::Plant, placeholder: *mut ptrs::Plant, board: *mut ptrs::Board,
) {
    if std::panic::catch_unwind(|| record_imitator_morph_inner(new_plant, placeholder, board)).is_err() {
        crate::runtime::hook::fail_and_request_unload();
    }
}

fn record_imitator_morph_inner(new_plant: *mut ptrs::Plant, placeholder: *mut ptrs::Plant, board: *mut ptrs::Board) {
    let (Some(new_plant), Some(placeholder)) = (NonNull::new(new_plant), NonNull::new(placeholder)) else {
        return;
    };
    // SAFETY: the verified callsite keeps the dying placeholder's occupied DataArray slot alive
    // through Board::AddPlant, and the returned pointer is the newly occupied successor slot.
    let placeholder_id = unsafe { ptrs::DataArray::<ptrs::Plant>::item_id(placeholder.as_ptr()) };
    // SAFETY: same verified callsite; Board::AddPlant returned the new occupied Plant slot.
    let successor_id = unsafe { ptrs::DataArray::<ptrs::Plant>::item_id(new_plant.as_ptr()) };
    crate::runtime::imitator_morph::record(board, placeholder_id, successor_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verified_original_call_targets_board_add_plant() {
        assert_eq!(
            encode_rel32(
                0xe8,
                IMITATOR_MORPH_CALL_SITE,
                BOARD_ADD_PLANT_TARGET,
                "imitator morph shim"
            )
            .expect("encode original call"),
            ORIGINAL_CALL
        );
    }

    #[test]
    fn relative_call_encoding_rejects_out_of_range_target() {
        assert!(encode_rel32(0xe8, 0, usize::MAX, "imitator morph shim").is_err());
    }
}
