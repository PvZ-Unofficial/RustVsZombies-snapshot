use rsvz_backend_api::backend::{
    AdvancedPauseBackend, AutoCollectBackend, BackendIdentityBackend, BattleEntryBackend, BattleStatusBackend,
    BoardReadinessBackend, BoardStateBackend, BoardSupportBackend, CardAppendSelectionBackend,
    CardSelectionReadBackend, ChooserCooldownReadBackend, ClockBackend, CobFireBackend, CobRuleEditBackend,
    CurrentWaveBackend, CursorQueryBackend, DancerClockWriteBackend, DropRuleEditBackend, FastForwardBackend,
    GameSpeedHintBackend, GameUiBackend, GridGeometryBackend, GridItemCreateBackend, GridItemEditBackend,
    GridItemReadBackend, GridItemStateBackend, GridTerrainBackend, ImitatorMorphBackend, ItemClickCollectBackend,
    ItemReadBackend, KeyboardStateBackend, LawnMowerClearBackend, MaidCheatsBackend, MainMenuBackend,
    PlantContactBackend, PlantCostBackend, PlantCreateBackend, PlantDamageRuleEditBackend,
    PlantEffectCountdownWriteBackend, PlantEffectRuleEditBackend, PlantHealthWriteBackend, PlantIdleAnimationBackend,
    PlantPlacementBackend, PlantPoolBackend, PlantReadBackend, PlantRemoveBackend, PlantSleepBackend,
    PlantStateBackend, PlantStateCountdownWriteBackend, PlantStateWriteBackend, PlantVisualStateBackend,
    PlantingRuleEditBackend, ProfileReadonlyBackend, ProjectileReadBackend, ProjectileRuleEditBackend,
    RandomControlBackend, SceneBackend, SceneEditBackend, SeedBankReadBackend, SeedChooserFastForwardBackend,
    SeedCooldownReadBackend, SeedPacketBackend, SeedRuleEditBackend, SpawnScheduleBackend, SunCostRuleEditBackend,
    SunMoneyBackend, SunProductionModeBackend, SunQueryBackend, SunWriteBackend, VisibilityEditBackend,
    WaveHealthBackend, WaveRefreshControlBackend, WaveTimingBackend, WorldResetBackend, ZombieBodyHealthWriteBackend,
    ZombieContactBackend, ZombieCreateBackend, ZombieKillBackend, ZombiePhaseCountdownWriteBackend,
    ZombiePositionWriteBackend, ZombieRawFactsBackend, ZombieReadBackend, ZombieRemoveBackend, ZombieRuleEditBackend,
    ZombieStateBackend, ZombieVerticalPositionBackend, ZombieXWriteBackend, pool_has_reserved_slot,
};
use rsvz_model::model::{
    AdvancedPauseOptions, BattleConfig, BattleStatus, CardSelection, CheckedCardSelection, ContactRect,
    DEFAULT_SPAWN_WAVES, DamageRangeFlags, FastForwardOptions, FastForwardPerformance, FastForwardStopReason,
    FiniteF32, GameUi, Grid, GridItemId, GridItemKind, I32RepresentableF32, ItemId, ItemKind, KernelPultProjectileRule,
    KeyCode, MaidCheat, NonNegativeI32, ObjectEditOutcome, PixelPos, PlantDamageRule, PlantId, PlantKind, PlantWeapon,
    Plantability, PositiveFiniteF32, PositiveHp, ProjectileId, RandomMode, RandomStreamKind, ResetCardCooldowns,
    SceneKind, SeedChooserFastForwardOptions, SeedSlot, SpawnWaveSlot, SunAmount, SunProductionMode, Wave,
    WorldResetConfig, ZombieId, ZombieKind, ZombiePhase,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

use crate::handles::{
    PortableGridItemHandle, PortableGridItemIter, PortableItemHandle, PortableItemIter, PortablePlantHandle,
    PortablePlantIter, PortableProjectileHandle, PortableProjectileIter, PortableSeedHandle, PortableSeedIter,
    PortableZombieHandle, PortableZombieIter,
};
use crate::{PortableBackend, PortableBackendError, Result};

macro_rules! read_raw {
    ($handle:expr, $($field:tt)+) => {{
        // SAFETY: Portable handles are created only from occupied native slots
        // under the current backend borrow; this copies one bindgen-verified field.
        unsafe { std::ptr::addr_of!((*$handle.as_ptr()).$($field)+).read() }
    }};
}

macro_rules! write_raw {
    ($handle:expr, $value:expr, $($field:tt)+) => {{
        // SAFETY: Portable handles are created only from occupied native slots
        // under the current backend borrow; this writes one bindgen-verified field.
        unsafe { std::ptr::addr_of_mut!((*$handle.as_mut_ptr()).$($field)+).write($value) }
    }};
}

mod grid_item;
mod item;
mod plant;
mod projectile;
mod seed;
mod zombie;

fn invalid_kind(kind: &'static str, value: i32) -> PortableBackendError {
    PortableBackendError::InvalidKind { kind, value }
}

fn card_selection(packet: i32, imitater: i32) -> Result<CheckedCardSelection> {
    let packet = PlantKind::try_from_code(packet).map_err(|_| invalid_kind("plant", packet))?;
    let target = if packet == PlantKind::Imitator {
        Some(PlantKind::try_from_code(imitater).map_err(|_| invalid_kind("plant", imitater))?)
    } else {
        None
    };
    CardSelection::from_packet_parts(packet, target)
        .map_err(|_| PortableBackendError::OperationRejected("invalid native seed packet selection"))
}

fn card_parts(selection: CheckedCardSelection) -> (i32, i32) {
    (
        selection.packet_kind().code(),
        selection.imitator_target().map_or(-1, PlantKind::code),
    )
}

fn validate_grid(backend: &PortableBackend, grid: Grid) -> Result<()> {
    if rsvz_model::FieldInfo::from_scene(backend.scene()?).contains_grid(grid) {
        Ok(())
    } else {
        Err(PortableBackendError::OperationRejected(
            "grid is outside the current field",
        ))
    }
}

fn validate_row(backend: &PortableBackend, row: i32) -> Result<()> {
    if rsvz_model::FieldInfo::from_scene(backend.scene()?).contains_row(row) {
        Ok(())
    } else {
        Err(PortableBackendError::OperationRejected(
            "row is outside the current field",
        ))
    }
}

impl PortableBackend {
    fn plant_id_from_handle(&self, handle: PortablePlantHandle<'_>) -> PlantId {
        // SAFETY: this handle borrows the current backend and remains occupied
        // until that borrow ends, including after same-frame death.
        PlantId::from_raw(unsafe { pvzp_rs::raw::pvzp_rs_plant_id(handle.as_ptr()) })
    }

    fn zombie_id_from_handle(&self, handle: PortableZombieHandle<'_>) -> ZombieId {
        // SAFETY: this handle borrows the current backend and remains occupied
        // until that borrow ends, including after same-frame death.
        ZombieId::from_raw(unsafe { pvzp_rs::raw::pvzp_rs_zombie_id(handle.as_ptr()) })
    }

    fn grid_item_id_from_handle(&self, handle: PortableGridItemHandle<'_>) -> GridItemId {
        // SAFETY: this handle borrows the current backend and remains occupied
        // until that borrow ends, including after same-frame death.
        GridItemId::from_raw(unsafe { pvzp_rs::raw::pvzp_rs_grid_item_id(handle.as_ptr()) })
    }

    fn item_id_from_handle(&self, handle: PortableItemHandle<'_>) -> ItemId {
        // SAFETY: this handle borrows the current backend and remains occupied
        // until that borrow ends, including after same-frame death.
        ItemId::from_raw(unsafe { pvzp_rs::raw::pvzp_rs_item_id(handle.as_ptr()) })
    }

    fn projectile_id_from_handle(&self, handle: PortableProjectileHandle<'_>) -> ProjectileId {
        // SAFETY: this handle borrows the current backend and remains occupied
        // until that borrow ends, including after same-frame death.
        ProjectileId::from_raw(unsafe { pvzp_rs::raw::pvzp_rs_projectile_id(handle.as_ptr()) })
    }
}

fn contact_rect(raw: pvzp_rs::raw::pvzp_rs_rect_i32) -> Result<ContactRect> {
    ContactRect::checked_from_pos_size(raw.x, raw.y, raw.width, raw.height).ok_or(
        PortableBackendError::OperationRejected("native contact rectangle overflow"),
    )
}

impl BackendIdentityBackend for PortableBackend {
    fn backend_name(&self) -> &'static str {
        "pvz-portable"
    }

    fn backend_version(&self) -> &'static str {
        "1.0.0.1051-source"
    }
}

