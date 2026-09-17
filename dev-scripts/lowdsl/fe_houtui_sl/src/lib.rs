//! FE retreat without cobs: native spawn-list rerolls reject bungees before each round.
use PlantKind::*;
use ZombieKind as Z;
use rsvz::core::backend::*;
use rsvz::core::logic::cards::{PlantSeedOutcome, card_cd, find_usable_seed, try_plant_seed};
use rsvz::core::logic::contact::{grid_explosion_shape, plant_threat_hits_zombie};
use rsvz::core::model::{GridExplosionKind, RandomStreamKind, SPAWN_SLOTS_PER_WAVE};
use rsvz::core::model::{PlantRejectReason, Plantability};
use rsvz::plant_fixer::PlantFixer;
use rsvz::prelude::{CardSelection, Grid, PlantKind, SmartRemoveOptions, Wave, ZombieKind};
use rsvz::runtime::{RuntimeError, RuntimeResult};
use rsvz::tick::{TickControl, TickOptions};
use rsvz::timeline::at;
use std::cell::Cell;
use std::io::{BufWriter, Write};
use std::time::Duration;
mod trace;

const LINEUP: &str = "LI5HjH7kAhQDFMNVWlAyHvrBiF5ADV9HJQxd1VZPV4JS1BFJ4tasWE9XTDR2zO5QZBhSVERuQjxX";
type Pos = (i32, i32);
const GLOOMS: [Pos; 8] = [(3, 7), (4, 7), (3, 6), (4, 6), (2, 5), (5, 5), (6, 2), (1, 2)];
const FUMES: [Pos; 6] = [(1, 4), (6, 4), (6, 3), (1, 3), (2, 4), (5, 4)];
const DOOMS: [Pos; 4] = [(3, 9), (4, 9), (3, 8), (4, 8)];
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Strategy {
    Red,
    White,
    Fast,
}
thread_local! {
    static STRATEGY: Cell<Strategy> = const { Cell::new(Strategy::Fast) };
    static CHERRY_ROW: Cell<i32> = const { Cell::new(2) };
    static EPOCH: Cell<Option<u64>> = const { Cell::new(None) };
}
fn err(e: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(e.to_string())
}
fn grid((row, col): Pos) -> Grid {
    Grid::from_one_based(row, col).expect("script grid")
}
fn sel(kind: PlantKind) -> CardSelection {
    CardSelection::Plant(kind)
}
fn usable(card: CardSelection) -> bool {
    find_usable_seed([card]).is_some()
}
fn sun() -> u32 {
    rsvz::with_backend(|b| b.sun().expect("sun"))
}
fn allowed(kind: ZombieKind) -> bool {
    rsvz::with_backend(|b| b.spawn_allowed(kind as u32).expect("spawn types"))
}
fn has(kind: PlantKind, pos: Pos) -> bool {
    rsvz::with_backend(|b| {
        let mut found = false;
        b.for_each_plant_at_anchor_grid(grid(pos), |p| {
            found |= b.plant_raw_kind(p)? == kind;
            Ok(())
        })
        .expect("plant at grid");
        found
    })
}
fn has_main(pos: Pos) -> bool {
    rsvz::with_backend(|b| {
        let mut found = false;
        b.for_each_plant_at_anchor_grid(grid(pos), |p| {
            let kind = b.plant_raw_kind(p)?;
            found |= !matches!(kind, LilyPad | FlowerPot | Pumpkin | CoffeeBean)
                && !(kind == Squash && b.plant_state(p) >= 5);
            Ok(())
        })
        .expect("main plant");
        found
    })
}
fn reject(card: CardSelection, pos: Pos) -> Plantability {
    rsvz::with_backend(|b| {
        b.can_plant_at(card.checked().expect("card"), grid(pos))
            .expect("placement")
    })
}
fn play(card: CardSelection, pos: Pos) -> bool {
    let Some(slot) = find_usable_seed([card]) else {
        trace::outcome(card, pos, PlantSeedOutcome::Unusable, None);
        return false;
    };
    if let Plantability::Rejected(reason) = reject(card, pos) {
        trace::outcome(card, pos, PlantSeedOutcome::Rejected(reason), None);
        return false;
    }
    let before = trace::enabled().then(sun);
    let outcome = try_plant_seed(slot, grid(pos));
    trace::outcome(card, pos, outcome, before);
    matches!(outcome, PlantSeedOutcome::Planted(_))
}
fn shovel(pos: Pos) -> RuntimeResult<()> {
    rsvz::core::shovel(pos.0, pos.1)
}
fn count(kind: ZombieKind, row: i32, wave: i32, low: f32, high: f32, hp: i32) -> usize {
    rsvz::with_backend(|b| {
        b.zombies()
            .expect("zombies")
            .filter(|&z| {
                b.zombie_kind(z).expect("zombie kind") == kind
                    && (row == 0 || b.zombie_row(z) == row - 1)
                    && (wave == 0 || b.zombie_from_wave(z) == wave - 1)
                    && b.zombie_hp(z) >= hp
                    && b.zombie_pos_x(z) >= low
                    && b.zombie_pos_x(z) < high
            })
            .count()
    })
}
fn red(wave: i32) -> bool {
    count(Z::GigaGargantuar, 0, wave, f32::NEG_INFINITY, f32::INFINITY, 1) != 0
}

