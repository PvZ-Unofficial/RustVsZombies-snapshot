//! Backend-neutral intelligent C9 fodder prediction.

use std::ops::RangeInclusive;

use super::solver::align_bite_check;
use super::{ExplosionPmf, FodderBehavior, FodderMorph, JackBlastEvent, JackDistributionInput, ReleaseTailKind};
use super::{
    FodderContactKind, OrderedExplosionPmf, SmartFodderChoice, SmartFodderDomain, SmartFodderModel, SmartFodderThreat,
    SmartFodderTimes, smart_fodder_tables,
};
use crate::logic::card_timing::IMITATOR_MORPH_DELAY;
use crate::logic::zombie_geometry::zombie_profile_attack_rect;
use crate::logic::{
    PLANT_THREAT_HORIZONTAL_OVERLAP, circle_hits_rect, native_horizontal_rect_overlap, pvz_trunc_f32_to_i32,
    vanilla_track_frames,
};
use crate::logic::{predict_stable_zombie_x_trace, unopened_jack_motion_state, zombie_motion_state};
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{GridGeometryBackend, PlantReadBackend, SceneBackend, ZombieRawFactsBackend, ZombieReadBackend};
use rsvz_model::CardSelection;
use rsvz_model::{
    AbsoluteContactRange, ContactCircle, ContactRect, PlantKind, ZombieKind, ZombieMovementModel, ZombiePhase,
};
use rsvz_model::{SceneKind, ZombieContactProfile, ZombieId};

const SLOWED_GARGANTUAR_SMASH_IMPACT: i32 = 266;
const SMART_FODDER_LATEST_ACTIVATION: i32 = 1_800;

/// Inputs for one no-shovel C9 fodder decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartFodderSpec {
    pub card: CardSelection,
    pub row: i32,
    pub plant_window: RangeInclusive<i32>,
    pub remove_by: Option<i32>,
    pub activation_at: i32,
}

/// Visible history retained by the scripting wrapper for a pre-plant replan.
///
/// The field stays private because this value is only intended to be passed
/// back to a later replan of the same scheduled action.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SmartFodderReplanContext {
    ice_effect_at: Option<i32>,
}

const fn is_modeled_kind(kind: ZombieKind) -> bool {
    matches!(
        kind,
        ZombieKind::Normal
            | ZombieKind::Flag
            | ZombieKind::Conehead
            | ZombieKind::Buckethead
            | ZombieKind::Newspaper
            | ZombieKind::ScreenDoor
            | ZombieKind::Yeti
            | ZombieKind::Ladder
            | ZombieKind::Football
            | ZombieKind::JackInTheBox
            | ZombieKind::PoleVaulting
            | ZombieKind::Catapult
            | ZombieKind::Gargantuar
            | ZombieKind::GigaGargantuar
    )
}

fn is_modeled_same_row_state(kind: ZombieKind, phase: ZombiePhase) -> bool {
    match kind {
        ZombieKind::JackInTheBox => matches!(
            phase,
            ZombiePhase::JackInTheBoxRunning | ZombiePhase::JackInTheBoxPopping
        ),
        ZombieKind::PoleVaulting => phase == ZombiePhase::PolevaulterPreVault,
        ZombieKind::Catapult => phase == ZombiePhase::ZombieNormal,
        ZombieKind::Gargantuar | ZombieKind::GigaGargantuar => phase == ZombiePhase::ZombieNormal,
        _ => is_modeled_kind(kind),
    }
}

const fn release_tail_kind(kind: ZombieKind) -> Option<ReleaseTailKind> {
    match kind {
        ZombieKind::Ladder => Some(ReleaseTailKind::Ladder),
        ZombieKind::Football => Some(ReleaseTailKind::Football),
        ZombieKind::JackInTheBox => Some(ReleaseTailKind::JackNoPop),
        _ => None,
    }
}

fn ordinary_cannon_damage_enabled(kind: ZombieKind, same_row_cannon: bool) -> bool {
    same_row_cannon && (release_tail_kind(kind).is_some() || kind == ZombieKind::PoleVaulting)
}

/// Evaluates a smart-fodder specification against the current visible board
/// without registering plant or removal operations.
pub fn predict_smart_fodder_at(spec: &SmartFodderSpec, now: i32) -> RuntimeResult<SmartFodderChoice>
where
    rsvz_current::CurrentBackend: crate::logic::ContactGeometryBackend + SceneBackend,
{
    predict_smart_fodder_with_context_at(spec, now, None).map(|(choice, _context)| choice)
}

/// Re-evaluates a decision while retaining only visible history recovered by
/// an earlier successful call.
#[doc(hidden)]
pub fn predict_smart_fodder_with_context_at(
    spec: &SmartFodderSpec, now: i32, context: Option<SmartFodderReplanContext>,
) -> RuntimeResult<(SmartFodderChoice, SmartFodderReplanContext)>
where
    rsvz_current::CurrentBackend: crate::logic::ContactGeometryBackend + SceneBackend,
{
    validate_card(spec.card)?;
    if !(1..=6).contains(&spec.row) {
        return Err(RuntimeError::new("smart fodder row must be in 1..=6"));
    }
    ensure_all_zombies_thawed()?;
    let domain = SmartFodderDomain::validate(SmartFodderTimes {
        now,
        plant_window: spec.plant_window.clone(),
        remove_by: spec.remove_by,
        activation_at: spec.activation_at,
    })
    .map_err(runtime_error)?;
    let (model, ice_effect_at) = build_model(
        domain,
        spec.card,
        spec.row,
        now,
        context.and_then(|context| context.ice_effect_at),
    )?;
    let tables = smart_fodder_tables().map_err(runtime_error)?;
    let choice = model.solve(tables).map_err(runtime_error)?;
    Ok((choice, SmartFodderReplanContext { ice_effect_at }))
}

