use super::*;

impl GridGeometryBackend for PeBackend {
    fn grid_to_pixel_x(&self, grid_x: i32, grid_y: i32) -> Result<i32> {
        Ok(self.with_current_world(|world| world.scene().grid_to_pixel_x(grid_x, grid_y))??)
    }

    fn grid_to_pixel_y(&self, grid_x: i32, grid_y: i32) -> Result<i32> {
        Ok(self.with_current_world(|world| world.scene().grid_to_pixel_y(grid_x, grid_y))??)
    }

    fn pos_y_based_on_row(&self, pos_x: FiniteF32, row: i32) -> Result<f32> {
        Ok(self.with_current_world(|world| world.scene().pos_y_based_on_row(pos_x.get(), row))??)
    }
}

impl GridTerrainBackend for PeBackend {
    fn is_pool_square(&self, grid_x: i32, grid_y: i32) -> Result<bool> {
        validate_grid(
            self,
            Grid::new(grid_y, grid_x).map_err(|_error| PeBackendError::InvalidGrid)?,
        )?;
        Ok(self.with_current_world(|world| world.scene().is_pool_square(grid_x, grid_y))?)
    }

    fn row_can_have_zombies(&self, row: i32) -> Result<bool> {
        Ok(self.with_current_world(|world| world.scene().row_can_have_zombies(row))?)
    }
}

impl SeedPacketBackend for PeBackend {
    fn seed_can_pick_up<'a>(&'a self, seed: Self::SeedHandle<'a>) -> Result<bool> {
        self.with_current_world(|world| {
            // SAFETY: the seed handle is tied to this backend borrow and points
            // into the current world's fixed card bank.
            world.seed_can_pick_up(unsafe { pe_rs::CardRef::from_non_null(seed.ptr) })
        })?
        .map_err(Into::into)
    }

    fn seed_was_planted<'a>(&'a self, seed: Self::SeedHandle<'a>) -> Result<()> {
        self.with_current_world(|world| {
            // SAFETY: the seed handle is tied to this backend borrow and no
            // invalidating operation occurs before the native call.
            world.seed_was_planted(unsafe { pe_rs::CardRef::from_non_null(seed.ptr) })
        })?
        .map_err(Into::into)
    }
}

impl PlantCostBackend for PeBackend {
    fn current_plant_cost(&self, selection: CheckedCardSelection) -> Result<SunAmount> {
        let (packet_type, imitater_type) = crate::convert::kind::card_to_pe(selection);
        let cost = self.with_current_world(|world| world.current_plant_cost(packet_type, imitater_type))??;
        SunAmount::new(cost).map_err(|_error| PeBackendError::NumericOutOfRange("plant cost"))
    }
}

impl SunMoneyBackend for PeBackend {
    fn can_take_sun_money(&self, amount: SunAmount) -> Result<bool> {
        self.with_current_world(|world| world.can_take_sun_money(amount.get()))?
            .map_err(Into::into)
    }

    fn take_sun_money(&self, amount: SunAmount) -> Result<bool> {
        self.with_current_world(|world| world.take_sun_money(amount.get()))?
            .map_err(Into::into)
    }
}

impl PlantPlacementBackend for PeBackend {
    fn can_plant_at(&self, selection: CheckedCardSelection, grid: Grid) -> Result<Plantability> {
        validate_grid(self, grid)?;
        let reason = self.with_current_world(|world| {
            world.can_plant_at(
                grid.col,
                grid.row,
                crate::convert::kind::plant_to_pe(selection.effective_kind()),
            )
        })??;
        Ok(Plantability::from_game_rule_code(reason as i32))
    }
}

impl PlantPoolBackend for PeBackend {
    fn plant_pool_active_count(&self) -> Result<u32> {
        self.with_current_world(|world| crate::access::plant_pool(world).active_count())
    }

    fn plant_pool_capacity(&self) -> Result<u32> {
        self.with_current_world(|world| crate::access::plant_pool(world).capacity())
    }
}

