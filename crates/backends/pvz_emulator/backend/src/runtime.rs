use rsvz_backend_api::backend::Backend;
use rsvz_model::DEFAULT_RESET_INITIAL_SUN;

use std::cell::RefCell;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::rc::Rc;

use crate::PeBackendError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeWorldConfig {
    pub scene: pe_rs::SceneType,
    pub battle_seed: u32,
    pub level_seed: u32,
    pub total_flags: u32,
    pub initial_sun: u32,
}

impl Default for PeWorldConfig {
    fn default() -> Self {
        Self {
            scene: pe_rs::SceneType::Pool,
            battle_seed: 0,
            level_seed: 0,
            total_flags: 1_000,
            initial_sun: DEFAULT_RESET_INITIAL_SUN,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeUpdateOutcome {
    Normal,
    ObjectiveReached,
    GameOver,
}

thread_local! {
    static CURRENT_CONTEXT: RefCell<Option<NonNull<PeContext>>> = const { RefCell::new(None) };
}

/// Linear capability for the PE world installed on the current worker.
///
/// The private marker keeps the zero-sized token on its owner thread and the
/// lack of `Clone`, `Copy`, and `Default` prevents safe capability duplication.
///
/// Same-epoch edits only need a shared receiver:
///
/// ```no_run
/// use rsvz_backend_api::backend::{PlantHealthWriteBackend, PlantReadBackend};
/// use rsvz_model::model::PositiveHp;
/// use rsvz_pvz_emulator_backend::PeBackend;
///
/// fn edit_first_plant(backend: &PeBackend) {
///     if let Some(plant) = backend.plants().unwrap().next() {
///         backend
///             .set_plant_hp(plant, PositiveHp::new(1).unwrap())
///             .unwrap();
///     }
/// }
/// ```
///
/// Epoch-changing operations cannot overlap a live handle:
///
/// ```compile_fail
/// use rsvz_backend_api::backend::{PlantReadBackend, SceneEditBackend};
/// use rsvz_model::model::SceneKind;
/// use rsvz_pvz_emulator_backend::PeBackend;
///
/// fn replace_scene(backend: &mut PeBackend) {
///     let plant = backend.plants().unwrap().next().unwrap();
///     backend.set_scene(SceneKind::Day).unwrap();
///     let _ = backend.plant_id(plant);
/// }
/// ```
///
/// ```compile_fail
/// use rsvz_pvz_emulator_backend::PeBackend;
/// let _backend = PeBackend::new();
/// ```
///
/// ```compile_fail
/// use rsvz_pvz_emulator_backend::PeBackend;
/// let _backend = PeBackend { _thread_bound: std::marker::PhantomData };
/// ```
///
/// ```compile_fail
/// use rsvz_pvz_emulator_backend::PeBackend;
/// fn require_default<T: Default>() {}
/// require_default::<PeBackend>();
/// ```
///
/// ```compile_fail
/// use rsvz_pvz_emulator_backend::PeBackend;
/// fn require_clone<T: Clone>() {}
/// require_clone::<PeBackend>();
/// ```
///
///
/// ```compile_fail
/// use rsvz_pvz_emulator_backend::PeBackend;
/// fn require_send<T: Send>() {}
/// require_send::<PeBackend>();
/// ```
///
/// ```compile_fail
/// use rsvz_pvz_emulator_backend::PeBackend;
/// fn require_sync<T: Sync>() {}
/// require_sync::<PeBackend>();
/// ```
#[derive(Debug)]
pub struct PeBackend {
    _thread_bound: PhantomData<Rc<()>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PeRuntimeState {
    config: PeWorldConfig,
    world_epoch: u64,
    spawn_ready: bool,
    spawn_flags_initialized: bool,
    battle_started: bool,
    normalize_origin_after_update: bool,
    objective_reached_visible: bool,
    game_over: bool,
    fast_forward_active: bool,
    // A native chooser retains the old bank until the replacement has been selected.
    pending_cards: Option<([Option<rsvz_model::CheckedCardSelection>; 10], usize)>,
}

impl PeRuntimeState {
    const fn new(config: PeWorldConfig, spawn_ready: bool) -> Self {
        Self {
            config,
            world_epoch: 0,
            spawn_ready,
            spawn_flags_initialized: false,
            battle_started: false,
            normalize_origin_after_update: false,
            objective_reached_visible: false,
            game_over: false,
            fast_forward_active: false,
            pending_cards: None,
        }
    }

    fn reset(&mut self, config: PeWorldConfig, spawn_ready: bool) {
        let world_epoch = self.world_epoch.wrapping_add(1);
        *self = Self::new(config, spawn_ready);
        self.world_epoch = world_epoch;
    }

    fn record_update(&mut self, outcome: PeUpdateOutcome) {
        match outcome {
            PeUpdateOutcome::Normal => {}
            PeUpdateOutcome::ObjectiveReached => {
                self.objective_reached_visible = true;
                self.pending_cards = Some(([None; 10], 0));
            }
            PeUpdateOutcome::GameOver => {
                self.game_over = true;
            }
        }
    }
}

struct PeContext {
    world: RefCell<pe_rs::World>,
    state: RefCell<PeRuntimeState>,
}

pub struct PeWorldOwner {
    context: PeContext,
    backend: PeBackend,
}

/// A PE world installed on the current worker for the rest of that worker run.
///
/// The boxed owner keeps the addresses installed in TLS stable. Dropping this
/// value clears the world/state slots before freeing their storage.

pub struct PeWorldRun {
    _guard: CurrentContextGuard,
    owner: Box<PeWorldOwner>,
}

impl PeWorldOwner {
    pub fn new_reset(config: PeWorldConfig) -> Result<Self, PeBackendError> {
        let mut owner = Self::new_reset_deferred_spawn(config)?;
        owner.context.world.get_mut().reset_spawn()?;
        owner.context.state.get_mut().spawn_ready = true;
        Ok(owner)
    }

    /// Builds a reset world without invoking broad spawn reset (B13).
    ///
    /// The runner uses this after it has registered the script so it can choose
    /// exactly one spawn initialization path: B13 for a random list, or direct
    /// B11/B12 writes for an explicit list. Keeping this runner-internal avoids
    /// exposing a PE-only lifecycle policy through the public backend traits.
    pub fn new_reset_deferred_spawn(config: PeWorldConfig) -> Result<Self, PeBackendError> {
        let world = new_world_before_spawn(config)?;
        Ok(Self {
            context: PeContext {
                world: RefCell::new(world),
                state: RefCell::new(PeRuntimeState::new(config, false)),
            },
            backend: PeBackend::owner_token(),
        })
    }

    /// Installs this world once on the current worker.
    ///
    /// The returned run keeps ownership of the unique backend capability.

    pub fn install_current(self) -> Result<PeWorldRun, PeBackendError> {
        let mut owner = Box::new(self);
        let guard = CurrentContextGuard::enter(NonNull::from(&mut owner.context))?;
        Ok(PeWorldRun { _guard: guard, owner })
    }

    /// Enters this owner as the current PE world for the duration of `f`.
    ///
    #[cfg(test)]
    #[doc(hidden)]
    pub fn with_current<R>(
        &mut self, f: impl for<'scope> FnOnce(&'scope mut PeBackend) -> R,
    ) -> Result<R, PeBackendError> {
        let guard = CurrentContextGuard::enter(NonNull::from(&mut self.context))?;
        let result = f(&mut self.backend);
        drop(guard);
        Ok(result)
    }
}

impl PeWorldRun {
    /// Temporarily borrows the runner-owned backend capability.
    pub fn with_backend<R>(&mut self, f: impl FnOnce(&mut PeBackend) -> R) -> R {
        f(&mut self.owner.backend)
    }

    pub fn update_world(&mut self) -> Result<PeUpdateOutcome, PeBackendError> {
        self.owner.backend.update_world()
    }
}

fn new_world_before_spawn(config: PeWorldConfig) -> Result<pe_rs::World, PeBackendError> {
    validate_world_config(config)?;
    let dancer_clock = rsvz_backend_api::opening::mix_dancer_clock(config.level_seed);
    let mut world = pe_rs::World::new_deterministic(config.scene, config.battle_seed, config.level_seed, dancer_clock)?;
    initialize_world_before_spawn(&mut world, config, None)?;
    Ok(world)
}

fn reset_world_before_spawn(
    world: &mut pe_rs::World, config: PeWorldConfig, locked_rng_value: Option<u32>,
) -> Result<(), PeBackendError> {
    validate_world_config(config)?;
    world.reset_deterministic(
        config.scene,
        config.battle_seed,
        config.level_seed,
        rsvz_backend_api::opening::mix_dancer_clock(config.level_seed),
    )?;
    initialize_world_before_spawn(world, config, locked_rng_value)
}

fn initialize_world_before_spawn(
    world: &mut pe_rs::World, config: PeWorldConfig, locked_rng_value: Option<u32>,
) -> Result<(), PeBackendError> {
    if let Some(value) = locked_rng_value {
        world.scene().lock_rngs(value)?;
    }
    world.reset_sun()?;

    let sun = world.scene().sun_data().as_ptr();
    let spawn = world.scene().spawn_data().as_ptr();
    // SAFETY: both pointers are borrowed from this live world. These are the
    // documented address-stable scalar setup writes performed before B13.
    unsafe {
        std::ptr::addr_of_mut!((*sun).sun).write(config.initial_sun);
        std::ptr::addr_of_mut!((*spawn).total_flags).write(config.total_flags);
    }

    Ok(())
}

fn fill_random_seed_slots(scene: pe_rs::Scene<'_>) -> Result<(), PeBackendError> {
    const TYPE_COUNT: usize = pe_rs::PlantType::Imitater.to_raw() as usize + 1;

    let mut used = [false; TYPE_COUNT];
    let mut empty_slots = [false; rsvz_model::MAX_SEED_SLOTS];
    for (slot, empty) in empty_slots.iter_mut().enumerate() {
        let card = scene.card_at(slot as u32)?.as_ptr();
        // SAFETY: card slots are address-stable for this world borrow.
        let raw = unsafe { std::ptr::addr_of!((*card).type_).read() };
        let plant_type = pe_rs::PlantType::from_raw(raw).ok_or(PeBackendError::InvalidKind { kind: "plant", raw })?;
        if plant_type == pe_rs::PlantType::None {
            *empty = true;
        } else {
            used[plant_type.to_raw() as usize] = true;
        }
    }
    if !empty_slots.contains(&true) {
        return Ok(());
    }
    if scene.rng_locked(false) {
        return Err(PeBackendError::Unsupported(
            "partial card selection with locked randomness",
        ));
    }

    for (slot, empty) in empty_slots.into_iter().enumerate() {
        if !empty {
            continue;
        }
        let plant_type = loop {
            let raw = scene.battle_randint(TYPE_COUNT as u32)? as usize;
            if raw != pe_rs::PlantType::Imitater.to_raw() as usize && !used[raw] {
                break pe_rs::PlantType::from_raw(raw as i32).expect("random seed type is in range");
            }
        };
        used[plant_type.to_raw() as usize] = true;
        let card = scene.card_at(slot as u32)?.as_ptr();
        // SAFETY: card slots are address-stable for this world borrow.
        unsafe {
            std::ptr::addr_of_mut!((*card).type_).write(plant_type.to_raw());
            std::ptr::addr_of_mut!((*card).imitater_type).write(pe_rs::PlantType::None.to_raw());
            std::ptr::addr_of_mut!((*card).cold_down).write(0);
        }
    }
    Ok(())
}

pub(crate) const MIN_COMPLETED_ROUNDS: u32 = 63;

fn validate_world_config(config: PeWorldConfig) -> Result<(), PeBackendError> {
    if config.total_flags / 2 < MIN_COMPLETED_ROUNDS {
        return Err(PeBackendError::Unsupported(
            "PE only supports the saturated high-round spawn model (at least 126 flags)",
        ));
    }
    Ok(())
}

struct CurrentContextGuard;

impl CurrentContextGuard {
    fn enter(context: NonNull<PeContext>) -> Result<Self, PeBackendError> {
        CURRENT_CONTEXT.with(|slot| {
            let mut slot = slot.try_borrow_mut().map_err(|_| PeBackendError::WorldScopeConflict)?;
            if slot.is_some() {
                return Err(PeBackendError::WorldScopeConflict);
            }
            *slot = Some(context);
            Ok(Self)
        })
    }
}

impl Drop for CurrentContextGuard {
    fn drop(&mut self) {
        CURRENT_CONTEXT.with_borrow_mut(|slot| *slot = None);
    }
}

#[cfg(test)]
fn with_current_context<R>(f: impl FnOnce(&PeContext) -> R) -> Result<R, PeBackendError> {
    CURRENT_CONTEXT.with(|slot| {
        let slot = slot.try_borrow().map_err(|_| PeBackendError::WorldScopeConflict)?;
        let context = slot.as_ref().ok_or(PeBackendError::NotInitialized)?;
        // SAFETY: the installed owner outlives this scope. Only shared context
        // references are created; world and state enforce their own borrows.
        Ok(f(unsafe { context.as_ref() }))
    })
}

fn with_current_pe<R>(
    context: &PeContext, f: impl FnOnce(&pe_rs::World, &mut PeRuntimeState) -> R,
) -> Result<R, PeBackendError> {
    let world = context
        .world
        .try_borrow()
        .map_err(|_| PeBackendError::WorldScopeConflict)?;
    let mut state = context
        .state
        .try_borrow_mut()
        .map_err(|_| PeBackendError::WorldScopeConflict)?;
    Ok(f(&world, &mut state))
}

fn with_current_pe_mut<R>(
    context: &PeContext, f: impl FnOnce(&mut pe_rs::World, &mut PeRuntimeState) -> R,
) -> Result<R, PeBackendError> {
    let mut world = context
        .world
        .try_borrow_mut()
        .map_err(|_| PeBackendError::WorldScopeConflict)?;
    let mut state = context
        .state
        .try_borrow_mut()
        .map_err(|_| PeBackendError::WorldScopeConflict)?;
    Ok(f(&mut world, &mut state))
}

#[cfg(test)]
fn with_current_pe_world<R>(f: impl FnOnce(&pe_rs::World) -> R) -> Result<R, PeBackendError> {
    with_current_context(|context| {
        let world = context
            .world
            .try_borrow()
            .map_err(|_| PeBackendError::WorldScopeConflict)?;
        Ok(f(&world))
    })?
}

fn with_current_pe_state<R>(
    context: &PeContext, f: impl FnOnce(&mut PeRuntimeState) -> R,
) -> Result<R, PeBackendError> {
    let mut state = context
        .state
        .try_borrow_mut()
        .map_err(|_| PeBackendError::WorldScopeConflict)?;
    Ok(f(&mut state))
}

impl PeBackend {
    const fn owner_token() -> Self {
        Self {
            _thread_bound: PhantomData,
        }
    }

    #[must_use]
    pub fn is_initialized(&self) -> bool {
        true
    }

    /// The owner installs its context before lending this unique token, and
    /// cannot uninstall or replace it until every token borrow ends.
    fn context(&self) -> &PeContext {
        CURRENT_CONTEXT.with_borrow(|slot| {
            // SAFETY: only the installed owner lends PeBackend. This method's
            // result borrows the token, so it cannot outlive that owner scope.
            unsafe { slot.unwrap_unchecked().as_ref() }
        })
    }

    pub(crate) fn with_current_world<R>(&self, f: impl FnOnce(&pe_rs::World) -> R) -> Result<R, PeBackendError> {
        let world = self
            .context()
            .world
            .try_borrow()
            .map_err(|_| PeBackendError::WorldScopeConflict)?;
        Ok(f(&world))
    }

    pub(crate) fn with_current_world_mut<R>(
        &mut self, f: impl FnOnce(&mut pe_rs::World) -> R,
    ) -> Result<R, PeBackendError> {
        let context = self.context();
        {
            let mut world = context
                .world
                .try_borrow_mut()
                .map_err(|_| PeBackendError::WorldScopeConflict)?;
            Ok(f(&mut world))
        }
    }

    pub fn config(&self) -> Result<PeWorldConfig, PeBackendError> {
        with_current_pe_state(self.context(), |state| state.config)
    }

    pub fn clock_value(&self) -> Result<i32, PeBackendError> {
        let clock = self.with_current_world(|world| world.scene().main_counter())?;
        Ok(clock as i32)
    }

    pub fn current_wave_initial_health(&self) -> Result<u32, PeBackendError> {
        let spawn = self.with_current_world(|world| world.scene().spawn_data().as_ptr())?;
        // SAFETY: `spawn` belongs to the currently installed world and this
        // copies one address-stable scalar immediately.
        Ok(unsafe { std::ptr::addr_of!((*spawn).hp.initial).read() })
    }

    pub fn current_wave_health(&self) -> Result<u32, PeBackendError> {
        Ok(self.with_current_world(|world| world.current_spawn_health())?)
    }

    pub fn current_wave_threshold_health(&self) -> Result<i32, PeBackendError> {
        let spawn = self.with_current_world(|world| world.scene().spawn_data().as_ptr())?;
        // SAFETY: `spawn` belongs to the currently installed world and this
        // copies one address-stable scalar immediately.
        Ok(unsafe { std::ptr::addr_of!((*spawn).hp.threshold).read() })
    }

    pub fn objective_reached_visible(&self) -> Result<bool, PeBackendError> {
        with_current_pe_state(self.context(), |state| state.objective_reached_visible)
    }

    pub fn game_over(&self) -> Result<bool, PeBackendError> {
        with_current_pe_state(self.context(), |state| state.game_over)
    }

    pub(crate) fn spawn_ready(&self) -> Result<bool, PeBackendError> {
        with_current_pe_state(self.context(), |state| state.spawn_ready)
    }

    pub(crate) fn battle_started(&self) -> Result<bool, PeBackendError> {
        with_current_pe_state(self.context(), |state| state.battle_started)
    }

    pub(crate) fn commit_card_selection(&self) -> Result<(), PeBackendError> {
        with_current_pe(self.context(), |world, state| -> Result<(), PeBackendError> {
            if let Some((cards, count)) = state.pending_cards {
                let mut types = [pe_rs::PlantType::None; 10];
                let mut imitater = pe_rs::PlantType::None;
                for index in 0..count {
                    let (kind, target) = crate::convert::kind::card_to_pe(cards[index].expect("selected prefix"));
                    types[index] = kind;
                    if kind == pe_rs::PlantType::Imitater {
                        imitater = target;
                    }
                }
                world.select_plants(&types[..count], imitater)?;
                state.pending_cards = None;
                fill_random_seed_slots(world.scene())?;
            }
            Ok(())
        })?
    }

    pub(crate) fn finish_battle_opening(&self) -> Result<(), PeBackendError> {
        self.commit_card_selection()?;
        with_current_pe(self.context(), |world, state| -> Result<(), PeBackendError> {
            fill_random_seed_slots(world.scene())?;
            if !state.battle_started {
                state.normalize_origin_after_update = true;
                state.battle_started = true;
            }
            Ok(())
        })?
    }

    pub(crate) fn write_spawn_flag(&self, raw: u32, allowed: bool) -> Result<(), PeBackendError> {
        with_current_pe(self.context(), |world, state| -> Result<(), PeBackendError> {
            world.scene().set_spawn_flag(raw, allowed)?;
            state.spawn_flags_initialized = true;
            Ok(())
        })?
    }

    pub(crate) fn write_spawn_slot(&self, wave: u32, slot: u32, kind: pe_rs::ZombieType) -> Result<(), PeBackendError> {
        with_current_pe(self.context(), |world, state| -> Result<(), PeBackendError> {
            world.scene().set_spawn_entry(wave, slot, kind)?;
            state.spawn_ready = true;
            Ok(())
        })?
    }

    pub(crate) fn pending_card_count(&self) -> Result<Option<usize>, PeBackendError> {
        with_current_pe_state(self.context(), |state| state.pending_cards.map(|(_, count)| count))
    }

    pub(crate) fn pending_card(
        &self, index: usize,
    ) -> Result<Option<rsvz_model::CheckedCardSelection>, PeBackendError> {
        with_current_pe_state(self.context(), |state| {
            state
                .pending_cards
                .and_then(|(cards, count)| (index < count).then(|| cards[index]).flatten())
        })
    }

    pub(crate) fn append_pending_card(&self, card: rsvz_model::CheckedCardSelection) -> Result<bool, PeBackendError> {
        with_current_pe_state(self.context(), |state| {
            let Some((cards, count)) = state.pending_cards.as_mut() else {
                return Ok(false);
            };
            if *count == cards.len() {
                return Err(PeBackendError::InvalidSeedSlot);
            }
            cards[*count] = Some(card);
            *count += 1;
            Ok(true)
        })?
    }

    pub(crate) fn initialize_spawn_list(&self) -> Result<(), PeBackendError> {
        with_current_pe(self.context(), |world, state| -> Result<(), PeBackendError> {
            if state.spawn_flags_initialized {
                if !world.pick_spawn_list()? {
                    return Err(PeBackendError::OperationRejected(
                        "spawn flags have no valid weighted candidate",
                    ));
                }
            } else {
                world.reset_spawn()?;
                state.spawn_flags_initialized = true;
            }
            state.spawn_ready = true;
            Ok(())
        })?
    }

    pub fn fast_forward_hint_active(&self) -> bool {
        // A safe token is borrowed from an installed owner. Keep state borrow protection,
        // but a read does not need the mutable RuntimeState borrow used by control operations.
        self.context().state.borrow().fast_forward_active
    }

    pub(crate) fn switch_scene_type(&mut self, scene: pe_rs::SceneType) -> Result<(), PeBackendError> {
        with_current_pe_mut(self.context(), |world, state| -> Result<(), PeBackendError> {
            world.set_scene_type(scene)?;
            state.config.scene = scene;
            Ok(())
        })?
    }
    /// Resets all world state except the broad random spawn initialization.
    /// Opening must follow this with either `pick_spawn_list` or explicit
    /// validated spawn-list writes before advancing the world.
    pub(crate) fn reset_with_config_deferred_spawn(&mut self, config: PeWorldConfig) -> Result<(), PeBackendError> {
        with_current_pe_mut(self.context(), |world, state| -> Result<(), PeBackendError> {
            let scene = world.scene();
            let random_mode = if scene.rng_locked(false) {
                Some(scene.rng_fixed(false))
            } else {
                None
            };
            reset_world_before_spawn(world, config, random_mode)?;
            state.reset(config, false);
            Ok(())
        })?
    }

    fn update_world(&mut self) -> Result<PeUpdateOutcome, PeBackendError> {
        with_current_pe_mut(
            self.context(),
            |world, state| -> Result<PeUpdateOutcome, PeBackendError> {
                if state.game_over {
                    return Ok(PeUpdateOutcome::GameOver);
                }

                state.objective_reached_visible = false;
                let event_sink = crate::event::sink();
                if let Some(sink) = event_sink {
                    let counter = world.scene().main_counter() as i32;
                    (sink.begin_logic_frame)(state.world_epoch, counter);
                }
                let result = (|| -> Result<PeUpdateOutcome, PeBackendError> {
                    let terminal = world.update()?;
                    if state.normalize_origin_after_update {
                        let scene = world.scene();
                        scene.set_main_counter(0)?;
                        world.restore_dancer_clock(rsvz_backend_api::opening::mix_dancer_clock(
                            state.config.level_seed,
                        ))?;
                        state.normalize_origin_after_update = false;
                    }
                    let outcome = if terminal && world.scene().game_over() {
                        PeUpdateOutcome::GameOver
                    } else if terminal {
                        PeUpdateOutcome::ObjectiveReached
                    } else {
                        PeUpdateOutcome::Normal
                    };
                    state.record_update(outcome);
                    Ok(outcome)
                })();
                if let Some(sink) = event_sink {
                    let status = match &result {
                        Ok(PeUpdateOutcome::Normal) => rsvz_model::model::BattleStatus::Running,
                        Ok(PeUpdateOutcome::ObjectiveReached) => rsvz_model::model::BattleStatus::ObjectiveReached,
                        Ok(PeUpdateOutcome::GameOver) => rsvz_model::model::BattleStatus::Lost,
                        Err(_) => rsvz_model::model::BattleStatus::Unknown,
                    };
                    (sink.end_logic_frame)(status);
                }
                result
            },
        )?
    }

    pub(crate) fn set_fast_forward_active(&self, active: bool) -> Result<(), PeBackendError> {
        with_current_pe_state(self.context(), |state| state.fast_forward_active = active)
    }
}

impl Backend for PeBackend {
    type Error = PeBackendError;
}

#[cfg(test)]
mod tests;
