//! Port of MGE 群曾 20260915.cpp. Native internal spawn, with bungees excluded.
use PlantKind::*;
use ZombieKind as Z;
use rsvz::core::backend::*;
use rsvz::core::logic::cards::{PlantSeedOutcome, find_usable_seed, try_plant_seed};
use rsvz::prelude::{CardSelection, Grid, PlantKind, SmartRemoveOptions, Wave, ZombieKind};
use rsvz::runtime::{RuntimeError, RuntimeResult};
use rsvz::tick::{TickControl, TickOptions};
use std::cell::{Cell, RefCell};
use std::io::{BufWriter, Write};
use std::time::Duration;
mod logic;
#[cfg(test)]
mod tests;
mod trace;

type Pos = (i32, i32);
const LINEUP: &str = "LI5HDH3tBiZ/13rXcldAXJjACH/tATT/lA7S4QTFwRTFwVY0ZZXhbIH0zVdU/ipHCVI=";
const ALL_X: (f32, f32) = (-1000.0, 1000.0);
const ALL_HP: (i32, i32) = (0, i32::MAX);
const GIANTS: [Z; 2] = [Z::GigaGargantuar, Z::Gargantuar];
const MIMIC_ICE: CardSelection = CardSelection::Imitator(IceShroom);
thread_local! {
    static LOGGER_INSTALLED: Cell<bool> = const { Cell::new(false) };
    static ACTIONS: Cell<bool> = const { Cell::new(false) };
    static STATE: RefCell<(Option<u64>,logic::State)> = RefCell::new((None,logic::State::default()));
}
fn err(e: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(e.to_string())
}
fn grid((r, c): Pos) -> Grid {
    Grid::from_one_based(r, c).expect("script grid")
}
fn sel(k: PlantKind) -> CardSelection {
    CardSelection::Plant(k)
}
fn usable(c: CardSelection) -> bool {
    find_usable_seed([c]).is_some()
}
fn sun() -> u32 {
    rsvz::with_backend(|b| b.sun().expect("sun"))
}
fn allowed(k: Z) -> bool {
    rsvz::with_backend(|b| b.spawn_allowed(k as u32).expect("spawn type"))
}
fn hp(k: PlantKind, pos: Pos) -> Option<i32> {
    rsvz::with_backend(|b| {
        let mut value = None;
        b.for_each_plant_at_anchor_grid(grid(pos), |p| {
            if b.plant_raw_kind(p)? == k {
                value = Some(b.plant_hp(p));
            }
            Ok(())
        })
        .expect("plant at grid");
        value
    })
}
fn has(k: PlantKind, p: Pos) -> bool {
    hp(k, p).is_some()
}
fn main_kind(pos: Pos) -> Option<PlantKind> {
    rsvz::with_backend(|b| {
        let mut value = None;
        b.for_each_plant_at_anchor_grid(grid(pos), |p| {
            let k = b.plant_raw_kind(p)?;
            if !matches!(k, Pumpkin | FlowerPot | LilyPad | CoffeeBean) && !(k == Squash && b.plant_state(p) >= 5) {
                value = Some(k);
            }
            Ok(())
        })
        .expect("main plant");
        value
    })
}
fn plant_count(k: PlantKind) -> usize {
    rsvz::with_backend(|b| {
        b.plants()
            .expect("plants")
            .filter(|&p| b.plant_raw_kind(p).expect("kind") == k)
            .count()
    })
}
fn shovel(pos: Pos) -> RuntimeResult<()> {
    trace::removal(pos, "shovel");
    rsvz::core::shovel(pos.0, pos.1)
}
fn remove(k: PlantKind, pos: Pos) -> RuntimeResult<()> {
    trace::removal(pos, "remove_kind");
    rsvz::core::modifier::remove_plant_kind_at(k, grid(pos)).map(|_| ())
}
fn play(c: CardSelection, pos: Pos) -> bool {
    let Some(slot) = find_usable_seed([c]) else {
        return false;
    };
    if c == sel(Blover) && !rsvz::is_safe_blover(grid(pos)).expect("Blover safety") {
        return false;
    }
    let outcome = try_plant_seed(slot, grid(pos));
    if ACTIONS.get() || trace::enabled() && (c == sel(Blover) || matches!(outcome, PlantSeedOutcome::Planted(_))) {
        rsvz::log(
            rsvz::LogLevel::Debug,
            format_args!(
                "mge_card card={c:?} row={} col={} outcome={outcome:?} sun={}",
                pos.0,
                pos.1,
                sun()
            ),
        );
    }
    if trace::enabled() && c == sel(Blover) {
        if let PlantSeedOutcome::Planted(id) = outcome {
            trace::watch(id, pos);
        }
    }
    matches!(outcome, PlantSeedOutcome::Planted(_))
}
fn try_positions(c: CardSelection, positions: &[Pos]) -> bool {
    if !usable(c) {
        return false;
    }
    positions.iter().any(|&p| play(c, p))
}
fn count(kinds: &[Z], states: &[i32], rows: &[i32], x: (f32, f32), hp: (i32, i32)) -> usize {
    rsvz::with_backend(|b| {
        b.zombies()
            .expect("zombies")
            .filter(|&z| {
                if !kinds.is_empty() && !kinds.contains(&b.zombie_kind(z).expect("kind")) {
                    return false;
                }
                if !rows.is_empty() && !rows.contains(&(b.zombie_row(z) + 1)) {
                    return false;
                }
                if !states.is_empty() && !states.contains(&(b.zombie_phase(z).expect("phase") as i32)) {
                    return false;
                }
                let zx = b.zombie_pos_x(z);
                if zx < x.0 || zx > x.1 {
                    return false;
                }
                let health = b.zombie_hp(z) + b.zombie_accessory_1_hp(z) + b.zombie_accessory_2_hp(z);
                hp.0 <= health && health <= hp.1
            })
            .count()
    })
}
fn n(kinds: &[Z]) -> usize {
    count(kinds, &[], &[], ALL_X, ALL_HP)
}
fn nr(kinds: &[Z], row: i32) -> usize {
    count(kinds, &[], &[row], ALL_X, ALL_HP)
}
fn nx(kinds: &[Z], row: i32, x: (f32, f32)) -> usize {
    count(kinds, &[], &[row], x, ALL_HP)
}
fn smash(pos: Pos, low: f32, high: f32, spike: bool, slowed_low: Option<f32>) -> bool {
    rsvz::with_backend(|b| {
        b.zombies().expect("zombies").any(|z| {
            if b.zombie_row(z) != pos.0 - 1
                || !GIANTS.contains(&b.zombie_kind(z).expect("kind"))
                || b.zombie_phase(z).expect("phase") as i32 != 70
            {
                return false;
            }
            let x = b.zombie_pos_x(z);
            let front = pos.1 as f32 * 80.0 + 41.0;
            if !(if spike {
                front - 110.0 < x && x < front
            } else {
                front < x && x < front + 30.0
            }) {
                return false;
            }
            let lo = if b.zombie_chilled_countdown(z) > 0 {
                slowed_low.unwrap_or(low)
            } else {
                low
            };
            b.zombie_reanim_anim_time(z)
                .expect("animation")
                .is_some_and(|rate| lo <= rate && rate <= high)
        })
    })
}
fn balloon_near(time: i32, limit: i32) -> bool {
    rsvz::with_backend(|b| {
        b.zombies().expect("zombies").any(|z| {
            if b.zombie_kind(z).expect("kind") != Z::Balloon
                || !b.zombie_is_alive(z)
                || b.zombie_is_blown_away(z)
                || b.zombie_phase(z).expect("phase") != rsvz::core::model::ZombiePhase::BalloonFlying
            {
                return false;
            }
            let slow = b.zombie_chilled_countdown(z);
            let speed = f64::from(b.zombie_speed_x(z));
            let dx = if slow == 0 {
                speed * f64::from(time)
            } else if slow > time {
                0.4 * speed * f64::from(time)
            } else {
                0.4 * speed * f64::from(slow - 1) + speed * f64::from(time - (slow - 1))
            };
            (f64::from(b.zombie_pos_x(z)) - dx) as i32 <= limit
        })
    })
}
fn jack_hits(x: f32, y: f32, kind: PlantKind, pos: Pos) -> bool {
    let (jx, jy) = ((x + 60.0) as i32, (y + 60.0) as i32);
    let (px, py) = (40 + (pos.1 - 1) * 80, 80 + (pos.0 - 1) * 100);
    let dy = if jy < py {
        py - jy
    } else if jy > py + 80 {
        jy - py - 80
    } else {
        0
    };
    if dy > 90 {
        return false;
    }
    let dx = f64::from(8100 - dy * dy).sqrt() as i32;
    let (left, right) = if kind == Pumpkin { (0, 100) } else { (10, 70) };
    px + left - dx <= jx && jx <= px + right + dx
}
fn choose_cards() -> RuntimeResult<Vec<CardSelection>> {
    // The source draws its random type list first, then removes Bungee without replacement.
    if allowed(Z::Bungee) {
        rsvz::with_backend(|b| -> RuntimeResult<()> {
            b.set_spawn_type_allowed(Z::Bungee, false).map_err(err)?;
            b.pick_spawn_list().map_err(err)?;
            b.refresh_spawn_preview().map_err(err)?;
            Ok(())
        })?;
    }
    let mut cards = vec![
        sel(IceShroom),
        MIMIC_ICE,
        sel(CherryBomb),
        sel(Squash),
        sel(Pumpkin),
        sel(FumeShroom),
    ];
    let giga = allowed(Z::GigaGargantuar);
    let garg = allowed(Z::Gargantuar);
    let car = allowed(Z::Zomboni);
    let jack = allowed(Z::JackInTheBox);
    if plant_count(GloomShroom) < 9 || sun() >= 5000 || jack && sun() >= 2000 {
        cards.push(sel(GloomShroom));
    }
    if car {
        cards.push(sel(Spikeweed));
    }
    if allowed(Z::Balloon) {
        cards.push(sel(Blover));
    }
    if giga || garg {
        cards.push(sel(DoomShroom));
    }
    if plant_count(TwinSunflower) < 6 {
        cards.push(sel(Sunflower));
    }
    if giga {
        cards.push(sel(PuffShroom));
    }
    if plant_count(Sunflower) > 0 {
        cards.push(sel(TwinSunflower));
        cards.retain(|&c| c != sel(Sunflower));
    }
    if !giga && !garg && !car {
        cards.retain(|&c| c != sel(DoomShroom) && (jack || c != sel(CherryBomb)));
    }
    for k in [SunShroom, PuffShroom, FlowerPot, ScaredyShroom] {
        if cards.len() >= 10 {
            break;
        }
        if !cards.contains(&sel(k)) {
            cards.push(sel(k));
        }
    }
    cards.truncate(10);
    rsvz::log(
        rsvz::LogLevel::Debug,
        format_args!(
            "mge_open epoch={} rounds={} sun={} cards={cards:?}",
            rsvz::session::world_epoch(),
            rsvz::session::completed_rounds(),
            sun()
        ),
    );
    Ok(cards)
}
fn setting(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .map(|x| x.parse().expect("integer setting"))
        .unwrap_or(default)
}
#[rsvz::script]
fn script() -> RuntimeResult<()> {
    if !LOGGER_INSTALLED.replace(true) {
        if let Ok(dir) = std::env::var("MGE_TRACE_DIR") {
            let writer = RefCell::new(BufWriter::new(
                std::fs::File::create_new(
                    std::path::Path::new(&dir).join(format!("worker-{}.trace", rsvz::session_shard().index)),
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
    ACTIONS.set(std::env::var("MGE_TRACE_ACTIONS").as_deref() == Ok("1"));
    trace::install()?;
    reload(MainUiOrFightUi);
    lineup(LINEUP);
    skip_seed_chooser();
    set_zombies(random_zombie_types([Z::Normal], [Z::Yeti]), ZombieSpawnMode::Natural);
    rsvz::setup::select_cards_with(choose_cards);
    rsvz::measure::trace_round_end_sun()?;
    if std::env::var("MGE_TRACE_LOSS").as_deref() != Ok("0") {
        // Crowded late waves can exceed the default 256 raw plant-effect events.
        // Reserve once at registration; never grow the queue during simulation.
        rsvz::event::reserve(4096).map_err(err)?;
        rsvz::measure::trace_plant_losses([
            GloomShroom,
            FumeShroom,
            WinterMelon,
            TwinSunflower,
            UmbrellaLeaf,
            Pumpkin,
            SunShroom,
        ])?;
    }
    let seconds = setting("MGE_SECONDS", 600);
    if seconds != 0 {
        rsvz::measure::expected_passes_for_with_end(
            Duration::from_secs(seconds),
            rsvz::WorldResetConfig {
                initial_sun: setting("MGE_SUN", 8000) as u32,
                completed_rounds: setting("MGE_ROUNDS", 1500) as u32,
                ..Default::default()
            },
            rsvz::measure::ExpectedPassesEnd::FinishActive,
        )?;
    }
    Ok(())
}
#[rsvz::state_hooks]
fn hooks() {
    rsvz::state_hook::on_enter_fight(|| {
        let result = (|| -> RuntimeResult<()> {
            rsvz::core::modifier::set_maid_cheat(rsvz::core::model::MaidCheat::CallPartner)?;
            STATE.with_borrow_mut(|(epoch, state)| {
                let current = rsvz::session::world_epoch();
                if *epoch != Some(current) {
                    *state = logic::State::default();
                    *epoch = Some(current);
                }
            });
            let smoke = setting("MGE_SMOKE_FRAMES", 0);
            let mut frames = 0u64;
            rsvz::tick::spawn(TickOptions::playing_frame(), move |_| {
                frames += 1;
                if smoke != 0 && frames >= smoke {
                    let scene = rsvz::with_backend(|b| b.scene().expect("scene"));
                    if scene != rsvz::core::model::SceneKind::MushroomGarden {
                        return Err(err("MGE scene was lost"));
                    }
                    rsvz::log(
                        rsvz::LogLevel::Debug,
                        format_args!("mge_smoke_ok scene={scene:?} frames={frames} sun={}", sun()),
                    );
                    rsvz::stop_script();
                    return Ok(TickControl::Stop);
                }
                STATE.with_borrow_mut(|(_, state)| state.tick())?;
                trace::tick();
                Ok(TickControl::Continue)
            });
            rsvz::smart_remove::start_with_options(SmartRemoveOptions { highlight: false })?;
            Ok(())
        })();
        if let Err(e) = result {
            rsvz::fail_script(e);
        }
    });
}