impl GameUiBackend for PortableBackend {
    fn game_ui(&self) -> Result<GameUi> {
        let raw = pvzp_rs::game_ui()?;
        GameUi::try_from_code(raw).map_err(|_| invalid_kind("game-ui", raw))
    }

    fn game_ui_unavailable(&self, error: &Self::Error) -> bool {
        matches!(error, PortableBackendError::Native(pvzp_rs::Error::NotFound { .. }))
    }
}

impl BoardReadinessBackend for PortableBackend {
    fn level_intro_board_ready(&self) -> Result<bool> {
        Ok(self.game_ui()? == GameUi::LevelIntro && self.world().is_ok())
    }

    fn playing_board_ready(&self) -> Result<bool> {
        Ok(self.game_ui()? == GameUi::Playing && self.world().is_ok())
    }
}

impl SceneBackend for PortableBackend {
    fn scene(&self) -> Result<SceneKind> {
        let raw = self.world()?.scene();
        SceneKind::try_from_code(raw).map_err(|_| invalid_kind("scene", raw))
    }
}

impl ClockBackend for PortableBackend {
    fn clock(&self) -> Result<i32> {
        i32::try_from(self.world()?.main_counter())
            .map_err(|_| PortableBackendError::NumericOutOfRange("board main counter"))
    }
}

