//! Declarative PvZ 1.0.0.1051 static patch toggles.
//!
//! Entries use `off => on` order. This is intentionally the reverse of Reisen's
//! C++ constructor order `{addr, on, off}` so migrated bytes must be checked
//! carefully. Byte arrays are explicit to avoid integer endianness ambiguity.

use super::spec::PatchBundle;

macro_rules! patch_mods {
    (
        $(
            $(#[$meta:meta])*
            $module:ident {
                $(
                    $(#[$entry_meta:meta])*
                    $name:literal @ $addr:literal: [$($off:literal),+ $(,)?] => [$($on:literal),+ $(,)?]
                ),+ $(,)?
            }
        )+
    ) => {
        $(
            $(#[$meta])*
            pub(crate) mod $module {
                use super::*;

                pub(crate) static BUNDLE: PatchBundle = PatchBundle {
                    name: stringify!($module),
                    specs: super::super::spec::checked_specs(&[
                        $(
                            $(#[$entry_meta])*
                            super::super::spec::PatchSpec {
                                name: $name,
                                addr: $addr,
                                off: &[$($off),+],
                                on: &[$($on),+],
                            },
                        )+
                    ]),
                };

            }
        )+
    };
}

patch_mods! {
    /// Ignores seed recharge/cooldown checks.
    seed_recharge_ignored {
        // PvZ 1.0.0.1051 EN objdump: SeedPacket::Update increments refresh_counter at
        // 0x48728c, compares it with refresh_time at 0x487293, then 0x487296 executes
        // `7e 14` (`jle 0x4872ac`). The patch owns only that one-byte condition opcode;
        // for the native non-negative counter/time domain, `jo` is not taken by the compare,
        // so activation runs immediately.
        // pvztools, AvZLib and Reisen's 1051 table independently use the same 0x7e -> 0x70 byte.
        "SeedPacket::Update::refresh_wait_branch_opcode" @ 0x487296: [0x7e] => [0x70],
    }

    /// Allows seed use without spending or requiring sun.
    sun_cost_ignored {
        // `Board::TakeSunMoney` at 0x41ba60: 0x41ba72 `7f 0c` (`jg 0x41ba80`).
        "Board::TakeSunMoney::sun_check_jump" @ 0x41ba72: [0x7f] => [0x70],
        // `Board::TakeSunMoney`: 0x41ba74 `2b f3` (`sub esi, ebx`).
        "Board::TakeSunMoney::subtract_to_compare" @ 0x41ba74: [0x2b] => [0x3b],
        // `Board::CanTakeSunMoney` at 0x41bab0: 0x41babf `0f 9e c0` (`setle al`).
        "Board::CanTakeSunMoney::result_predicate" @ 0x41bac0: [0x9e] => [0x91],
        // Seed chooser sun check: 0x427a91 `0f 8f c3 01 00 00`; owns condition byte.
        "SeedChooserScreen::CanPickSeed::sun_check_a" @ 0x427a92: [0x8f] => [0x80],
        // Seed chooser sun check: 0x427dfc `0f 8f 52 01 00 00`; owns condition byte.
        "SeedChooserScreen::CanPickSeed::sun_check_b" @ 0x427dfd: [0x8f] => [0x80],
        // I, Zombie flow: 0x42487f `74 06` (`je 0x424887`).
        "IZombie::sun_check" @ 0x42487f: [0x74] => [0xeb],
    }

    /// Forces fog update logic to reveal the fogged region.
    fog_revealed {
        // PvZ 1.0.0.1051 EN objdump: 0x41a68d `3b f2` (`cmp esi, edx`).
        // Reisen `RemoveFog` and pvztools `NoFog` patch it to `31 d2` (`xor edx, edx`).
        "Board::UpdateFog::clear_fog_compare" @ 0x41a68d: [0x3b, 0xf2] => [0x31, 0xd2],
    }

    /// Makes vase contents visible without waiting for the game reveal path.
    vase_contents_visible {
        // PvZ 1.0.0.1051 EN objdump: 0x44e5cc `85 c0; 7e 06`.
        // pvztoolkit/pvztools replace these four bytes with `66 b8 33 00`.
        "Vase::Draw::contents_visibility" @ 0x44e5cc: [0x85, 0xc0, 0x7e, 0x06] => [0x66, 0xb8, 0x33, 0x00],
    }

    /// Uses patched native timing for ice and ash plant effects.
    instant_ice_and_ash_effects {
        // `Plant::Update` effect-time branch: 0x463408 `75 06` (`jne ...`); owns only opcode byte.
        // The native branch is the generic `mDoSpecialCountdown` dispatch, so this can also affect
        // other plants using the same countdown path.
        // This follows Reisen `AshInstantExplode` (`jo`) rather than pvztools' `Explode::Immediately` (`je`).
        "Plant::Update::ice_ash_effect_time" @ 0x463408: [0x75] => [0x70],
    }

    /// Uses a fixed native cob impact delay. Core/DSL timing is not automatically adjusted.
    cob_fixed_delay {
        // `Projectile::UpdateMotion`: immediate delay bytes at 0x46d672.
        "Projectile::UpdateMotion::cob_fixed_delay" @ 0x46d672: [0xd0, 0x96] => [0x00, 0x97],
    }

    /// Shortens native cob recharge; this is not literal zero cooldown.
    cob_recharge_shortened {
        // `Plant::Update`: 0x46103a `0f 85 ...`; owns only condition opcode byte.
        "Plant::Update::cob_recharge_condition" @ 0x46103b: [0x85] => [0x80],
    }

    /// Disables jack-in-the-box zombie explosions.
    jack_explosions_disabled {
        // Reisen header says 0x5261fc, but 1051 EN/pvztools/pvztoolkit verify 0x526afc.
        // Objdump: 0x526afb `0f 8f 01 02 00 00` (`jg ...`); owns only condition opcode byte.
        "Zombie::UpdateJackInTheBox::explosion_jump" @ 0x526afc: [0x8f] => [0x81],
    }

    /// Disables pepper zombie explosions.
    pepper_explosions_disabled {
        // Objdump: `0f 85 ...` pepper explosion branch; owns only condition opcode byte.
        "Zombie::UpdateJalapeno::explosion_jump" @ 0x5275dd: [0x85] => [0x81],
    }

    /// Disables regular item drops.
    item_drop_disabled {
        // `Zombie::DropLoot`: control-flow byte from Reisen/pvztools table.
        "Zombie::DropLoot::item_drop_branch" @ 0x530276: [0x5b] => [0x66],
    }

    /// Disables graves, grave zombies, coral zombies, and bungee special events.
    special_events_disabled {
        // `Board::Update`: 0x413083 `75 ..`; owns only opcode byte.
        "Board::Update::special_event_a" @ 0x413083: [0x75] => [0xeb],
        // `Board::Update`: 0x42694a `75 ..`; owns only opcode byte.
        "Board::Update::special_event_b" @ 0x42694a: [0x75] => [0xeb],
    }

    /// Disables natural falling sun drops.
    natural_sun_drop_disabled {
        // `Board::Update`: skip both `add [sun_countdown], -1` and its drop branch.
        // The rel32 jump targets 0x413bf1, preserving a stable dormant countdown.
        "Board::Update::natural_sun_drop" @ 0x413b7c:
            [0x83, 0x86, 0x38, 0x55, 0x00, 0x00, 0xff, 0x75, 0x6c] =>
            [0xe9, 0x70, 0x00, 0x00, 0x00, 0x90, 0x90, 0x90, 0x90],
    }

    /// Uses the native Coin::Update path to automatically collect items.
    auto_collect_normal {
        // pvztoolkit 1051 `auto_collected`: 0x0043158f `75` -> `eb`.
        "Coin::Update::auto_collect" @ 0x43158f: [0x75] => [0xeb],
    }

    /// Fixes native cob drift by replacing a complete 10-byte instruction sequence.
    cob_drift_fixed {
        // Objdump 0x46dce3 covers: `jne +8; fld [esi+0x34]; fadd st,st(1); fstp [esi+0x34]`.
        "Projectile::UpdateMotion::cob_drift_sequence" @ 0x46dce3: [0x75, 0x08, 0xd9, 0x46, 0x34, 0xd8, 0xc1, 0xd9, 0x5e, 0x34] => [0x83, 0x7e, 0x5c, 0x0b, 0x75, 0x04, 0xdd, 0xd8, 0xeb, 0x1b],
    }

    /// Treats mushrooms as awake by native checks.
    mushrooms_awake {
        // `Plant::Update`: 0x45de8e `74 ..`; owns only opcode byte.
        "Plant::Update::mushroom_sleep_branch" @ 0x45de8e: [0x74] => [0xeb],
    }

    /// Kills house-entering zombies instead of taking the failure branch.
    zombies_die_at_house {
        // `Zombie::Update`: 0x52b308 `74 07` (`je ...`), replaced by two NOPs.
        "Zombie::Update::house_enter_failure_branch" @ 0x52b308: [0x74, 0x07] => [0x90, 0x90],
    }

    /// Ignores native planting placement restrictions. This is a dangerous overlap/placement rule.
    planting_restrictions_ignored {
        // Native plantability condition opcode; owns only opcode byte.
        "Board::CanPlantAt::condition_a" @ 0x40fe30: [0x84] => [0x81],
        // Native plantability condition opcode; owns only opcode byte.
        "Board::CanPlantAt::condition_b" @ 0x42a2d9: [0x84] => [0x8d],
        // Native plantability branch; owns only opcode byte.
        "Board::CanPlantAt::placement_branch" @ 0x438e40: [0x74] => [0xeb],
    }

    /// Makes profile save data readonly through native save/load checks.
    profile_readonly {
        // Objdump 0x413320: Board::SurvivalSaveScore has no stack
        // arguments, its callers ignore EAX, and it starts with `push esi`;
        // an immediate `ret` prevents its pre-save write to
        // PlayerInfo::mChallengeRecords.
        "Board::SurvivalSaveScore::readonly" @ 0x413320: [0x56] => [0xc3],
        // Profile write path byte from Reisen table.
        "Profile::Save::readonly_a" @ 0x482149: [0x13] => [0x2e],
        // Profile write path branch from Reisen table; owns only opcode byte.
        "Profile::Save::readonly_b" @ 0x54b267: [0x74] => [0x70],
    }

    /// Stops automatic zombie spawn refresh.
    zombie_spawn_stopped {
        // `Board::UpdateZombieSpawning`: 0x4265dc `74 ..`; owns only opcode byte.
        "Board::UpdateZombieSpawning::spawn_branch" @ 0x4265dc: [0x74] => [0xeb],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_recharge_uses_verified_seed_packet_update_branch_only() {
        let specs = seed_recharge_ignored::BUNDLE.specs;
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].addr, 0x487296);
        assert_eq!(specs[0].off, &[0x7e]);
        assert_eq!(specs[0].on, &[0x70]);
        assert!(
            !specs
                .iter()
                .any(|spec| matches!(spec.addr, 0x461565 | 0x461e36 | 0x461e37))
        );
    }

    #[test]
    fn natural_sun_disable_skips_countdown_aging_and_drop() {
        let spec = &natural_sun_drop_disabled::BUNDLE.specs[0];
        let displacement = i32::from_le_bytes(spec.on[1..5].try_into().expect("rel32 bytes"));
        assert_eq!((spec.addr + 5) as i64 + i64::from(displacement), 0x413bf1);
    }

    #[test]
    fn zombie_explosion_patches_are_independent_verified_bundles() {
        for jack in jack_explosions_disabled::BUNDLE.specs {
            for pepper in pepper_explosions_disabled::BUNDLE.specs {
                assert!(jack.addr + jack.off.len() <= pepper.addr || pepper.addr + pepper.off.len() <= jack.addr);
            }
        }
    }
}
