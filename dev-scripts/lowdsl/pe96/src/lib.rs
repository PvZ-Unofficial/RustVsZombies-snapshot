//! Port of PE 九六 202609131832.cpp, using the shared SmartRemove rules.
use PlantKind::*;
use ZombieKind as Z;
use rsvz::prelude::*;
use rsvz::runtime::RuntimeResult;
use rsvz::{Frame, PlantRef};
use std::cell::Cell;
use std::io::{BufWriter, Write};
use std::time::Duration;

const LINEUP: &str = "LI5HDH/tAib/1HpXSFw4wch97UF84sI1eRCQNXFEOiEMneRlNHlQFhE4RHbM7lBkGFJU8MFKAFY=";
thread_local! {
    // AvZ globals survive normal level reloads; fresh independent lives do not.
    static CARD_CLOCK: Cell<u32> = const { Cell::new(0) };
    static EPOCH: Cell<Option<u64>> = const { Cell::new(None) };
}

fn grid(row: i32, col: i32) -> Grid {
    Grid::from_one_based(row, col).expect("positive script grid")
}

fn plant<'a>(f: &'a Frame<'_>, kind: PlantKind, row: i32, col: i32) -> Option<PlantRef<'a>> {
    if !(1..=9).contains(&col) {
        return None;
    }
    f.plant_at(grid(row, col), |p| p.raw_kind() == kind)
}

fn has(f: &Frame<'_>, kind: PlantKind, row: i32, col: i32) -> bool {
    plant(f, kind, row, col).is_some_and(|p| p.hp() <= 10_000)
}

fn zombie(f: &Frame<'_>, kind: ZombieKind, row: i32, max_x: i32, min_hp: i32) -> bool {
    f.zombies().any(|z| {
        z.kind() == kind
            && (row == 0 || z.row() == row - 1)
            && z.x() >= -1000.0
            && z.x() <= max_x as f32
            && z.hp() >= min_hp
    })
}

fn usable(kind: PlantKind) -> bool {
    rsvz::core::logic::cards::find_usable_seed([CardSelection::Plant(kind)]).is_some()
}

fn use_card(f: &Frame<'_>, kind: PlantKind, row: i32, col: i32) -> RuntimeResult<()> {
    if let Some(slot) = rsvz::core::logic::cards::find_usable_seed([CardSelection::Plant(kind)])
        && f.can_plant_at(kind, grid(row, col))
    {
        rsvz::core::card_slot(slot, row, col).map_err(|error| rsvz::runtime::RuntimeError::new(error.to_string()))?;
    }
    Ok(())
}

fn fix_gloom(f: &Frame<'_>, row: i32, col: i32) -> RuntimeResult<()> {
    if !zombie(f, Z::GigaGargantuar, row, col * 80 + 51, 0)
        && !zombie(f, Z::Gargantuar, row, col * 80 + 51, 0)
        && !zombie(f, Z::Zomboni, row, col * 80 + 11, 0)
        && !zombie(f, Z::Football, row, col * 80 - 29, 90)
    {
        if matches!(row, 3 | 4) && !has(f, LilyPad, row, col) {
            use_card(f, LilyPad, row, col)?;
        }
        if !has(f, FumeShroom, row, col) {
            use_card(f, FumeShroom, row, col)?;
        }
        if !has(f, GloomShroom, row, col) {
            use_card(f, GloomShroom, row, col)?;
        }
    }
    Ok(())
}