/// Predicts a fixed C9 removal deadline from the first matching Gargantuar contact.
pub fn predict_c9_remove_by_at(row: i32, from_wave: i32, now: i32) -> RuntimeResult<Option<i32>>
where
    rsvz_current::CurrentBackend: crate::logic::ContactGeometryBackend + SceneBackend,
{
    crate::access::with_backend(|backend| {
        let target_row = row.saturating_sub(1);
        let scene = crate::live_value::read_or_abort(backend.scene(), "scene");
        if row <= 0 || usize::try_from(target_row).map_or(true, |row| row >= scene.row_count()) {
            return Err(RuntimeError::new("C9 remove-by row is outside the current lawn"));
        }
        let horizon = u32::try_from(SMART_FODDER_LATEST_ACTIVATION.saturating_sub(now))
            .map_err(|_error| RuntimeError::new("C9 remove-by prediction time is outside the supported window"))?;
        let fodder_x = crate::live_value::read_or_abort(backend.grid_to_pixel_x(8, target_row), "grid_to_pixel_x");
        let fodder_y = crate::live_value::read_or_abort(backend.grid_to_pixel_y(8, target_row), "grid_to_pixel_y");
        let fodder_rect = ContactRect::new(fodder_x + 10, fodder_y, fodder_x + 70, fodder_y + 80);
        let mut earliest = None;

        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            if !backend.zombie_is_alive(zombie)
                || backend.zombie_is_disappeared(zombie)
                || backend.zombie_is_mind_controlled(zombie)
                || backend.zombie_from_wave(zombie) != from_wave
                || backend.zombie_row(zombie) != target_row
            {
                continue;
            }
            let kind = crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind");
            if !matches!(kind, ZombieKind::Gargantuar | ZombieKind::GigaGargantuar)
                || crate::live_value::read_or_abort(backend.zombie_phase(zombie), "zombie_phase")
                    != ZombiePhase::ZombieNormal
            {
                continue;
            }
            let id = backend.zombie_id(zombie);
            let motion = zombie_motion_state(id)
                .ok_or_else(|| RuntimeError::new("current-wave Gargantuar motion state vanished"))?;
            let trace = predict_stable_zombie_x_trace(&motion, horizon).map_err(runtime_error)?;
            let attack_offsets = zombie_attack_offsets(id)?;
            if let Some((contact, _last)) = contact_window(now, &trace, attack_offsets, fodder_rect)? {
                earliest = Some(earliest.map_or(contact, |known: i32| known.min(contact)));
            }
        }
        Ok(earliest)
    })
}

fn ensure_all_zombies_thawed() -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: rsvz_backend_api::backend::ZombieRawFactsBackend,
{
    crate::access::with_backend(|backend| {
        for zombie in crate::live_value::read_or_abort(backend.zombies(), "zombies") {
            if !backend.zombie_is_alive(zombie)
                || backend.zombie_is_disappeared(zombie)
                || backend.zombie_is_mind_controlled(zombie)
            {
                continue;
            }
            let kind = crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind");
            if is_modeled_kind(kind) && backend.zombie_frozen_countdown(zombie) != 0 {
                return Err(RuntimeError::new(
                    "smart fodder can only be called after every modeled live zombie has thawed",
                ));
            }
        }
        Ok(())
    })
}

