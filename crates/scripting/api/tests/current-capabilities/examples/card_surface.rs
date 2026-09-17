use rsvz::prelude::*;

fn main() {
    let _ = try_card(PlantKind::Sunflower, 1, 1);
    let _ = try_card(SeedSlot::from_index_unchecked(0), 1, 1);
    let _ = try_card([PlantKind::LilyPad, PlantKind::DoomShroom], 3, 4);
    let _ = try_card(PlantKind::Sunflower, [(1, 1), (1, 2)]);
    let _ = try_card([PlantKind::Sunflower, PlantKind::Peashooter], [(1, 1), (1, 2)]);
    let _ = card([(PlantKind::Sunflower, 1, 1), (PlantKind::Peashooter, 1, 2)]);
    let _ = card(PlantKind::Sunflower, 1, 1);
    let _ = card(PlantKind::Sunflower, [(1, 1), (1, 2)]);
    let _read_cd: fn(PlantKind) -> i32 = card_cd::<PlantKind>;
    let _ = card_cd(PlantKind::Sunflower);
    let _ = try_normalize_card_effect(PlantKind::IceShroom, 1, 1);
    normalize_card_effect(PlantKind::IceShroom, 1, 1);
    let _ = try_set_sun_cost_ignored(false);
    set_sun_cost_ignored(false);
}
