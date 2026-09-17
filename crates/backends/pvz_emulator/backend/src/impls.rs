use std::ptr::NonNull;

use rsvz_backend_api::backend::{
    BackendIdentityBackend, BattleEntryBackend, BattleStatusBackend, BoardReadinessBackend, BoardStateBackend,
    BoardSupportBackend, CardAppendSelectionBackend, CardSelectionReadBackend, ChooserCooldownReadBackend,
    ClockBackend, CobFireBackend, CobImpactDelayBackend, CobRuleEditBackend, CommonZombieDanceBackend,
    CurrentWaveBackend, CursorQueryBackend, DancerClockWriteBackend, DropRuleEditBackend, FastForwardBackend,
    GameSpeedHintBackend, GameUiBackend, GridGeometryBackend, GridItemCreateBackend, GridItemEditBackend,
    GridItemReadBackend, GridItemStateBackend, GridTerrainBackend, ImitatorMorphBackend, LawnMowerClearBackend,
    MaidCheatsBackend, PlantContactBackend, PlantCostBackend, PlantCreateBackend, PlantDamageRuleEditBackend,
    PlantEffectCountdownWriteBackend, PlantEffectRuleEditBackend, PlantHealthWriteBackend, PlantIdleAnimationBackend,
    PlantPlacementBackend, PlantPoolBackend, PlantReadBackend, PlantRemoveBackend, PlantSleepBackend,
    PlantStateBackend, PlantStateCountdownWriteBackend, PlantStateWriteBackend, PlantVisualStateBackend,
    PlantingRuleEditBackend, ProfileReadonlyBackend, ProjectileReadBackend, ProjectileRuleEditBackend,
    RandomControlBackend, SceneBackend, SceneEditBackend, SeedBankReadBackend, SeedChooserFastForwardBackend,
    SeedCooldownReadBackend, SeedPacketBackend, SeedRuleEditBackend, SpawnScheduleBackend, SunCostRuleEditBackend,
    SunMoneyBackend, SunProductionModeBackend, SunQueryBackend, SunWriteBackend, WaveHealthBackend,
    WaveRefreshControlBackend, WaveTimingBackend, WorldResetBackend, ZombieBodyHealthWriteBackend,
    ZombieContactBackend, ZombieCreateBackend, ZombiePhaseCountdownWriteBackend, ZombiePositionWriteBackend,
    ZombieRawFactsBackend, ZombieReadBackend, ZombieRemoveBackend, ZombieRuleEditBackend, ZombieStateBackend,
    ZombieVerticalPositionBackend, ZombieXWriteBackend,
};
use rsvz_model::model::{
    BattleConfig, BattleStatus, CheckedCardSelection, ContactRect, DEFAULT_SPAWN_WAVES, DamageRangeFlags,
    FastForwardOptions, FastForwardStopReason, FiniteF32, GameUi, Grid, GridItemId, GridItemKind, I32RepresentableF32,
    KernelPultProjectileRule, MaidCheat, NonNegativeI32, ObjectEditOutcome, PixelPos, PlantDamageRule, PlantId,
    PlantKind, PlantWeapon, Plantability, PositiveFiniteF32, PositiveHp, ProjectileId, RandomMode, RandomStreamKind,
    RefreshDance, ResetCardCooldowns, SceneKind, SeedChooserFastForwardOptions, SeedSlot, SpawnWaveSlot, SunAmount,
    SunProductionMode, Wave, WorldResetConfig, ZombieId, ZombieKind, ZombiePhase,
};

use crate::handles::{PeGridItemHandle, PePlantHandle, PeZombieHandle};
use crate::{PeBackend, PeBackendError, Result};

fn normalize_plant_state(state: i32) -> i32 {
    match state {
        // PE orders launch/armed as 0x25/0x26; PvZ 1051 and the backend-neutral
        // cob logic use firing/ready as 0x26/0x25.
        0x25 => 0x26,
        0x26 => 0x25,
        state => state,
    }
}

macro_rules! read_pe_raw_field {
    ($ptr:expr, $($field:tt)+) => {{
        let ptr = $ptr;
        // SAFETY: Call sites pass PE current-frame object or card-slot pointers.
        // Field reads copy scalar values immediately and never build safe references
        // into PE world-owned storage.
        unsafe { ::std::ptr::addr_of!((*ptr).$($field)+).read() }
    }};
}

