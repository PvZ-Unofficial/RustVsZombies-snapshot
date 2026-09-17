use super::*;

impl BattleStatusBackend for PeBackend {
    fn battle_status(&self) -> Result<BattleStatus> {
        if self.game_over()? {
            return Ok(BattleStatus::Lost);
        }
        if self.objective_reached_visible()? {
            return Ok(BattleStatus::ObjectiveReached);
        }
        Ok(BattleStatus::Running)
    }
}

impl BattleEntryBackend for PeBackend {
    fn enter_game(&mut self, _config: BattleConfig) -> Result<()> {
        Ok(())
    }

    fn start_battle(&mut self) -> Result<()> {
        self.finish_battle_opening()
    }
}

impl WorldResetBackend for PeBackend {
    fn reset_world(&mut self, reset: WorldResetConfig) -> Result<()> {
        if reset.card_cooldowns == ResetCardCooldowns::PreserveNative {
            return Err(PeBackendError::Unsupported(
                "preserving card cooldowns across world reset",
            ));
        }
        if reset.completed_rounds < crate::runtime::MIN_COMPLETED_ROUNDS {
            return Err(PeBackendError::Unsupported("PE only supports completed_rounds >= 63"));
        }
        let total_flags = reset
            .completed_rounds
            .checked_mul(2)
            .ok_or(PeBackendError::NumericOutOfRange("completed_rounds * 2"))?;
        i32::try_from(reset.initial_sun).map_err(|_error| PeBackendError::NumericOutOfRange("initial_sun"))?;
        let mut config = self.config()?;
        config.battle_seed = reset.seed;
        config.level_seed = reset.seed;
        config.total_flags = total_flags;
        config.initial_sun = reset.initial_sun;
        self.reset_with_config_deferred_spawn(config)
    }
}

impl GameUiBackend for PeBackend {
    fn game_ui(&self) -> Result<GameUi> {
        Ok(if self.game_over()? {
            GameUi::ZombiesWon
        } else if !self.spawn_ready()? {
            GameUi::LevelIntro
        } else {
            GameUi::Playing
        })
    }

    fn game_ui_unavailable(&self, _error: &Self::Error) -> bool {
        false
    }
}

impl BoardReadinessBackend for PeBackend {
    fn level_intro_board_ready(&self) -> Result<bool> {
        Ok(!self.spawn_ready()?)
    }

    fn playing_board_ready(&self) -> Result<bool> {
        self.spawn_ready()
    }
}

impl SceneBackend for PeBackend {
    fn scene(&self) -> Result<SceneKind> {
        Ok(crate::convert::scene::scene_from_pe(self.config()?.scene))
    }
}

impl ClockBackend for PeBackend {
    fn clock(&self) -> Result<i32> {
        self.clock_value()
    }
}

impl CursorQueryBackend for PeBackend {
    fn cursor_type(&self) -> Result<i32> {
        Ok(0)
    }
}

impl CurrentWaveBackend for PeBackend {
    fn current_wave(&self) -> Result<Wave> {
        let wave = self.with_current_world(|world| {
            let spawn = crate::access::spawn_data(world).as_ptr();
            // SAFETY: `spawn` belongs to the current world and the scalar is copied immediately.
            Ok::<_, pe_rs::Error>(unsafe { std::ptr::addr_of!((*spawn).wave).read() })
        })??;
        i32::try_from(wave)
            .map(Wave)
            .map_err(|_| PeBackendError::NumericOutOfRange("current wave"))
    }
}

impl WaveTimingBackend for PeBackend {
    fn total_waves(&self) -> Result<i32> {
        Ok(DEFAULT_SPAWN_WAVES as i32)
    }
    fn refresh_countdown(&self) -> Result<i32> {
        let value = self.with_current_world(|world| {
            let spawn = crate::access::spawn_data(world).as_ptr();
            Ok::<_, pe_rs::Error>(read_pe_raw_field!(spawn, countdown.next_wave))
        })??;
        crate::convert::timing::i32_from_u32("timing.next_wave_countdown", value)
    }
    fn initial_countdown(&self) -> Result<i32> {
        let value = self.with_current_world(|world| {
            let spawn = crate::access::spawn_data(world).as_ptr();
            Ok::<_, pe_rs::Error>(read_pe_raw_field!(spawn, countdown.next_wave_initial))
        })??;
        crate::convert::timing::i32_from_u32("timing.next_wave_initial_countdown", value)
    }
    fn huge_wave_countdown(&self) -> Result<i32> {
        let value = self.with_current_world(|world| {
            let spawn = crate::access::spawn_data(world).as_ptr();
            Ok::<_, pe_rs::Error>(read_pe_raw_field!(spawn, countdown.hugewave_fade))
        })??;
        crate::convert::timing::i32_from_u32("timing.huge_wave_countdown", value)
    }
    fn level_end_countdown(&self) -> Result<i32> {
        let value = self.with_current_world(|world| {
            let spawn = crate::access::spawn_data(world).as_ptr();
            Ok::<_, pe_rs::Error>(read_pe_raw_field!(spawn, countdown.endgame))
        })??;
        crate::convert::timing::i32_from_u32("timing.level_end_countdown", value)
    }
}