impl CursorQueryBackend for PortableBackend {
    fn cursor_type(&self) -> Result<i32> {
        self.world()?.cursor_type().map_err(Into::into)
    }
}

impl CurrentWaveBackend for PortableBackend {
    fn current_wave(&self) -> Result<Wave> {
        Ok(Wave(self.world()?.current_wave()))
    }
}

impl SunQueryBackend for PortableBackend {
    fn sun(&self) -> Result<u32> {
        Ok(self.world()?.sun().max(0) as u32)
    }
}

impl GridGeometryBackend for PortableBackend {
    fn grid_to_pixel_x(&self, grid_x: i32, grid_y: i32) -> Result<i32> {
        self.world()?.grid_to_pixel_x(grid_x, grid_y).map_err(Into::into)
    }

    fn grid_to_pixel_y(&self, grid_x: i32, grid_y: i32) -> Result<i32> {
        self.world()?.grid_to_pixel_y(grid_x, grid_y).map_err(Into::into)
    }

    fn pos_y_based_on_row(&self, pos_x: FiniteF32, row: i32) -> Result<f32> {
        self.world()?.pos_y_based_on_row(pos_x.get(), row).map_err(Into::into)
    }
}

impl GridTerrainBackend for PortableBackend {
    fn is_pool_square(&self, grid_x: i32, grid_y: i32) -> Result<bool> {
        self.world()?.is_pool_square(grid_x, grid_y).map_err(Into::into)
    }

    fn row_can_have_zombies(&self, row: i32) -> Result<bool> {
        self.world()?.row_can_have_zombies(row).map_err(Into::into)
    }
}