fn build_model(
    domain: SmartFodderDomain, selection: CardSelection, target_row: i32, now: i32,
    inherited_ice_effect_at: Option<i32>,
) -> RuntimeResult<(SmartFodderModel, Option<i32>)>
where
    rsvz_current::CurrentBackend: crate::logic::ContactGeometryBackend + SceneBackend,
{
    crate::access::with_backend(|backend| {
        let target_row = target_row - 1;
        let scene = crate::live_value::read_or_abort(backend.scene(), "scene");
        if usize::try_from(target_row).map_or(true, |row| row >= scene.row_count()) {
            return Err(RuntimeError::new("smart fodder row is outside the current lawn"));
        }
        if matches!(scene, SceneKind::Pool | SceneKind::Fog) && matches!(target_row, 2 | 3) {
            return Err(RuntimeError::new(
                "smart-fodder v1 only supports land rows and will not auto-create a lily pad",
            ));
        }
        let horizon = u32::try_from(domain.activation_at.saturating_sub(now))
            .map_err(|_error| RuntimeError::new("smart-fodder prediction horizon is invalid"))?;
        let mut cannons = Vec::<(i32, ContactRect)>::new();
        let mut occupied_c9_rows = vec![false; scene.row_count()];
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            let grid = crate::plant::grid_from_handle(backend, plant);
            if backend.plant_is_alive(plant) && grid.col == 8 {
                if let Some(occupied) = usize::try_from(grid.row)
                    .ok()
                    .and_then(|row| occupied_c9_rows.get_mut(row))
                {
                    *occupied = true;
                }
                if grid.row == target_row {
                    return Err(RuntimeError::new(
                        "smart-fodder v1 requires the target C9 cell to be empty at the call time",
                    ));
                }
            }
            if backend.plant_is_alive(plant)
                && crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") == PlantKind::CobCannon
                && grid.col == 6
                && let Some(contact) =
                    crate::logic::contact::plant_contact_rect(backend.plant_id(plant)).map_err(runtime_error)?
            {
                cannons.push((grid.row, contact.rect));
            }
        }
        let same_row_cannon = cannons.iter().any(|(row, _rect)| *row == target_row);
        let fodder_x = crate::live_value::read_or_abort(backend.grid_to_pixel_x(8, target_row), "grid_to_pixel_x");
        let fodder_y = crate::live_value::read_or_abort(backend.grid_to_pixel_y(8, target_row), "grid_to_pixel_y");
        let fodder_rect = ContactRect::new(fodder_x + 10, fodder_y, fodder_x + 70, fodder_y + 80);
        let mut threats = Vec::new();
        let mut deterministic_explosions = Vec::new();
        let mut deterministic_jack_blast_damage = 0.0;
        let mut ice_effect_at = inherited_ice_effect_at;
        let zombie_ids = crate::live_value::read_or_abort(backend.zombies(), "zombies")
            .filter(|zombie| {
                backend.zombie_is_alive(*zombie)
                    && !backend.zombie_is_disappeared(*zombie)
                    && !backend.zombie_is_mind_controlled(*zombie)
            })
            .map(|zombie| backend.zombie_id(zombie))
            .collect::<Vec<_>>();
        for id in &zombie_ids {
            let zombie = crate::live_value::read_or_abort(backend.zombie(*id), "zombie")
                .ok_or_else(|| RuntimeError::new("live zombie vanished while recovering the ice history"))?;
            if crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind") != ZombieKind::JackInTheBox
                || crate::live_value::read_or_abort(backend.zombie_phase(zombie), "zombie_phase")
                    != ZombiePhase::JackInTheBoxRunning
            {
                continue;
            }
            let chilled = backend.zombie_chilled_countdown(zombie);
            if chilled > 1_000 {
                let recovered = now.saturating_sub(2_000_i32.saturating_sub(chilled));
                if ice_effect_at.is_some_and(|known| known != recovered) {
                    return Err(RuntimeError::new(
                        "live zombies imply inconsistent ice-effect histories",
                    ));
                }
                ice_effect_at = Some(recovered);
            }
        }

        for (update_rank, id) in zombie_ids.into_iter().enumerate() {
            let zombie = crate::live_value::read_or_abort(backend.zombie(id), "zombie")
                .ok_or_else(|| RuntimeError::new("live zombie vanished during smart-fodder sampling"))?;
            if !backend.zombie_is_alive(zombie) || backend.zombie_is_disappeared(zombie) {
                continue;
            }
            let kind = crate::live_value::read_or_abort(backend.zombie_kind(zombie), "zombie_kind");
            if !is_modeled_kind(kind) {
                continue;
            }
            let phase = crate::live_value::read_or_abort(backend.zombie_phase(zombie), "zombie_phase");
            let zombie_age = backend.zombie_age(zombie);
            let bite_check_residue = release_tail_kind(kind).map(|_| now.saturating_sub(zombie_age).rem_euclid(8));
            let row = backend.zombie_row(zombie);

            let running_jack = kind == ZombieKind::JackInTheBox && phase == ZombiePhase::JackInTheBoxRunning;
            let popping_jack = kind == ZombieKind::JackInTheBox && phase == ZombiePhase::JackInTheBoxPopping;
            if popping_jack {
                let explosion_at = now.saturating_add(backend.zombie_phase_counter(zombie).max(0));
                if explosion_at < domain.activation_at {
                    let center =
                        ContactCircle::new(backend.zombie_int_x(zombie) + 60, backend.zombie_int_y(zombie) + 60, 90);
                    deterministic_jack_blast_damage += 300.0
                        * cannons
                            .iter()
                            .filter(|(_row, cannon)| circle_hits_rect(center, *cannon))
                            .count() as f64;
                    if circle_hits_rect(center, fodder_rect) {
                        deterministic_explosions.push(OrderedExplosionPmf {
                            pmf: ExplosionPmf::from_probabilities(explosion_at, vec![1.0])
                                .ok_or_else(|| RuntimeError::new("invalid deterministic jack explosion"))?,
                            update_rank,
                        });
                    }
                }
                continue;
            }
            if (row != target_row && !running_jack) || (row == target_row && !is_modeled_same_row_state(kind, phase)) {
                continue;
            }

            let motion = if running_jack {
                unopened_jack_motion_state(id)
            } else {
                zombie_motion_state(id)
            }
            .ok_or_else(|| RuntimeError::new("live zombie motion state vanished during smart-fodder sampling"))?;
            let trace = predict_stable_zombie_x_trace(&motion, horizon).map_err(runtime_error)?;
            if backend.zombie_chilled_countdown(zombie) < domain.activation_at.saturating_sub(now) {
                return Err(RuntimeError::new(format!(
                    "zombie {kind:?} is modeled through activation, but its slow expires first",
                )));
            }
            let (attack_offsets, fodder_contact_window) = if row == target_row {
                let offsets = zombie_attack_offsets(id)?;
                (Some(offsets), contact_window(now, &trace, offsets, fodder_rect)?)
            } else {
                (None, None)
            };

            let mut jack_pmf = if running_jack {
                let chilled = backend.zombie_chilled_countdown(zombie);
                if ice_effect_at.is_none() && (1..=1_000).contains(&chilled) {
                    return Err(RuntimeError::new(
                        "an unopened jack is slowed, but its ice history cannot be distinguished from an ordinary slow",
                    ));
                }
                let input = JackDistributionInput {
                    spawned_at: now.saturating_sub(zombie_age),
                    observed_at: now,
                    // Native freeze/thaw only changes counters and animation rate,
                    // not mVelX. Per the v1 contract, this visible current value is
                    // used as the unavailable birth-speed proxy even if an earlier
                    // StopEating transition rerolled it.
                    birth_speed: f64::from(backend.zombie_speed_x(zombie)),
                    first_ice_at: ice_effect_at,
                    first_freeze_in_pool: backend.zombie_is_in_pool(zombie),
                };
                let mut compatible_freezes = compatible_first_freezes(&motion, input)?;
                // While a left-walking Jack is still to the right of the first
                // possible C9 contact, no lawn plant can have stopped it: C9 is
                // the rightmost cell. Its visible displacement can therefore
                // narrow the hidden first-freeze duration using native spawn-x
                // support. After this boundary, past eating is not recoverable
                // from the current board and position is no longer safe evidence.
                let jack_row_c9_is_empty = usize::try_from(row)
                    .ok()
                    .and_then(|row| occupied_c9_rows.get(row))
                    .is_some_and(|occupied| !occupied);
                let before_first_c9_contact = jack_row_c9_is_empty && {
                    let jack_row_c9_right =
                        crate::live_value::read_or_abort(backend.grid_to_pixel_x(8, row), "grid_to_pixel_x")
                            .saturating_add(70);
                    let first_contact_x = jack_row_c9_right
                        .saturating_sub(PLANT_THREAT_HORIZONTAL_OVERLAP as i32)
                        .saturating_sub(running_jack_attack_offsets()?.0);
                    motion.x > first_contact_x as f32
                };
                if before_first_c9_contact && let Some(durations) = &mut compatible_freezes {
                    let spawn_shift = if matches!(backend.zombie_from_wave(zombie), 9 | 19) {
                        40.0
                    } else {
                        0.0
                    };
                    let filtered = position_compatible_first_freezes(&motion, input, durations, spawn_shift);
                    // Rounded vanilla ground tracks and backend-created test
                    // zombies can disagree with native spawn support. Position is
                    // extra evidence, never a new rejection condition.
                    if !filtered.is_empty() {
                        *durations = filtered;
                    }
                }
                Some(
                    ExplosionPmf::conditional_unopened_with_freezes(input, compatible_freezes.as_deref())
                        .map_err(runtime_error)?,
                )
            } else {
                None
            };

            let mut jack_blast_events = Vec::new();
            if let Some(pmf) = &jack_pmf {
                jack_blast_events = direct_blast_for_jack(
                    now,
                    domain.activation_at,
                    &trace,
                    backend.zombie_int_y(zombie),
                    pmf,
                    &cannons,
                    fodder_rect,
                );
            }

            if row != target_row {
                if jack_pmf.is_some() {
                    deterministic_jack_blast_damage += fold_off_row_jack_baseline(&mut jack_blast_events);
                    let Some(hit_pmf) = off_row_fodder_hit_pmf(&jack_blast_events) else {
                        continue;
                    };
                    jack_pmf = Some(hit_pmf);
                    jack_blast_events.retain(|event| event.hits_fodder_without_block);
                    threats.push(SmartFodderThreat {
                        update_rank,
                        trace_start: now,
                        first_contact_at: None,
                        last_contact_at: None,
                        x_trace: trace,
                        baseline_damage: 0.0,
                        baseline_contact_at: None,
                        bite_check_residue,
                        ordinary_damage_enabled: false,
                        release_kind: None,
                        jack_pmf,
                        pole_vault: false,
                        fodder_contact_kind: FodderContactKind::Bite,
                        can_contact_fodder: false,
                        jack_blast_events,
                        jack_geometry_counts: jack_geometry_counts(&cannons, row, scene),
                    });
                }
                continue;
            }
            let release_kind = release_tail_kind(kind);
            let (baseline_damage, baseline_contact_at) = if release_kind.is_some()
                && let Some((_row, cannon)) = cannons.iter().find(|(cannon_row, _rect)| *cannon_row == row)
            {
                baseline_bite_damage(
                    now,
                    domain.activation_at,
                    &trace,
                    attack_offsets.ok_or_else(|| RuntimeError::new("missing same-row attack geometry"))?,
                    *cannon,
                    bite_check_residue.ok_or_else(|| RuntimeError::new("missing fast-zombie bite-check phase"))?,
                )?
            } else {
                (0.0, None)
            };
            let (first_contact_at, last_contact_at) = fodder_contact_window.unzip();
            let fodder_contact_kind = if kind == ZombieKind::Catapult {
                FodderContactKind::Crush
            } else if matches!(kind, ZombieKind::Gargantuar | ZombieKind::GigaGargantuar) {
                FodderContactKind::Smash {
                    impact_after: SLOWED_GARGANTUAR_SMASH_IMPACT,
                }
            } else {
                FodderContactKind::Bite
            };
            threats.push(SmartFodderThreat {
                update_rank,
                trace_start: now,
                first_contact_at,
                last_contact_at,
                x_trace: trace,
                baseline_damage,
                baseline_contact_at,
                bite_check_residue,
                ordinary_damage_enabled: ordinary_cannon_damage_enabled(kind, same_row_cannon),
                release_kind,
                jack_pmf,
                pole_vault: kind == ZombieKind::PoleVaulting,
                fodder_contact_kind,
                can_contact_fodder: true,
                jack_blast_events,
                jack_geometry_counts: jack_geometry_counts(&cannons, row, scene),
            });
        }

        let (fodder_hp, fodder_behavior, fodder_morph, fodder_invulnerable_for) = fodder_health(selection);
        Ok((
            SmartFodderModel {
                domain,
                fodder_hp,
                fodder_behavior,
                fodder_morph,
                fodder_invulnerable_for,
                threats,
                deterministic_explosions,
                deterministic_jack_blast_damage,
            },
            ice_effect_at,
        ))
    })
}

