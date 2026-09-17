use std::ptr::NonNull;

use rsvz_backend_api::backend::{
    GameUiBackend, SceneBackend, ZombieBodyHealthWriteBackend, ZombieCreateBackend, ZombieKillBackend,
    ZombiePhaseCountdownWriteBackend, ZombiePositionWriteBackend, ZombieRemoveBackend, ZombieXWriteBackend,
    pool_has_reserved_slot,
};
use rsvz_model::model::{GameUi, Grid, I32RepresentableF32, NonNegativeI32, ObjectEditOutcome, PositiveHp, ZombieKind};

use crate::error::{Pvz1051Error, Result};
use crate::impls::handles::PvzZombieHandle;
use crate::raw::kind::PvzZombieType;
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

impl Pvz1051Backend {
    fn zombie_create_board(&self) -> Result<NonNull<ptrs::Board>> {
        match self.game_ui()? {
            GameUi::LevelIntro | GameUi::Playing => self.board(),
            _ => Err(Pvz1051Error::UnsupportedToolMode(
                "zombie creation requires level-intro or playing UI",
            )),
        }
    }

    pub(crate) fn validate_zombie_row(&self, row: i32) -> Result<()> {
        let field = rsvz_model::FieldInfo::from_scene(self.scene()?);
        if !field.contains_row(row) {
            return Err(Pvz1051Error::InvalidGrid);
        }
        Ok(())
    }

    fn ensure_izombie_place_capacity(&self, board: NonNull<ptrs::Board>) -> Result<()> {
        // SAFETY: `board` is current and the zombie DataArray is embedded at Board+0x90.
        let zombies = unsafe { ptrs::Board::zombies(board.as_ptr()) };
        // SAFETY: the DataArray is an embedded field of the valid Board.
        let zombies = unsafe { NonNull::new_unchecked(zombies) };
        // Board::AddZombieInRow returns null when mSize >= mMaxSize - 1. Z10 immediately
        // dereferences that result, so prove the exact source precondition before calling it.
        // SAFETY: `zombies` is the current embedded DataArray; these are copied header fields.
        let (active, capacity) = unsafe {
            (
                ptrs::DataArray::active_count(zombies.as_ptr()),
                ptrs::DataArray::max_size(zombies.as_ptr()),
            )
        };
        if !pool_has_reserved_slot(active, capacity) {
            return Err(Pvz1051Error::AbiPreconditionFailed(
                "zombie pool lacks the slot reserved by AddZombieInRow",
            ));
        }
        Ok(())
    }

    fn set_zombie_row_and_y_ptr(&self, zombie: NonNull<ptrs::Zombie>, row: i32, y: I32RepresentableF32) -> Result<()> {
        self.validate_zombie_row(row)?;
        // SAFETY: `zombie` is current; row/render-order are copied scalars.
        let (old_row, old_render_order) = unsafe {
            (
                ptrs::Zombie::row(zombie.as_ptr()),
                ptrs::Zombie::render_order(zombie.as_ptr()),
            )
        };
        let render_delta = row
            .checked_sub(old_row)
            .and_then(|diff| diff.checked_mul(10_000))
            .ok_or(Pvz1051Error::AbiPreconditionFailed(
                "zombie render-order delta overflow",
            ))?;
        let render_order = old_render_order
            .checked_add(render_delta)
            .ok_or(Pvz1051Error::AbiPreconditionFailed("zombie render order overflow"))?;
        // SAFETY: all values were validated before the first write. This mirrors Zombie::SetRow's
        // row/render-order writes and also keeps mPosY/mY synchronized as required by the atom.
        unsafe {
            ptrs::Zombie::set_row(zombie.as_ptr(), row);
            ptrs::Zombie::set_render_order(zombie.as_ptr(), render_order);
            ptrs::Zombie::set_pos_y(zombie.as_ptr(), y.get());
            ptrs::Zombie::set_y(zombie.as_ptr(), y.truncated());
        }
        Ok(())
    }
}