fn logic(f: Frame<'_>) -> RuntimeResult<()> {
    let inclock = CARD_CLOCK.get();
    let timing = rsvz::core::timing::wave_timing()?;
    for row in [6, 1] {
        let danger = zombie(&f, Z::GigaGargantuar, row, 400, 0);
        if danger {
            use_card(&f, Squash, row, 4)?;
        }
        if !danger && !has(&f, FumeShroom, row, 4) {
            use_card(&f, FumeShroom, row, 4)?;
        }
    }
    if f.sun() > 3000 && usable(FumeShroom) {
        for (row, support_row) in [(1, 2), (6, 5)] {
            if !zombie(&f, Z::GigaGargantuar, row, 700, 0)
                && !has(&f, FumeShroom, row, 5)
                && has(&f, GloomShroom, support_row, 6)
            {
                use_card(&f, FumeShroom, row, 5)?;
            }
        }
    }
    if inclock == 1
        && (zombie(&f, Z::GigaGargantuar, 0, 1000, 0)
            || zombie(&f, Z::Zomboni, 0, 1000, 0)
            || zombie(&f, Z::Balloon, 1, 1000, 0))
    {
        use_card(&f, Jalapeno, 1, 5)?;
        use_card(&f, Jalapeno, 1, 6)?;
    }
    if inclock == 2501 && (zombie(&f, Z::GigaGargantuar, 0, 1000, 0) || zombie(&f, Z::Zomboni, 0, 1000, 0)) {
        use_card(&f, CherryBomb, 5, 8)?;
        use_card(&f, CherryBomb, 5, 7)?;
    }
    if matches!(inclock, 12 | 2512) && timing.current_wave.0 != 20 {
        use_card(&f, CoffeeBean, 4, 4)?;
        rsvz::cards::set_plant_active_time(IceShroom, 299)?;
    }
    let pause = matches!(inclock, 0 | 11 | 2500 | 2511)
        && rsvz::core::timeline::with_timeline_ref(|timeline| {
            [timing.current_wave.0, timing.current_wave.0 + 1]
                .into_iter()
                .any(|wave| {
                    (1..=20).contains(&wave)
                        && timeline
                            .wave_clocks()
                            .refresh_clock(Wave(wave))
                            .is_some_and(|refresh| (-600..300).contains(&(timing.clock - refresh)))
                })
        });
    if !pause {
        CARD_CLOCK.set((inclock + 1) % 5001);
    }

    // Preserve each branch's original priority and all same-frame placement effects.
    for group in [[(3, 7), (3, 6), (2, 5)], [(4, 7), (4, 6), (5, 5)]] {
        for (row, col) in group {
            if !has(&f, GloomShroom, row, col) {
                fix_gloom(&f, row, col)?;
                break;
            }
        }
    }
    for (row, water_row) in [(2, 3), (5, 4)] {
        if !has(&f, GloomShroom, row, 6)
            && has(&f, GloomShroom, water_row, 7)
            && has(&f, GloomShroom, water_row, 6)
            && has(&f, GloomShroom, row, 5)
        {
            fix_gloom(&f, row, 6)?;
        }
    }
    for (row, support_row) in [(3, 2), (4, 5)] {
        if !has(&f, GloomShroom, row, 9) && has(&f, GloomShroom, row, 7) && has(&f, GloomShroom, support_row, 6) {
            fix_gloom(&f, row, 9)?;
        }
    }
    for row in [3, 4] {
        if !has(&f, GloomShroom, row, 8) && has(&f, GloomShroom, 3, 9) && has(&f, GloomShroom, 4, 9) {
            fix_gloom(&f, row, 8)?;
        }
    }
    for p in f.plants() {
        let g = p.grid();
        if p.is_sleeping()
            && (p.raw_kind() == GloomShroom || (p.raw_kind() == FumeShroom && matches!(g.row, 0 | 1 | 4 | 5)))
        {
            use_card(&f, CoffeeBean, g.row + 1, g.col + 1)?;
        }
    }
    if usable(Pumpkin) {
        match (plant(&f, Pumpkin, 3, 7), plant(&f, Pumpkin, 4, 7)) {
            (None, _) => use_card(&f, Pumpkin, 3, 7)?,
            (_, None) => use_card(&f, Pumpkin, 4, 7)?,
            (Some(a), Some(b)) if a.hp() < 1500 || b.hp() < 1500 => {
                use_card(&f, Pumpkin, if a.hp() < b.hp() { 3 } else { 4 }, 7)?;
            }
            _ => {}
        }
    }
    if (f.spawn_allowed(Z::Zomboni) || f.spawn_allowed(Z::JackInTheBox)) && f.sun() > 3000 && usable(FumeShroom) {
        for (row, water_row) in [(2, 3), (5, 4)] {
            if !has(&f, FumeShroom, row, 7)
                && has(&f, GloomShroom, water_row, 9)
                && !zombie(&f, Z::GigaGargantuar, 0, 1000, 0)
                && !zombie(&f, Z::Gargantuar, row, 611, 0)
                && !zombie(&f, Z::Zomboni, row, 571, 0)
                && !zombie(&f, Z::Football, row, 531, 90)
                && !zombie(&f, Z::Dancing, row, 1000, 0)
            {
                use_card(&f, FumeShroom, row, 7)?;
            }
        }
    }
    Ok(())
}