fn fold_off_row_jack_baseline(events: &mut [JackBlastEvent]) -> f64 {
    300.0
        * events
            .iter_mut()
            .map(|event| {
                let damage = event.probability * event.baseline_hits;
                event.baseline_hits = 0.0;
                damage
            })
            .sum::<f64>()
}

fn off_row_fodder_hit_pmf(events: &[JackBlastEvent]) -> Option<ExplosionPmf> {
    let start = events
        .iter()
        .filter(|event| event.hits_fodder_without_block && event.probability > 0.0)
        .map(|event| event.explosion_at)
        .min()?;
    let end = events
        .iter()
        .filter(|event| event.hits_fodder_without_block && event.probability > 0.0)
        .map(|event| event.explosion_at)
        .max()?;
    let mut probabilities = vec![0.0; usize::try_from(end - start + 1).ok()?];
    for event in events
        .iter()
        .filter(|event| event.hits_fodder_without_block && event.probability > 0.0)
    {
        probabilities[usize::try_from(event.explosion_at - start).ok()?] += event.probability;
    }
    ExplosionPmf::from_subprobabilities(start, probabilities)
}

fn compatible_first_freezes(
    motion: &rsvz_model::ZombieMotionState, input: JackDistributionInput,
) -> RuntimeResult<Option<Vec<i32>>> {
    let Some(ice_at) = input.first_ice_at.filter(|ice_at| *ice_at >= input.spawned_at) else {
        return Ok(None);
    };
    if input.first_freeze_in_pool {
        let elapsed_after_ice = input.observed_at.saturating_sub(ice_at);
        if elapsed_after_ice < 300 {
            return Err(RuntimeError::new(
                "the visible thawed pool jack is incompatible with the recovered ice time",
            ));
        }
        return Ok(Some(vec![300]));
    }
    let elapsed_after_ice = input.observed_at.saturating_sub(ice_at);
    let maximum = elapsed_after_ice.min(600);
    if maximum < 400 {
        return Err(RuntimeError::new(
            "the visible thawed jack is incompatible with the recovered ice time",
        ));
    }
    let mut compatible = (400..=maximum).collect::<Vec<_>>();
    if let ZombieMovementModel::Track {
        profile,
        progress,
        playback_rate_fps,
        ..
    } = motion.model
    {
        let frame_count = vanilla_track_frames(profile).len() as f32;
        let step = playback_rate_fps * 0.01 / frame_count;
        let active_before_ice = ice_at.saturating_sub(input.spawned_at) as f32;
        let tolerance = step.abs() * 2.5 + 1.0e-5;
        compatible.retain(|duration| {
            let active_after_ice = elapsed_after_ice.saturating_sub(*duration) as f32;
            let predicted = (active_before_ice * step + active_after_ice * step * 0.5).rem_euclid(1.0);
            let distance = (predicted - progress).abs();
            distance.min(1.0 - distance) <= tolerance
        });
    }
    if compatible.is_empty() {
        return Err(RuntimeError::new(
            "the unopened jack animation is incompatible with every legal first-freeze duration",
        ));
    }
    Ok(Some(compatible))
}

