use std::cell::Cell;

use rsvz::core::backend::ZombieReadBackend;
use rsvz::core::runtime::{RuntimeError, RuntimeResult};
use rsvz::dsl::{LowExpr, d, empty, garlic, pot, rm, try_act, try_card, umbrella};
use rsvz::prelude::{CardSelection, Grid, PlantKind, ZombieKind};

thread_local! {
    static PHASE: Cell<i32> = const { Cell::new(1) };
}

fn runtime_error(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}

fn c<const N: usize>(delay: i32, rows: [i32; N]) -> LowExpr {
    let grids = rows.map(|row| (row, 9));
    let cards = try_card([garlic, pot, umbrella], grids);
    if delay <= 0 {
        cards
    } else {
        let cleanup = grids.into_iter().fold(empty(), |expr, (row, col)| {
            expr + rm(PlantKind::Garlic, row, col)
                + rm(PlantKind::FlowerPot, row, col)
                + rm(PlantKind::UmbrellaLeaf, row, col)
        });
        cards + d(delay) + cleanup
    }
}

fn use_doom(row: i32, col: i32) -> RuntimeResult<()> {
    let grid = Grid::from_one_based(row, col).map_err(runtime_error)?;
    let needs_doom = rsvz::core::logic::cob::find_plant_at_kind(grid, PlantKind::DoomShroom)
        .is_none();
    rsvz::__private::with_board_access(|_access| {
        if needs_doom {
            rsvz::core::card(CardSelection::Plant(PlantKind::DoomShroom), row, col).map_err(runtime_error)?;
        }
        rsvz::core::card(CardSelection::Plant(PlantKind::CoffeeBean), row, col)
            .map(|_id| ())
            .map_err(runtime_error)
    })
    .and_then(|result| result)
}

fn avz_n(effect_time: i32, row: i32, col: i32) {
    let timing = rsvz::core::logic::card_timing::mushroom_effect_timing(CardSelection::Plant(PlantKind::DoomShroom))
        .expect("doom timing");
    (effect_time - timing.day_coffee_lead) << try_act(move || use_doom(row, col));
    (effect_time - timing.normalize_lead)
        << try_act(move || rsvz::cards::try_normalize_card_effect(PlantKind::DoomShroom, row, col));
}

fn ensure_giga_only_row_2() -> LowExpr {
    try_act(|| {
        rsvz::__private::with_board_access(|access| {
            let backend = access.backend();
            let mut ids = Vec::new();
            for zombie in backend.zombies() {
                if backend.zombie_kind(zombie).map_err(runtime_error)? == ZombieKind::GigaGargantuar
                    && backend.zombie_age(zombie) <= rsvz::core::logic::zombies::NEWLY_SPAWNED_ZOMBIE_MAX_AGE
                    && matches!(backend.zombie_row(zombie), 0 | 4 | 5)
                {
                    ids.push(backend.zombie_id(zombie));
                }
            }
            for id in ids {
                rsvz::core::logic::zombies::move_zombie_to_row_by_id(id, 1).map_err(runtime_error)?;
            }
            Ok(())
        })
        .and_then(|result| result)
    })
}

