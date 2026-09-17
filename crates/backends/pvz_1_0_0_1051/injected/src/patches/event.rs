//! Verified 1051 event call-site ownership and trampolines.

use rsvz_model::model::EventInterest;

use crate::error::{Pvz1051Error, Result};

use super::memory::{encode_rel32, read_bytes, write_bytes};

const CALL_LEN: usize = 5;
const FRAME_SITE: usize = 0x4526f7;
const FRAME_TARGET: usize = 0x5d59a0;
const JACK_RADIUS_SITE: usize = 0x526c7f;
#[cfg(test)]
const JACK_RADIUS_TARGET: usize = 0x41cbf0;
const JACK_DIE_SITE: usize = 0x41cc39;
#[cfg(test)]
const JACK_DIE_TARGET: usize = 0x4679b0;
const JACK_RULE_SITE: usize = 0x41cc2f;
const BITE_SITE: usize = 0x52fcf0;
const BASKETBALL_SITE: usize = 0x46d7a6;
const GARG_SPIKEROCK_SITE: usize = 0x526d88;
#[cfg(test)]
const GARG_SPIKEROCK_TARGET: usize = 0x45ec00;
const GARG_SQUISH_SITE: usize = 0x52e97b;
#[cfg(test)]
const GARG_SQUISH_TARGET: usize = 0x462b80;
const GARG_RULE_SITE: usize = 0x52e93b;
const BUNGEE_LIFT_SITE: usize = 0x524dca;
const BUNGEE_LIFT_ORIGINAL: &[&[u8]] = &[&[0xc7, 0x86, 0x34, 0x01, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00]];
const HOME_SITE: usize = 0x52b301;
const HOME_TARGET: usize = 0x4537d0;
const GARG_ALLOC_SITE: usize = 0x40de1d;
const GARG_ALLOC_TARGET: usize = 0x522580;
const IMP_READY_SITE: usize = 0x527141;
const IMP_READY_TARGET: usize = 0x52c310;
const GARG_ASH_SITE: usize = 0x532b96;

const FRAME_ORIGINAL: &[u8] = &[0xe8, 0xa4, 0x32, 0x18, 0x00];
const JACK_RADIUS_ORIGINAL: &[u8] = &[0xe8, 0x6c, 0x5f, 0xef, 0xff];
const JACK_DIE_ORIGINAL: &[u8] = &[0xe8, 0x72, 0xad, 0x04, 0x00];
const JACK_RULE_VARIANTS: &[&[u8]] = &[&[0x74], &[0xeb]];
const BITE_VARIANTS: &[&[u8]] = &[
    &[0x83, 0x46, 0x40, 0xfc, 0x8b, 0x4e, 0x40],
    &[0x83, 0x46, 0x40, 0x00, 0x8b, 0x4e, 0x40],
    &[0x83, 0x66, 0x40, 0x00, 0x8b, 0x4e, 0x40],
];
const BASKETBALL_VARIANTS: &[&[u8]] = &[
    &[0x29, 0x4e, 0x40, 0x83, 0xf8, 0x19],
    &[0x90, 0x90, 0x90, 0x83, 0xf8, 0x19],
    &[0x29, 0x76, 0x40, 0x83, 0xf8, 0x19],
];
const GARG_SPIKEROCK_ORIGINAL: &[u8] = &[0xe8, 0x73, 0x7e, 0xf3, 0xff];
const GARG_SQUISH_ORIGINAL: &[u8] = &[0xe8, 0x00, 0x42, 0xf3, 0xff];
const GARG_RULE_VARIANTS: &[&[u8]] = &[&[0x74], &[0xeb]];
const HOME_ORIGINAL: &[u8] = &[0xe8, 0xca, 0x84, 0xf2, 0xff];
const GARG_ALLOC_ORIGINAL: &[u8] = &[0xe8, 0x5e, 0x47, 0x11, 0x00];
const IMP_READY_ORIGINAL: &[u8] = &[0xe8, 0xca, 0x51, 0x00, 0x00];
const GARG_ASH_ORIGINAL: &[&[u8]] = &[&[0x81, 0xbe, 0xc8, 0x00, 0x00, 0x00, 0x08, 0x07, 0x00, 0x00]];

struct OwnedPatch {
    site: usize,
    original: Vec<u8>,
    installed: Vec<u8>,
}

pub(crate) struct EventHookGuard {
    interest: EventInterest,
    patches: Vec<OwnedPatch>,
    restored: bool,
}