fn position_compatible_first_freezes(
    motion: &rsvz_model::ZombieMotionState, input: JackDistributionInput, durations: &[i32], spawn_shift: f32,
) -> Vec<i32> {
    const POSITION_TOLERANCE: f32 = 3.0;
    let spawn_min = 780.0 + spawn_shift - POSITION_TOLERANCE;
    let spawn_max = 819.0 + spawn_shift + POSITION_TOLERANCE;
    let ZombieMovementModel::Track {
        profile,
        playback_rate_fps,
        ..
    } = motion.model
    else {
        return Vec::new();
    };
    let Some(ice_at) = input.first_ice_at else {
        return Vec::new();
    };
    let frames = vanilla_track_frames(profile);
    if frames.len() < 2 || !playback_rate_fps.is_finite() || playback_rate_fps <= 0.0 {
        return Vec::new();
    }

    let mut progress = (playback_rate_fps * 0.01 / frames.len() as f32).rem_euclid(1.0);
    let mut displacement = 0.0;
    if !advance_jack_track(
        frames,
        ice_at.saturating_sub(input.spawned_at).max(0),
        playback_rate_fps,
        motion.scale,
        &mut progress,
        &mut displacement,
    ) {
        return Vec::new();
    }

    let elapsed_after_ice = input.observed_at.saturating_sub(ice_at);
    let mut advanced = 0;
    let mut compatible = Vec::with_capacity(durations.len());
    for duration in durations.iter().rev().copied() {
        let active_after_ice = elapsed_after_ice.saturating_sub(duration).max(0);
        if !advance_jack_track(
            frames,
            active_after_ice - advanced,
            playback_rate_fps * 0.5,
            motion.scale,
            &mut progress,
            &mut displacement,
        ) {
            return Vec::new();
        }
        advanced = active_after_ice;
        let spawn_x = motion.x + displacement;
        if spawn_x.is_finite() && (spawn_min..=spawn_max).contains(&spawn_x) {
            compatible.push(duration);
        }
    }
    compatible.reverse();
    compatible
}

#[cfg(test)]
fn inferred_jack_spawn_x(
    motion: &rsvz_model::ZombieMotionState, input: JackDistributionInput, freeze_duration: i32,
) -> Option<f32> {
    let ZombieMovementModel::Track {
        profile,
        playback_rate_fps,
        ..
    } = motion.model
    else {
        return None;
    };
    let ice_at = input.first_ice_at?;
    let frames = vanilla_track_frames(profile);
    let frame_count = frames.len();
    if frame_count < 2 || !playback_rate_fps.is_finite() || playback_rate_fps <= 0.0 {
        return None;
    }
    let full_step = playback_rate_fps * 0.01 / frame_count as f32;
    let mut progress = full_step.rem_euclid(1.0);
    let mut displacement = 0.0;
    let active_before_ice = ice_at.saturating_sub(input.spawned_at).max(0);
    let active_after_ice = input
        .observed_at
        .saturating_sub(ice_at)
        .saturating_sub(freeze_duration)
        .max(0);
    if !advance_jack_track(
        frames,
        active_before_ice,
        playback_rate_fps,
        motion.scale,
        &mut progress,
        &mut displacement,
    ) || !advance_jack_track(
        frames,
        active_after_ice,
        playback_rate_fps * 0.5,
        motion.scale,
        &mut progress,
        &mut displacement,
    ) {
        return None;
    }
    let spawn_x = motion.x + displacement;
    spawn_x.is_finite().then_some(spawn_x)
}