impl WaveRefreshControlBackend for PeBackend {
    fn commit_timer_only_wave_refresh(&self, expected_current_wave: Wave, initial_countdown: i32) -> Result<()> {
        let initial = u32::try_from(initial_countdown)
            .map_err(|_| PeBackendError::NumericOutOfRange("wave refresh initial countdown"))?;
        if initial == 0 || initial > 5_500 {
            return Err(PeBackendError::OperationRejected(
                "wave refresh initial countdown must be in 1..=5500",
            ));
        }
        let expected = u32::try_from(expected_current_wave.0)
            .map_err(|_| PeBackendError::NumericOutOfRange("expected current wave"))?;
        if self.current_wave()? != expected_current_wave {
            return Err(PeBackendError::OperationRejected(
                "wave refresh control expected wave is not current",
            ));
        }
        if expected >= DEFAULT_SPAWN_WAVES as u32 {
            return Err(PeBackendError::OperationRejected(
                "wave refresh control cannot target the final wave",
            ));
        }
        self.with_current_world(|world| {
            let spawn = crate::access::spawn_data(world).as_ptr();
            // SAFETY: `spawn` belongs to the current exclusive world scope;
            // preconditions are checked before writing the refresh threshold and timers.
            unsafe {
                debug_assert_eq!(std::ptr::addr_of!((*spawn).wave).read(), expected);
                std::ptr::addr_of_mut!((*spawn).hp.threshold).write(-1);
                std::ptr::addr_of_mut!((*spawn).countdown.next_wave).write(initial - 1);
                std::ptr::addr_of_mut!((*spawn).countdown.next_wave_initial).write(initial);
            }
            Ok::<(), pe_rs::Error>(())
        })??;
        Ok(())
    }
}

impl WaveHealthBackend for PeBackend {
    fn zombie_health_wave_start(&self) -> Result<i32> {
        i32::try_from(PeBackend::current_wave_initial_health(self)?)
            .map_err(|_| PeBackendError::NumericOutOfRange("current wave initial health"))
    }

    fn total_zombies_health_in_wave(&self, wave: i32) -> Result<i32> {
        let wave = u32::try_from(wave).map_err(|_| PeBackendError::NumericOutOfRange("wave health index"))?;
        self.with_current_world(|world| world.total_zombies_health_in_wave(wave))
    }
}

impl GameSpeedHintBackend for PeBackend {
    fn set_game_speed(&self, _speed: PositiveFiniteF32) -> std::result::Result<(), Self::Error> {
        Ok(())
    }
}

impl CardAppendSelectionBackend for PeBackend {
    fn finish_card_selection(&self) -> Result<()> {
        self.commit_card_selection()
    }

    fn select_card(&self, selection: CheckedCardSelection) -> Result<()> {
        if self.append_pending_card(selection)? {
            return Ok(());
        }
        let (packet_type, imitater_type) = crate::convert::kind::card_to_pe(selection);
        self.with_current_world(|world| {
            let scene = world.scene();
            for slot in 0..rsvz_model::MAX_SEED_SLOTS {
                let card = scene.card_at(slot as u32)?.as_ptr();
                if read_pe_raw_field!(card, type_) != pe_rs::PlantType::None.to_raw() {
                    continue;
                }
                // SAFETY: `card` is the current world's fixed slot. Appending a
                // selection is three address-stable scalar writes.
                unsafe {
                    std::ptr::addr_of_mut!((*card).type_).write(packet_type.to_raw());
                    std::ptr::addr_of_mut!((*card).imitater_type).write(imitater_type.to_raw());
                    std::ptr::addr_of_mut!((*card).cold_down).write(0);
                }
                return Ok(());
            }
            Err(pe_rs::Error::InvalidArgument {
                detail: rsvz_model::MAX_SEED_SLOTS as i32,
            })
        })??;
        Ok(())
    }
}

impl CardSelectionReadBackend for PeBackend {
    fn selected_card_count(&self) -> Result<usize> {
        if let Some(count) = self.pending_card_count()? {
            return Ok(count);
        }
        Ok(self.seeds()?.count())
    }

