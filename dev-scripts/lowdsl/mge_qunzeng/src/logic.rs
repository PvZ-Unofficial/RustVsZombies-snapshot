use super::*;

#[derive(Default)]
pub(super) struct State {
    clock: i32,
    copy_wait: Option<i32>,
    copy_fallback: bool,
}
fn wave_time(wave: i32, now: i32) -> Option<i32> {
    rsvz::core::timeline::with_timeline_ref(|t| t.wave_clocks().refresh_clock(Wave(wave)).map(|c| now - c))
}
fn brought(k: PlantKind) -> bool {
    rsvz::core::logic::cards::card_cd(sel(k)).is_some()
}
fn disposable(pos: Pos) -> bool {
    matches!(
        main_kind(pos),
        Some(PuffShroom | SunShroom | ScaredyShroom | Sunflower | Spikeweed)
    ) || has(Blover, pos) && !blover_waiting(pos)
}
fn place_clearing_fodder(card: CardSelection, positions: &[Pos]) -> RuntimeResult<bool> {
    if !usable(card) {
        return Ok(false);
    }
    if try_positions(card, positions) {
        return Ok(true);
    }
    for &pos in positions {
        if disposable(pos) {
            shovel(pos)?;
            if play(card, pos) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn copy_ice(fallback: bool) -> RuntimeResult<bool> {
    if !usable(MIMIC_ICE) {
        return Ok(false);
    }
    // The safety query rejects frozen boards; postpone this request until thawed.
    if rsvz::with_backend(|b| {
        b.zombies()
            .expect("zombies")
            .any(|z| b.zombie_is_alive(z) && b.zombie_frozen_countdown(z) > 0)
    }) {
        return Ok(false);
    }
    const POS: [Pos; 8] = [(1, 4), (5, 4), (1, 6), (5, 6), (2, 7), (4, 7), (1, 5), (5, 5)];
    // Empty squares, disposable fodder, column-five fumes, then delayed column-four fallback.
    for tier in 0..=if fallback { 3 } else { 2 } {
        for pos in POS {
            let kind = main_kind(pos);
            let eligible = match tier {
                0 => kind.is_none(),
                1 => disposable(pos),
                2 => pos.1 == 5 && kind == Some(FumeShroom),
                _ => pos.1 == 4 && kind == Some(FumeShroom),
            };
            if !eligible || !rsvz::is_safe_imitator_ice(grid(pos))? {
                continue;
            }
            if kind.is_some() {
                shovel(pos)?;
            }
            if play(MIMIC_ICE, pos) {
                rsvz::log(
                    rsvz::LogLevel::Debug,
                    format_args!("mge_copy_ice grid={pos:?} tier={tier}"),
                );
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn blover_waiting(pos: Pos) -> bool {
    rsvz::with_backend(|b| {
        let mut waiting = false;
        b.for_each_plant_at_anchor_grid(grid(pos), |p| {
            if b.plant_raw_kind(p)? == Blover && b.plant_state(p) != 2 {
                waiting = true;
            }
            Ok(())
        })
        .expect("Blover at grid");
        waiting
    })
}
fn meatshield(pos: Pos, allow_spike: bool) -> RuntimeResult<()> {
    if has(FlowerPot, pos) {
        return Ok(());
    }
    let car = count(
        &[Z::Zomboni, Z::Catapult],
        &[0],
        &[pos.0],
        (-1000.0, (pos.1 * 80 + 5) as f32),
        ALL_HP,
    ) > 0;
    if car {
        if allow_spike {
            play(sel(Spikeweed), pos);
        }
        play(sel(Blover), pos);
    } else {
        for k in [
            PuffShroom,
            FlowerPot,
            SunShroom,
            ScaredyShroom,
            Sunflower,
            FumeShroom,
            Spikeweed,
            Blover,
        ] {
            if k == Spikeweed && !allow_spike {
                continue;
            }
            // The flower pot stops the original loop on subsequent iterations.
            if has(FlowerPot, pos) {
                break;
            }
            play(sel(k), pos);
        }
    }
    Ok(())
}
fn fix_gloom(pos: Pos, need_pumpkin: bool) {
    if has(GloomShroom, pos) {
        return;
    }
    if !usable(sel(FumeShroom)) && !usable(sel(GloomShroom)) {
        return;
    }
    if need_pumpkin && !has(Pumpkin, pos) && !usable(sel(Pumpkin)) {
        return;
    }
    if nx(&GIANTS, pos.0, (-1000.0, (pos.1 * 80 + 51) as f32)) > 0
        || nx(&[Z::Zomboni], pos.0, (-1000.0, (pos.1 * 80 + 11) as f32)) > 0
        || count(
            &[Z::Football],
            &[],
            &[pos.0],
            (-1000.0, (pos.1 * 80 - 29) as f32),
            (90, i32::MAX),
        ) > 0
    {
        return;
    }
    if !has(FumeShroom, pos) {
        play(sel(FumeShroom), pos);
    }
    if !has(GloomShroom, pos) {
        play(sel(GloomShroom), pos);
    }
}
fn fix_front_glooms() {
    for pos in [(3, 6), (2, 5), (4, 5), (3, 7)] {
        fix_gloom(pos, pos == (3, 7));
    }
    if has(GloomShroom, (3, 7)) {
        for pos in [(2, 6), (4, 6)] {
            fix_gloom(pos, false);
        }
    }
}
fn fix_edge_fumes() {
    if !usable(sel(FumeShroom))
        || [(3, 6), (2, 5), (4, 5), (3, 7), (2, 6), (4, 6)]
            .into_iter()
            .any(|p| !has(GloomShroom, p))
        || [(1, 4), (5, 4)].into_iter().any(|p| !has(FumeShroom, p))
    {
        return;
    }
    for row in [5, 1] {
        let pos = (row, 5);
        // Local, deliberately coarse gate: do not rebuild into the next hammer/bite/crush.
        if main_kind(pos).is_none()
            && nx(&GIANTS, row, (-1000.0, 520.0)) == 0
            && count(&[Z::Football], &[], &[row], (-1000.0, 470.0), (90, i32::MAX)) == 0
            && nx(&[Z::Zomboni, Z::Catapult], row, (-1000.0, 411.0)) == 0
            && play(sel(FumeShroom), pos)
        {
            rsvz::log(
                rsvz::LogLevel::Debug,
                format_args!("mge_fume_repair grid={pos:?} sun={}", sun()),
            );
            break;
        }
    }
}
fn fix_pumpkins() {
    if !usable(sel(Pumpkin)) {
        return;
    }
    // Stable order breaks equal predicted lifetimes exactly as the source script.
    const POS: [(Pos, f32); 16] = [
        ((1, 1), 8.0),
        ((2, 1), 3.0),
        ((3, 1), 1.0),
        ((4, 1), 3.0),
        ((5, 1), 8.0),
        ((3, 6), 2.0),
        ((3, 7), 7.0),
        ((2, 5), 1.0),
        ((4, 5), 1.0),
        ((2, 4), 1.0),
        ((3, 4), 1.0),
        ((4, 4), 1.0),
        ((2, 6), 2.0),
        ((4, 6), 2.0),
        ((1, 4), 1.0),
        ((5, 4), 1.0),
    ];
    let no_giga = n(&[Z::GigaGargantuar]) == 0;
    let maintain_eyes = no_giga
        && n(&[Z::Gargantuar]) == 0
        && !allowed(Z::GigaGargantuar)
        && !allowed(Z::Gargantuar)
        && !allowed(Z::Zomboni)
        && (n(&[Z::Football]) > 0 || allowed(Z::Football));
    let mut giant = [10000.0f32; 5];
    rsvz::with_backend(|b| {
        for z in b.zombies().expect("zombies") {
            if !GIANTS.contains(&b.zombie_kind(z).expect("kind")) {
                continue;
            }
            let row = b.zombie_row(z) as usize;
            let x = b.zombie_pos_x(z);
            if row < 5 && (-1000.0..=1000.0).contains(&x) {
                giant[row] = giant[row].min(x);
            }
        }
    });
    let outer = [(1, 1), (5, 1)]
        .iter()
        .any(|&p| (main_kind(p).is_some() || has(Pumpkin, p)) && hp(Pumpkin, p).unwrap_or(0) < 800);
    let mut best = None;
    let mut min = f32::MAX;
    let mut second = f32::MAX;
    for (p, rate) in POS {
        if ([(2, 6), (4, 6)].contains(&p) && !maintain_eyes) || ([(1, 4), (5, 4)].contains(&p) && !no_giga) {
            continue;
        }
        if giant[(p.0 - 1) as usize] <= (p.1 * 80 + 81) as f32 || main_kind(p).is_none() {
            continue;
        }
        if outer && [(3, 7), (2, 5), (4, 5)].contains(&p) {
            continue;
        }
        let life = hp(Pumpkin, p).unwrap_or(0) as f32 / rate;
        if life < min {
            second = min;
            min = life;
            best = Some(p);
        } else if life < second {
            second = life;
        }
    }
    if min <= 150.0 || second <= 300.0 {
        if let Some(p) = best {
            play(sel(Pumpkin), p);
        }
    }
}
fn kill_jack() -> RuntimeResult<()> {
    if !usable(sel(CherryBomb)) {
        return Ok(());
    }
    let target = rsvz::with_backend(|b| {
        for z in b.zombies().expect("zombies") {
            if b.zombie_kind(z).expect("kind") != Z::JackInTheBox
                || b.zombie_phase(z).expect("phase") as i32 != 16
                || b.zombie_phase_counter(z) != 110
            {
                continue;
            }
            for p in b.plants().expect("plants") {
                if b.plant_raw_kind(p).expect("kind") != GloomShroom {
                    continue;
                }
                let pos = (b.plant_row(p) + 1, b.plant_col(p) + 1);
                if [(2, 6), (3, 7), (4, 6)].contains(&pos)
                    && jack_hits(b.zombie_pos_x(z), b.zombie_pos_y(z), GloomShroom, pos)
                {
                    return Some((pos.0, pos.1 + 1));
                }
            }
        }
        None
    });
    if let Some(p) = target {
        shovel(p)?;
        play(sel(CherryBomb), p);
    }
    Ok(())
}
impl State {
    fn balloon(&mut self) -> RuntimeResult<()> {
        trace::air();
        if !usable(sel(Blover)) {
            return Ok(());
        }
        if !balloon_near(470, -50) {
            return Ok(());
        }
        const POS: [Pos; 10] = [
            (1, 5),
            (5, 5),
            (1, 6),
            (5, 6),
            (2, 7),
            (4, 7),
            (1, 4),
            (5, 4),
            (3, 8),
            (3, 9),
        ];
        for pos in POS {
            if main_kind(pos).is_none() && play(sel(Blover), pos) {
                return Ok(());
            }
        }
        for tier in 0..3 {
            for pos in POS {
                let eligible = match tier {
                    0 => disposable(pos),
                    1 => pos.1 == 5 && has(FumeShroom, pos),
                    _ => pos.1 == 4 && has(FumeShroom, pos),
                };
                if eligible && rsvz::is_safe_blover(grid(pos))? {
                    shovel(pos)?;
                    if play(sel(Blover), pos) {
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
    pub(super) fn tick(&mut self) -> RuntimeResult<()> {
        rsvz::core::modifier::set_dance_mode(true)?;
        let timing = rsvz::core::timing::wave_timing()?;
        let wave = timing.current_wave.0;
        let now = timing.clock;
        let mut inside = self.clock % 5001;
        let mut advance = true;
        let giga = n(&[Z::GigaGargantuar]);
        let garg = n(&[Z::Gargantuar]);
        let football = n(&[Z::Football]) > 0;
        let giga_level = allowed(Z::GigaGargantuar);
        let garg_level = allowed(Z::Gargantuar);
        let car_level = allowed(Z::Zomboni);
        let need_ice = wave != 20 && (giga + garg > 0 || football && (!has(Pumpkin, (2, 6)) || !has(Pumpkin, (4, 6))));
        if usable(sel(Squash)) && (giga_level || garg_level || allowed(Z::Football)) {
            let special = [1, 9, 19, 20].contains(&wave);
            if (!special && nr(&[Z::GigaGargantuar, Z::Gargantuar, Z::Football], 3) == 0)
                || (special && nr(&[], 3) > nx(&[Z::BackupDancer], 3, (800.0, 1000.0)) && nr(&GIANTS, 3) == 0)
            {
                if nx(&[Z::GigaGargantuar], 1, (0.0, 520.0)) > 0 {
                    if !place_clearing_fodder(sel(Squash), &[(1, 6), (1, 5)])? && has(FumeShroom, (1, 5)) {
                        shovel((1, 5))?;
                        play(sel(Squash), (1, 5));
                    }
                }
            } else if !giga_level && !garg_level {
                try_positions(sel(Squash), &[(3, 8), (3, 9)]);
            } else {
                try_positions(sel(Squash), &[(3, 9), (3, 8)]);
            }
        }
        if inside == 1 && giga + nr(&[Z::Gargantuar, Z::Normal], 3) > 0 {
            let positions: &[Pos] = if count(&[Z::GigaGargantuar], &[], &[5], ALL_X, (500, i32::MAX)) == 0 {
                &[
                    (1, 8),
                    (1, 9),
                    (1, 7),
                    (2, 8),
                    (2, 9),
                    (5, 8),
                    (5, 9),
                    (5, 7),
                    (2, 7),
                    (4, 7),
                ]
            } else if count(&[Z::GigaGargantuar], &[], &[1], ALL_X, (2000, i32::MAX)) == 0 {
                &[
                    (4, 8),
                    (4, 9),
                    (2, 8),
                    (2, 9),
                    (1, 7),
                    (5, 8),
                    (5, 9),
                    (5, 7),
                    (2, 7),
                    (4, 7),
                ]
            } else if giga > 0 {
                &[
                    (2, 8),
                    (2, 9),
                    (4, 8),
                    (4, 9),
                    (1, 8),
                    (1, 9),
                    (1, 7),
                    (5, 8),
                    (5, 9),
                    (5, 7),
                    (2, 7),
                    (4, 7),
                    (3, 9),
                ]
            } else {
                &[
                    (4, 8),
                    (4, 9),
                    (1, 8),
                    (1, 9),
                    (1, 7),
                    (5, 8),
                    (5, 9),
                    (5, 7),
                    (2, 7),
                    (4, 7),
                ]
            };
            try_positions(sel(DoomShroom), positions);
        }
        if inside != 102 || !need_ice {
            self.copy_wait = None;
            self.copy_fallback = false;
        }
        if inside == 102 && need_ice {
            if copy_ice(self.copy_fallback)? {
                rsvz::log(
                    rsvz::LogLevel::Debug,
                    format_args!("mge_copy_wait cs={}", self.copy_wait.map_or(0, |start| now - start)),
                );
                self.copy_wait = None;
                self.copy_fallback = false;
            } else {
                let start = *self.copy_wait.get_or_insert(now);
                advance = false;
                if !self.copy_fallback && now - start >= 200 {
                    self.copy_fallback = true;
                    if copy_ice(true)? {
                        rsvz::log(rsvz::LogLevel::Debug, format_args!("mge_copy_wait cs={}", now - start));
                        advance = true;
                        self.copy_wait = None;
                        self.copy_fallback = false;
                    }
                }
            }
        }
        let early = [1, 5].into_iter().any(|row| {
            hp(Pumpkin, (row, 1)).is_some_and(|hp| hp < 300)
                && count(&[Z::Digger], &[36, 37], &[row], ALL_X, ALL_HP) > 0
        });
        if early && wave != 20 {
            if !place_clearing_fodder(sel(IceShroom), &[(5, 6), (1, 6)])? {
                advance = false;
            }
            self.clock += 3422 - inside;
            inside = 3422;
        } else if inside == 3422 && need_ice && !place_clearing_fodder(sel(IceShroom), &[(5, 6), (1, 6)])? {
            advance = false;
        }
        if inside == 2501
            && (giga_level || car_level)
            && nr(&[Z::Zomboni], 3)
                + count(&[Z::GigaGargantuar], &[], &[2, 3, 4], ALL_X, ALL_HP)
                + nr(&[Z::Gargantuar], 3)
                > 0
        {
            try_positions(sel(CherryBomb), &[(3, 9), (3, 8)]);
        }
        // Only current/next wave can affect these source conditions.
        if [0, 3421, 2500].contains(&inside) {
            for w in [wave, wave + 1] {
                if (1..=20).contains(&w) && wave_time(w, now).is_some_and(|t| (-600..257).contains(&t)) {
                    advance = false;
                }
            }
        }
        let allow_spike = nr(&[Z::Normal], 3) == 0
            && !(wave < 20 && wave_time(wave + 1, now).is_some_and(|t| (-200..1).contains(&t)));
        if usable(sel(Spikeweed))
            && ((((200 < inside && inside < 2450) || (2600 < inside && inside < 4950))
                && plant_count(DoomShroom) + plant_count(CherryBomb) == 0)
                || !brought(DoomShroom))
        {
            for (pos, range) in [
                ((3, 9), (639.0, 727.0)),
                ((3, 8), (555.0, 647.0)),
                ((1, 5), (319.0, 357.0)),
                ((5, 5), (319.0, 357.0)),
            ] {
                if nx(&[Z::Zomboni], pos.0, range) > 0
                    && (pos.0 != 3 || !smash(pos, 0.518787, 0.65, true, Some(0.581818)))
                {
                    play(sel(Spikeweed), pos);
                }
            }
        }
        self.clock += i32::from(advance);
        const COL: [i32; 5] = [5, 7, 8, 7, 5];
        const X0: [f32; 5] = [360.0, 520.0, 630.0, 520.0, 360.0];
        const X1: [f32; 5] = [430.0, 590.0, 700.0, 590.0, 430.0];
        const HP0: [i32; 5] = [300, 500, 100, 500, 300];
        const K: [i32; 5] = [40, 40, 70, 40, 40];
        let guard_fume = [has(FumeShroom, (1, 5)), false, false, false, has(FumeShroom, (5, 5))];
        let mut guard_threat = [0i32; 5];
        let mut threat = [0i32; 5];
        rsvz::with_backend(|b| {
            for z in b.zombies().expect("zombies") {
                if !GIANTS.contains(&b.zombie_kind(z).expect("kind")) || b.zombie_phase(z).expect("phase") as i32 != 0 {
                    continue;
                }
                let r = b.zombie_row(z) as usize;
                let x = b.zombie_pos_x(z);
                if r < 5 && guard_fume[r] {
                    if (440.0..=510.0).contains(&x) {
                        let need = 300 + (x - 440.0) as i32 * 40;
                        guard_threat[r] = guard_threat[r].max(b.zombie_hp(z) - need);
                    }
                    continue;
                }
                if r < 5 && X0[r] <= x && x <= X1[r] {
                    let need = HP0[r] + (x - X0[r]) as i32 * K[r];
                    threat[r] =
                        threat[r].max(b.zombie_hp(z) + b.zombie_accessory_1_hp(z) + b.zombie_accessory_2_hp(z) - need);
                }
            }
        });
        for r in 0..5 {
            if threat[r] > 0 && smash((r as i32 + 1, COL[r]), 0.2, 0.65, false, None) {
                threat[r] = 0;
            }
        }
        for _ in 0..5 {
            let mut best = None;
            for r in 0..5 {
                if threat[r] > 0 && best.is_none_or(|old: usize| threat[r] > threat[old]) {
                    best = Some(r);
                }
            }
            let Some(r) = best else {
                break;
            };
            let pos = (r as i32 + 1, COL[r]);
            meatshield(pos, allow_spike)?;
            threat[r] = 0;
        }
        if giga == 0 && !has(Pumpkin, (2, 6)) && !has(Pumpkin, (4, 6)) {
            for row in [2, 4] {
                if count(&[Z::Football], &[0], &[row], (520.0, 550.0), (500, i32::MAX)) > 0 {
                    meatshield((row, 7), false)?;
                }
            }
        }
        fix_front_glooms();
        // Column four remains the permanent half of each double-fume lane.
        try_positions(sel(FumeShroom), &[(5, 4), (1, 4)]);
        if giga == nr(&[Z::GigaGargantuar], 1) + nr(&[Z::GigaGargantuar], 5) && nr(&[Z::Gargantuar], 3) == 0 {
            kill_jack()?;
        }
        if giga == 0 && allowed(Z::Football) && hp(Pumpkin, (3, 7)).is_some_and(|h| (0..=500).contains(&h)) {
            play(sel(CherryBomb), (3, 8));
        }
        for row in [2, 4] {
            if count(&[Z::GigaGargantuar], &[0], &[row], (-600.0, 522.0), (500, 6000)) > 0
                && nx(&[Z::Normal], row, (-600.0, 511.0)) == 0
            {
                play(sel(Pumpkin), (row, 6));
            }
        }
        self.balloon()?;
        for r in [0, 4] {
            let pos = (r + 1, 6);
            if guard_threat[r as usize] > 0 && !smash(pos, 0.2, 0.65, false, None) && main_kind(pos).is_none() {
                for kind in [PuffShroom, FlowerPot, SunShroom, ScaredyShroom] {
                    if play(sel(kind), pos) {
                        break;
                    }
                }
            }
        }
        try_positions(sel(Sunflower), &[(1, 1), (3, 1), (5, 1)]);
        try_positions(sel(TwinSunflower), &[(1, 1), (3, 1), (5, 1)]);
        fix_pumpkins();
        fix_edge_fumes();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{put, scene, scene_with_setup};
    use super::*;

    #[test]
    fn eye_repairs_wait_for_middle_seven() {
        scene(|| {
            for pos in [(3, 6), (2, 5), (4, 5)] {
                put(GloomShroom, pos);
            }
            fix_front_glooms();
            assert!(!has(FumeShroom, (2, 6)));
            assert!(!has(GloomShroom, (2, 6)));
            put(GloomShroom, (3, 7));
            fix_front_glooms();
            assert!(has(GloomShroom, (2, 6)));
        });
    }

    #[test]
    fn unsafe_shovel_candidate_is_preserved_and_next_candidate_is_used() {
        scene_with_setup(
            || {
                rsvz::with_backend(|b| {
                    b.place_zombie(Z::Zomboni, grid((1, 6))).unwrap();
                    let z = b.place_zombie(Z::Balloon, grid((3, 1))).unwrap();
                    b.set_zombie_x(z, rsvz::core::model::I32RepresentableF32::new(-40.0).unwrap())
                        .unwrap();
                });
            },
            || {
                for pos in [
                    (1, 5),
                    (5, 5),
                    (1, 6),
                    (5, 6),
                    (2, 7),
                    (4, 7),
                    (1, 4),
                    (5, 4),
                    (3, 8),
                    (3, 9),
                ] {
                    put(PuffShroom, pos);
                }
                put(Pumpkin, (1, 5));
                assert!(balloon_near(470, -50));
                State::default().balloon().unwrap();
                assert!(has(PuffShroom, (1, 5)));
                assert!(has(Blover, (5, 5)));
            },
        );
    }

    #[test]
    fn copy_ice_does_not_remove_waiting_blover_but_emergency_shovel_can() {
        scene(|| {
            for pos in [(1, 5), (5, 5), (1, 6), (5, 6), (2, 7), (4, 7), (1, 4), (5, 4)] {
                put(Blover, pos);
                assert!(blover_waiting(pos));
            }
            assert!(!copy_ice(false).unwrap());
            assert!(!copy_ice(true).unwrap());
            assert_eq!(plant_count(Blover), 8);
            shovel((1, 4)).unwrap();
            assert!(!has(Blover, (1, 4)));
        });
    }
}

#[cfg(test)]
mod four_fume_tests {
    use super::super::tests::{put, scene, scene_with_setup};
    use super::*;

    #[test]
    fn ordinary_ice_clears_fodder_but_preserves_pending_effects() {
        scene(|| {
            put(PuffShroom, (5, 6));
            put(Blover, (1, 6));
            assert!(place_clearing_fodder(sel(IceShroom), &[(5, 6), (1, 6)]).unwrap());
            assert!(has(IceShroom, (5, 6)));
            assert!(blover_waiting((1, 6)));
        });
    }

    #[test]
    fn copy_sacrifices_upper_five_before_permanent_four() {
        scene(|| {
            for p in [(1, 4), (5, 4), (1, 5), (5, 5)] {
                put(FumeShroom, p);
            }
            for p in [(1, 6), (5, 6), (2, 7), (4, 7)] {
                put(Blover, p);
            }
            assert!(copy_ice(false).unwrap());
            assert!(has(Imitator, (1, 5)));
            for p in [(1, 4), (5, 4), (5, 5)] {
                assert!(has(FumeShroom, p));
            }
        });
    }

    #[test]
    fn edge_repair_waits_for_core_and_uses_local_not_level_threats() {
        scene_with_setup(
            || {
                rsvz::with_backend(|b| {
                    let z = b.place_zombie(Z::GigaGargantuar, grid((5, 6))).unwrap();
                    b.set_zombie_x(z, rsvz::core::model::I32RepresentableF32::new(500.0).unwrap())
                        .unwrap();
                });
            },
            || {
                for p in [(1, 4), (5, 4)] {
                    put(FumeShroom, p);
                }
                fix_edge_fumes();
                assert!(!has(FumeShroom, (1, 5)));
                for p in [(3, 6), (2, 5), (4, 5), (3, 7), (2, 6), (4, 6)] {
                    put(GloomShroom, p);
                }
                fix_edge_fumes();
                assert!(!has(FumeShroom, (5, 5)));
                assert!(has(FumeShroom, (1, 5)));
            },
        );
    }
}