fn advance_jack_track(
    frames: &[f32], count: i32, rate: f32, scale: f32, progress: &mut f32, displacement: &mut f32,
) -> bool {
    let step = rate * 0.01 / frames.len() as f32;
    for _ in 0..count {
        let index = (*progress * (frames.len() as f32 - 1.0) + 1.0) as usize;
        let (Some(current), Some(previous)) = (frames.get(index), frames.get(index.saturating_sub(1))) else {
            return false;
        };
        *displacement += (*current - *previous) * rate * 0.01 * scale;
        *progress = (*progress + step).rem_euclid(1.0);
    }
    true
}

fn zombie_attack_offsets(id: ZombieId) -> RuntimeResult<(i32, i32)>
where
    rsvz_current::CurrentBackend: crate::logic::ContactGeometryBackend,
{
    let bounds = crate::logic::contact::zombie_attack_bounds(id)
        .map_err(runtime_error)?
        .ok_or_else(|| RuntimeError::new("live zombie has no supported attack geometry"))?;
    Ok((
        bounds.range.left.saturating_sub(bounds.x),
        bounds.range.right.saturating_sub(bounds.x),
    ))
}

fn running_jack_attack_offsets() -> RuntimeResult<(i32, i32)> {
    let rect = zombie_profile_attack_rect(ZombieContactProfile::CommonBody)
        .map_err(|error| RuntimeError::new(format!("invalid running Jack attack profile: {error:?}")))?;
    let right = rect
        .x_offset
        .checked_add(rect.width)
        .ok_or_else(|| RuntimeError::new("running Jack attack profile overflows"))?;
    Ok((rect.x_offset, right))
}

fn contact_window(
    now: i32, trace: &[f32], attack_offsets: (i32, i32), plant: ContactRect,
) -> RuntimeResult<Option<(i32, i32)>> {
    let mut first = None;
    let mut last = None;
    let mut left_contact = false;
    for (offset, x) in trace.iter().copied().enumerate() {
        let x = pvz_trunc_f32_to_i32(x)
            .map_err(|error| RuntimeError::new(format!("invalid predicted contact coordinate: {error:?}")))?;
        let attack = AbsoluteContactRange::new(x.saturating_add(attack_offsets.0), x.saturating_add(attack_offsets.1));
        let hits = native_horizontal_rect_overlap(attack, plant.horizontal_range()) >= PLANT_THREAT_HORIZONTAL_OVERLAP;
        let time = now.saturating_add(i32::try_from(offset).unwrap_or(i32::MAX));
        if hits {
            if left_contact {
                return Err(RuntimeError::new(
                    "predicted zombie contact is non-contiguous and unsupported by smart-fodder v1",
                ));
            }
            first.get_or_insert(time);
            last = Some(time);
        } else if first.is_some() {
            left_contact = true;
        }
    }
    Ok(first.zip(last))
}

fn baseline_bite_damage(
    now: i32, activation_at: i32, trace: &[f32], attack_offsets: (i32, i32), cannon: ContactRect,
    bite_check_residue: i32,
) -> RuntimeResult<(f64, Option<i32>)> {
    let Some(window) = contact_window(now, trace, attack_offsets, cannon)? else {
        return Ok((0.0, None));
    };
    let contact = align_bite_check(window.0, bite_check_residue);
    if contact > window.1 || contact >= activation_at {
        return Ok((0.0, None));
    }
    let checks = (activation_at - contact + 7) / 8;
    Ok((f64::from(checks.saturating_mul(4).min(300)), Some(contact)))
}

fn direct_blast_for_jack(
    now: i32, activation_at: i32, trace: &[f32], zombie_y: i32, pmf: &ExplosionPmf, cannons: &[(i32, ContactRect)],
    fodder_rect: ContactRect,
) -> Vec<JackBlastEvent> {
    let mut events = Vec::new();
    for explosion_at in pmf.start().max(now)..=pmf.end().min(activation_at.saturating_sub(1)) {
        let probability = pmf.probability_at(explosion_at);
        if probability <= 0.0 {
            continue;
        }
        // PE switches to jackbox_pop before updating position on the transition tick,
        // and that status is immobile.  The explosion center therefore stays at the
        // final walking position from the preceding tick.
        let last_walking_at = explosion_at.saturating_sub(111).max(now);
        let Some(x) = last_walking_at
            .checked_sub(now)
            .and_then(|offset| usize::try_from(offset).ok())
            .and_then(|offset| trace.get(offset))
        else {
            continue;
        };
        let center = ContactCircle::new(*x as i32 + 60, zombie_y + 60, 90);
        let mut baseline_hits = 0.0;
        for (_row, cannon) in cannons {
            if circle_hits_rect(center, *cannon) {
                baseline_hits += 1.0;
            }
        }
        events.push(JackBlastEvent {
            explosion_at,
            probability,
            baseline_hits,
            hits_fodder_without_block: circle_hits_rect(center, fodder_rect),
        });
    }
    events
}

fn jack_geometry_counts(cannons: &[(i32, ContactRect)], jack_row: i32, scene: SceneKind) -> [u16; 5] {
    let mut counts = [0_u16; 5];
    let short_rows = matches!(
        scene,
        SceneKind::Pool | SceneKind::Fog | SceneKind::Roof | SceneKind::MoonNight
    );
    for (cannon_row, _rect) in cannons {
        let index = match *cannon_row - jack_row {
            0 => Some(0),
            -1 if short_rows => Some(3),
            1 if short_rows => Some(4),
            -1 => Some(1),
            1 => Some(2),
            _ => None,
        };
        if let Some(count) = index.and_then(|index| counts.get_mut(index)) {
            *count = count.saturating_add(1);
        }
    }
    counts
}