#[rustfmt::skip]
#[rsvz::script]
fn script() {
    use rsvz::core::runtime::{RuntimeError};
    use rsvz::prelude::{MaidCheat, MaidCheatsBackend};

    reload(MainUiOrFightUi);
    lineup(
        "LI43RDGUUFO1cNCRtCSEVRZvFAxaYgROHBaxMhaBmjDk1eZ04FmWSFTs3lsRVg",
        rsvz::LineupReloadPolicy::Reapply,
    );
    set_zombies("普杆舞车豚丑矿跳偷梯白红");
    select_cards("IIKLNAWUF", [garlic]);
    skip_until((3, 500));

    assume_wavelength(1, 601);
    assume_wavelength([2, 11], 601);
    assume_wavelength([3, 12], 1350);
    assume_wavelength(4, 1502);
    assume_wavelength(5, 1348);
    assume_wavelength([6, 7, 15, 16], 1374);
    assume_wavelength(14, 1373);
    assume_wavelength(18, 1341);

    wave(1);
    (-599) << auto_cobs()
        + set_ice([(4, 1), (5, 4), (5, 6), (6, 1), (6, 4), (2, 4)])
        + dancing();

    let phase = PHASE.with(Cell::get);
    match phase {
        1 => {
            assume_wavelength(10, 601);
            wave(5);  348 << doom(3, 5);
            wave(8); 1253 << doom(3, 4);
            wave(15); 1026 << doom(3, 6);
            waves([4, 17]);
            542 << p(2, 9);
            549 << p(5, 9);
            avz_n(762, 3, 6);
            wave(4); 896 << c(382, [5, 6]);
            wave(17); 912 << c(366, [5, 6]);
            wave(8);
            377 << p(2, 9);
            531 << p(5, 9);
            avz_n(762, 3, 5);
            864 << c(382, [5, 6]);
            1070 << p(3, 9);
            1301 << p(1, 9) + d(1) + p(5, 9);
            1513 << ci();
            1535 << p(1, 8.7);
            wave(9); 2400 << umbrella(2, 4);
            wave(10);
            225 << p(22, 9);
            avz_n(360, 3, 9);
            466 << p(5, 7.8);
            395 << shovel([(1, 2), (2, 4)]);
            wave(13);
            377 << p(2, 9);
            506 << p(5, 9);
            avz_n(762, 3, 4);
            840 << c(438, [5, 6]);
            wave(20); 400 << doom(3, 5);
        }
        2 => {
            assume_wavelength(10, 601);
            wave(5); 348 << doom(3, 4);
            wave(8); 1253 << doom(3, 6);
            wave(15); 1026 << doom(3, 5);
            waves([4, 17]);
            377 << p(2, 9);
            531 << p(5, 9);
            avz_n(762, 3, 5);
            wave(4); 864 << c(412, [5, 6]);
            wave(17); 872 << c(406, [5, 6]);
            wave(8);
            377 << p(2, 9);
            527 << p(5, 9);
            avz_n(783, 3, 4);
            888 << c(390, [5, 6]);
            1070 << p(3, 9);
            1322 << p(1, 8.4) + d(1) + p(5, 9);
            1534 << ci();
            1556 << p(1, 8.7);
            wave(10);
            0 << c(262, [5, 6]);
            225 << p(2, 9) + d(26) + p(2, 9);
            avz_n(395, 3, 9);
            501 << p(5, 7.6);
            395 << shovel(1, 2);
            wave(13);
            542 << p(2, 9);
            549 << p(5, 9);
            avz_n(762, 3, 6);
            896 << c(382, [5, 6]);
            wave(20); 400 << doom(3, 4);
        }
        3 => {
            assume_wavelength(10, 601);
            wave(5); 348 << doom(3, 6);
            wave(8); 1253 << doom(3, 5);
            wave(15); 1026 << doom(3, 4);
            wave(4);
            377 << p(2, 9);
            506 << p(5, 9);
            avz_n(762, 3, 4);
            848 << c(430, [5, 6]);
            wave(8);
            542 << p(2, 9);
            549 << p(5, 9);
            avz_n(762, 3, 6);
            896 << c(382, [5, 6]);
            1070 << p(3, 9);
            1301 << p(1, 9) + d(1) + p(5, 9);
            1513 << ci();
            1535 << p(1, 8.7);
            wave(9); 2400 << umbrella(2, 4);
            wave(10);
            225 << p(22, 9);
            avz_n(360, 3, 9);
            466 << p(5, 7.8);
            395 << shovel([(1, 2), (2, 4)]);
            wave(13);
            377 << p(2, 9);
            531 << p(5, 9);
            avz_n(762, 3, 5);
            864 << c(382, [5, 6]);
            wave(17);
            398 << p(2, 9);
            527 << p(5, 9);
            avz_n(783, 3, 4);
            904 << c(374, [5, 6]);
            wave(20); 400 << doom(3, 6);
        }
        _ => unreachable!(),
    }
    wave(20);
    401 << act(|| PHASE.with(|phase| phase.set(phase.get() % 3 + 1)));

    wave(1);
    (-599) << c(-1, [2, 1]);
    (-599) << doom(4, 9);
    225 << p(2, 9) + d(19) + p(2, 9);
    avz_n(360, 4, 9);
    466 << p(5, 7.8);

    wave(2);
    199 << c(9, [1, 2]);
    249 << p(5, 9) + a(5, 9) + d(110) + p(5, 9);
    393 << p(2, 8.7) + d(101) + w(2, 9);

    wave(3);
    1 << ci();
    33 << p(1, 8.6625);
    104 << p(5, 9);
    816 << c(334, [2, 5, 6]);
    1150 << pp(8.65);

    wave(4);
    11 << ci();
    800 << c(480, [1]);
    1070 << p(3, 9);
    1302 << p(15, 9);

    wave(5);
    (-10) << shovel(2, 9, PlantKind::Squash);
    11 << ci();
    33 << p(1, 8.7);
    45 << w(2, 9);
    49 << c(1100, [2]);
    394 << p(5, 8.7) + d(228) + p(5, 8.5875);
    808 << c(340, [1, 5, 6]);
    1148 << pp(8.7);

    wave(6);
    11 << ci();
    498 << p(1, 9) + d(213) + a(1, 8);
    433 << p(5, 8.55) + d(230) + p(5, 8.5375);
    808 << c(366, [2, 5, 6]);
    1174 << pp(8.7);

    wave(7);
    11 << ci();
    366 << w(2, 9) + d(256) + p(1, 8);
    438 << p(5, 8.5125) + d(229) + p(5, 8.5375);
    440 << shovel(2, 9, PlantKind::Squash);
    808 << c(365, [2, 5, 6]);
    1174 << pp(8.7);

    wave(8);
    11 << ci();
    808 << c(480, [1]);

    wave(9);
    0 << ensure_giga_only_row_2();
    451 << p(5, 8.4) + d(229) + p(5, 8.5125);
    800 << c(240, [2]);
    848 << c(430, [5, 6]);
    1042 << p(2, 9);
    1252 << w(2, 9);
    1472 << a(1, 9);
    1222 << p(5, 9);
    1389 << p(5, 9);
    1774 << p(5, 4) + p(5, 9);
    1551 << c(550, [2]);
    1551 << lily(3, 9) + d(975) + shovel(3, 9);
    1600 << c(500, [5, 6]);
    1800 << recover_p([(5, 9.0), (2, 9.0), (1, 8.0), (1, 1.0)]);
    2400 << shovel(2, 1) + garlic(2, 1) + d(900) + umbrella(1, 2);
    4753 << doom(3, 9);

    wave(11);
    199 << c(9, [1, 2]);
    249 << p(55, 9) + d(110) + p(5, 8.7);
    393 << p(2, 8.7) + d(101) + w(2, 9);

    wave(12);
    1 << ci();
    33 << p(1, 8.6625);
    105 << a(5, 9);
    808 << c(342, [2, 5, 6]);
    1150 << pp(8.65);

    wave(13);
    11 << ci();
    800 << c(480, [1]);
    1070 << p(3, 9);
    1301 << p(1, 8.4) + d(1) + p(5, 9);
    1513 << ci();
    1535 << p(1, 8.7);

    wave(14);
    394 << p(5, 8.7) + d(228) + p(5, 8.5875);
    808 << c(365, [2, 5, 6]);
    1173 << pp(8.7);

    wave(15);
    11 << ci();
    366 << w(2, 9) + d(256) + p(1, 8);
    435 << p(5, 8.5375) + d(230) + p(5, 8.5375);
    440 << shovel(2, 9, PlantKind::Squash);
    808 << c(366, [2, 5, 6]);
    1174 << pp(8.7);

    wave(16);
    11 << ci();
    530 << p(2, 9) + d(213) + a(1, 8);
    435 << p(5, 8.5375) + d(230) + p(5, 8.5375);
    808 << c(366, [2, 5, 6]);
    1174 << pp(8.7);

    wave(17);
    11 << ci();
    800 << c(480, [1]);
    1090 << p(3, 9);
    1349 << p(1, 8.4) + d(1) + p(5, 9);
    1561 << ci();
    1583 << p(1, 8.7);

    wave(18);
    394 << p(5, 8.7) + d(228) + p(5, 8.5875);
    808 << c(342, [2, 5, 6]);
    1141 << pp(8.7);

    wave(19);
    11 << ci();
    579 << pp(9);
    avz_n(924, 4, 4);
    808 << c(242, [2]);
    808 << c(310, [5, 6]);
    1050 << p(2, 9);
    1260 << w(2, 9);
    1480 << a(1, 9);
    1127 << p(5, 9);
    1316 << p(5, 9);
    1703 << p(5, 3.5);
    1710 << recover_p([(5, 9.0), (5, 9.0), (2, 9.0), (1, 9.0), (1, 2.0)]);
    3000 << shovel(2, 1) + umbrella(2, 1);

    wave(20);
    250 << p(4, 7.5875);
    305 << p(1256, 8.7) + d(100) + p(1256, 8.7);
    395 << shovel(2, 1);
    813 << pp();
    4796 << try_act(|| {
        rsvz::__private::with_board_access(|access| access.backend().set_maid_cheat(MaidCheat::Move))
            .and_then(|result| result.map_err(|error| RuntimeError::new(error.to_string())))
            .map_err(|error| RuntimeError::new(error.to_string()))
    });
    5500 << pp();
    5999 << act(rsvz::ice_filler::reset);
}
