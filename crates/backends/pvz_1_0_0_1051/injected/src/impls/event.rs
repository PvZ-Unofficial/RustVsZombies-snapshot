use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::{self, NonNull};

use rsvz_backend_api::backend::{NativeEventBackend, NativeEventSink};
use rsvz_model::model::{
    BattleStatus, BeginPlantEffect, EventDecision, EventToken, GameUi, GargantuarAshHitFact, GargantuarSpawnedFact,
    Grid, HomeEntryFact, ImpThrownFact, PlantDamageRule, PlantEffect, PlantEffectAttemptFact, PlantEffectOutcome,
    PlantEffectSource, PlantId, ProjectileId, ZombieId,
};

use crate::error::{Pvz1051Error, Result};
use crate::patches::EventHookGuard;
use crate::raw::{abi as asm, layout as ptrs};
use crate::runtime::Pvz1051Backend;

thread_local! {
    static SINK: Cell<Option<NativeEventSink>> = const { Cell::new(None) };
    static ACTIVE: RefCell<Option<ActiveEventHooks>> = const { RefCell::new(None) };
    static ORIGINAL_DAMAGE_RULE: Cell<Option<PlantDamageRule>> = const { Cell::new(None) };
    static BOARD_EPOCH: Cell<u64> = const { Cell::new(0) };
    static TERMINAL_LATCHED: Cell<bool> = const { Cell::new(false) };
    static FRAME_OPEN: Cell<bool> = const { Cell::new(false) };
    static FRAME_COUNTER: Cell<i32> = const { Cell::new(0) };
    static JACK_ACTOR: Cell<*mut ptrs::Zombie> = const { Cell::new(ptr::null_mut()) };
}

struct ActiveEventHooks {
    guard: EventHookGuard,
    rule: PlantDamageRule,
}

pub(crate) fn sink() -> Option<NativeEventSink> {
    if TERMINAL_LATCHED.get() { None } else { SINK.get() }
}

pub(crate) fn note_world_reset() {
    BOARD_EPOCH.set(BOARD_EPOCH.get().wrapping_add(1));
    TERMINAL_LATCHED.set(false);
    FRAME_OPEN.set(false);
}

pub(crate) fn damage_rule() -> Result<PlantDamageRule> {
    ACTIVE
        .with_borrow(|active| active.as_ref().map(|active| active.rule))
        .map_or_else(crate::patches::variant::plant_damage_rule::rule, Ok)
}

fn active_damage_rule() -> PlantDamageRule {
    ACTIVE.with_borrow(|active| active.as_ref().map_or(PlantDamageRule::Normal, |active| active.rule))
}

pub(crate) fn set_damage_rule(rule: PlantDamageRule) -> Result<()> {
    let Some((interest, old_rule)) =
        ACTIVE.with_borrow(|active| active.as_ref().map(|active| (active.guard.interest(), active.rule)))
    else {
        return crate::patches::variant::plant_damage_rule::set_rule(rule);
    };

    ACTIVE.with_borrow_mut(|active| {
        active
            .as_mut()
            .expect("active event hook checked above")
            .guard
            .restore()
    })?;
    ACTIVE.with_borrow_mut(Option::take);

    if let Err(error) = crate::patches::variant::plant_damage_rule::set_rule(rule) {
        if let Err(restore) = reinstall_after_rule_change(interest, old_rule) {
            crate::runtime::hook::fail_and_request_unload();
            return Err(Pvz1051Error::Patch(format!(
                "set plant damage rule failed: {error}; restore also failed: {restore}"
            )));
        }
        return Err(error);
    }
    match EventHookGuard::install(interest) {
        Ok(hooks) => {
            ACTIVE.with_borrow_mut(|active| *active = Some(ActiveEventHooks { guard: hooks, rule }));
            Ok(())
        }
        Err(error) => {
            if let Err(restore) = crate::patches::variant::plant_damage_rule::set_rule(old_rule)
                .and_then(|()| reinstall_after_rule_change(interest, old_rule))
            {
                crate::runtime::hook::fail_and_request_unload();
                return Err(Pvz1051Error::Patch(format!(
                    "install event hooks for damage rule failed: {error}; restore also failed: {restore}"
                )));
            }
            Err(error)
        }
    }
}