fn fodder_health(selection: CardSelection) -> (i32, FodderBehavior, Option<FodderMorph>, i32) {
    let target = match selection {
        CardSelection::Plant(kind) | CardSelection::Imitator(kind) => kind,
    };
    let target_hp = 300;
    let target_behavior = if target == PlantKind::Blover {
        FodderBehavior::Blover
    } else {
        FodderBehavior::Biteable
    };
    let target_invulnerable_for = if target == PlantKind::FlowerPot { 100 } else { 0 };
    match selection {
        CardSelection::Plant(_) => (target_hp, target_behavior, None, target_invulnerable_for),
        CardSelection::Imitator(_) => (
            300,
            FodderBehavior::Biteable,
            Some(FodderMorph {
                after_plant: IMITATOR_MORPH_DELAY,
                hp: target_hp,
                behavior: target_behavior,
                invulnerable_for: target_invulnerable_for,
            }),
            0,
        ),
    }
}

fn validate_card(selection: CardSelection) -> RuntimeResult<()> {
    let kind = match selection {
        CardSelection::Plant(kind) | CardSelection::Imitator(kind) => kind,
    };
    // V1 intentionally treats these cards as 300-HP fodder except for the
    // explicitly modeled flower-pot invulnerability and Blover lifecycle.
    if !matches!(
        kind,
        PlantKind::Peashooter
            | PlantKind::Sunflower
            | PlantKind::SnowPea
            | PlantKind::Chomper
            | PlantKind::Repeater
            | PlantKind::PuffShroom
            | PlantKind::SunShroom
            | PlantKind::FumeShroom
            | PlantKind::ScaredyShroom
            | PlantKind::Torchwood
            | PlantKind::Plantern
            | PlantKind::Cactus
            | PlantKind::Blover
            | PlantKind::SplitPea
            | PlantKind::Starfruit
            | PlantKind::MagnetShroom
            | PlantKind::CabbagePult
            | PlantKind::FlowerPot
            | PlantKind::KernelPult
            | PlantKind::UmbrellaLeaf
            | PlantKind::Marigold
            | PlantKind::MelonPult
    ) {
        return Err(RuntimeError::new(format!(
            "card {kind:?} is outside the smart-fodder v1 whitelist",
        )));
    }
    Ok(())
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_row_jack_that_cannot_hit_fodder_folds_to_constant_damage() {
        let mut events = [
            JackBlastEvent {
                explosion_at: 100,
                probability: 0.25,
                baseline_hits: 1.0,
                hits_fodder_without_block: false,
            },
            JackBlastEvent {
                explosion_at: 200,
                probability: 0.5,
                baseline_hits: 2.0,
                hits_fodder_without_block: false,
            },
        ];
        let mut relevant = events;
        relevant[0].hits_fodder_without_block = true;
        relevant[1].hits_fodder_without_block = true;

        assert_eq!(fold_off_row_jack_baseline(&mut events), 375.0);
        assert!(events.iter().all(|event| event.baseline_hits == 0.0));
        assert_eq!(off_row_fodder_hit_pmf(&events), None);

        let pmf = off_row_fodder_hit_pmf(&relevant).expect("hit PMF");
        assert_eq!(pmf.probability_at(100), 0.25);
        assert_eq!(pmf.probability_at(200), 0.5);
    }

    #[test]
    fn visible_animation_conditions_first_freeze_without_reading_the_countdown() {
        let profile = rsvz_model::ZombieTrackProfile::JackBoxWalk;
        let playback_rate_fps = 12.0;
        let step = playback_rate_fps * 0.01 / vanilla_track_frames(profile).len() as f32;
        let expected_duration = 500;
        let progress = (80.0 * step + (620 - expected_duration) as f32 * step * 0.5).rem_euclid(1.0);
        let motion = rsvz_model::ZombieMotionState {
            id: rsvz_model::ZombieId::from_raw(1),
            kind: ZombieKind::JackInTheBox,
            row: 0,
            x: 700.0,
            speed_x: 0.5,
            scale: 1.0,
            counters: rsvz_model::ZombieMotionCounters {
                frozen: 0,
                chilled: 1_380,
                buttered: 0,
            },
            model: ZombieMovementModel::Track {
                profile,
                progress,
                playback_rate_fps,
                direction: rsvz_model::ZombieMotionDirection::Left,
            },
        };
        let compatible = compatible_first_freezes(
            &motion,
            JackDistributionInput {
                spawned_at: 0,
                observed_at: 700,
                birth_speed: 0.5,
                first_ice_at: Some(80),
                first_freeze_in_pool: false,
            },
        )
        .expect("compatible freezes")
        .expect("ice applies to jack");

        assert!(compatible.contains(&expected_duration));
        assert!(!compatible.contains(&400));
    }

    #[test]
    fn visible_position_narrows_freezes_only_to_native_spawn_support() {
        let profile = rsvz_model::ZombieTrackProfile::JackBoxWalk;
        let mut motion = rsvz_model::ZombieMotionState {
            id: rsvz_model::ZombieId::from_raw(1),
            kind: ZombieKind::JackInTheBox,
            row: 0,
            x: 0.0,
            speed_x: 0.67,
            scale: 1.0,
            counters: rsvz_model::ZombieMotionCounters {
                frozen: 0,
                chilled: 1_350,
                buttered: 0,
            },
            model: ZombieMovementModel::Track {
                profile,
                progress: 0.25,
                playback_rate_fps: 29.9,
                direction: rsvz_model::ZombieMotionDirection::Left,
            },
        };
        let input = JackDistributionInput {
            spawned_at: 0,
            observed_at: 650,
            birth_speed: 0.67,
            first_ice_at: Some(1),
            first_freeze_in_pool: false,
        };
        let expected_duration = 500;
        let displacement = inferred_jack_spawn_x(&motion, input, expected_duration).expect("track displacement");
        motion.x = 819.0 - displacement;

        let compatible = position_compatible_first_freezes(&motion, input, &[400, 500, 600], 0.0);
        assert!(compatible.contains(&expected_duration));
        assert!(!compatible.contains(&400));
    }

    #[test]
    fn native_attack_profiles_produce_distinct_c9_contact_windows() {
        let trace = (600..=800).rev().map(|x| x as f32).collect::<Vec<_>>();
        let fodder = ContactRect::new(690, 0, 750, 80);
        let window = |offsets| contact_window(0, &trace, offsets, fodder).expect("window");

        assert_eq!(window((10, 60)), Some((80, 150))); // ladder: x=720..650
        assert_eq!(window((50, 70)), Some((120, 160))); // football: x=680..640
        assert_eq!(window((20, 70)), Some((90, 160))); // jack: x=710..640
        assert_eq!(window((-29, 41)), Some((41, 131))); // pole: x=759..669
    }

    #[test]
    fn baseline_fast_bites_start_on_the_age_check_and_cap_at_300() {
        let trace = vec![0.0; 1_001];
        let cannon = ContactRect::new(0, 0, 100, 80);
        assert_eq!(
            baseline_bite_damage(0, 8, &trace, (0, 100), cannon, 0),
            Ok((4.0, Some(0)))
        );
        assert_eq!(
            baseline_bite_damage(0, 8, &trace, (0, 100), cannon, 7),
            Ok((4.0, Some(7)))
        );
        assert_eq!(baseline_bite_damage(0, 7, &trace, (0, 100), cannon, 7), Ok((0.0, None)));
        assert_eq!(
            baseline_bite_damage(0, 1_000, &trace, (0, 100), cannon, 0),
            Ok((300.0, Some(0)))
        );
    }

    #[test]
    fn jack_pop_transition_keeps_the_previous_walking_position() {
        let pmf = ExplosionPmf::from_probabilities(112, vec![1.0]).expect("point PMF");
        let cannon = ContactRect::new(240, 0, 250, 80);
        let events = direct_blast_for_jack(
            0,
            200,
            &[100.0, 90.0, 80.0],
            0,
            &pmf,
            &[(0, cannon)],
            ContactRect::new(1_000, 0, 1_010, 80),
        );

        assert_eq!(events.len(), 1);
        // The pop transition is at t=2. PE does not perform the stable-motion
        // step to x=80 on that tick, so the center is x=90+60 and touches x=240.
        assert_eq!(events[0].baseline_hits, 1.0);
    }

    #[test]
    fn closed_zombie_scope_ignores_imp_ducky_and_unlisted_kinds() {
        assert!(!is_modeled_kind(ZombieKind::Imp));
        assert!(!is_modeled_kind(ZombieKind::DuckyTube));
        assert!(!is_modeled_kind(ZombieKind::Zomboni));
        assert_eq!(release_tail_kind(ZombieKind::Normal), None);
        assert_eq!(release_tail_kind(ZombieKind::Catapult), None);
        assert_eq!(release_tail_kind(ZombieKind::Gargantuar), None);
        assert_eq!(release_tail_kind(ZombieKind::Ladder), Some(ReleaseTailKind::Ladder));
        assert!(ordinary_cannon_damage_enabled(ZombieKind::PoleVaulting, true));
        assert!(!ordinary_cannon_damage_enabled(ZombieKind::PoleVaulting, false));
        assert!(!ordinary_cannon_damage_enabled(ZombieKind::Normal, true));
        assert!(is_modeled_same_row_state(
            ZombieKind::Catapult,
            ZombiePhase::ZombieNormal
        ));
        assert!(!is_modeled_same_row_state(
            ZombieKind::Catapult,
            ZombiePhase::CatapultLaunching
        ));
    }

    #[test]
    fn card_contract_matches_the_v1_whitelist_and_resets_every_imitator() {
        for safe in [
            PlantKind::Peashooter,
            PlantKind::Sunflower,
            PlantKind::SnowPea,
            PlantKind::Chomper,
            PlantKind::Repeater,
            PlantKind::PuffShroom,
            PlantKind::SunShroom,
            PlantKind::FumeShroom,
            PlantKind::ScaredyShroom,
            PlantKind::Torchwood,
            PlantKind::Plantern,
            PlantKind::Cactus,
            PlantKind::Blover,
            PlantKind::SplitPea,
            PlantKind::Starfruit,
            PlantKind::MagnetShroom,
            PlantKind::CabbagePult,
            PlantKind::FlowerPot,
            PlantKind::KernelPult,
            PlantKind::UmbrellaLeaf,
            PlantKind::Marigold,
            PlantKind::MelonPult,
        ] {
            assert!(validate_card(CardSelection::Plant(safe)).is_ok(), "{safe:?}");
            let (placeholder_hp, placeholder_behavior, morph, initial_invulnerability) =
                fodder_health(CardSelection::Imitator(safe));
            assert_eq!(placeholder_hp, 300);
            assert_eq!(placeholder_behavior, FodderBehavior::Biteable);
            assert_eq!(initial_invulnerability, 0);
            assert_eq!(morph.expect("morph").after_plant, IMITATOR_MORPH_DELAY);
        }
        for unsupported in [
            PlantKind::CherryBomb,
            PlantKind::WallNut,
            PlantKind::Squash,
            PlantKind::TallNut,
            PlantKind::Garlic,
            PlantKind::GatlingPea,
        ] {
            assert!(
                validate_card(CardSelection::Plant(unsupported)).is_err(),
                "{unsupported:?}"
            );
            assert!(
                validate_card(CardSelection::Imitator(unsupported)).is_err(),
                "{unsupported:?}"
            );
        }
        let (_, _, flower_pot_morph, _) = fodder_health(CardSelection::Imitator(PlantKind::FlowerPot));
        assert_eq!(flower_pot_morph.expect("flower-pot morph").invulnerable_for, 100);
        let (_, blover_behavior, _, _) = fodder_health(CardSelection::Plant(PlantKind::Blover));
        assert_eq!(blover_behavior, FodderBehavior::Blover);
        let (_, _, blover_morph, _) = fodder_health(CardSelection::Imitator(PlantKind::Blover));
        assert_eq!(blover_morph.expect("blover morph").behavior, FodderBehavior::Blover);
    }
}