macro_rules! write_pe_raw_field {
    ($ptr:expr, $value:expr, $($field:tt)+) => {{
        let ptr = $ptr;
        // SAFETY: Call sites pass a current-world, borrow-bound pool object and
        // write one address-stable scalar field without retaining a reference.
        unsafe { ::std::ptr::addr_of_mut!((*ptr).$($field)+).write($value) }
    }};
}

fn pe_plant_kind_from_handle(handle: crate::handles::PePlantHandle<'_>) -> Result<PlantKind> {
    pe_raw_plant_kind(read_pe_raw_field!(handle.as_ptr(), type_))
}

fn pe_grid_item_kind_from_handle(handle: crate::handles::PeGridItemHandle<'_>) -> Result<GridItemKind> {
    pe_raw_grid_item_kind(read_pe_raw_field!(handle.as_ptr(), type_))
}

fn pe_zombie_kind_from_handle(handle: crate::handles::PeZombieHandle<'_>) -> Result<ZombieKind> {
    pe_raw_zombie_kind(read_pe_raw_field!(handle.as_ptr(), type_))
}

fn pe_contact_rect(raw: pe_rs::raw::pe_rs_rect) -> Result<ContactRect> {
    ContactRect::checked_from_pos_size(raw.x, raw.y, raw.width, raw.height)
        .ok_or(PeBackendError::OperationRejected("invalid PE contact rectangle"))
}

const fn pe_plant_weapon(weapon: PlantWeapon) -> pe_rs::PlantWeapon {
    match weapon {
        PlantWeapon::Primary => pe_rs::PlantWeapon::Primary,
        PlantWeapon::Secondary => pe_rs::PlantWeapon::Secondary,
    }
}