impl PlantCreateBackend for PeBackend {
    fn morph_imitator<'a>(&'a self, placeholder: PePlantHandle<'a>) -> Result<PePlantHandle<'a>> {
        let ptr = self.with_current_world(|world| {
            // SAFETY: the placeholder is borrowed from this world's fixed pool.
            world
                .plant_imitater_morph(unsafe { pe_rs::PlantRef::from_non_null(placeholder.ptr) })
                .map(pe_rs::Borrowed::as_non_null)
        })??;
        Ok(PePlantHandle::new(ptr))
    }
    fn new_plant<'a>(&'a self, selection: CheckedCardSelection, grid: Grid) -> Result<PePlantHandle<'a>> {
        validate_grid(self, grid)?;
        let (packet_type, imitater_type) = crate::convert::kind::card_to_pe(selection);
        let plant = self.with_current_world(|world| -> pe_rs::Result<_> {
            Ok(world
                .new_plant(grid.col, grid.row, packet_type, imitater_type)?
                .map(pe_rs::Borrowed::as_non_null))
        })??;
        plant
            .map(PePlantHandle::new)
            .ok_or(PeBackendError::OperationRejected("new plant"))
    }

    fn add_plant<'a>(&'a self, selection: CheckedCardSelection, grid: Grid) -> Result<PePlantHandle<'a>> {
        validate_grid(self, grid)?;
        let (packet_type, imitater_type) = crate::convert::kind::card_to_pe(selection);
        let plant = self.with_current_world(|world| -> pe_rs::Result<_> {
            Ok(world
                .add_plant(grid.col, grid.row, packet_type, imitater_type)?
                .map(pe_rs::Borrowed::as_non_null))
        })??;
        plant
            .map(PePlantHandle::new)
            .ok_or(PeBackendError::OperationRejected("add plant"))
    }
}

impl PlantRemoveBackend for PeBackend {
    fn remove_plant<'a>(&'a self, handle: Self::PlantHandle<'a>) -> Result<()> {
        self.with_current_world(|world| {
            // SAFETY: the handle is tied to this backend borrow and PE PlantDie
            // queues in-place destruction without relocating the pool.
            world.plant_die(unsafe { pe_rs::PlantRef::from_non_null(handle.ptr) })
        })??;
        Ok(())
    }
}

impl PlantSleepBackend for PeBackend {
    fn set_plant_sleeping<'a>(&'a self, handle: Self::PlantHandle<'a>, asleep: bool) -> Result<()> {
        self.with_current_world(|world| {
            // SAFETY: the current borrowed handle remains valid for this
            // address-stable native state transition.
            world.plant_set_sleep(unsafe { pe_rs::PlantRef::from_non_null(handle.ptr) }, asleep)
        })??;
        Ok(())
    }

    fn set_plant_wake_up_counter<'a>(&'a self, handle: Self::PlantHandle<'a>, counter: i32) -> Result<()> {
        write_pe_raw_field!(handle.as_mut_ptr(), counter, countdown.awake);
        Ok(())
    }
}

impl PlantIdleAnimationBackend for PeBackend {
    fn play_plant_idle_animation<'a>(&'a self, handle: Self::PlantHandle<'a>, fps: PositiveFiniteF32) -> Result<()> {
        self.with_current_world(|world| {
            // SAFETY: the handle is current and the positive finite fps is copied into
            // the existing PE idle-animation atom.
            world.plant_play_idle(unsafe { pe_rs::PlantRef::from_non_null(handle.ptr) }, fps.get())
        })??;
        Ok(())
    }
}

impl PlantStateCountdownWriteBackend for PeBackend {
    fn set_plant_state_countdown<'a>(&'a self, handle: Self::PlantHandle<'a>, countdown: i32) -> Result<()> {
        write_pe_raw_field!(handle.as_mut_ptr(), countdown, countdown.status);
        Ok(())
    }
}

impl PlantStateWriteBackend for PeBackend {
    fn set_plant_state<'a>(&'a self, handle: Self::PlantHandle<'a>, state: i32) -> Result<()> {
        write_pe_raw_field!(handle.as_mut_ptr(), normalize_plant_state(state), status);
        Ok(())
    }
}