impl BattleStatusBackend for PortableBackend {
    fn battle_status(&self) -> Result<BattleStatus> {
        match self.game_ui()? {
            GameUi::ZombiesWon => Ok(BattleStatus::Lost),
            GameUi::Award | GameUi::Credit => Ok(BattleStatus::Ended),
            GameUi::Playing if self.world()?.level_complete() => Ok(BattleStatus::ObjectiveReached),
            GameUi::Playing => Ok(BattleStatus::Running),
            GameUi::Loading | GameUi::Menu | GameUi::LevelIntro | GameUi::Challenge => Ok(BattleStatus::Unknown),
        }
    }
}

impl SunMoneyBackend for PortableBackend {
    fn can_take_sun_money(&self, amount: SunAmount) -> Result<bool> {
        self.world()?.can_take_sun(amount.get()).map_err(Into::into)
    }

    fn take_sun_money(&self, amount: SunAmount) -> Result<bool> {
        self.world()?.take_sun(amount.get()).map_err(Into::into)
    }
}

impl BoardSupportBackend for PortableBackend {
    fn ensure_board_supported(&self) -> Result<()> {
        self.world().map(|_| ())
    }
}

impl LawnMowerClearBackend for PortableBackend {
    fn clear_lawn_mowers(&self) -> Result<()> {
        self.world()?.clear_lawn_mowers().map_err(Into::into)
    }
}

impl BattleEntryBackend for PortableBackend {
    fn enter_game(&mut self, config: BattleConfig) -> Result<()> {
        let BattleConfig::Endless(config) = config;
        if !matches!(
            config.scene,
            SceneKind::Day | SceneKind::Night | SceneKind::Pool | SceneKind::Fog | SceneKind::Roof
        ) {
            return Err(PortableBackendError::Unsupported("survival endless scene"));
        }
        pvzp_rs::enter_endless(config.scene as i32).map_err(Into::into)
    }

    fn start_battle(&mut self) -> Result<()> {
        self.ensure_seed_chooser_ready_for_auto_selection()?;
        pvzp_rs::start_battle().map_err(Into::into)
    }
}

impl MainMenuBackend for PortableBackend {
    fn back_to_main_menu(&mut self) -> Result<()> {
        pvzp_rs::back_to_main_menu().map_err(Into::into)
    }
}

impl FastForwardBackend for PortableBackend {
    fn start_fast_forward(&self, options: FastForwardOptions) -> Result<()> {
        let performance = match options.performance {
            FastForwardPerformance::Off => 0,
            FastForwardPerformance::Basic => 1,
            FastForwardPerformance::Aggressive => 2,
        };
        pvzp_rs::set_fast_forward(true, performance, options.suppress_window_update).map_err(Into::into)
    }

    fn stop_fast_forward(&self, _reason: FastForwardStopReason) -> Result<()> {
        pvzp_rs::set_fast_forward(false, 0, false).map_err(Into::into)
    }

    fn fast_forward_active(&self) -> bool {
        pvzp_rs::fast_forward_active()
    }
}

impl AdvancedPauseBackend for PortableBackend {
    fn set_advanced_pause_with_options(&self, enabled: bool, options: AdvancedPauseOptions) -> Result<()> {
        let color = options.mask_color;
        let rgba = u32::from(color.red) << 24
            | u32::from(color.green) << 16
            | u32::from(color.blue) << 8
            | u32::from(color.alpha);
        pvzp_rs::set_advanced_pause(
            enabled,
            options.draw_mask,
            rgba,
            options.play_sound,
            options.refresh_cursor_preview,
        )
        .map_err(Into::into)
    }

    fn advanced_pause_active(&self) -> bool {
        pvzp_rs::advanced_pause_active()
    }
}