fn reinstall_after_rule_change(interest: rsvz_model::EventInterest, rule: PlantDamageRule) -> Result<()> {
    let hooks = EventHookGuard::install(interest)?;
    ACTIVE.with_borrow_mut(|active| *active = Some(ActiveEventHooks { guard: hooks, rule }));
    Ok(())
}

pub(crate) fn set_owned_damage_rule(rule: PlantDamageRule) -> Result<()> {
    let original = match ORIGINAL_DAMAGE_RULE.get() {
        Some(original) => original,
        None => damage_rule()?,
    };
    set_damage_rule(rule)?;
    ORIGINAL_DAMAGE_RULE.set(Some(original));
    Ok(())
}

pub(crate) fn restore_owned_damage_rule() -> Result<()> {
    let Some(original) = ORIGINAL_DAMAGE_RULE.get() else {
        return Ok(());
    };
    set_damage_rule(original)?;
    ORIGINAL_DAMAGE_RULE.set(None);
    Ok(())
}

impl NativeEventBackend for Pvz1051Backend {
    fn install_native_event_sink(&mut self, sink: NativeEventSink) -> Result<()> {
        if sink.interest.is_empty() {
            return Err(Pvz1051Error::InvariantViolated("empty native event interest"));
        }
        if SINK.get().is_some() {
            return Err(Pvz1051Error::InvariantViolated("native event sink already installed"));
        }
        let rule = crate::patches::variant::plant_damage_rule::rule()?;
        let hooks = EventHookGuard::install(sink.interest)?;
        ACTIVE.with_borrow_mut(|active| *active = Some(ActiveEventHooks { guard: hooks, rule }));
        SINK.set(Some(sink));
        TERMINAL_LATCHED.set(false);
        Ok(())
    }

    fn remove_native_event_sink(&mut self) -> Result<()> {
        if let Err(error) = ACTIVE.with_borrow_mut(|active| {
            let Some(active) = active.as_mut() else {
                return Ok(());
            };
            active.guard.restore()
        }) {
            crate::runtime::hook::fail_and_request_unload();
            return Err(error);
        }
        ACTIVE.with_borrow_mut(Option::take);
        SINK.set(None);
        TERMINAL_LATCHED.set(false);
        FRAME_OPEN.set(false);
        JACK_ACTOR.set(ptr::null_mut());
        Ok(())
    }
}

pub(crate) extern "C" fn begin_logic_frame_from_patch() {
    boundary_catching(begin_logic_frame_inner);
}

pub(crate) extern "C" fn end_logic_frame_from_patch() {
    boundary_catching(end_logic_frame_inner);
}

pub(crate) fn begin_logic_frame_direct() {
    boundary_catching(begin_logic_frame_inner);
}

pub(crate) fn end_logic_frame_direct() {
    boundary_catching(end_logic_frame_inner);
}

fn boundary_catching(callback: fn()) {
    if catch_unwind(AssertUnwindSafe(callback)).is_err() {
        crate::runtime::hook::fail_and_request_unload();
    }
}

fn begin_logic_frame_inner() {
    let Some(sink) = sink() else {
        return;
    };
    let Ok(board) = crate::runtime::backend::current_board() else {
        return;
    };
    // SAFETY: the current Board pointer is checked and the clock is copied.
    let counter = unsafe { ptrs::Board::clock(board.as_ptr()) };
    (sink.begin_logic_frame)(BOARD_EPOCH.get(), counter);
    FRAME_COUNTER.set(counter);
    FRAME_OPEN.set(true);
}

fn end_logic_frame_inner() {
    let Some(sink) = sink() else {
        return;
    };
    let status = current_battle_status();
    (sink.end_logic_frame)(status);
    FRAME_OPEN.set(false);
    latch_terminal_status(status);
}

fn latch_terminal_status(status: BattleStatus) {
    if matches!(
        status,
        BattleStatus::ObjectiveReached | BattleStatus::Lost | BattleStatus::Ended
    ) {
        TERMINAL_LATCHED.set(true);
    }
}

