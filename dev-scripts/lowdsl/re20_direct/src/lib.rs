#[rustfmt::skip]
#[rsvz::script]
fn script() {
    use rsvz::core::model::ReloadMode::MainUiOrFightUi;
    use rsvz::prelude::{
        CardSelection, CobManager, PlantKind, ZombieKind, card, card_cd, ensure_zombie_row, fire,
        auto_set_cobs, lineup, normalize_card_effect, recover_fire, set_cob_columns, set_sun_cost_ignored, shovel,
    };
    use rsvz::setup::{reload, select_cards, set_wave_zombies, set_zombies};

    reload(MainUiOrFightUi);
    lineup("LI4/bIyUhNTYOU6EEExMXN80h1FU4aRw2VA=");
    set_sun_cost_ignored(true);
    set_zombies("杆车丑梯篮白红跳");
    set_wave_zombies(20, "普杆障车丑梯篮白红跳");
    select_cards("IIKJAPTNWF");

    if card_cd(PlantKind::IceShroom)
        < card_cd(CardSelection::Imitator(PlantKind::IceShroom))
    {
        rsvz::try_at(4, -20, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 1, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 4981, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 4, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 9982, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 1, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 14983, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 5, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 19984, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 4, 9);
            Ok(())
        })?;
        rsvz::try_at(1, 225, || {
            card(PlantKind::IceShroom, 3, 9);
            Ok(())
        })?;
        rsvz::try_at(1, 5226, || {
            card(PlantKind::IceShroom, 1, 9);
            Ok(())
        })?;
        rsvz::try_at(1, 10227, || {
            card(PlantKind::IceShroom, 2, 9);
            Ok(())
        })?;
        rsvz::try_at(15, 1050, || {
            card(PlantKind::IceShroom, 4, 9);
            Ok(())
        })?;
        rsvz::try_at(19, 2420, || {
            card(PlantKind::IceShroom, 5, 9);
            Ok(())
        })?;
        rsvz::try_at(19, 7421, || {
            card(PlantKind::IceShroom, 1, 9);
            Ok(())
        })?;
        rsvz::try_at(4, -25, || {
            shovel(1, 9);
            card(PlantKind::FlowerPot, 1, 9);
            Ok(())
        })?;
        rsvz::try_at(17, 1050, || {
            card(PlantKind::FlowerPot, 5, 9);
            Ok(())
        })?;
    } else {
        rsvz::try_at(4, 300, || {
            card(PlantKind::IceShroom, 1, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 5301, || {
            card(PlantKind::IceShroom, 4, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 10302, || {
            card(PlantKind::IceShroom, 1, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 15303, || {
            card(PlantKind::IceShroom, 5, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 20304, || {
            card(PlantKind::IceShroom, 4, 9);
            Ok(())
        })?;
        rsvz::try_at(1, -95, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 3, 9);
            Ok(())
        })?;
        rsvz::try_at(1, 4906, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 1, 9);
            Ok(())
        })?;
        rsvz::try_at(1, 9907, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 2, 9);
            Ok(())
        })?;
        rsvz::try_at(15, 1050, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 4, 9);
            Ok(())
        })?;
        rsvz::try_at(19, 2420, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 5, 9);
            Ok(())
        })?;
        rsvz::try_at(19, 7421, || {
            card(CardSelection::Imitator(PlantKind::IceShroom), 1, 9);
            Ok(())
        })?;
        rsvz::try_at(4, 150, || {
            shovel(1, 9);
            card(PlantKind::FlowerPot, 1, 9);
            Ok(())
        })?;
        rsvz::try_at(17, 1307, || {
            card(PlantKind::FlowerPot, 5, 9);
            Ok(())
        })?;
    }

    rsvz::try_at(1, 32, || {
        card(PlantKind::CoffeeBean, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(3, -198, || {
        card(PlantKind::CoffeeBean, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(5, -199, || {
        card(PlantKind::CoffeeBean, 3, 9);
        Ok(())
    })?;
    rsvz::try_at(7, -198, || {
        card(PlantKind::CoffeeBean, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(8, 304, || {
        card(PlantKind::CoffeeBean, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(11, -198, || {
        card(PlantKind::CoffeeBean, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(13, -198, || {
        card(PlantKind::CoffeeBean, 2, 9);
        Ok(())
    })?;
    rsvz::try_at(14, 304, || {
        card(PlantKind::CoffeeBean, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(16, 304, || {
        card(PlantKind::CoffeeBean, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(18, 304, || {
        card(PlantKind::CoffeeBean, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(20, -300, || {
        card(PlantKind::CoffeeBean, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(2, 126, || {
        card(PlantKind::CherryBomb, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(3, 0, || {
        ensure_zombie_row(ZombieKind::Zomboni, 5);
        Ok(())
    })?;
    rsvz::try_at(3, -66, || {
        card(PlantKind::Jalapeno, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(3, 374, || {
        card(PlantKind::Squash, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(4, 480, || {
        card(PlantKind::Pumpkin, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(4, 610, || {
        shovel(1, 9, true);
        Ok(())
    })?;
    rsvz::try_at(5, 652, || {
        card([PlantKind::DoomShroom, PlantKind::CoffeeBean], 3, 9);
        Ok(())
    })?;
    rsvz::try_at(5, 850, || {
        card(PlantKind::FlowerPot, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(6, 248, || {
        card(PlantKind::TallNut, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(6, 500, || {
        card(PlantKind::FlowerPot, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(6, 1251, || {
        card(PlantKind::FlowerPot, 2, 9);
        Ok(())
    })?;
    rsvz::try_at(6, 600, || {
        shovel(4, 9);
        shovel(5, 9);
        Ok(())
    })?;
    rsvz::try_at(6, 601, || {
        shovel(4, 9);
        Ok(())
    })?;
    rsvz::try_at(7, 1029, || {
        card(PlantKind::CherryBomb, 2, 9);
        Ok(())
    })?;
    rsvz::try_at(8, 260, || {
        shovel(2, 9);
        card(PlantKind::FlowerPot, 2, 9);
        Ok(())
    })?;
    rsvz::try_at(9, 0, || {
        ensure_zombie_row(ZombieKind::GigaGargantuar, 1);
        ensure_zombie_row(ZombieKind::Zomboni, 2);
        Ok(())
    })?;
    rsvz::try_at(9, 101, || {
        card(PlantKind::Jalapeno, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(9, 19, || {
        card(PlantKind::Squash, 2, 9);
        Ok(())
    })?;
    rsvz::try_at(9, 1162, || {
        shovel(1, 9);
        shovel(2, 9);
        Ok(())
    })?;
    rsvz::try_at(9, 591, || {
        card(PlantKind::FlowerPot, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(9, 1374, || {
        card(PlantKind::FlowerPot, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(9, 2400, || {
        card(PlantKind::FlowerPot, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(9, 2265, || {
        shovel(1, 9);
        Ok(())
    })?;
    rsvz::try_at(9, 3151, || {
        card(PlantKind::FlowerPot, 2, 9);
        Ok(())
    })?;
    rsvz::try_at(11, -4, || {
        card(PlantKind::CherryBomb, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(11, 950, || {
        shovel(4, 9);
        Ok(())
    })?;
    rsvz::try_at(12, 150, || {
        card(PlantKind::Jalapeno, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(12, 480, || {
        card(PlantKind::Pumpkin, 2, 9);
        Ok(())
    })?;
    rsvz::try_at(12, 610, || {
        shovel(2, 9, true);
        Ok(())
    })?;
    rsvz::try_at(13, 652, || {
        card([PlantKind::DoomShroom, PlantKind::CoffeeBean], 2, 9);
        Ok(())
    })?;
    rsvz::try_at(13, 819, || {
        card(PlantKind::FlowerPot, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(14, 420, || {
        card(PlantKind::FlowerPot, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(14, 600, || {
        shovel(4, 9);
        shovel(5, 9);
        Ok(())
    })?;
    rsvz::try_at(15, 1050, || {
        shovel(1, 9);
        Ok(())
    })?;
    rsvz::try_at(17, 51, || {
        card(PlantKind::CherryBomb, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(17, 1050, || {
        shovel(4, 9);
        Ok(())
    })?;
    rsvz::try_at(19, 0, || {
        ensure_zombie_row(ZombieKind::GigaGargantuar, 5);
        Ok(())
    })?;
    rsvz::try_at(19, 1050, || {
        shovel(5, 9);
        Ok(())
    })?;
    rsvz::try_at(19, 1265, || {
        card(PlantKind::FlowerPot, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(19, 2300, || {
        shovel(5, 9);
        Ok(())
    })?;
    rsvz::try_at(19, 3201, || {
        card(PlantKind::FlowerPot, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(19, 3952, || {
        card(PlantKind::FlowerPot, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(20, 0, || {
        ensure_zombie_row(ZombieKind::Normal, 5);
        ensure_zombie_row(ZombieKind::Conehead, 5);
        Ok(())
    })?;
    rsvz::try_at(20, 249, || {
        card(PlantKind::TallNut, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(20, 5950, || {
        shovel(5, 9);
        Ok(())
    })?;
    rsvz::try_at(20, 5951, || {
        shovel(5, 9);
        Ok(())
    })?;
    rsvz::try_at(20, 5952, || {
        shovel(5, 9);
        Ok(())
    })?;

    rsvz::try_at(1, 321, || {
        normalize_card_effect(PlantKind::IceShroom, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(3, 91, || {
        normalize_card_effect(PlantKind::IceShroom, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(5, 90, || {
        normalize_card_effect(PlantKind::IceShroom, 3, 9);
        Ok(())
    })?;
    rsvz::try_at(7, 91, || {
        normalize_card_effect(PlantKind::IceShroom, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(8, 593, || {
        normalize_card_effect(PlantKind::IceShroom, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(11, 91, || {
        normalize_card_effect(PlantKind::IceShroom, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(13, 91, || {
        normalize_card_effect(PlantKind::IceShroom, 2, 9);
        Ok(())
    })?;
    rsvz::try_at(14, 593, || {
        normalize_card_effect(PlantKind::IceShroom, 1, 9);
        Ok(())
    })?;
    rsvz::try_at(16, 593, || {
        normalize_card_effect(PlantKind::IceShroom, 4, 9);
        Ok(())
    })?;
    rsvz::try_at(18, 593, || {
        normalize_card_effect(PlantKind::IceShroom, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(20, -11, || {
        normalize_card_effect(PlantKind::IceShroom, 5, 9);
        Ok(())
    })?;
    rsvz::try_at(5, 941, || {
        normalize_card_effect(PlantKind::DoomShroom, 3, 9);
        Ok(())
    })?;
    rsvz::try_at(13, 941, || {
        normalize_card_effect(PlantKind::DoomShroom, 2, 9);
        Ok(())
    })?;

    let l1 = CobManager::new();
    let l3 = CobManager::new();
    let l5 = CobManager::new();
    let l7 = CobManager::new();

    rsvz::try_at(1, -599, {
        let l1 = l1.clone();
        let l3 = l3.clone();
        let l5 = l5.clone();
        let l7 = l7.clone();
        move || {
            auto_set_cobs();
            set_cob_columns(&l1, 1);
            set_cob_columns(&l3, 3);
            set_cob_columns(&l5, 5);
            set_cob_columns(&l7, 7);
            Ok(())
        }
    })?;

    rsvz::try_at(1, -162, {
        let l1 = l1.clone();
        let l7 = l7.clone();
        move || {
            fire(&l1, 2, 9.1625);
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(1, 336, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(2, -162, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(2, -117, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(2, -16, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 8.9125);
            Ok(())
        }
    })?;
    rsvz::try_at(2, -7, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(2, 94, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(2, 309, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(3, 563, {
        let l1 = l1.clone();
        let l5 = l5.clone();
        move || {
            fire(&l1, 2, 9.1625);
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(3, 775, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 8.925);
            Ok(())
        }
    })?;
    rsvz::try_at(3, 817, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 8.4875);
            Ok(())
        }
    })?;
    rsvz::try_at(4, -162, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(4, -138, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 9.9125);
            Ok(())
        }
    })?;
    rsvz::try_at(4, -83, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 8.225);
            Ok(())
        }
    })?;
    rsvz::try_at(4, -28, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(4, 208, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(4, 309, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(5, 301, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 8.2625);
            Ok(())
        }
    })?;
    rsvz::try_at(5, 775, {
        let l1 = l1.clone();
        let l7 = l7.clone();
        move || {
            fire(&l1, 2, 8.925);
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(6, -162, {
        let l5 = l5.clone();
        let l3 = l3.clone();
        move || {
            fire(&l5, 4, 9.0);
            fire(&l3, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(6, -52, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 4, 7.8);
            Ok(())
        }
    })?;
    rsvz::try_at(6, -16, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 8.9125);
            Ok(())
        }
    })?;
    rsvz::try_at(6, 94, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(6, 307, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(7, 113, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 7.5);
            Ok(())
        }
    })?;
    rsvz::try_at(7, 563, {
        let l1 = l1.clone();
        let l5 = l5.clone();
        move || {
            fire(&l1, 2, 9.1625);
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(7, 775, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 4, 8.925);
            Ok(())
        }
    })?;
    rsvz::try_at(8, -162, {
        let l7 = l7.clone();
        let l5 = l5.clone();
        move || {
            fire(&l7, 1, 8.65);
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(8, -52, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 7.8);
            Ok(())
        }
    })?;
    rsvz::try_at(8, -16, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 8.9125);
            Ok(())
        }
    })?;
    rsvz::try_at(8, 94, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(9, 204, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(9, 429, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 8.2875);
            Ok(())
        }
    })?;
    rsvz::try_at(9, 563, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(9, 775, {
        let l1 = l1.clone();
        let l7 = l7.clone();
        move || {
            fire(&l1, 2, 8.55);
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(9, 987, {
        let l5 = l5.clone();
        let l3 = l3.clone();
        move || {
            fire(&l5, 4, 9.0);
            fire(&l3, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(9, 1212, {
        let l3 = l3.clone();
        let l1 = l1.clone();
        move || {
            fire(&l3, 2, 9.0);
            fire(&l1, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(9, 2363, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 1, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(10, -162, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(10, -135, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 3, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(10, -25, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 9.1125);
            Ok(())
        }
    })?;
    rsvz::try_at(10, -8, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(10, 85, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(10, 127, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(10, 247, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(11, 563, {
        let l1 = l1.clone();
        let l5 = l5.clone();
        move || {
            fire(&l1, 2, 9.1625);
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(11, 775, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(11, 817, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 8.4875);
            Ok(())
        }
    })?;
    rsvz::try_at(12, -162, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(12, -138, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 9.1625);
            Ok(())
        }
    })?;
    rsvz::try_at(12, -83, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 8.225);
            Ok(())
        }
    })?;
    rsvz::try_at(12, -28, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(12, 309, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(12, 403, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 8.625);
            Ok(())
        }
    })?;
    rsvz::try_at(13, 313, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 8.1125);
            Ok(())
        }
    })?;
    rsvz::try_at(13, 775, {
        let l1 = l1.clone();
        let l7 = l7.clone();
        move || {
            fire(&l1, 2, 9.0);
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(14, -162, {
        let l3 = l3.clone();
        let l7 = l7.clone();
        move || {
            fire(&l3, 2, 9.0);
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(14, -16, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 8.9125);
            Ok(())
        }
    })?;
    rsvz::try_at(14, -12, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 7.8);
            Ok(())
        }
    })?;
    rsvz::try_at(14, 94, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 2, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(14, 247, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(15, 53, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 5, 7.8);
            Ok(())
        }
    })?;
    rsvz::try_at(15, 663, {
        let l1 = l1.clone();
        let l5 = l5.clone();
        move || {
            fire(&l1, 2, 9.1625);
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(15, 875, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 4, 8.7875);
            Ok(())
        }
    })?;
    rsvz::try_at(15, 917, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 1, 8.4);
            Ok(())
        }
    })?;
    rsvz::try_at(16, -162, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(16, -138, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 4, 9.1625);
            Ok(())
        }
    })?;
    rsvz::try_at(16, -83, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 1, 8.225);
            Ok(())
        }
    })?;
    rsvz::try_at(16, -28, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 4, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(17, 204, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 1, 8.375);
            Ok(())
        }
    })?;
    rsvz::try_at(17, 443, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 1, 8.0625);
            Ok(())
        }
    })?;
    rsvz::try_at(17, 663, {
        let l5 = l5.clone();
        let l1 = l1.clone();
        move || {
            fire(&l5, 4, 9.0);
            fire(&l1, 2, 8.6625);
            Ok(())
        }
    })?;
    rsvz::try_at(17, 875, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 4, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(17, 920, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 1, 8.4);
            Ok(())
        }
    })?;
    rsvz::try_at(18, -162, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(18, -138, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 4, 9.1625);
            Ok(())
        }
    })?;
    rsvz::try_at(18, -83, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 1, 8.225);
            Ok(())
        }
    })?;
    rsvz::try_at(18, -28, {
        let l3 = l3.clone();
        move || {
            fire(&l3, 4, 9.9875);
            Ok(())
        }
    })?;
    rsvz::try_at(18, 247, {
        let l7 = l7.clone();
        move || {
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(19, 204, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 8.6375);
            Ok(())
        }
    })?;
    rsvz::try_at(19, 443, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 8.275);
            Ok(())
        }
    })?;
    rsvz::try_at(19, 663, {
        let l3 = l3.clone();
        let l5 = l5.clone();
        move || {
            fire(&l3, 2, 8.7875);
            fire(&l5, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(19, 878, {
        let l5 = l5.clone();
        let l7 = l7.clone();
        move || {
            fire(&l5, 2, 9.0);
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(19, 1093, {
        let l1 = l1.clone();
        let l7 = l7.clone();
        move || {
            fire(&l1, 2, 9.0);
            fire(&l7, 4, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(19, 2163, {
        let l5 = l5.clone();
        move || {
            fire(&l5, 5, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(20, -162, {
        let l5 = l5.clone();
        let l1 = l1.clone();
        move || {
            fire(&l5, 4, 9.0);
            fire(&l1, 2, 9.1625);
            Ok(())
        }
    })?;
    rsvz::try_at(20, -138, {
        let l7 = l7.clone();
        let l3 = l3.clone();
        move || {
            fire(&l7, 2, 9.0);
            fire(&l7, 4, 9.0);
            fire(&l7, 4, 9.0);
            fire(&l3, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(20, -28, {
        let l3 = l3.clone();
        let l1 = l1.clone();
        move || {
            fire(&l3, 4, 9.9875);
            fire(&l1, 2, 9.0);
            Ok(())
        }
    })?;
    rsvz::try_at(20, 426, {
        let l1 = l1.clone();
        move || {
            fire(&l1, 2, 8.8);
            fire(&l1, 4, 8.8);
            Ok(())
        }
    })?;

    rsvz::try_at(9, 1500, || {
        recover_fire(3, 9.0);
        recover_fire(4, 9.0);
        Ok(())
    })?;
    rsvz::try_at(19, 1500, || {
        recover_fire(3, 9.0);
        recover_fire(2, 9.0);
        Ok(())
    })?;
    rsvz::try_at(20, 5500, || {
        recover_fire(5, 9.0);
        Ok(())
    })?;

    rsvz::try_at(1, 282, || {
        rsvz::plant_fixer::start_plant_fixer(PlantKind::TallNut, [(5, 9)])?;
        rsvz::plant_fixer::set_plant_fixer_hp(50)
    })?;
}