impl KeyboardStateBackend for PortableBackend {
    fn key_is_down(&self, key: KeyCode) -> bool {
        // SAFETY: this reads process-independent copied keyboard state for one virtual-key code.
        unsafe { GetAsyncKeyState(i32::from(key.raw_virtual_key())) < 0 }
    }

    fn input_focused(&self) -> Result<bool> {
        pvzp_rs::input_focused().map_err(Into::into)
    }
}

impl RandomControlBackend for PortableBackend {
    fn set_random_mode(&mut self, mode: RandomMode) -> Result<()> {
        let (mode, value) = match mode {
            RandomMode::Seeded(seed) => (0, seed),
            RandomMode::Locked(value) => (1, value),
        };
        pvzp_rs::set_random_mode(mode, value).map_err(Into::into)
    }

    fn set_wave_spawn_random_seed(&mut self, base_seed: Option<u32>) -> Result<()> {
        pvzp_rs::set_wave_spawn_random_seed(base_seed).map_err(Into::into)
    }

    fn random_seed(&self, stream: RandomStreamKind) -> Result<u32> {
        let stream = match stream {
            RandomStreamKind::Battle => 0,
            RandomStreamKind::Level => 1,
        };
        pvzp_rs::random_seed(stream).map_err(Into::into)
    }
    fn random_locked(&self, stream: RandomStreamKind) -> bool {
        pvzp_rs::random_locked(matches!(stream, RandomStreamKind::Level))
    }
    fn random_fixed(&self, stream: RandomStreamKind) -> u32 {
        pvzp_rs::random_fixed(matches!(stream, RandomStreamKind::Level))
    }
}

impl SunProductionModeBackend for PortableBackend {
    fn set_sun_production_mode(&mut self, mode: SunProductionMode) -> Result<()> {
        let mode = match mode {
            SunProductionMode::DirectCredit => 0,
            SunProductionMode::NativePickup => 1,
        };
        pvzp_rs::set_sun_production_mode(mode).map_err(Into::into)
    }
}

impl WorldResetBackend for PortableBackend {
    fn reset_world(&mut self, config: WorldResetConfig) -> Result<()> {
        let mut world = self.world()?;
        world
            .reset(
                config.completed_rounds,
                config.seed,
                config.initial_sun,
                config.card_cooldowns == ResetCardCooldowns::Ready,
            )
            .map_err(Into::into)
    }
}

impl GameSpeedHintBackend for PortableBackend {
    fn set_game_speed(&self, speed: PositiveFiniteF32) -> Result<()> {
        self.world()?;
        pvzp_rs::set_game_speed(speed.get()).map_err(Into::into)
    }
}

impl SunWriteBackend for PortableBackend {
    fn set_sun(&self, value: u32) -> Result<()> {
        self.world()?.set_sun(value).map_err(Into::into)
    }
}

impl DancerClockWriteBackend for PortableBackend {
    fn set_dancer_clock(&self, value: u32) -> Result<()> {
        self.world()?;
        pvzp_rs::set_app_counter(value).map_err(Into::into)
    }
}

impl SunCostRuleEditBackend for PortableBackend {
    fn set_sun_cost_ignored(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_sun_cost_ignored(enabled).map_err(Into::into)
    }

    fn sun_cost_ignored(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::sun_cost_ignored())
    }
}

impl VisibilityEditBackend for PortableBackend {
    fn set_fog_revealed(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_fog_revealed(enabled).map_err(Into::into)
    }

    fn fog_revealed(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::fog_revealed())
    }

    fn set_vase_contents_visible(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_vase_contents_visible(enabled).map_err(Into::into)
    }

    fn vase_contents_visible(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::vase_contents_visible())
    }
}