impl GridItemCreateBackend for PeBackend {
    fn add_ladder<'a>(&'a self, grid: Grid) -> Result<PeGridItemHandle<'a>> {
        validate_grid(self, grid)?;
        let item = self.with_current_world(|world| -> pe_rs::Result<_> {
            Ok(world.add_ladder(grid.col, grid.row)?.map(pe_rs::Borrowed::as_non_null))
        })??;
        item.map(PeGridItemHandle::new)
            .ok_or(PeBackendError::OperationRejected("add ladder"))
    }

    fn add_crater<'a>(&'a self, grid: Grid) -> Result<PeGridItemHandle<'a>> {
        validate_grid(self, grid)?;
        let item = self.with_current_world(|world| -> pe_rs::Result<_> {
            Ok(world.add_crater(grid.col, grid.row)?.map(pe_rs::Borrowed::as_non_null))
        })??;
        item.map(PeGridItemHandle::new)
            .ok_or(PeBackendError::OperationRejected("add crater"))
    }

    fn add_gravestone<'a>(&'a self, grid: Grid) -> Result<PeGridItemHandle<'a>> {
        validate_grid(self, grid)?;
        let item = self.with_current_world(|world| -> pe_rs::Result<_> {
            Ok(world
                .add_gravestone(grid.col, grid.row)?
                .map(pe_rs::Borrowed::as_non_null))
        })??;
        item.map(PeGridItemHandle::new)
            .ok_or(PeBackendError::OperationRejected("add gravestone"))
    }
}

impl CobFireBackend for PeBackend {
    fn fire_cob<'a>(&'a self, cob: Self::PlantHandle<'a>, target: PixelPos) -> Result<()> {
        if self.plant_kind(cob)? != PlantKind::CobCannon {
            return Err(PeBackendError::OperationRejected("plant is not CobCannon"));
        }
        let fired = self.with_current_world(|world| {
            // SAFETY: the live borrow-bound handle is consumed immediately by
            // the existing PE cob-cannon subsystem action.
            world.fire_cob(unsafe { pe_rs::PlantRef::from_non_null(cob.ptr) }, target.x, target.y)
        })??;
        if fired {
            Ok(())
        } else {
            Err(PeBackendError::OperationRejected("fire cob"))
        }
    }
}

impl ZombieCreateBackend for PeBackend {
    fn add_zombie_in_row<'a>(
        &'a self, kind: ZombieKind, row: i32, from_wave: i32,
    ) -> Result<Option<PeZombieHandle<'a>>> {
        if from_wave < 0 {
            return Err(PeBackendError::Unsupported(
                "negative AddZombieInRow from_wave sentinel",
            ));
        }
        validate_target_row(self, row)?;
        let kind = crate::convert::kind::zombie_to_pe(kind)?;
        let zombie = self.with_current_world(|world| -> pe_rs::Result<_> {
            Ok(world
                .add_zombie_in_row(kind, row, from_wave)?
                .map(pe_rs::Borrowed::as_non_null))
        })??;
        Ok(zombie.map(PeZombieHandle::new))
    }

    fn place_zombie<'a>(&'a self, kind: ZombieKind, grid: Grid) -> Result<PeZombieHandle<'a>> {
        let kind = crate::convert::kind::zombie_to_pe(kind)?;
        let zombie = self.with_current_world(|world| -> pe_rs::Result<_> {
            Ok(world
                .place_zombie(kind, grid.col, grid.row)?
                .map(pe_rs::Borrowed::as_non_null))
        })??;
        zombie
            .map(PeZombieHandle::new)
            .ok_or(PeBackendError::OperationRejected("place zombie"))
    }
}

impl ZombieRemoveBackend for PeBackend {
    fn remove_zombie<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<()> {
        self.with_current_world(|world| {
            // SAFETY: the current handle is borrow-bound and DieNoLoot only
            // marks/frees this pool object through the native atom.
            world.zombie_die_no_loot(unsafe { pe_rs::ZombieRef::from_non_null(handle.ptr) })
        })??;
        Ok(())
    }
}

impl ZombiePositionWriteBackend for PeBackend {
    fn set_zombie_row_and_y<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, row: i32, y: I32RepresentableF32,
    ) -> Result<()> {
        validate_target_row(self, row)?;
        let row = u32::try_from(row).map_err(|_| PeBackendError::InvalidGrid)?;
        write_pe_raw_field!(handle.as_mut_ptr(), row, row);
        write_pe_raw_field!(handle.as_mut_ptr(), y.get(), y);
        write_pe_raw_field!(handle.as_mut_ptr(), y.truncated(), int_y);
        Ok(())
    }
}

#[cfg(test)]
mod plant_state_tests {
    use super::normalize_plant_state;

    #[test]
    fn pe_cob_launch_and_ready_states_map_to_backend_neutral_values() {
        assert_eq!(normalize_plant_state(0x25), 0x26);
        assert_eq!(normalize_plant_state(0x26), 0x25);
        assert_eq!(normalize_plant_state(0x24), 0x24);
        assert_eq!(normalize_plant_state(normalize_plant_state(0x25)), 0x25);
    }
}