fn current_battle_status() -> BattleStatus {
    let Ok(raw_ui) = crate::runtime::backend::current_raw_game_ui() else {
        return BattleStatus::Unknown;
    };
    let Ok(ui) = GameUi::try_from_code(raw_ui) else {
        return BattleStatus::Unknown;
    };
    let level_complete = if ui == GameUi::Playing {
        crate::runtime::backend::current_board().ok().map(|board| {
            // SAFETY: current_board returned a live checked Board pointer.
            unsafe { ptrs::Board::level_complete(board.as_ptr()) }
        })
    } else {
        None
    };
    super::base::classify_battle_status(ui, level_complete)
}

pub(crate) extern "C" fn process_jack_radius_from_patch(
    zombie: *mut ptrs::Zombie, board: *mut ptrs::Board, x: i32, y: i32,
) {
    native_catching(|| {
        let _actor = JackActorGuard::enter(zombie);
        // SAFETY: the verified callsite supplies the active Board, Jack actor,
        // and exact native radius coordinates/register convention.
        unsafe { asm::board_kill_all_plants_in_radius(board, x, y) };
    });
}

struct JackActorGuard(*mut ptrs::Zombie);

impl JackActorGuard {
    fn enter(actor: *mut ptrs::Zombie) -> Self {
        Self(JACK_ACTOR.replace(actor))
    }
}

impl Drop for JackActorGuard {
    fn drop(&mut self) {
        JACK_ACTOR.set(self.0);
    }
}

pub(crate) extern "C" fn process_jack_plant_from_patch(plant: *mut ptrs::Plant) {
    native_catching(|| process_jack_plant(plant));
}

pub(crate) extern "C" fn process_bite_from_patch(zombie: *mut ptrs::Zombie, plant: *mut ptrs::Plant) {
    native_catching(|| process_direct_damage(zombie.cast(), plant, EffectSourceTag::Bite, 4));
}

pub(crate) extern "C" fn process_basketball_from_patch(
    projectile: *mut ptrs::Projectile, plant: *mut ptrs::Plant, damage: i32,
) {
    native_catching(|| process_direct_damage(projectile.cast(), plant, EffectSourceTag::Basketball, damage));
}

pub(crate) extern "C" fn process_garg_spikerock_from_patch(zombie: *mut ptrs::Zombie, plant: *mut ptrs::Plant) {
    native_catching(|| process_garg_spikerock(zombie, plant));
}

pub(crate) extern "C" fn process_garg_squish_from_patch(
    zombie: *mut ptrs::Zombie, plant: *mut ptrs::Plant, attack_type: i32,
) {
    native_catching(|| process_garg_squish(zombie, plant, attack_type));
}

pub(crate) extern "C" fn process_bungee_lift_from_patch(zombie: *mut ptrs::Zombie, plant: *mut ptrs::Plant) {
    native_catching(|| {
        let (Some(zombie), Some(plant)) = (NonNull::new(zombie), NonNull::new(plant)) else {
            return;
        };
        // SAFETY: 0x524dca follows native full-ID validation and supplies EDI/ESI.
        let id = unsafe { ptrs::DataArray::<ptrs::Zombie>::item_id(zombie.as_ptr()) };
        let begin = begin_effect(
            PlantEffectSource::Bungee(ZombieId::from_raw(id)),
            plant,
            PlantEffect::Steal,
        );
        if begin.decision == EventDecision::SuppressByMeasurement {
            finish(begin.token, PlantEffectOutcome::SuppressedByMeasurement);
            return;
        }
        // SAFETY: replay the verified mov dword ptr [esi+0x134],2.
        unsafe { plant.as_ptr().cast::<u8>().add(0x134).cast::<i32>().write(2) };
        finish(begin.token, PlantEffectOutcome::Stolen);
    });
}

pub(crate) extern "C" fn emit_home_entry_from_patch(zombie: *mut ptrs::Zombie) {
    native_catching(|| emit_home_entry(zombie));
}

pub(crate) extern "C" fn emit_gargantuar_allocated_from_patch(
    zombie: *mut ptrs::Zombie, from_wave: i32, raw_kind: i32, row: i32,
) {
    native_catching(|| emit_gargantuar_allocated(zombie, from_wave, raw_kind, row));
}