    fn selected_card(&self, index: usize) -> Result<CheckedCardSelection> {
        if self.pending_card_count()?.is_some() {
            return self.pending_card(index)?.ok_or(PeBackendError::InvalidSeedSlot);
        }
        let seed = self.seeds()?.nth(index).ok_or(PeBackendError::InvalidSeedSlot)?;
        self.seed_selection(seed)
    }
}

impl SeedBankReadBackend for PeBackend {
    type SeedHandle<'a> = crate::handles::PeSeedHandle<'a>;
    type SeedIter<'a> = crate::handles::PeSeedIter<'a>;

    fn seeds(&self) -> Result<Self::SeedIter<'_>> {
        Ok(crate::handles::PeSeedIter::new(self))
    }

    fn seed_slot<'a>(&'a self, handle: Self::SeedHandle<'a>) -> SeedSlot {
        handle.slot
    }

    fn seed_selection<'a>(&'a self, handle: Self::SeedHandle<'a>) -> Result<CheckedCardSelection> {
        pe_card_selection_from_raw(handle.as_ptr())?.ok_or(PeBackendError::InvalidSeedSlot)
    }

    fn seed_is_usable<'a>(&'a self, handle: Self::SeedHandle<'a>) -> bool {
        SeedPacketBackend::seed_can_pick_up(self, handle)
            .unwrap_or_else(|error| panic!("PE seed usability invariant failed: {error}"))
    }
}

impl SeedCooldownReadBackend for PeBackend {
    fn seed_cooldown_remaining<'a>(&'a self, seed: Self::SeedHandle<'a>) -> i32 {
        // SAFETY: PE seed handles point into the current world's fixed card array.
        let remaining = unsafe { (*seed.as_ptr()).cold_down };
        remaining.min(i32::MAX as u32) as i32
    }
}

impl ChooserCooldownReadBackend for PeBackend {}

impl BoardSupportBackend for PeBackend {
    fn ensure_board_supported(&self) -> Result<()> {
        Ok(())
    }
}

impl LawnMowerClearBackend for PeBackend {
    fn clear_lawn_mowers(&self) -> Result<()> {
        Ok(())
    }
}

impl SceneEditBackend for PeBackend {
    fn set_scene(&mut self, scene: SceneKind) -> Result<()> {
        let scene = crate::convert::scene::scene_to_pe(scene)?;
        if self.battle_started()? {
            return Err(PeBackendError::OperationRejected(
                "scene switching is only allowed during Opening",
            ));
        }
        let config = self.config()?;
        if config.scene == scene {
            return Ok(());
        }
        match self.game_ui()? {
            GameUi::LevelIntro => {}
            GameUi::Playing if self.current_wave()?.0 == 0 => {}
            _ => {
                return Err(PeBackendError::OperationRejected(
                    "scene switching requires level intro or opening wave 0",
                ));
            }
        }
        self.switch_scene_type(scene)
    }
}

impl GridItemEditBackend for PeBackend {
    fn remove_grid_item<'a>(&'a self, handle: Self::GridItemHandle<'a>) -> Result<()> {
        self.with_current_world(|world| {
            // SAFETY: `handle` is tied to the current backend borrow and PE's
            // GridItemDie atom only marks this pool object disappeared.
            world.griditem_die(unsafe { pe_rs::GridItemRef::from_non_null(handle.ptr) })
        })??;
        Ok(())
    }
}

impl PlantEffectCountdownWriteBackend for PeBackend {
    fn set_plant_effect_countdown<'a>(
        &'a self, handle: Self::PlantHandle<'a>, target_countdown: NonNegativeI32,
    ) -> Result<ObjectEditOutcome> {
        write_pe_raw_field!(handle.as_mut_ptr(), target_countdown.get(), countdown.effect);
        Ok(ObjectEditOutcome::Applied)
    }
}

impl SpawnScheduleBackend for PeBackend {
    fn spawn_wave_count(&self) -> Result<usize> {
        Ok(DEFAULT_SPAWN_WAVES)
    }

    fn set_spawn_type_allowed(&self, kind: ZombieKind, allowed: bool) -> Result<()> {
        let raw = if allowed {
            crate::convert::kind::zombie_to_pe(kind)?.to_raw() as u32
        } else {
            kind.code() as u32
        };
        self.write_spawn_flag(raw, allowed)
    }

