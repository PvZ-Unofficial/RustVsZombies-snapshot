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
fn copy_danger(pos: Pos) -> bool {
    rsvz::with_backend(|b| {
        b.zombies().expect("zombies").any(|z| {
            if b.zombie_row(z) != pos.0 - 1 || b.zombie_kind(z).expect("kind") != Z::GigaGargantuar {
                return false;
            }
            let state = b.zombie_phase(z).expect("phase") as i32;
            let front = (40 + (pos.1 - 1) * 80 + 50) as f32;
            let x = b.zombie_pos_x(z);
            front < x && ((state == 0 && x <= front + 80.0) || (state == 69 && x <= front + 50.0))
        })
    })
}
fn copy_ice(fallback: bool) -> RuntimeResult<bool> {
    if !usable(MIMIC_ICE) {
        return Ok(false);
    }
    if fallback {
        for row in [1, 5] {
            let pos = (row, 4);
            if copy_danger(pos) || blover_waiting(pos) {
                continue;
            }
            let other = rsvz::with_backend(|b| {
                let mut kind = None;
                b.for_each_plant_at_anchor_grid(grid(pos), |p| {
                    let k = b.plant_raw_kind(p)?;
                    if k != Pumpkin {
                        kind = Some(k);
                    }
                    Ok(())
                })
                .expect("plant");
                kind
            });
            if let Some(k) = other {
                remove(k, pos)?;
                return Ok(false);
            }
            if play(MIMIC_ICE, pos) {
                return Ok(true);
            }
        }
    } else {
        const POS: [Pos; 6] = [(1, 5), (5, 5), (1, 6), (5, 6), (2, 7), (4, 7)];
        for pos in POS {
            if !copy_danger(pos) && main_kind(pos).is_none() && play(MIMIC_ICE, pos) {
                return Ok(true);
            }
        }
        for pos in POS {
            if !copy_danger(pos) && !blover_waiting(pos) && main_kind(pos).is_some() {
                shovel(pos)?;
                break;
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
fn fix_pumpkins() {
    if !usable(sel(Pumpkin)) {
        return;
    }
    // Stable order breaks equal predicted lifetimes exactly as the source script.
    const POS: [(Pos, f32); 12] = [
        ((1, 1), 8.0),
        ((2, 1), 3.0),
        ((3, 1), 1.0),
        ((4, 1), 3.0),
        ((5, 1), 8.0),
        ((3, 6), 2.0),
        ((3, 7), 4.0),
        ((2, 5), 1.0),
        ((4, 5), 1.0),
        ((2, 4), 1.0),
        ((3, 4), 1.0),
        ((4, 4), 1.0),
    ];
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
        for pos in POS {
            if main_kind(pos).is_some() && rsvz::is_safe_blover(grid(pos))? {
                shovel(pos)?;
                play(sel(Blover), pos);
                break;
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
                    play(sel(Squash), (1, 5));
                }
            } else if !giga_level && !garg_level {
                try_positions(sel(Squash), &[(3, 8), (3, 9)]);
            } else {
                try_positions(sel(Squash), &[(3, 9), (3, 8)]);
            }
        }
        if inside == 1 && giga + nr(&[Z::Gargantuar], 3) > 0 {
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
                self.copy_wait = None;
                self.copy_fallback = false;
            } else {
                let start = *self.copy_wait.get_or_insert(now);
                advance = false;
                if !self.copy_fallback && now - start >= 200 {
                    self.copy_fallback = true;
                    if copy_ice(true)? {
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
            if !try_positions(sel(IceShroom), &[(5, 6), (1, 6)]) {
                advance = false;
            }
            self.clock += 3422 - inside;
            inside = 3422;
        } else if inside == 3422 && need_ice && !try_positions(sel(IceShroom), &[(5, 6), (1, 6)]) {
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
        const HP0: [i32; 5] = [300, 500, 200, 500, 300];
        const K: [i32; 5] = [40, 50, 70, 50, 40];
        let mut threat = [0i32; 5];
        rsvz::with_backend(|b| {
            for z in b.zombies().expect("zombies") {
                if !GIANTS.contains(&b.zombie_kind(z).expect("kind")) || b.zombie_phase(z).expect("phase") as i32 != 0 {
                    continue;
                }
                let r = b.zombie_row(z) as usize;
                let x = b.zombie_pos_x(z);
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
        for row in [2, 4] {
            if count(&[Z::GigaGargantuar], &[], &[row], (-600.0, 521.0), (400, 6000)) > 0
                && nx(&[Z::Normal], row, (-600.0, 511.0)) == 0
            {
                play(sel(Pumpkin), (row, 6));
            }
        }
        for pos in [(3, 6), (2, 5), (4, 5), (3, 7), (2, 6), (4, 6)] {
            fix_gloom(pos, pos == (3, 7));
        }
        let dangerous = allowed(Z::JackInTheBox) || giga_level;
        if !dangerous && brought(SunShroom) {
            for pos in [(5, 4), (1, 4)] {
                if has(FumeShroom, pos) {
                    remove(FumeShroom, pos)?;
                }
            }
            try_positions(sel(SunShroom), &[(5, 4), (1, 4)]);
        } else if dangerous {
            for pos in [(5, 4), (1, 4)] {
                if has(SunShroom, pos) {
                    remove(SunShroom, pos)?;
                }
            }
            try_positions(sel(FumeShroom), &[(5, 4), (1, 4)]);
        }
        if giga == nr(&[Z::GigaGargantuar], 1) + nr(&[Z::GigaGargantuar], 5) && nr(&[Z::Gargantuar], 3) == 0 {
            kill_jack()?;
        }
        if giga + garg == 0 && !giga_level && !garg_level && !car_level && (football || allowed(Z::Football)) {
            try_positions(sel(Pumpkin), &[(2, 6), (4, 6)]);
        }
        if giga == 0 && allowed(Z::Football) && hp(Pumpkin, (3, 7)).is_some_and(|h| (0..=500).contains(&h)) {
            play(sel(CherryBomb), (3, 8));
        }
        self.balloon()?;
        if giga == 0 && !football {
            try_positions(sel(SunShroom), &[(1, 5), (5, 5)]);
        }
        try_positions(sel(Sunflower), &[(1, 1), (3, 1), (5, 1)]);
        try_positions(sel(TwinSunflower), &[(1, 1), (3, 1), (5, 1)]);
        fix_pumpkins();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{put, scene, scene_with_setup};
    use super::*;

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