// The original delayed operation is a single attempt, not an unbounded retry.
fn later(delay: i32, mut action: impl FnMut() -> RuntimeResult<()> + 'static) -> RuntimeResult<()> {
    if delay <= 0 {
        return action();
    }
    let due = rsvz::core::timing::wave_timing()?.clock + delay;
    rsvz::tick::spawn(TickOptions::playing_frame(), move |meta| {
        if meta.clock.is_some_and(|clock| clock >= due) {
            action()?;
            Ok(TickControl::Stop)
        } else {
            Ok(TickControl::Continue)
        }
    });
    Ok(())
}
fn recover_now(card: CardSelection, positions: &[Pos]) -> RuntimeResult<bool> {
    if !usable(card) {
        if let Some(&pos) = positions.first() {
            trace::outcome(card, pos, PlantSeedOutcome::Unusable, None);
        }
        return Ok(false);
    }
    for &pos in positions {
        match reject(card, pos) {
            Plantability::Allowed => {
                if play(card, pos) {
                    return Ok(true);
                }
            }
            Plantability::Rejected(PlantRejectReason::GameRule(1)) if has_main(pos) => {
                shovel(pos)?;
                if play(card, pos) {
                    return Ok(true);
                }
            }
            Plantability::Rejected(PlantRejectReason::GameRule(11)) => {
                if play(sel(LilyPad), pos) && play(card, pos) {
                    return Ok(true);
                }
            }
            Plantability::Rejected(reason) => {
                trace::outcome(card, pos, PlantSeedOutcome::Rejected(reason), None);
            }
        }
    }
    Ok(false)
}
fn recover(card: CardSelection, positions: &'static [Pos]) -> RuntimeResult<()> {
    if usable(card) {
        trace::request(card, positions, Some(0));
        recover_now(card, positions)?;
    } else if let Some(cd) = card_cd(card).filter(|&cd| cd > 0) {
        trace::request(card, positions, Some(cd));
        later(cd, move || recover_now(card, positions).map(|_| ()))?;
    } else {
        trace::request(card, positions, None);
    }
    Ok(())
}
fn smart_ice() -> RuntimeResult<()> {
    let ice = sel(IceShroom);
    let mimic = CardSelection::Imitator(IceShroom);
    if usable(mimic) {
        play(mimic, (4, 5));
    } else if usable(ice) {
        later(320, move || {
            play(ice, (4, 5));
            Ok(())
        })?;
    } else {
        let a = card_cd(ice).unwrap_or(i32::MAX);
        let b = card_cd(mimic).unwrap_or(i32::MAX);
        // (3,5) contains a permanent cattail. Never shovel it to recover ice.
        recover(if a <= b { ice } else { mimic }, &[(4, 5)])?;
    }
    Ok(())
}
fn smart_cherry() -> RuntimeResult<()> {
    let (up, down) = rsvz::with_backend(|b| {
        let mut counts = (0, 0);
        for z in b.zombies().expect("zombies") {
            if b.zombie_kind(z).expect("zombie kind") == Z::GigaGargantuar && b.zombie_hp(z) >= 1800 {
                if b.zombie_row(z) < 3 {
                    counts.0 += 1;
                } else {
                    counts.1 += 1;
                }
            }
        }
        counts
    });
    let row = if up > down { 2 } else { 5 };
    CHERRY_ROW.set(row);
    recover(
        sel(CherryBomb),
        if row == 2 { &[(2, 7), (2, 6)] } else { &[(5, 7), (5, 6)] },
    )
}
fn opposite_jalapeno() -> RuntimeResult<()> {
    recover(sel(Jalapeno), if CHERRY_ROW.get() == 2 { &[(6, 5)] } else { &[(1, 5)] })
}
fn fix_glooms() -> RuntimeResult<()> {
    if sun() < 1000 {
        trace::gloom_check(0);
        return Ok(());
    }
    if !usable(sel(FumeShroom)) {
        trace::gloom_check(1);
        return Ok(());
    }
    if card_cd(sel(GloomShroom)) != Some(0) {
        trace::gloom_check(2);
        return Ok(());
    }
    for pos @ (row, col) in GLOOMS {
        if has(FumeShroom, pos) && usable(sel(GloomShroom)) {
            play(sel(GloomShroom), pos);
            return Ok(());
        }
        if has(GloomShroom, pos) {
            continue;
        }
        let pumpkin = has(Pumpkin, pos);
        let danger = rsvz::with_backend(|b| {
            b.zombies().expect("zombies").any(|z| {
                let zr = b.zombie_row(z) + 1;
                let x = b.zombie_pos_x(z);
                match b.zombie_kind(z).expect("zombie kind") {
                    Z::Gargantuar | Z::GigaGargantuar => zr == row && x < (col * 80 + 80) as f32,
                    Z::Football => !pumpkin && zr == row && x < (col * 80 + 41) as f32,
                    Z::Zomboni => zr == row && x < (col * 80) as f32,
                    Z::JackInTheBox => {
                        (zr - row).abs() <= 1
                            && b.zombie_phase(z).expect("phase") as i32 == 16
                            && x < ((col + 2) * 80) as f32
                    }
                    _ => false,
                }
            })
        });
        if danger {
            trace::gloom_grid(pos, false);
            continue;
        }
        if has_main(pos) {
            shovel(pos)?;
        }
        if !reject(sel(FumeShroom), pos).is_allowed() {
            if matches!(row, 3 | 4) && play(sel(LilyPad), pos) {
            } else {
                trace::gloom_grid(pos, true);
                continue;
            }
        }
        play(sel(FumeShroom), pos);
        play(sel(GloomShroom), pos);
        return Ok(());
    }
    Ok(())
}
fn fodder() -> RuntimeResult<()> {
    if !usable(sel(PuffShroom)) && !usable(sel(FumeShroom)) {
        return Ok(());
    }
    let (front, counts, vehicles) = rsvz::with_backend(|b| {
        let mut front: Option<(i32, f32)> = None;
        let mut counts = [0; 6];
        let mut vehicles = [f32::INFINITY; 6];
        for z in b.zombies().expect("zombies") {
            let row = b.zombie_row(z);
            let x = b.zombie_pos_x(z);
            if !(0..6).contains(&row) {
                continue;
            }
            match b.zombie_kind(z).expect("zombie kind") {
                Z::GigaGargantuar => {
                    let low = if matches!(row, 0 | 5) { 320.0 } else { 400.0 };
                    if matches!(row, 0 | 1 | 4 | 5) && (low..low + 80.0).contains(&x) && b.zombie_hp(z) >= 250 {
                        counts[row as usize] += 1;
                    }
                    if b.zombie_phase(z).expect("phase") as i32 == 0 && front.is_none_or(|(_, old)| x < old) {
                        front = Some((row + 1, x));
                    }
                }
                Z::Zomboni => vehicles[row as usize] = vehicles[row as usize].min(x),
                _ => {}
            }
        }
        (front, counts, vehicles)
    });
    let Some((row, x)) = front else {
        return Ok(());
    };
    let col = (x / 80.0) as i32;
    if !(1..=9).contains(&col) || vehicles[(row - 1) as usize] < x || !reject(sel(PuffShroom), (row, col)).is_allowed()
    {
        return Ok(());
    }
    play(sel(PuffShroom), (row, col));
    if !usable(sel(PuffShroom)) && usable(sel(FumeShroom)) {
        let mut best = 0;
        for i in 1..6 {
            if counts[i] > counts[best] {
                best = i;
            }
        }
        if counts[best] > 0 {
            let row = best as i32 + 1;
            let col = if matches!(row, 1 | 6) { 5 } else { 6 };
            if vehicles[best] >= (col * 80) as f32 {
                play(sel(FumeShroom), (row, col));
            }
        }
    }
    Ok(())
}
fn emergency() -> RuntimeResult<()> {
    if !usable(sel(DoomShroom)) && !usable(sel(Jalapeno)) && !usable(sel(CherryBomb)) {
        return Ok(());
    }
    // Each target is revalidated before use. No object references escape the native borrow.
    let targets = rsvz::with_backend(|b| {
        let mut targets = [None; 6];
        for z in b.zombies().expect("zombies") {
            let row = b.zombie_row(z);
            if !(0..6).contains(&row)
                || b.zombie_kind(z).expect("zombie kind") != Z::JackInTheBox
                || b.zombie_phase(z).expect("phase") as i32 != 16
                || b.zombie_phase_counter(z) <= 100
            {
                continue;
            }
            let limit = match row {
                0 => 442.0,
                1 => 602.0,
                4 => 613.0,
                5 => 453.0,
                _ => continue,
            };
            if b.zombie_pos_x(z) <= limit && targets[row as usize].is_none() {
                targets[row as usize] = Some(b.zombie_id(z));
            }
        }
        targets
    });
    for (index, id) in targets.into_iter().enumerate() {
        let Some(id) = id else {
            continue;
        };
        if usable(sel(DoomShroom)) {
            for pos in DOOMS {
                // Keep both potential cherry squares available while a cherry is ready.
                if pos.1 == 8 && usable(sel(CherryBomb)) {
                    continue;
                }
                let shape = grid_explosion_shape(GridExplosionKind::DoomShroom, grid(pos)).map_err(err)?;
                if plant_threat_hits_zombie(shape, id).map_err(err)? && recover_now(sel(DoomShroom), &[pos])? {
                    return Ok(());
                }
            }
        }
        if matches!(index, 0 | 5) && usable(sel(Jalapeno)) {
            if recover_now(sel(Jalapeno), &[(index as i32 + 1, 5)])? {
                return Ok(());
            }
        } else if matches!(index, 1 | 4) && usable(sel(CherryBomb)) {
            let pos = if index == 1 { (3, 8) } else { (4, 8) };
            let shape = grid_explosion_shape(GridExplosionKind::CherryBomb, grid(pos)).map_err(err)?;
            if plant_threat_hits_zombie(shape, id).map_err(err)? && recover_now(sel(CherryBomb), &[pos])? {
                return Ok(());
            }
        }
    }
    Ok(())
}
fn maintenance() -> RuntimeResult<()> {
    for row in [1, 6] {
        let danger =
            count(Z::GigaGargantuar, row, 0, 0.0, 440.0, 80) > 0 || count(Z::Gargantuar, row, 0, 0.0, 440.0, 80) > 0;
        if !danger && !has(FumeShroom, (row, 4)) && !reject(sel(FumeShroom), (row, 4)).is_allowed() {
            shovel((row, 4))?;
        }
    }
    fodder()?;
    for row in [1, 6] {
        if count(Z::GigaGargantuar, row, 0, f32::NEG_INFINITY, f32::INFINITY, 1) > 0 {
            for col in [3, 4] {
                rsvz::core::shovel_target(row, col, PuffShroom)?;
            }
        }
    }
    Ok(())
}
// Opening has already initialized the allowed types and the first natural list.
// Rejection sampling changes the list, never the allowed-type flags or the lawn.
fn reroll_without_bungees() -> RuntimeResult<()> {
    let (rerolls, initial_bungees, allowed_before, allowed_after) = rsvz::with_backend(|b| -> RuntimeResult<_> {
        let read_allowed = || -> RuntimeResult<u64> {
            let mut mask = 0;
            for kind in 0..=Z::GigaGargantuar as u32 {
                if b.spawn_allowed(kind).map_err(err)? {
                    mask |= 1u64 << kind;
                }
            }
            Ok(mask)
        };
        let waves = b.spawn_wave_count().map_err(err)?;
        let count_bungees = || -> RuntimeResult<usize> {
            let mut count = 0;
            for wave in 0..waves {
                for slot in 0..SPAWN_SLOTS_PER_WAVE {
                    count += usize::from(b.spawn_entry(wave as u32, slot as u32).map_err(err)? == Z::Bungee as i32);
                }
            }
            Ok(count)
        };
        let allowed_before = read_allowed()?;
        let initial_bungees = count_bungees()?;
        let mut bungees = initial_bungees;
        let mut rerolls = 0u64;
        if bungees != 0 && b.random_locked(RandomStreamKind::Battle) {
            return Err(err("spawn-list reroll requires an unlocked battle RNG"));
        }
        while bungees != 0 {
            b.pick_spawn_list().map_err(err)?;
            rerolls += 1;
            bungees = count_bungees()?;
        }
        let allowed_after = read_allowed()?;
        if allowed_before != allowed_after {
            return Err(err("native spawn-list reroll changed the allowed zombie types"));
        }
        if rerolls != 0 {
            b.refresh_spawn_preview().map_err(err)?;
        }
        Ok((rerolls, initial_bungees, allowed_before, allowed_after))
    })?;
    rsvz::log(
        rsvz::LogLevel::Debug,
        format_args!(
            "houtui_sl_spawn epoch={} rounds={} rerolls={} bungees_before={} bungees_after=0 allowed_before={:x} allowed_after={:x}",
            rsvz::session::world_epoch(),
            rsvz::session::completed_rounds(),
            rerolls,
            initial_bungees,
            allowed_before,
            allowed_after
        ),
    );
    Ok(())
}
fn choose_cards() -> RuntimeResult<Vec<CardSelection>> {
    reroll_without_bungees()?;
    STRATEGY.set(if allowed(Z::GigaGargantuar) {
        Strategy::Red
    } else if allowed(Z::Gargantuar) {
        Strategy::White
    } else {
        Strategy::Fast
    });
    Ok(vec![
        sel(IceShroom),
        sel(LilyPad),
        sel(DoomShroom),
        sel(Jalapeno),
        sel(FumeShroom),
        sel(Pumpkin),
        sel(GloomShroom),
        CardSelection::Imitator(IceShroom),
        sel(PuffShroom),
        sel(CherryBomb),
    ])
}
fn schedule_strategy() {
    match STRATEGY.get() {
        Strategy::Red => {
            at(1, -140, smart_ice);
            for wave in [2, 5, 8, 13, 16, 19] {
                at(wave, 179, || {
                    if red(0) {
                        opposite_jalapeno()?;
                    }
                    Ok(())
                });
                at(wave, -140, smart_ice);
            }
            for wave in [3, 6, 9, 11, 14, 17, 19] {
                at(wave, 301, || {
                    if count(Z::GigaGargantuar, 0, 0, f32::NEG_INFINITY, f32::INFINITY, 600) > 0 {
                        recover(sel(DoomShroom), &DOOMS)?;
                    }
                    Ok(())
                });
            }
            for wave in [4, 7, 12, 15, 18] {
                at(wave, -140, smart_ice);
                at(wave, 2000, || {
                    if red(0) {
                        smart_cherry()?;
                    }
                    Ok(())
                });
            }
            for wave in [9, 19] {
                at(wave, 1500, move || {
                    if red(wave) {
                        smart_cherry()?;
                        if red(0) {
                            opposite_jalapeno()?;
                        }
                    }
                    Ok(())
                });
            }
            at(20, 320, || {
                if allowed(Z::GigaGargantuar) {
                    shovel((1, 6))?;
                    recover(sel(Jalapeno), &[(1, 6)])?;
                }
                recover(sel(DoomShroom), &DOOMS)
            });
            rsvz::tick::spawn(TickOptions::playing_frame(), |_| {
                let t = rsvz::core::timing::wave_timing()?;
                let time = rsvz::core::timeline::with_timeline_ref(|timeline| {
                    timeline.wave_clocks().refresh_clock(Wave(20)).map(|c| t.clock - c)
                });
                if t.current_wave == Wave(20)
                    && time.is_some_and(|t| (320..6320).contains(&t))
                    && count(Z::GigaGargantuar, 5, 20, 1.0, 540.0, 1) > 0
                {
                    // One live demand: try only while both the window and target remain valid.
                    recover_now(sel(CherryBomb), &[(5, 6)])?;
                }
                Ok(TickControl::Continue)
            });
        }
        Strategy::White => {
            for wave in [1, 2, 4, 5, 7, 8, 11, 13, 14, 16, 17, 19] {
                at(wave, -140, smart_ice);
            }
            for wave in [3, 6, 9, 12, 15, 18] {
                at(wave, 400, || recover(sel(DoomShroom), &DOOMS));
            }
        }
        Strategy::Fast => {
            for wave in (1..20).filter(|&w| w != 10) {
                at(wave, -140, move || {
                    let mimic = CardSelection::Imitator(IceShroom);
                    if usable(mimic) {
                        play(mimic, (4, 5));
                    } else if usable(sel(IceShroom)) {
                        later(320, || {
                            play(sel(IceShroom), (4, 5));
                            Ok(())
                        })?;
                    } else if usable(sel(DoomShroom)) {
                        at(wave, 500, || recover(sel(DoomShroom), &DOOMS));
                    }
                    Ok(())
                });
            }
        }
    }
    for wave in [10, 20] {
        // No bungees: these waves still need ordinary jack-in-the-box control.
        at(wave, -140, smart_ice);
    }
}
fn start_fight() -> RuntimeResult<()> {
    if trace::enabled() {
        rsvz::tick::spawn(
            TickOptions::any_dispatch()
                .trigger(rsvz::tick::TickTrigger::OnceReady(rsvz::tick::TickPhase::RoundComplete))
                .lane(rsvz::tick::TickLane::After)
                .idle_neutral(),
            |_| {
                trace::flush("round_end");
                Ok(TickControl::Stop)
            },
        );
    }
    rsvz::log(
        rsvz::LogLevel::Debug,
        format_args!(
            "houtui_sl_open epoch={} rounds={} strategy={:?} sun={}",
            rsvz::session::world_epoch(),
            rsvz::session::completed_rounds(),
            STRATEGY.get(),
            sun()
        ),
    );
    if std::env::var("HOUTUI_SL_TRACE_WAVES").as_deref() == Ok("1") {
        for wave in 1..=20 {
            at(wave, 0, || {
                rsvz::log(
                    rsvz::LogLevel::Debug,
                    format_args!(
                        "houtui_resources sun={} ice_cd={:?} mimic_cd={:?} doom_cd={:?} cherry_cd={:?} jala_cd={:?}",
                        sun(),
                        card_cd(sel(IceShroom)),
                        card_cd(CardSelection::Imitator(IceShroom)),
                        card_cd(sel(DoomShroom)),
                        card_cd(sel(CherryBomb)),
                        card_cd(sel(Jalapeno))
                    ),
                );
            });
        }
    }
    schedule_strategy();
    let mut fume = PlantFixer::new();
    fume.start_with(FumeShroom, FUMES, 0.0, false).map_err(err)?;
    fume.set_trace(trace::enabled());
    let mut pumpkins = vec![
        (1, 3),
        (6, 3),
        (1, 2),
        (3, 6),
        (4, 6),
        (3, 7),
        (4, 7),
        (1, 1),
        (2, 1),
        (5, 1),
        (6, 1),
        (2, 4),
        (5, 4),
        (1, 4),
        (6, 4),
        (6, 2),
    ];
    if STRATEGY.get() == Strategy::Fast {
        pumpkins.extend([(2, 5), (5, 5)]);
    }
    let mut pumpkin = PlantFixer::new();
    pumpkin.start_with(Pumpkin, pumpkins, 1667.0, false).map_err(err)?;
    pumpkin.set_trace(trace::enabled());
    let mut coral = PlantFixer::new();
    coral.start_with(Pumpkin, [(3, 5), (4, 5)], 500.0, false).map_err(err)?;
    coral.set_trace(trace::enabled());
    rsvz::tick::spawn(TickOptions::playing_frame(), move |_| {
        fix_glooms()?;
        fume.tick().map_err(err)?;
        pumpkin.tick().map_err(err)?;
        let t = rsvz::core::timing::wave_timing()?;
        let wave20 = rsvz::core::timeline::with_timeline_ref(|timeline| {
            timeline.wave_clocks().refresh_clock(Wave(20)).map(|c| t.clock - c)
        });
        if t.current_wave == Wave(20) && wave20.is_some_and(|t| t >= 1000) {
            coral.tick().map_err(err)?;
        }
        if STRATEGY.get() == Strategy::Fast {
            emergency()?;
        }
        maintenance()?;
        Ok(TickControl::Continue)
    });
    rsvz::smart_remove::start_with_options(SmartRemoveOptions { highlight: false })?;
    Ok(())
}
fn setting(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .map(|x| x.parse().expect("integer setting"))
        .unwrap_or(default)
}
#[rsvz::script]
fn script() -> RuntimeResult<()> {
    trace::configure();
    if EPOCH.get().is_none() {
        if let Ok(directory) = std::env::var("HOUTUI_SL_TRACE_DIR") {
            let writer = std::cell::RefCell::new(BufWriter::new(
                std::fs::File::create_new(
                    std::path::Path::new(&directory).join(format!("worker-{}.trace", rsvz::session_shard().index)),
                )
                .map_err(err)?,
            ));
            rsvz::set_logger(move |record: &rsvz::LogRecord<'_>| {
                if let Err(e) = writeln!(writer.borrow_mut(), "{record}") {
                    rsvz::fail_script(e);
                }
            });
        } else {
            rsvz::set_logger(|record: &rsvz::LogRecord<'_>| eprintln!("{record}"));
        }
    }
    let epoch = rsvz::session::world_epoch();
    if EPOCH.replace(Some(epoch)) != Some(epoch) {
        CHERRY_ROW.set(2);
    }
    reload(MainUiOrFightUi);
    lineup(LINEUP);
    skip_seed_chooser();
    set_zombies(random_zombie_types([], [Z::Yeti]), ZombieSpawnMode::Natural);
    rsvz::setup::select_cards_with(choose_cards);
    if std::env::var("HOUTUI_SL_TRACE_ROUNDS").as_deref() != Ok("0") {
        rsvz::measure::trace_round_end_sun()?;
    }
    if std::env::var("HOUTUI_SL_TRACE_LOSS").as_deref() != Ok("0") {
        rsvz::measure::trace_plant_losses([
            GloomShroom,
            FumeShroom,
            WinterMelon,
            TwinSunflower,
            Cattail,
            UmbrellaLeaf,
            Pumpkin,
            LilyPad,
        ])?;
    }
    let seconds = setting("HOUTUI_SL_SECONDS", 600);
    if seconds != 0 {
        rsvz::measure::expected_passes_for_with_end(
            Duration::from_secs(seconds),
            rsvz::WorldResetConfig {
                initial_sun: setting("HOUTUI_SL_SUN", 8000) as u32,
                completed_rounds: setting("HOUTUI_SL_ROUNDS", 500) as u32,
                ..Default::default()
            },
            if std::env::var("HOUTUI_SL_FINISH_ACTIVE").as_deref() == Ok("0") {
                rsvz::measure::ExpectedPassesEnd::AtDeadline
            } else {
                rsvz::measure::ExpectedPassesEnd::FinishActive
            },
        )?;
    }
    Ok(())
}
#[rsvz::state_hooks]
fn hooks() {
    rsvz::state_hook::on_exit_fight(|| trace::flush("exit"));
    rsvz::state_hook::on_enter_fight(|| {
        if let Err(e) = start_fight() {
            rsvz::fail_script(e);
        }
    });
}
