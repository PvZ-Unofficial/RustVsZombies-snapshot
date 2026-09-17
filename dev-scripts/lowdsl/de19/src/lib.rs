#[rustfmt::skip]
#[rsvz::script]
fn script() {
    use rsvz::core::runtime::{RuntimeError};
    use rsvz::prelude::CobRuleEditBackend;
    use std::cell::RefCell;
    use std::rc::Rc;

    reload(MainUiOrFightUi);
    lineup("LI43bJyUlNTYBS1cRU90cPBSEVBU4udamFQ=");
    rsvz::with_backend(|backend| backend.set_cob_fixed_delay(true))
        .map_err(|error| RuntimeError::new(error.to_string()))?;
    set_zombies("普障杆舞车丑气矿跳偷梯篮白红");
    select_cards("IIKAJ", [scaredy, pot, sunflower, fume, torch]);

    let c = |removal_delay: i32, packed_rows: i32| {
        let rows = packed_rows
            .to_string()
            .bytes()
            .map(|row| i32::from(row - b'0'))
            .collect::<Vec<_>>();
        let planted = Rc::new(RefCell::new(Vec::new()));
        let planted_on_card = Rc::clone(&planted);
        let place_expr = try_act(move || {
            rsvz::__private::with_board_access(|_access| {

                let candidates = [
                    scaredy.selection(),
                    pot.selection(),
                    sunflower.selection(),
                    fume.selection(),
                    torch.selection(),
                ];
                let mut next = 0;
                for &row in &rows {
                    let selection = loop {
                        let Some(&selection) = candidates.get(next) else {
                            return Ok(());
                        };
                        next += 1;
                        if rsvz::core::logic::cards::find_usable_seed([selection]).is_some() {
                            break selection;
                        }
                    };
                    if let Ok(id) = rsvz::core::card(selection, row, 9) {
                        planted_on_card.borrow_mut().push(id);
                    }
                }
                Ok(())
            }).and_then(|result| result)
        });
        let remove = try_act(move || {
            rsvz::__private::with_board_access(|_access| {
                for id in planted.borrow_mut().drain(..) {
                    rsvz::core::remove_plant_by_id(id)
                        .map_err(|error| RuntimeError::new(error.to_string()))?;
                }
                Ok(())
            }).and_then(|result| result)
        });
        place_expr + d(removal_delay) + remove
    };

    assume_wavelength([1, 2, 3, 6, 10, 13, 16], 601);
    assume_wavelength([4, 11], 1275);
    assume_wavelength(5, 1438);
    assume_wavelength([7, 14, 17], 1253);
    assume_wavelength(8, 1392);
    assume_wavelength(12, 1423);
    assume_wavelength(15, 1393);
    assume_wavelength(18, 1390);

    wave(1);
    (-599) << auto_cobs() + set_ice([(3, 3), (3, 4)]);
    359 << pp() + d(107) + p(14, 7.8) + d(1) + j(3, 9);

    wave(2);
    170 << c(80, 3);
    318 << p(14, 9) + ci();

    wave(3);
    225 << pp(8.825) + d(24) + pp() + d(110) + p(1, 9) + d(1) + a(4, 9);

    wave(4);
    1 << ci();
    301 << p(14, 9);
    815 << c(255, 12345);
    1075 << pp(8.9);

    wave(5);
    11 << ci();
    398 << p(4, 8.8125) + d(231) + p(4, 8.575);
    579 << p(2, 9) + d(363) + p(2, 3.5);
    839 << c(399, 12345);
    1238 << p([(1, 8.7), (4, 9.0)]) + d(233) + p(4, 8.6) + d(131) + p(2, 3.5);

    wave(6);
    258 << c(1, 12);
    259 << p(24, 8.7625) + d(127) + p(1, 8.4) + d(137) + p(4, 3.5) + d(91) + p(1, 4);
    420 << c(1, 3);
    420 << c(181, 45);

    wave(7);
    1 << ci();
    301 << p(1, 9);
    329 << p(4, 7.8375);
    815 << c(255, 123);
    960 << c(85, 45);
    1053 << pp(8.9);

    wave(8);
    11 << ci();
    397 << p(4, 8.825) + d(231) + p(4, 8.575);
    579 << p(2, 9) + d(363) + p(2, 3.5);
    831 << c(361, 12345);
    1192 << p([(1, 8.8), (4, 9.0)]) + d(364) + p(14, 3.5);

    wave(9);
    ensure_exist("红", 5);
    0 << set_ice([(3, 9), (3, 3), (3, 4)]);
    199 << c(12, 12345);
    393 << pp(8.7) + d(95) + pp(9) + d(118) + a(4, 9) + d(9) + p(1, 8.4) + j(1, 9);
    762 << p(3, 9);
    for time in (1025..).step_by(208).take(15) {
        time << c(1, 5);
    }
    1600 << p(3, 9);
    4278 << p(5, 9);

    wave(10);
    225 << pp() + d(63) + pp() + d(111) + a(1, 9) + d(120) + p(4, 4.3);

    wave(11);
    1 << ci();
    2 << set_ice([(3, 3), (3, 4)]);
    301 << p(14, 9);
    815 << c(255, 12345);
    1075 << pp(8.9);

    wave(12);
    11 << ci();
    398 << p(4, 8.8125) + d(231) + p(4, 8.575);
    579 << p(2, 9) + d(363) + p(2, 3.5);
    839 << c(384, 12345);
    1223 << p([(1, 8.7), (4, 9.0)]) + d(233) + p(4, 8.6) + d(131) + p(2, 3.5);

    wave(13);
    273 << c(1, 12);
    274 << p(24, 8.725) + j(4, 9) + d(127) + p(1, 8.4) + d(137) + p(4, 3.5) + d(91) + p(1, 4);
    420 << c(1, 3);
    420 << c(181, 45);

    wave(14);
    1 << ci();
    301 << p(1, 9);
    329 << p(4, 7.8375);
    815 << c(255, 123);
    960 << c(85, 45);
    1053 << pp(8.9);

    wave(15);
    11 << ci();
    398 << p(4, 8.8125) + d(231) + p(4, 8.575);
    579 << p(2, 9) + d(363) + p(2, 3.5);
    839 << c(364, 12345);
    1193 << p([(1, 8.7), (4, 9.0)]) + d(233) + p(4, 8.6) + d(131) + p(2, 3.5);

    wave(16);
    261 << p(4, 8.8) + d(264) + p(4, 3.5);
    268 << c(1, 1);
    269 << a(2, 9) + d(123) + p(1, 8.4) + d(228) + p(1, 4);
    420 << c(1, 3);
    420 << c(181, 45);

    wave(17);
    1 << ci();
    301 << p(1, 9);
    329 << p(4, 7.8375);
    815 << c(255, 123);
    960 << c(85, 45);
    1053 << pp(8.9);

    wave(18);
    11 << ci();
    398 << p(4, 8.8125) + d(231) + p(4, 8.575);
    579 << p(2, 9) + d(363) + p(2, 3.5);
    831 << c(359, 12345);
    1190 << pp() + d(213) + j(3, 9) + d(20) + p(14, 8.6);

    wave(19);
    ensure_exist("红", 5);
    264 << p(14, 8.575) + d(106) + ci();
    760 << p(222, 8.9);
    970 << p(4, 9) + d(215) + p(4, 9);
    1133 << p([(1, 3.0), (3, 9.0)]);
    2115 << c(1, 5);
    2450 << c(1, 5);
    2700 << p(24, 9);

    wave(20);
    ensure_exist("红", 5);
    327 << pp() + d(105) + p(24, 9) + d(110) + p(14, 9);
    327 << c(396, 5);
    394 << p(3, 3.5);
    813 << p(23, 9);
    for time in (793..).step_by(208).take(7) {
        time << c(1, 5);
    }
    1399 << p(4, 7.2875);
    1599 << p(4, 7.2875);
    1828 << p(4, 7.2875);
    2091 << p(4, 7.2875);
    2500 << p(5, 9);
}