impl CobRuleEditBackend for PortableBackend {
    fn cob_fixed_delay(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::cob_fixed_delay())
    }

    fn set_cob_fixed_delay(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_cob_fixed_delay(enabled).map_err(Into::into)
    }

    fn cob_recharge_shortened(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::cob_recharge_shortened())
    }

    fn set_cob_recharge_shortened(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_cob_recharge_shortened(enabled).map_err(Into::into)
    }

    fn cob_drift_fixed(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::cob_drift_fixed())
    }

    fn set_cob_drift_fixed(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_cob_drift_fixed(enabled).map_err(Into::into)
    }
}

impl DropRuleEditBackend for PortableBackend {
    fn item_drop_disabled(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::item_drop_disabled())
    }

    fn set_item_drop_disabled(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_item_drop_disabled(enabled).map_err(Into::into)
    }

    fn natural_sun_drop_disabled(&self) -> Result<bool> {
        self.world()?;
        Ok(pvzp_rs::modifier::natural_sun_drop_disabled())
    }

    fn set_natural_sun_drop_disabled(&self, enabled: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_natural_sun_drop_disabled(enabled).map_err(Into::into)
    }

    fn set_natural_sun_generated(&self, count: NonNegativeI32) -> Result<()> {
        self.world()?.set_natural_sun_generated(count.get()).map_err(Into::into)
    }

    fn set_natural_sun_countdown(&self, countdown: NonNegativeI32) -> Result<()> {
        self.world()?
            .set_natural_sun_countdown(countdown.get())
            .map_err(Into::into)
    }
}

impl MaidCheatsBackend for PortableBackend {
    fn maid_cheat(&self) -> Result<MaidCheat> {
        self.world()?;
        match pvzp_rs::modifier::maid_cheat()? {
            0 => Ok(MaidCheat::Stop),
            1 => Ok(MaidCheat::CallPartner),
            2 => Ok(MaidCheat::Dancing),
            3 => Ok(MaidCheat::Move),
            value => Err(invalid_kind("maid cheat", value)),
        }
    }

    fn set_maid_cheat(&self, cheat: MaidCheat) -> Result<()> {
        self.world()?;
        let raw = match cheat {
            MaidCheat::Stop => 0,
            MaidCheat::CallPartner => 1,
            MaidCheat::Dancing => 2,
            MaidCheat::Move => 3,
        };
        pvzp_rs::modifier::set_maid_cheat(raw).map_err(Into::into)
    }
}

impl ProfileReadonlyBackend for PortableBackend {
    fn set_profile_readonly(&self, readonly: bool) -> Result<()> {
        self.world()?;
        pvzp_rs::modifier::set_profile_readonly(readonly).map_err(Into::into)
    }
}

impl BoardStateBackend for PortableBackend {
    fn natural_sun_generated(&self) -> Result<i32> {
        Ok(self.world()?.natural_sun_generated())
    }

    fn natural_sun_countdown(&self) -> Result<i32> {
        Ok(self.world()?.natural_sun_countdown())
    }

    fn dancer_clock(&self) -> Result<u32> {
        pvzp_rs::app_counter().map_err(Into::into)
    }

    fn ice_path_x(&self, row: u32) -> Result<i32> {
        self.world()?.ice_path_x(row).map_err(Into::into)
    }

    fn ice_path_countdown(&self, row: u32) -> Result<u32> {
        self.world()?.ice_path_countdown(row).map_err(Into::into)
    }

    fn spawn_allowed(&self, raw_zombie_kind: u32) -> Result<bool> {
        self.world()?.spawn_allowed(raw_zombie_kind).map_err(Into::into)
    }

    fn spawn_entry(&self, wave: u32, slot: u32) -> Result<i32> {
        self.world()?.spawn_entry(wave, slot).map_err(Into::into)
    }
}

impl WaveTimingBackend for PortableBackend {
    fn total_waves(&self) -> Result<i32> {
        Ok(self.world()?.total_waves())
    }
    fn refresh_countdown(&self) -> Result<i32> {
        Ok(self.world()?.refresh_countdown())
    }
    fn initial_countdown(&self) -> Result<i32> {
        Ok(self.world()?.initial_countdown())
    }
    fn huge_wave_countdown(&self) -> Result<i32> {
        Ok(self.world()?.huge_wave_countdown())
    }
    fn level_end_countdown(&self) -> Result<i32> {
        Ok(self.world()?.level_end_countdown())
    }
}