#[rsvz::script]
fn script() -> RuntimeResult<()> {
    if EPOCH.get().is_none() {
        if let Ok(directory) = std::env::var("PE96_TRACE_DIR") {
            let path = std::path::Path::new(&directory).join(format!("worker-{}.trace", rsvz::session_shard().index));
            let writer = std::cell::RefCell::new(BufWriter::new(
                std::fs::File::create_new(path).map_err(|error| rsvz::runtime::RuntimeError::new(error.to_string()))?,
            ));
            rsvz::set_logger(move |record: &rsvz::LogRecord<'_>| {
                if let Err(error) = writeln!(writer.borrow_mut(), "{record}") {
                    rsvz::fail_script(error);
                }
            });
        } else {
            rsvz::set_logger(|record: &rsvz::LogRecord<'_>| eprintln!("{record}"));
        }
    }
    reload(MainUiOrFightUi);
    lineup(LINEUP);
    skip_seed_chooser();
    set_zombies(random_zombie_types([], [Z::Yeti]), ZombieSpawnMode::Natural);
    select_cards([
        CardSelection::Imitator(IceShroom),
        CardSelection::Plant(IceShroom),
        CardSelection::Plant(CherryBomb),
        CardSelection::Plant(Jalapeno),
        CardSelection::Plant(Squash),
        CardSelection::Plant(CoffeeBean),
        CardSelection::Plant(FumeShroom),
        CardSelection::Plant(GloomShroom),
        CardSelection::Plant(LilyPad),
        CardSelection::Plant(Pumpkin),
    ]);
    let epoch = rsvz::session::world_epoch();
    if EPOCH.replace(Some(epoch)) != Some(epoch) {
        CARD_CLOCK.set(0);
    }
    rsvz::tick::on_frame(logic);
    if std::env::var("PE96_TRACE_LOSS").as_deref() != Ok("0") {
        rsvz::measure::trace_plant_losses([GloomShroom, FumeShroom, WinterMelon])?;
    }
    let seconds = std::env::var("PE96_SECONDS")
        .ok()
        .map(|s| s.parse::<u64>().expect("PE96_SECONDS integer"))
        .unwrap_or(1800);
    if seconds != 0 {
        rsvz::measure::expected_passes_for_with_end(
            Duration::from_secs(seconds),
            rsvz::WorldResetConfig {
                initial_sun: 9990,
                completed_rounds: 1128,
                ..Default::default()
            },
            if std::env::var("PE96_FINISH_ACTIVE").as_deref() == Ok("1") {
                rsvz::measure::ExpectedPassesEnd::FinishActive
            } else {
                rsvz::measure::ExpectedPassesEnd::AtDeadline
            },
        )?;
    }
    Ok(())
}

fn start_managers() -> RuntimeResult<()> {
    rsvz::smart_remove::start_with_options(SmartRemoveOptions { highlight: false })?;
    rsvz::ice_filler::start([(4, 4)])?;
    rsvz::plant_fixer::start_with(
        Pumpkin,
        [
            (1, 3),
            (1, 2),
            (3, 7),
            (4, 7),
            (1, 1),
            (2, 1),
            (5, 1),
            (6, 1),
            (3, 9),
            (4, 9),
            (3, 8),
            (4, 8),
            (2, 5),
            (5, 5),
            (3, 6),
            (4, 6),
        ],
        1333.0,
        true,
    )?;
    Ok(())
}

#[rsvz::state_hooks]
fn hooks() {
    rsvz::state_hook::on_enter_fight(|| {
        if let Err(error) = start_managers() {
            rsvz::fail_script(error);
        }
    });
}