pub(crate) extern "C" fn emit_imp_thrown_from_patch(parent: *mut ptrs::Zombie, imp: *mut ptrs::Zombie) {
    native_catching(|| emit_imp_thrown(parent, imp));
}

pub(crate) extern "C" fn emit_gargantuar_ash_hit_from_patch(zombie: *mut ptrs::Zombie) {
    native_catching(|| emit_gargantuar_ash_hit(zombie));
}

fn native_catching(callback: impl FnOnce()) {
    if catch_unwind(AssertUnwindSafe(callback)).is_err() {
        crate::runtime::hook::fail_and_request_unload();
    }
}

#[derive(Clone, Copy)]
enum EffectSourceTag {
    Bite,
    Basketball,
}

fn source(tag: EffectSourceTag, actor: NonNull<u8>) -> PlantEffectSource {
    // SAFETY: the verified hook and tag identify an occupied actor DataArray slot.
    unsafe {
        match tag {
            EffectSourceTag::Bite => PlantEffectSource::Bite(ZombieId::from_raw(
                ptrs::DataArray::<ptrs::Zombie>::item_id(actor.as_ptr().cast()),
            )),
            EffectSourceTag::Basketball => {
                PlantEffectSource::Basketball(ProjectileId::from_raw(ptrs::DataArray::<ptrs::Projectile>::item_id(
                    actor.as_ptr().cast(),
                )))
            }
        }
    }
}

fn direct_damage_after(
    tag: EffectSourceTag, rule: PlantDamageRule, hp_before: i32, plant_address: i32, requested: i32,
) -> i32 {
    match (tag, rule) {
        (_, PlantDamageRule::Invincible) => hp_before,
        (EffectSourceTag::Basketball, PlantDamageRule::Weak) => hp_before.wrapping_sub(plant_address),
        (EffectSourceTag::Bite, PlantDamageRule::Weak) => 0,
        (_, PlantDamageRule::Normal) => hp_before.wrapping_sub(requested),
    }
}

fn spikerock_outcome(before: i32, after: i32, dead: bool, rule: PlantDamageRule) -> PlantEffectOutcome {
    if dead || after <= 0 {
        PlantEffectOutcome::Killed
    } else if before != after {
        PlantEffectOutcome::HpDelta {
            applied: before.wrapping_sub(after),
        }
    } else if rule == PlantDamageRule::Invincible {
        PlantEffectOutcome::PreventedByRule
    } else {
        PlantEffectOutcome::NoEffect
    }
}

fn process_direct_damage(actor: *mut u8, plant: *mut ptrs::Plant, tag: EffectSourceTag, requested: i32) {
    let (Some(actor), Some(plant_ptr)) = (NonNull::new(actor), NonNull::new(plant)) else {
        return;
    };
    let source = source(tag, actor);
    let begin = begin_effect(
        source,
        plant_ptr,
        PlantEffect::HpDamage {
            native_requested: requested,
        },
    );
    // SAFETY: the verified hook supplied this occupied Plant slot.
    let hp_before = unsafe { ptrs::Plant::health(plant) };
    if begin.decision == EventDecision::SuppressByMeasurement {
        finish(begin.token, PlantEffectOutcome::SuppressedByMeasurement);
        return;
    }
    let rule = active_damage_rule();
    let hp_after = direct_damage_after(tag, rule, hp_before, plant.addr() as i32, requested);
    // SAFETY: the verified hook supplies the live target Plant slot and owns
    // the exact native HP write being replayed.
    unsafe { ptrs::Plant::set_health(plant, hp_after) };
    let outcome = if rule == PlantDamageRule::Invincible {
        PlantEffectOutcome::PreventedByRule
    } else if hp_after <= 0 {
        PlantEffectOutcome::Killed
    } else {
        PlantEffectOutcome::HpDelta {
            applied: hp_before.wrapping_sub(hp_after),
        }
    };
    finish(begin.token, outcome);
}