impl WaveRefreshControlBackend for PortableBackend {
    fn commit_timer_only_wave_refresh(&self, expected_current_wave: Wave, initial_countdown: i32) -> Result<()> {
        self.world()?
            .commit_wave_refresh(expected_current_wave.0, initial_countdown)
            .map_err(Into::into)
    }
}

impl WaveHealthBackend for PortableBackend {
    fn zombie_health_wave_start(&self) -> Result<i32> {
        self.world()?.zombie_health_wave_start().map_err(Into::into)
    }

    fn total_zombies_health_in_wave(&self, wave: i32) -> Result<i32> {
        self.world()?.total_zombie_health_in_wave(wave).map_err(Into::into)
    }
}

impl SceneEditBackend for PortableBackend {
    fn set_scene(&mut self, scene: SceneKind) -> Result<()> {
        if self.scene()? == scene {
            return Ok(());
        }
        if !matches!(self.game_ui()?, GameUi::LevelIntro | GameUi::Playing) || self.current_wave()?.0 > 0 {
            return Err(PortableBackendError::OperationRejected(
                "scene switch requires level intro or opening wave",
            ));
        }
        self.world()?.set_scene(scene.code()).map_err(Into::into)
    }
}

impl SpawnScheduleBackend for PortableBackend {
    fn spawn_wave_count(&self) -> Result<usize> {
        let count = self.world()?.total_waves();
        let count = usize::try_from(count).map_err(|_| PortableBackendError::NumericOutOfRange("spawn wave count"))?;
        if (1..=DEFAULT_SPAWN_WAVES).contains(&count) {
            Ok(count)
        } else {
            Err(PortableBackendError::OperationRejected(
                "spawn wave count is outside Portable range",
            ))
        }
    }

    fn set_spawn_type_allowed(&self, kind: ZombieKind, allowed: bool) -> Result<()> {
        self.world()?
            .set_spawn_allowed(kind.code(), allowed)
            .map_err(Into::into)
    }

    fn set_spawn_slot(&self, wave: usize, slot: SpawnWaveSlot, kind: Option<ZombieKind>) -> Result<()> {
        if wave >= self.spawn_wave_count()? {
            return Err(PortableBackendError::OperationRejected(
                "spawn wave is outside current board range",
            ));
        }
        let wave = u32::try_from(wave).map_err(|_| PortableBackendError::NumericOutOfRange("spawn wave"))?;
        let slot = u32::try_from(slot.index()).map_err(|_| PortableBackendError::NumericOutOfRange("spawn slot"))?;
        self.world()?
            .set_spawn_entry(wave, slot, kind.map_or(-1, ZombieKind::code))
            .map_err(Into::into)
    }

    fn pick_spawn_list(&self) -> Result<()> {
        self.world()?.pick_spawn_list().map_err(Into::into)
    }
}

impl PortableBackend {
    pub(crate) fn plant_in_pool<'a>(
        &'a self, pool: pvzp_rs::PlantPool<'a>, id: PlantId,
    ) -> Option<PortablePlantHandle<'a>> {
        pool.try_to_get(id.raw())
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortablePlantHandle::new)
            .filter(|handle| handle.is_live())
    }
}

impl PortableBackend {
    pub(crate) fn zombie_in_pool<'a>(
        &'a self, pool: pvzp_rs::ZombiePool<'a>, id: ZombieId,
    ) -> Option<PortableZombieHandle<'a>> {
        pool.try_to_get(id.raw())
            .map(pvzp_rs::Borrowed::as_non_null)
            .map(PortableZombieHandle::new)
            .filter(|handle| handle.is_live())
    }
}