    fn set_spawn_slot(&self, wave: usize, slot: SpawnWaveSlot, kind: Option<ZombieKind>) -> Result<()> {
        if wave >= DEFAULT_SPAWN_WAVES {
            return Err(PeBackendError::OperationRejected(
                "spawn list wave or slot is outside PE range",
            ));
        }
        let kind = kind
            .map(crate::convert::kind::zombie_to_pe)
            .transpose()?
            .unwrap_or(pe_rs::ZombieType::None);
        let wave = u32::try_from(wave).map_err(|_err| PeBackendError::NumericOutOfRange("wave"))?;
        let slot = u32::try_from(slot.index()).map_err(|_err| PeBackendError::NumericOutOfRange("slot"))?;
        self.write_spawn_slot(wave, slot, kind)
    }

    fn pick_spawn_list(&self) -> Result<()> {
        self.initialize_spawn_list()
    }
}

impl MaidCheatsBackend for PeBackend {
    fn maid_cheat(&self) -> Result<MaidCheat> {
        Ok(maid_cheat_from_pe(
            self.with_current_world(|world| world.scene().maid_cheat())??,
        ))
    }

    fn set_maid_cheat(&self, cheat: MaidCheat) -> Result<()> {
        self.with_current_world(|world| world.scene().set_maid_cheat(maid_cheat_to_pe(cheat)))??;
        Ok(())
    }
}

#[cfg(test)]
mod card_selection_tests {
    use super::*;
    use rsvz_model::CardSelection;

    fn checked(kind: PlantKind) -> CheckedCardSelection {
        CardSelection::Plant(kind).checked().expect("ordinary card selection")
    }

    #[test]
    fn partial_selection_is_filled_without_replacement_from_battle_rng() {
        let mut owner = crate::runtime::PeWorldOwner::new_reset(crate::PeWorldConfig::default()).expect("PE world");
        owner
            .with_current(|backend| {
                backend
                    .set_random_mode(RandomMode::Seeded(5489))
                    .expect("seed battle RNG");
                backend
                    .select_card(checked(PlantKind::Peashooter))
                    .expect("select peashooter");

                backend.start_battle().expect("fill missing slots");

                let selections = (0..backend.selected_card_count().expect("selected count"))
                    .map(|index| backend.selected_card(index).expect("selected card").selection())
                    .collect::<Vec<_>>();
                assert_eq!(selections.len(), rsvz_model::MAX_SEED_SLOTS);
                assert_eq!(selections[0], CardSelection::Plant(PlantKind::Peashooter));
                for (index, selection) in selections.iter().enumerate() {
                    assert!(!selections[..index].contains(selection));
                }
            })
            .expect("install fixture");
    }

    #[test]
    fn full_selection_does_not_consume_battle_rng() {
        let mut owner = crate::runtime::PeWorldOwner::new_reset(crate::PeWorldConfig::default()).expect("PE world");
        owner
            .with_current(|backend| {
                backend.set_random_mode(RandomMode::Locked(7)).expect("lock RNG");
                for kind in [
                    PlantKind::Peashooter,
                    PlantKind::Sunflower,
                    PlantKind::CherryBomb,
                    PlantKind::WallNut,
                    PlantKind::PotatoMine,
                    PlantKind::SnowPea,
                    PlantKind::Chomper,
                    PlantKind::Repeater,
                    PlantKind::PuffShroom,
                    PlantKind::SunShroom,
                ] {
                    backend.select_card(checked(kind)).expect("select card");
                }

                backend.start_battle().expect("full selection starts");

                assert_eq!(
                    backend.selected_card_count().expect("selected count"),
                    rsvz_model::MAX_SEED_SLOTS
                );
            })
            .expect("install fixture");
    }

    #[test]
    fn locked_partial_selection_is_rejected_before_drawing() {
        let mut owner = crate::runtime::PeWorldOwner::new_reset(crate::PeWorldConfig::default()).expect("PE world");
        owner
            .with_current(|backend| {
                backend
                    .select_card(checked(PlantKind::Peashooter))
                    .expect("select peashooter");
                backend.set_random_mode(RandomMode::Locked(7)).expect("lock RNG");

                assert!(matches!(
                    backend.start_battle(),
                    Err(PeBackendError::Unsupported(
                        "partial card selection with locked randomness"
                    ))
                ));
                assert_eq!(backend.selected_card_count().expect("selected count"), 1);
            })
            .expect("install fixture");
    }

    #[test]
    fn reset_rejects_preserving_a_card_bank_that_pe_destroys() {
        let mut owner = crate::runtime::PeWorldOwner::new_reset(crate::PeWorldConfig::default()).expect("PE world");
        owner
            .with_current(|backend| {
                assert!(matches!(
                    backend.reset_world(WorldResetConfig {
                        card_cooldowns: ResetCardCooldowns::PreserveNative,
                        ..WorldResetConfig::default()
                    }),
                    Err(PeBackendError::Unsupported(
                        "preserving card cooldowns across world reset"
                    ))
                ));
            })
            .expect("install fixture");
    }
}