fn pe_u32_i32(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

const fn canonical_relation_id(value: i32) -> u32 {
    if value == -1 { 0 } else { value as u32 }
}

fn pe_card_selection_from_raw(ptr: *const pe_rs::raw::pe_rs_card) -> Result<Option<CheckedCardSelection>> {
    let plant_type = pe_raw_plant_type(read_pe_raw_field!(ptr, type_))?;
    if matches!(plant_type, pe_rs::PlantType::None) {
        return Ok(None);
    }
    let imitater_type = pe_raw_plant_type(read_pe_raw_field!(ptr, imitater_type))?;
    crate::convert::kind::card_from_pe(plant_type, imitater_type).map(Some)
}

fn validate_grid(backend: &PeBackend, grid: Grid) -> Result<()> {
    let field = rsvz_model::FieldInfo::from_scene(backend.scene()?);
    if !field.contains_grid(grid) {
        return Err(PeBackendError::InvalidGrid);
    }
    Ok(())
}

impl PeBackend {
    fn plant_handle_from_raw_item<'a>(
        &'a self, ptr: NonNull<pe_rs::raw::pe_rs_plant>,
    ) -> crate::handles::PePlantHandle<'a> {
        crate::handles::PePlantHandle::new(ptr)
    }

    fn zombie_handle_from_raw_item<'a>(
        &'a self, ptr: NonNull<pe_rs::raw::pe_rs_zombie>,
    ) -> crate::handles::PeZombieHandle<'a> {
        crate::handles::PeZombieHandle::new(ptr)
    }

    pub(crate) fn pe_grid_item_is_live<'a>(&self, handle: crate::handles::PeGridItemHandle<'a>) -> bool {
        !read_pe_raw_field!(handle.as_ptr(), is_disappeared)
    }

    fn plant_id_from_handle<'a>(&self, handle: crate::handles::PePlantHandle<'a>) -> PlantId {
        // SAFETY: the backend handle is tied to this current backend borrow.
        let plant = unsafe { pe_rs::PlantRef::from_non_null(handle.ptr) };
        let id = self
            .with_current_world(|world| crate::access::plant_pool(world).get_id(plant))
            .expect("PE world must remain installed while a plant handle is live")
            .expect("PE plant handle must belong to its current pool");
        PlantId::from_raw(id)
    }

    fn grid_item_id_from_handle<'a>(&self, handle: crate::handles::PeGridItemHandle<'a>) -> GridItemId {
        // SAFETY: the backend handle is tied to this current backend borrow.
        let item = unsafe { pe_rs::GridItemRef::from_non_null(handle.ptr) };
        let id = self
            .with_current_world(|world| crate::access::grid_item_pool(world).get_id(item))
            .expect("PE world must remain installed while a grid-item handle is live")
            .expect("PE grid-item handle must belong to its current pool");
        GridItemId::from_raw(id)
    }

    fn zombie_id_from_handle<'a>(&self, handle: crate::handles::PeZombieHandle<'a>) -> ZombieId {
        // SAFETY: the backend handle is tied to this current backend borrow.
        let zombie = unsafe { pe_rs::ZombieRef::from_non_null(handle.ptr) };
        let id = self
            .with_current_world(|world| crate::access::zombie_pool(world).get_id(zombie))
            .expect("PE world must remain installed while a zombie handle is live")
            .expect("PE zombie handle must belong to its current pool");
        ZombieId::from_raw(id)
    }

    fn visit_plant_map_grid<'a, F>(
        &'a self, grid: Grid, visited: &mut [*mut pe_rs::raw::pe_rs_plant], visited_len: &mut usize, visit: &mut F,
    ) -> Result<()>
    where
        F: FnMut(crate::handles::PePlantHandle<'a>) -> Result<()>,
    {
        let status = self.with_current_world(|world| {
            Ok::<_, pe_rs::Error>(world.scene().grid_plant_status_at(grid.col, grid.row)?.as_ptr())
        })??;
        // SAFETY: `status` is the current world's grid-map entry. Each field is
        // copied independently; no four-pointer snapshot is constructed.
        let pumpkin = unsafe { std::ptr::addr_of!((*status).pumpkin).read() };
        self.visit_plant_map_ptr(pumpkin, visited, visited_len, visit)?;
        // SAFETY: same current grid-map entry and scalar pointer read as above.
        let base = unsafe { std::ptr::addr_of!((*status).base).read() };
        self.visit_plant_map_ptr(base, visited, visited_len, visit)?;
        // SAFETY: same current grid-map entry and scalar pointer read as above.
        let content = unsafe { std::ptr::addr_of!((*status).content).read() };
        self.visit_plant_map_ptr(content, visited, visited_len, visit)?;
        // SAFETY: same current grid-map entry and scalar pointer read as above.
        let coffee = unsafe { std::ptr::addr_of!((*status).coffee_bean).read() };
        self.visit_plant_map_ptr(coffee, visited, visited_len, visit)?;
        Ok(())
    }

    fn visit_plant_map_ptr<'a, F>(
        &'a self, ptr: *mut pe_rs::raw::pe_rs_plant, visited: &mut [*mut pe_rs::raw::pe_rs_plant],
        visited_len: &mut usize, visit: &mut F,
    ) -> Result<()>
    where
        F: FnMut(crate::handles::PePlantHandle<'a>) -> Result<()>,
    {
        let Some(ptr) = NonNull::new(ptr) else {
            return Ok(());
        };
        let raw = ptr.as_ptr();
        let handle = crate::handles::PePlantHandle::new(ptr);
        if !self.pe_plant_is_live(handle) || visited[..*visited_len].contains(&raw) {
            return Ok(());
        }
        if *visited_len < visited.len() {
            visited[*visited_len] = raw;
            *visited_len += 1;
        }
        visit(handle)
    }

    pub(crate) fn plant_handle_from_pool<'a>(
        &'a self, pool: pe_rs::PlantPool<'_>, id: PlantId,
    ) -> Option<crate::handles::PePlantHandle<'a>> {
        let ptr = pool.try_to_get(id.raw()).map(pe_rs::Borrowed::as_non_null);
        ptr.map(|ptr| self.plant_handle_from_raw_item(ptr))
    }

    pub(crate) fn zombie_handle_from_pool<'a>(
        &'a self, pool: pe_rs::ZombiePool<'_>, id: ZombieId,
    ) -> Option<crate::handles::PeZombieHandle<'a>> {
        let ptr = pool.try_to_get(id.raw()).map(pe_rs::Borrowed::as_non_null);
        ptr.map(|ptr| self.zombie_handle_from_raw_item(ptr))
    }

    pub(crate) fn pe_plant_is_live<'a>(&self, handle: crate::handles::PePlantHandle<'a>) -> bool {
        !read_pe_raw_field!(handle.as_ptr(), is_dead)
            && !read_pe_raw_field!(handle.as_ptr(), is_smashed)
            && read_pe_raw_field!(handle.as_ptr(), edible)
                != pe_rs::raw::pvz_emulator_object_plant_edible_status_invisible_and_not_edible
            && !(read_pe_raw_field!(handle.as_ptr(), type_) == pe_rs::raw::pvz_emulator_object_plant_type_squash
                && read_pe_raw_field!(handle.as_ptr(), status)
                    == pe_rs::raw::pvz_emulator_object_plant_status_squash_crushed)
    }

    pub(crate) fn pe_zombie_is_live<'a>(&self, handle: crate::handles::PeZombieHandle<'a>) -> bool {
        let status = read_pe_raw_field!(handle.as_ptr(), status);
        !read_pe_raw_field!(handle.as_ptr(), is_dead)
            && status != pe_rs::raw::pvz_emulator_object_zombie_status_dying
            && status != pe_rs::raw::pvz_emulator_object_zombie_status_dying_from_instant_kill
            && status != pe_rs::raw::pvz_emulator_object_zombie_status_dying_from_lawnmower
    }
}