impl EventHookGuard {
    pub(crate) fn install(interest: EventInterest) -> Result<Self> {
        let mut guard = Self {
            interest,
            patches: Vec::with_capacity(14),
            restored: false,
        };
        let result = guard.install_all();
        if result.is_err() {
            return super::memory::finish_restoration(result, guard.restore(), "event hook install").map(|()| guard);
        }
        Ok(guard)
    }

    pub(crate) fn interest(&self) -> EventInterest {
        self.interest
    }

    fn install_all(&mut self) -> Result<()> {
        let interest = self.interest;
        install_patch_plan(interest, self)
    }

    fn install_call(&mut self, site: usize, original: &'static [u8], target: usize) -> Result<()> {
        let replacement = encode_rel32(0xe8, site, target, "event shim")?.to_vec();
        self.install_bytes(site, &[original], &replacement)
    }

    fn install_nop_call(&mut self, site: usize, originals: &[&[u8]], target: usize) -> Result<()> {
        let len = originals.first().map_or(0, |bytes| bytes.len());
        if len < CALL_LEN || originals.iter().any(|bytes| bytes.len() != len) {
            return Err(Pvz1051Error::Patch(
                "invalid event trampoline fixture length".to_owned(),
            ));
        }
        let mut replacement = vec![0x90; len];
        replacement[..CALL_LEN].copy_from_slice(&encode_rel32(0xe8, site, target, "event shim")?);
        self.install_bytes(site, originals, &replacement)
    }

    fn install_bytes(&mut self, site: usize, originals: &[&[u8]], installed: &[u8]) -> Result<()> {
        let len = installed.len();
        if len == 0 || originals.iter().any(|bytes| bytes.len() != len) {
            return Err(Pvz1051Error::Patch("invalid event patch fixture length".to_owned()));
        }
        let current = read_bytes(site, len);
        if !originals.iter().any(|expected| current == *expected) {
            return Err(Pvz1051Error::Patch(format!(
                "event hook at 0x{site:x} expected one of {originals:02x?}, found {current:02x?}"
            )));
        }
        write_bytes(site, installed)?;
        self.patches.push(OwnedPatch {
            site,
            original: current,
            installed: installed.to_vec(),
        });
        Ok(())
    }