fn process_jack_plant(plant: *mut ptrs::Plant) {
    let Some(plant_ptr) = NonNull::new(plant) else {
        return;
    };
    let Some(actor) = NonNull::new(JACK_ACTOR.get()) else {
        // SAFETY: fail-open replay of the original verified Plant::Die call.
        unsafe { asm::plant_die(plant) };
        return;
    };
    // SAFETY: JACK_ACTOR is set only around the verified active Jack callsite.
    let source = PlantEffectSource::Jack(ZombieId::from_raw(unsafe {
        ptrs::DataArray::<ptrs::Zombie>::item_id(actor.as_ptr())
    }));
    let begin = begin_effect(source, plant_ptr, PlantEffect::InstantKill);
    let rule = active_damage_rule();
    if begin.decision == EventDecision::SuppressByMeasurement || rule == PlantDamageRule::Invincible {
        undo_plant_eaten_counter();
        finish(
            begin.token,
            if begin.decision == EventDecision::SuppressByMeasurement {
                PlantEffectOutcome::SuppressedByMeasurement
            } else {
                PlantEffectOutcome::PreventedByRule
            },
        );
        return;
    }
    // SAFETY: this replays the exact original Plant::Die call for the live slot.
    unsafe { asm::plant_die(plant) };
    // SAFETY: the native call returned while the same Plant slot remains readable.
    let outcome = if unsafe { ptrs::Plant::is_dead(plant) } {
        PlantEffectOutcome::Killed
    } else {
        PlantEffectOutcome::NoEffect
    };
    finish(begin.token, outcome);
}

fn process_garg_spikerock(zombie: *mut ptrs::Zombie, plant: *mut ptrs::Plant) {
    let (Some(zombie), Some(plant_ptr)) = (NonNull::new(zombie), NonNull::new(plant)) else {
        return;
    };
    // SAFETY: the verified hook supplied this occupied Zombie slot.
    let source = PlantEffectSource::Gargantuar(ZombieId::from_raw(unsafe {
        ptrs::DataArray::<ptrs::Zombie>::item_id(zombie.as_ptr())
    }));
    let begin = begin_effect(source, plant_ptr, PlantEffect::HpDamage { native_requested: 50 });
    // SAFETY: the verified hook supplied this occupied Plant slot.
    let before = unsafe { ptrs::Plant::health(plant) };
    if begin.decision == EventDecision::SuppressByMeasurement {
        finish(begin.token, PlantEffectOutcome::SuppressedByMeasurement);
        return;
    }
    // SAFETY: replays the verified original SpikeRockTakeDamage call.
    unsafe { asm::plant_spikerock_take_damage(plant) };
    // SAFETY: the native call returned while the same Plant slot remains readable.
    let after = unsafe { ptrs::Plant::health(plant) };
    // SAFETY: the native call returned while the same Plant slot remains readable.
    let dead = unsafe { ptrs::Plant::is_dead(plant) };
    let outcome = spikerock_outcome(before, after, dead, active_damage_rule());
    finish(begin.token, outcome);
}