fn pe_raw_plant_type(value: pe_rs::raw::pvz_emulator_object_plant_type) -> Result<pe_rs::PlantType> {
    pe_rs::PlantType::from_raw(value).ok_or(PeBackendError::NumericOutOfRange("plant.type"))
}

fn pe_raw_zombie_type(value: pe_rs::raw::pvz_emulator_object_zombie_type) -> Result<pe_rs::ZombieType> {
    pe_rs::ZombieType::from_raw(value).ok_or(PeBackendError::NumericOutOfRange("zombie.type"))
}

fn pe_raw_grid_item_type(value: pe_rs::raw::pvz_emulator_object_griditem_type) -> Result<pe_rs::GridItemType> {
    pe_rs::GridItemType::from_raw(value).ok_or(PeBackendError::NumericOutOfRange("griditem.type"))
}

fn pe_raw_plant_kind(value: pe_rs::raw::pvz_emulator_object_plant_type) -> Result<PlantKind> {
    crate::convert::kind::plant_from_pe(pe_raw_plant_type(value)?)
}

fn pe_raw_grid_item_kind(value: pe_rs::raw::pvz_emulator_object_griditem_type) -> Result<GridItemKind> {
    crate::convert::kind::grid_item_from_pe(pe_raw_grid_item_type(value)?)
}

fn pe_raw_zombie_kind(value: pe_rs::raw::pvz_emulator_object_zombie_type) -> Result<ZombieKind> {
    crate::convert::kind::zombie_from_pe(pe_raw_zombie_type(value)?)
}

fn pe_zombie_phase_from_raw(raw: i32) -> Result<ZombiePhase> {
    ZombiePhase::try_from_code(raw).map_err(|_error| PeBackendError::InvalidKind {
        kind: "zombie phase",
        raw,
    })
}

#[cfg(test)]
mod zombie_phase_tests {
    use super::pe_zombie_phase_from_raw;
    use rsvz_model::model::ZombiePhase;

    #[test]
    fn pe_phase_values_map_at_the_backend_boundary() {
        for raw in 0..=95 {
            assert!(pe_zombie_phase_from_raw(raw).is_ok(), "missing PE phase {raw}");
        }
        assert_eq!(
            pe_zombie_phase_from_raw(70).expect("smashing"),
            ZombiePhase::GargantuarSmashing
        );
        assert_eq!(pe_zombie_phase_from_raw(91).expect("yeti"), ZombiePhase::YetiRunning);
        assert!(pe_zombie_phase_from_raw(96).is_err());
    }
}

fn validate_target_row(backend: &PeBackend, row: i32) -> Result<()> {
    let field = rsvz_model::FieldInfo::from_scene(backend.scene()?);
    if !field.contains_row(row) {
        return Err(PeBackendError::InvalidGrid);
    }
    Ok(())
}

fn maid_cheat_to_pe(cheat: MaidCheat) -> pe_rs::MaidCheat {
    match cheat {
        MaidCheat::Stop => pe_rs::MaidCheat::Stop,
        MaidCheat::CallPartner => pe_rs::MaidCheat::CallPartner,
        MaidCheat::Dancing => pe_rs::MaidCheat::Dancing,
        MaidCheat::Move => pe_rs::MaidCheat::Move,
    }
}

fn maid_cheat_from_pe(cheat: pe_rs::MaidCheat) -> MaidCheat {
    match cheat {
        pe_rs::MaidCheat::Stop => MaidCheat::Stop,
        pe_rs::MaidCheat::CallPartner => MaidCheat::CallPartner,
        pe_rs::MaidCheat::Dancing => MaidCheat::Dancing,
        pe_rs::MaidCheat::Move => MaidCheat::Move,
    }
}

mod gameplay;
mod item;
mod modifier;
mod opening;
mod plant;
mod state;
mod zombie;