    pub(crate) fn restore(&mut self) -> Result<()> {
        if self.restored {
            return Ok(());
        }
        let mut first_error = None;
        for patch in self.patches.iter().rev() {
            let current = read_bytes(patch.site, patch.installed.len());
            if current == patch.original {
                continue;
            }
            if current != patch.installed {
                first_error.get_or_insert_with(|| {
                    Pvz1051Error::Patch(format!(
                        "event hook at 0x{:x} changed while owned: expected {:02x?}, found {:02x?}",
                        patch.site, patch.installed, current
                    ))
                });
                continue;
            }
            if let Err(error) = write_bytes(patch.site, &patch.original) {
                first_error.get_or_insert(error);
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        self.patches.clear();
        self.restored = true;
        Ok(())
    }
}

trait EventPatchSink {
    fn call(&mut self, site: usize, original: &'static [u8], target: usize) -> Result<()>;
    fn nop_call(&mut self, site: usize, originals: &[&[u8]], target: usize) -> Result<()>;
    fn bytes(&mut self, site: usize, originals: &[&[u8]], installed: &[u8]) -> Result<()>;
}

impl EventPatchSink for EventHookGuard {
    fn call(&mut self, site: usize, original: &'static [u8], target: usize) -> Result<()> {
        self.install_call(site, original, target)
    }

    fn nop_call(&mut self, site: usize, originals: &[&[u8]], target: usize) -> Result<()> {
        self.install_nop_call(site, originals, target)
    }

    fn bytes(&mut self, site: usize, originals: &[&[u8]], installed: &[u8]) -> Result<()> {
        self.install_bytes(site, originals, installed)
    }
}

fn install_patch_plan(interest: EventInterest, sink: &mut impl EventPatchSink) -> Result<()> {
    sink.call(FRAME_SITE, FRAME_ORIGINAL, logical_frame_shim as *const () as usize)?;
    if interest.contains(EventInterest::JACK) {
        sink.bytes(JACK_RULE_SITE, JACK_RULE_VARIANTS, &[0x74])?;
        sink.call(
            JACK_RADIUS_SITE,
            JACK_RADIUS_ORIGINAL,
            jack_radius_shim as *const () as usize,
        )?;
        sink.call(JACK_DIE_SITE, JACK_DIE_ORIGINAL, jack_die_shim as *const () as usize)?;
    }
    if interest.contains(EventInterest::BITE) {
        sink.nop_call(BITE_SITE, BITE_VARIANTS, bite_shim as *const () as usize)?;
    }
    if interest.contains(EventInterest::GARGANTUAR) || interest.contains(EventInterest::VEHICLE_CRUSH) {
        sink.bytes(GARG_RULE_SITE, GARG_RULE_VARIANTS, &[0x74])?;
    }
    if interest.contains(EventInterest::GARGANTUAR) {
        sink.call(
            GARG_SPIKEROCK_SITE,
            GARG_SPIKEROCK_ORIGINAL,
            garg_spikerock_shim as *const () as usize,
        )?;
    }
    if interest.contains(EventInterest::GARGANTUAR) || interest.contains(EventInterest::VEHICLE_CRUSH) {
        sink.call(
            GARG_SQUISH_SITE,
            GARG_SQUISH_ORIGINAL,
            garg_squish_shim as *const () as usize,
        )?;
    }
    if interest.contains(EventInterest::BUNGEE) {
        sink.nop_call(
            BUNGEE_LIFT_SITE,
            BUNGEE_LIFT_ORIGINAL,
            bungee_lift_shim as *const () as usize,
        )?;
    }
    if interest.contains(EventInterest::BASKETBALL) {
        sink.nop_call(
            BASKETBALL_SITE,
            BASKETBALL_VARIANTS,
            basketball_shim as *const () as usize,
        )?;
    }
    if interest.contains(EventInterest::HOME_ENTRY) {
        sink.call(HOME_SITE, HOME_ORIGINAL, home_entry_shim as *const () as usize)?;
    }
    if interest.contains(EventInterest::IMP_DIAGNOSTIC) {
        sink.call(
            GARG_ALLOC_SITE,
            GARG_ALLOC_ORIGINAL,
            gargantuar_alloc_shim as *const () as usize,
        )?;
        sink.call(IMP_READY_SITE, IMP_READY_ORIGINAL, imp_ready_shim as *const () as usize)?;
        sink.nop_call(
            GARG_ASH_SITE,
            GARG_ASH_ORIGINAL,
            gargantuar_ash_shim as *const () as usize,
        )?;
    }
    Ok(())
}

impl Drop for EventHookGuard {
    fn drop(&mut self) {
        let _restore = self.restore();
    }
}

#[unsafe(naked)]
unsafe extern "C" fn logical_frame_shim() {
    core::arch::naked_asm!(
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
        "call {begin}",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "mov eax, {target}",
        "call eax",
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
        "call {end}",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret",
        begin = sym crate::impls::event::begin_logic_frame_from_patch,
        end = sym crate::impls::event::end_logic_frame_from_patch,
        target = const FRAME_TARGET,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn jack_radius_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "mov eax, [edx + 12]",
        "push dword ptr [edx + 16]",
        "push dword ptr [eax + 8]",
        "push dword ptr [edx]",
        "push dword ptr [edx + 4]",
        "call {callback}",
        "add esp, 16",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret 4",
        callback = sym crate::impls::event::process_jack_radius_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn jack_die_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "mov eax, [edx + 12]",
        "push dword ptr [eax + 8]",
        "call {callback}",
        "add esp, 4",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret 4",
        callback = sym crate::impls::event::process_jack_plant_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn bite_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "push dword ptr [edx + 4]",
        "push dword ptr [edx + 8]",
        "call {callback}",
        "add esp, 8",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "mov ecx, [esi + 0x40]",
        "ret",
        callback = sym crate::impls::event::process_bite_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn basketball_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "push dword ptr [edx + 24]",
        "push dword ptr [edx + 4]",
        "push dword ptr [edx + 8]",
        "call {callback}",
        "add esp, 12",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "cmp eax, 0x19",
        "ret",
        callback = sym crate::impls::event::process_basketball_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn garg_spikerock_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "push dword ptr [edx + 4]",
        "push dword ptr [edx + 16]",
        "call {callback}",
        "add esp, 8",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret",
        callback = sym crate::impls::event::process_garg_spikerock_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn garg_squish_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "mov eax, [edx + 12]",
        // Saved pre-pushad ESP is entry ESP - 4; attack type is entry ESP + 0x24.
        "push dword ptr [eax + 0x28]",
        "push dword ptr [eax + 8]",
        "push dword ptr [edx]",
        "call {callback}",
        "add esp, 12",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret 4",
        callback = sym crate::impls::event::process_garg_squish_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn bungee_lift_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        // 0x524dca: ESI is the validated Plant, EDI is the acting Zombie.
        "push dword ptr [edx + 4]",
        "push dword ptr [edx]",
        "call {callback}",
        "add esp, 8",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret",
        callback = sym crate::impls::event::process_bungee_lift_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn home_entry_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "push dword ptr [edx + 4]",
        "call {callback}",
        "add esp, 4",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "mov edx, {target}",
        "call edx",
        "ret",
        callback = sym crate::impls::event::emit_home_entry_from_patch,
        target = const HOME_TARGET,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn gargantuar_alloc_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "mov ecx, [edx + 8]",
        "push dword ptr [ecx + 12]",
        "push dword ptr [ecx + 8]",
        "push dword ptr [edx + 16]",
        "push dword ptr [edx]",
        "call {callback}",
        "add esp, 16",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "mov edx, {target}",
        "jmp edx",
        target = const GARG_ALLOC_TARGET,
        callback = sym crate::impls::event::emit_gargantuar_allocated_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn imp_ready_shim() {
    core::arch::naked_asm!(
        "mov edx, {target}",
        "call edx",
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
        "mov edx, [esp + 512]",
        "push dword ptr [edx + 4]",
        "push dword ptr [edx + 16]",
        "call {callback}",
        "add esp, 8",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "ret",
        target = const IMP_READY_TARGET,
        callback = sym crate::impls::event::emit_imp_thrown_from_patch,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn gargantuar_ash_shim() {
    core::arch::naked_asm!(
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
        "mov edx, [esp + 512]",
        "push dword ptr [edx + 4]",
        "call {callback}",
        "add esp, 4",
        "fxrstor [esp]",
        "mov esp, [esp + 512]",
        "popad",
        "popfd",
        "cmp dword ptr [esi + 0xc8], 0x708",
        "ret",
        callback = sym crate::impls::event::emit_gargantuar_ash_hit_from_patch,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    struct RecordedPatch {
        site: usize,
        variants: usize,
        len: usize,
        target: Option<usize>,
        installed: Vec<u8>,
    }

    #[derive(Default)]
    struct Recorder(Vec<RecordedPatch>);

    impl EventPatchSink for Recorder {
        fn call(&mut self, site: usize, original: &'static [u8], target: usize) -> Result<()> {
            self.0.push(RecordedPatch {
                site,
                variants: 1,
                len: original.len(),
                target: Some(target),
                installed: Vec::new(),
            });
            Ok(())
        }

        fn nop_call(&mut self, site: usize, originals: &[&[u8]], target: usize) -> Result<()> {
            self.0.push(RecordedPatch {
                site,
                variants: originals.len(),
                len: originals.first().map_or(0, |bytes| bytes.len()),
                target: Some(target),
                installed: Vec::new(),
            });
            Ok(())
        }

        fn bytes(&mut self, site: usize, originals: &[&[u8]], installed: &[u8]) -> Result<()> {
            self.0.push(RecordedPatch {
                site,
                variants: originals.len(),
                len: installed.len(),
                target: None,
                installed: installed.to_vec(),
            });
            Ok(())
        }
    }

    fn recorded_sites(interest: EventInterest) -> Vec<usize> {
        let mut recorder = Recorder::default();
        install_patch_plan(interest, &mut recorder).expect("record patch plan");
        recorder.0.into_iter().map(|patch| patch.site).collect()
    }

    #[test]
    fn raw_call_fixtures_target_the_verified_1051_functions() {
        for (site, target, expected) in [
            (FRAME_SITE, FRAME_TARGET, FRAME_ORIGINAL),
            (JACK_RADIUS_SITE, JACK_RADIUS_TARGET, JACK_RADIUS_ORIGINAL),
            (JACK_DIE_SITE, JACK_DIE_TARGET, JACK_DIE_ORIGINAL),
            (GARG_SPIKEROCK_SITE, GARG_SPIKEROCK_TARGET, GARG_SPIKEROCK_ORIGINAL),
            (GARG_SQUISH_SITE, GARG_SQUISH_TARGET, GARG_SQUISH_ORIGINAL),
            (HOME_SITE, HOME_TARGET, HOME_ORIGINAL),
            (GARG_ALLOC_SITE, GARG_ALLOC_TARGET, GARG_ALLOC_ORIGINAL),
            (IMP_READY_SITE, IMP_READY_TARGET, IMP_READY_ORIGINAL),
        ] {
            assert_eq!(encode_rel32(0xe8, site, target, "event shim").unwrap(), expected);
        }
    }

    #[test]
    fn direct_instruction_fixtures_cover_whole_instructions() {
        assert!(BITE_VARIANTS.iter().all(|bytes| bytes.len() == 7));
        assert!(BASKETBALL_VARIANTS.iter().all(|bytes| bytes.len() == 6));
        assert_eq!(JACK_RULE_VARIANTS, &[&[0x74][..], &[0xeb][..]]);
        assert_eq!(GARG_RULE_VARIANTS, &[&[0x74][..], &[0xeb][..]]);
        assert_eq!(GARG_ASH_ORIGINAL[0].len(), 10);
        assert_eq!(BUNGEE_LIFT_ORIGINAL[0].len(), 10);
    }

    #[test]
    fn event_owned_ranges_do_not_overlap() {
        let ranges = [
            (FRAME_SITE, 5),
            (JACK_RULE_SITE, 1),
            (JACK_RADIUS_SITE, 5),
            (JACK_DIE_SITE, 5),
            (BITE_SITE, 7),
            (BASKETBALL_SITE, 6),
            (GARG_RULE_SITE, 1),
            (GARG_SPIKEROCK_SITE, 5),
            (GARG_SQUISH_SITE, 5),
            (HOME_SITE, 5),
            (GARG_ALLOC_SITE, 5),
            (IMP_READY_SITE, 5),
            (GARG_ASH_SITE, 10),
            (BUNGEE_LIFT_SITE, 10),
        ];
        for (index, left) in ranges.iter().copied().enumerate() {
            for right in ranges[index + 1..].iter().copied() {
                assert!(left.0 + left.1 <= right.0 || right.0 + right.1 <= left.0);
            }
        }
    }

    #[test]
    fn every_interest_combination_installs_only_its_verified_trampolines() {
        assert_eq!(
            recorded_sites(EventInterest::JACK),
            [FRAME_SITE, JACK_RULE_SITE, JACK_RADIUS_SITE, JACK_DIE_SITE]
        );
        assert_eq!(recorded_sites(EventInterest::BITE), [FRAME_SITE, BITE_SITE]);
        assert_eq!(
            recorded_sites(EventInterest::VEHICLE_CRUSH),
            [FRAME_SITE, GARG_RULE_SITE, GARG_SQUISH_SITE]
        );
        assert_eq!(recorded_sites(EventInterest::BUNGEE), [FRAME_SITE, BUNGEE_LIFT_SITE]);
        assert_eq!(
            recorded_sites(EventInterest::GARGANTUAR),
            [FRAME_SITE, GARG_RULE_SITE, GARG_SPIKEROCK_SITE, GARG_SQUISH_SITE]
        );
        assert_eq!(recorded_sites(EventInterest::BASKETBALL), [FRAME_SITE, BASKETBALL_SITE]);
        assert_eq!(recorded_sites(EventInterest::HOME_ENTRY), [FRAME_SITE, HOME_SITE]);
        assert_eq!(
            recorded_sites(EventInterest::IMP_DIAGNOSTIC),
            [FRAME_SITE, GARG_ALLOC_SITE, IMP_READY_SITE, GARG_ASH_SITE]
        );
        assert_eq!(
            recorded_sites(EventInterest::PLANT_EFFECT.union(EventInterest::HOME_ENTRY)),
            [
                FRAME_SITE,
                JACK_RULE_SITE,
                JACK_RADIUS_SITE,
                JACK_DIE_SITE,
                BITE_SITE,
                GARG_RULE_SITE,
                GARG_SPIKEROCK_SITE,
                GARG_SQUISH_SITE,
                BUNGEE_LIFT_SITE,
                BASKETBALL_SITE,
                HOME_SITE,
            ]
        );
    }

    #[test]
    fn combined_plan_accepts_modifier_variants_and_normalizes_shared_gates() {
        let mut recorder = Recorder::default();
        install_patch_plan(EventInterest::PLANT_EFFECT, &mut recorder).expect("record patch plan");
        let patch = |site| {
            recorder
                .0
                .iter()
                .find(|patch| patch.site == site)
                .expect("planned site")
        };

        assert_eq!(
            (
                patch(JACK_RULE_SITE).variants,
                patch(JACK_RULE_SITE).installed.as_slice()
            ),
            (2, &[0x74][..])
        );
        assert_eq!(
            (
                patch(GARG_RULE_SITE).variants,
                patch(GARG_RULE_SITE).installed.as_slice()
            ),
            (2, &[0x74][..])
        );
        assert_eq!((patch(BITE_SITE).variants, patch(BITE_SITE).len), (3, 7));
        assert_eq!((patch(BASKETBALL_SITE).variants, patch(BASKETBALL_SITE).len), (3, 6));
    }
}