fn process_garg_squish(zombie: *mut ptrs::Zombie, plant: *mut ptrs::Plant, attack_type: i32) {
    let (Some(zombie), Some(plant_ptr)) = (NonNull::new(zombie), NonNull::new(plant)) else {
        return;
    };
    // SAFETY: the verified hook supplied this occupied Zombie slot.
    let id = ZombieId::from_raw(unsafe { ptrs::DataArray::<ptrs::Zombie>::item_id(zombie.as_ptr()) });
    // SAFETY: same occupied Zombie slot; ATTACKTYPE_DRIVE_OVER is 1 at 0x52e958.
    let raw_kind = unsafe { ptrs::Zombie::zombie_type(zombie.as_ptr()) };
    let source = match (attack_type, raw_kind) {
        (1, 12) => Some(PlantEffectSource::ZomboniCrush(id)),
        (1, 22) => Some(PlantEffectSource::CatapultCrush(id)),
        (_, 23 | 32) => Some(PlantEffectSource::Gargantuar(id)),
        _ => None,
    };
    let Some(source) = source else {
        // SAFETY: preserve unrelated SquishAllInSquare callers without inventing a source.
        unsafe { asm::plant_squish(plant) };
        return;
    };
    let begin = begin_effect(source, plant_ptr, PlantEffect::Squish);
    // SAFETY: the verified hook supplied this occupied Plant slot.
    let (before_dead, before_squished, before_state) = unsafe {
        (
            ptrs::Plant::is_dead(plant),
            ptrs::Plant::is_squished(plant),
            ptrs::Plant::state(plant),
        )
    };
    if begin.decision == EventDecision::SuppressByMeasurement {
        undo_plant_eaten_counter();
        finish(begin.token, PlantEffectOutcome::SuppressedByMeasurement);
        return;
    }
    // SAFETY: replays the verified original Plant::Squish call. The active
    // invincible modifier retains its native `ret 4` entry behavior.
    unsafe { asm::plant_squish(plant) };
    // SAFETY: the native call returned while the same Plant slot remains readable.
    let (after_dead, after_squished, after_state) = unsafe {
        (
            ptrs::Plant::is_dead(plant),
            ptrs::Plant::is_squished(plant),
            ptrs::Plant::state(plant),
        )
    };
    // SAFETY: the same slot remains readable; Squish does not change seed type or sleep.
    let activates = unsafe {
        !ptrs::Plant::is_asleep(plant)
            && (matches!(ptrs::Plant::seed_type(plant), 2 | 20 | 15 | 14)
                || (ptrs::Plant::seed_type(plant) == 4 && before_state != 0))
    } && active_damage_rule() != PlantDamageRule::Invincible;
    let outcome = if activates && !before_dead && (after_dead || before_state != after_state) {
        PlantEffectOutcome::Activated
    } else if !before_dead && after_dead {
        PlantEffectOutcome::Killed
    } else if !before_squished && after_squished {
        PlantEffectOutcome::Squished
    } else if before_state != after_state {
        PlantEffectOutcome::Activated
    } else if active_damage_rule() == PlantDamageRule::Invincible {
        undo_plant_eaten_counter();
        PlantEffectOutcome::PreventedByRule
    } else {
        PlantEffectOutcome::NoEffect
    };
    finish(begin.token, outcome);
}

fn undo_plant_eaten_counter() {
    let Ok(board) = crate::runtime::backend::current_board() else {
        return;
    };
    // SAFETY: current_board returned a live Board; the hook runs immediately
    // after the native increment that is being suppressed.
    unsafe {
        let counter = ptrs::Board::plants_eaten_mut(board.as_ptr());
        *counter = (*counter).saturating_sub(1);
    }
}

fn begin_effect(source: PlantEffectSource, plant: NonNull<ptrs::Plant>, effect: PlantEffect) -> BeginPlantEffect {
    let Some(sink) = sink() else {
        return BeginPlantEffect::FAIL_OPEN;
    };
    if !sink.interest.contains(source.interest()) {
        return BeginPlantEffect::FAIL_OPEN;
    }
    // SAFETY: every physical hook supplies a live occupied Plant DataArray slot.
    let (raw_kind, plant_id, row, col, hp_before, max_hp) = unsafe {
        (
            ptrs::Plant::seed_type(plant.as_ptr()),
            ptrs::DataArray::<ptrs::Plant>::item_id(plant.as_ptr()),
            ptrs::Plant::row(plant.as_ptr()),
            ptrs::Plant::col(plant.as_ptr()),
            ptrs::Plant::health(plant.as_ptr()),
            ptrs::Plant::max_health(plant.as_ptr()),
        )
    };
    let Ok(raw_kind) = crate::raw::kind::plant_kind_from_raw(raw_kind) else {
        return BeginPlantEffect::FAIL_OPEN;
    };
    let Ok(effective_kind) = crate::ops::plant::effective_seed_type(plant) else {
        return BeginPlantEffect::FAIL_OPEN;
    };
    (sink.begin_plant_effect)(PlantEffectAttemptFact {
        source,
        plant_id: PlantId::from_raw(plant_id),
        raw_kind,
        effective_kind: effective_kind.to_core(),
        grid: Grid { row, col },
        hp_before,
        max_hp,
        effect,
        main_counter: current_main_counter(),
    })
}

fn finish(token: EventToken, outcome: PlantEffectOutcome) {
    if token.is_valid()
        && let Some(sink) = sink()
    {
        (sink.finish_plant_effect)(token, outcome);
    }
}