impl ZombieCreateBackend for Pvz1051Backend {
    fn add_zombie_in_row<'a>(
        &'a self, kind: ZombieKind, row: i32, from_wave: i32,
    ) -> Result<Option<PvzZombieHandle<'a>>> {
        let board = self.zombie_create_board()?;
        self.validate_zombie_row(row)?;
        // SAFETY: board and row are valid, kind is converted to the exact native enum, and objdump
        // confirms EAX=Board*, EBX=from_wave, stack=(type,row), nullable EAX return.
        let zombie = unsafe {
            crate::raw::abi::board_add_zombie_in_row(board.as_ptr(), PvzZombieType::from(kind).raw(), row, from_wave)
        };
        Ok(NonNull::new(zombie).map(PvzZombieHandle::new))
    }

    fn place_zombie<'a>(&'a self, kind: ZombieKind, grid: Grid) -> Result<PvzZombieHandle<'a>> {
        let board = self.zombie_create_board()?;
        self.validate_grid(grid)?;
        self.ensure_izombie_place_capacity(board)?;
        // SAFETY: capacity and allocator-header invariants were checked above. DataArrayAlloc
        // consumes exactly mFreeListHead, whether it names a recycled slot or the unused tail.
        let zombie = unsafe { ptrs::DataArray::next_allocation(ptrs::Board::zombies(board.as_ptr())) }
            .ok_or(Pvz1051Error::InvariantViolated("next zombie allocation slot"))?;
        // SAFETY: the current Board's Challenge is required by the ECX chain and checked before the
        // wrapper evaluates it.
        let challenge = unsafe { ptrs::Board::challenge(board.as_ptr()) };
        if challenge.is_null() {
            return Err(Pvz1051Error::NullChallenge);
        }
        // SAFETY: Z10's exact void ABI is ECX=Challenge*, EAX=grid_y, stack=(type,grid_x).
        // Capacity is preflighted because the native body dereferences Z09's nullable result.
        unsafe { crate::raw::abi::challenge_izombie_place_zombie(grid.row, grid.col, PvzZombieType::from(kind).raw()) };
        Ok(PvzZombieHandle::new(zombie))
    }
}

impl ZombieRemoveBackend for Pvz1051Backend {
    fn remove_zombie<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<()> {
        // SAFETY: handle was constructed from a live current-frame zombie DataArray slot.
        unsafe { crate::raw::abi::zombie_die_no_loot(handle.ptr.as_ptr()) };
        Ok(())
    }
}

impl ZombieKillBackend for Pvz1051Backend {
    fn kill_zombie<'a>(&'a self, handle: Self::ZombieHandle<'a>) -> Result<()> {
        // SAFETY: handle was constructed from a live current-frame zombie DataArray slot.
        unsafe { crate::raw::abi::zombie_die_with_loot(handle.ptr.as_ptr()) };
        Ok(())
    }
}

impl ZombiePositionWriteBackend for Pvz1051Backend {
    fn set_zombie_row_and_y<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, row: i32, y: I32RepresentableF32,
    ) -> Result<()> {
        self.set_zombie_row_and_y_ptr(handle.ptr, row, y)
    }
}

impl ZombieBodyHealthWriteBackend for Pvz1051Backend {
    fn set_zombie_body_hp<'a>(&'a self, handle: Self::ZombieHandle<'a>, hp: PositiveHp) -> Result<ObjectEditOutcome> {
        // SAFETY: handle was constructed from a live current-frame zombie DataArray slot; this writes
        // only Zombie::mBodyHealth.
        unsafe { ptrs::Zombie::set_body_health(handle.ptr.as_ptr(), hp.get()) };
        Ok(ObjectEditOutcome::Applied)
    }
}

impl ZombieXWriteBackend for Pvz1051Backend {
    fn set_zombie_x<'a>(&'a self, handle: Self::ZombieHandle<'a>, x: I32RepresentableF32) -> Result<ObjectEditOutcome> {
        // SAFETY: handle was constructed from a live current-frame zombie DataArray slot; this writes
        // the paired horizontal-coordinate scalars used by native update and contact/damage queries.
        unsafe {
            ptrs::Zombie::set_pos_x(handle.ptr.as_ptr(), x.get());
            ptrs::Zombie::set_x(handle.ptr.as_ptr(), x.truncated());
        }
        Ok(ObjectEditOutcome::Applied)
    }
}

impl ZombiePhaseCountdownWriteBackend for Pvz1051Backend {
    fn set_zombie_phase_countdown<'a>(
        &'a self, handle: Self::ZombieHandle<'a>, countdown: NonNegativeI32,
    ) -> Result<ObjectEditOutcome> {
        // SAFETY: the handle is a live current-frame zombie and 0x68 is the
        // native mPhaseCounter field read by ZombieRawFactsBackend.
        unsafe { ptrs::Zombie::set_phase_counter(handle.ptr.as_ptr(), countdown.get()) };
        Ok(ObjectEditOutcome::Applied)
    }
}