fn emit_home_entry(zombie: *mut ptrs::Zombie) {
    let (Some(sink), Some(zombie)) = (sink(), NonNull::new(zombie)) else {
        return;
    };
    // SAFETY: the verified hook supplied this occupied Zombie DataArray slot.
    let (raw_kind, zombie_id, row) = unsafe {
        (
            ptrs::Zombie::zombie_type(zombie.as_ptr()),
            ptrs::DataArray::<ptrs::Zombie>::item_id(zombie.as_ptr()),
            ptrs::Zombie::row(zombie.as_ptr()),
        )
    };
    let Ok(zombie_kind) = crate::raw::kind::zombie_kind_from_raw(raw_kind) else {
        return;
    };
    (sink.emit_home_entry)(HomeEntryFact {
        zombie_id: ZombieId::from_raw(zombie_id),
        zombie_kind,
        row,
        main_counter: current_main_counter(),
    });
}

fn emit_gargantuar_allocated(zombie: *mut ptrs::Zombie, from_wave: i32, raw_kind: i32, row: i32) {
    if !FRAME_OPEN.get() {
        return;
    }
    let (Some(sink), Some(zombie)) = (sink(), NonNull::new(zombie)) else {
        return;
    };
    let Ok(parent_kind) = crate::raw::kind::zombie_kind_from_raw(raw_kind) else {
        return;
    };
    if !matches!(
        parent_kind,
        rsvz_model::ZombieKind::Gargantuar | rsvz_model::ZombieKind::GigaGargantuar
    ) {
        return;
    }
    let parent_id = unsafe { ptrs::DataArray::<ptrs::Zombie>::item_id(zombie.as_ptr()) };
    (sink.emit_gargantuar_spawned)(GargantuarSpawnedFact {
        parent_id: ZombieId::from_raw(parent_id),
        parent_kind,
        from_wave,
        row,
        main_counter: FRAME_COUNTER.get(),
    });
}

fn emit_imp_thrown(parent: *mut ptrs::Zombie, imp: *mut ptrs::Zombie) {
    if !FRAME_OPEN.get() {
        return;
    }
    let (Some(sink), Some(parent), Some(imp)) = (sink(), NonNull::new(parent), NonNull::new(imp)) else {
        return;
    };
    let raw_kind = unsafe { ptrs::Zombie::zombie_type(parent.as_ptr()) };
    let raw_phase = unsafe { ptrs::Zombie::phase(parent.as_ptr()) };
    let (Ok(parent_kind), Ok(parent_phase)) = (
        crate::raw::kind::zombie_kind_from_raw(raw_kind),
        rsvz_model::ZombiePhase::try_from_code(raw_phase),
    ) else {
        return;
    };
    (sink.emit_imp_thrown)(ImpThrownFact {
        parent_id: ZombieId::from_raw(unsafe { ptrs::DataArray::<ptrs::Zombie>::item_id(parent.as_ptr()) }),
        imp_id: ZombieId::from_raw(unsafe { ptrs::DataArray::<ptrs::Zombie>::item_id(imp.as_ptr()) }),
        parent_kind,
        from_wave: unsafe { ptrs::Zombie::from_wave(parent.as_ptr()) },
        row: unsafe { ptrs::Zombie::row(parent.as_ptr()) },
        main_counter: current_main_counter(),
        parent_x: unsafe { ptrs::Zombie::pos_x(parent.as_ptr()) },
        parent_hp: unsafe { ptrs::Zombie::body_health(parent.as_ptr()) },
        parent_max_hp: unsafe { ptrs::Zombie::body_max_health(parent.as_ptr()) },
        parent_phase,
        parent_speed_x: unsafe { ptrs::Zombie::speed_x(parent.as_ptr()) },
        parent_frozen: unsafe { ptrs::Zombie::frozen_countdown(parent.as_ptr()) },
        parent_chilled: unsafe { ptrs::Zombie::chilled_countdown(parent.as_ptr()) },
        parent_buttered: unsafe { ptrs::Zombie::buttered_countdown(parent.as_ptr()) },
    });
}

fn emit_gargantuar_ash_hit(zombie: *mut ptrs::Zombie) {
    if !FRAME_OPEN.get() {
        return;
    }
    let (Some(sink), Some(zombie)) = (sink(), NonNull::new(zombie)) else {
        return;
    };
    let raw_kind = unsafe { ptrs::Zombie::zombie_type(zombie.as_ptr()) };
    let raw_phase = unsafe { ptrs::Zombie::phase(zombie.as_ptr()) };
    let (Ok(parent_kind), Ok(phase)) = (
        crate::raw::kind::zombie_kind_from_raw(raw_kind),
        rsvz_model::ZombiePhase::try_from_code(raw_phase),
    ) else {
        return;
    };
    if !matches!(
        parent_kind,
        rsvz_model::ZombieKind::Gargantuar | rsvz_model::ZombieKind::GigaGargantuar
    ) {
        return;
    }
    (sink.emit_gargantuar_ash_hit)(GargantuarAshHitFact {
        parent_id: ZombieId::from_raw(unsafe { ptrs::DataArray::<ptrs::Zombie>::item_id(zombie.as_ptr()) }),
        main_counter: current_main_counter(),
        x: unsafe { ptrs::Zombie::pos_x(zombie.as_ptr()) },
        hp_before: unsafe { ptrs::Zombie::body_health(zombie.as_ptr()) },
        phase,
        frozen: unsafe { ptrs::Zombie::frozen_countdown(zombie.as_ptr()) },
        chilled: unsafe { ptrs::Zombie::chilled_countdown(zombie.as_ptr()) },
        buttered: unsafe { ptrs::Zombie::buttered_countdown(zombie.as_ptr()) },
    });
}

fn current_main_counter() -> i32 {
    let Ok(board) = crate::runtime::backend::current_board() else {
        return 0;
    };
    // SAFETY: current_board returned the live current Board.
    unsafe { ptrs::Board::clock(board.as_ptr()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_rule_is_absent_without_event_hook() {
        assert!(ACTIVE.with_borrow(Option::is_none));
    }

    #[test]
    fn board_epoch_wraps_without_allocating_state() {
        let before = BOARD_EPOCH.get();
        TERMINAL_LATCHED.set(true);
        note_world_reset();
        assert_eq!(BOARD_EPOCH.get(), before.wrapping_add(1));
        assert!(!TERMINAL_LATCHED.get());
    }

    #[test]
    fn only_terminal_frame_status_latches_the_event_stream() {
        for status in [BattleStatus::Running, BattleStatus::Unknown] {
            TERMINAL_LATCHED.set(false);
            latch_terminal_status(status);
            assert!(!TERMINAL_LATCHED.get());
        }
        for status in [BattleStatus::ObjectiveReached, BattleStatus::Lost, BattleStatus::Ended] {
            TERMINAL_LATCHED.set(false);
            latch_terminal_status(status);
            assert!(TERMINAL_LATCHED.get());
        }
        TERMINAL_LATCHED.set(false);
    }

    #[test]
    fn jack_actor_scope_restores_on_unwind() {
        let mut original_slot = std::mem::MaybeUninit::<ptrs::Zombie>::uninit();
        let mut nested_slot = std::mem::MaybeUninit::<ptrs::Zombie>::uninit();
        let original = original_slot.as_mut_ptr();
        let nested = nested_slot.as_mut_ptr();
        JACK_ACTOR.set(original);
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _actor = JackActorGuard::enter(nested);
            assert_eq!(JACK_ACTOR.get(), nested);
            panic!("scope probe");
        }));
        assert!(result.is_err());
        assert_eq!(JACK_ACTOR.replace(ptr::null_mut()), original);
    }

    #[test]
    fn weak_basketball_replays_the_native_negative_hp_write() {
        assert_eq!(
            direct_damage_after(EffectSourceTag::Basketball, PlantDamageRule::Weak, 300, 0x1_000, 75,),
            -3_796,
        );
        assert_eq!(
            direct_damage_after(EffectSourceTag::Bite, PlantDamageRule::Weak, 300, 0x1_000, 4),
            0,
        );
    }

    #[test]
    fn terminal_spikerock_damage_is_killed_before_hp_delta() {
        assert_eq!(
            spikerock_outcome(50, 0, false, PlantDamageRule::Normal),
            PlantEffectOutcome::Killed,
        );
        assert_eq!(
            spikerock_outcome(100, 50, false, PlantDamageRule::Normal),
            PlantEffectOutcome::HpDelta { applied: 50 },
        );
    }
}
