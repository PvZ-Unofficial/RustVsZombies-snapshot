use rsvz_model::model::{CardSelection, CheckedCardSelection, GridItemKind, PlantKind, ZombieKind};

use crate::{PeBackendError, Result};

pub const fn plant_from_pe(kind: pe_rs::PlantType) -> Result<PlantKind> {
    Ok(match kind {
        pe_rs::PlantType::PeaShooter => PlantKind::Peashooter,
        pe_rs::PlantType::Sunflower => PlantKind::Sunflower,
        pe_rs::PlantType::CherryBomb => PlantKind::CherryBomb,
        pe_rs::PlantType::WallNut => PlantKind::WallNut,
        pe_rs::PlantType::PotatoMine => PlantKind::PotatoMine,
        pe_rs::PlantType::SnowPea => PlantKind::SnowPea,
        pe_rs::PlantType::Chomper => PlantKind::Chomper,
        pe_rs::PlantType::Repeater => PlantKind::Repeater,
        pe_rs::PlantType::PuffShroom => PlantKind::PuffShroom,
        pe_rs::PlantType::SunShroom => PlantKind::SunShroom,
        pe_rs::PlantType::FumeShroom => PlantKind::FumeShroom,
        pe_rs::PlantType::GraveBuster => PlantKind::GraveBuster,
        pe_rs::PlantType::HypnoShroom => PlantKind::HypnoShroom,
        pe_rs::PlantType::ScaredyShroom => PlantKind::ScaredyShroom,
        pe_rs::PlantType::IceShroom => PlantKind::IceShroom,
        pe_rs::PlantType::DoomShroom => PlantKind::DoomShroom,
        pe_rs::PlantType::LilyPad => PlantKind::LilyPad,
        pe_rs::PlantType::Squash => PlantKind::Squash,
        pe_rs::PlantType::Threepeater => PlantKind::Threepeater,
        pe_rs::PlantType::TangleKelp => PlantKind::TangleKelp,
        pe_rs::PlantType::Jalapeno => PlantKind::Jalapeno,
        pe_rs::PlantType::Spikeweed => PlantKind::Spikeweed,
        pe_rs::PlantType::Torchwood => PlantKind::Torchwood,
        pe_rs::PlantType::TallNut => PlantKind::TallNut,
        pe_rs::PlantType::SeaShroom => PlantKind::SeaShroom,
        pe_rs::PlantType::Plantern => PlantKind::Plantern,
        pe_rs::PlantType::Cactus => PlantKind::Cactus,
        pe_rs::PlantType::Blover => PlantKind::Blover,
        pe_rs::PlantType::SplitPea => PlantKind::SplitPea,
        pe_rs::PlantType::Starfruit => PlantKind::Starfruit,
        pe_rs::PlantType::Pumpkin => PlantKind::Pumpkin,
        pe_rs::PlantType::MagnetShroom => PlantKind::MagnetShroom,
        pe_rs::PlantType::CabbagePult => PlantKind::CabbagePult,
        pe_rs::PlantType::FlowerPot => PlantKind::FlowerPot,
        pe_rs::PlantType::KernelPult => PlantKind::KernelPult,
        pe_rs::PlantType::CoffeeBean => PlantKind::CoffeeBean,
        pe_rs::PlantType::Garlic => PlantKind::Garlic,
        pe_rs::PlantType::UmbrellaLeaf => PlantKind::UmbrellaLeaf,
        pe_rs::PlantType::Marigold => PlantKind::Marigold,
        pe_rs::PlantType::MelonPult => PlantKind::MelonPult,
        pe_rs::PlantType::GatlingPea => PlantKind::GatlingPea,
        pe_rs::PlantType::TwinSunflower => PlantKind::TwinSunflower,
        pe_rs::PlantType::GloomShroom => PlantKind::GloomShroom,
        pe_rs::PlantType::Cattail => PlantKind::Cattail,
        pe_rs::PlantType::WinterMelon => PlantKind::WinterMelon,
        pe_rs::PlantType::GoldMagnet => PlantKind::GoldMagnet,
        pe_rs::PlantType::Spikerock => PlantKind::Spikerock,
        pe_rs::PlantType::CobCannon => PlantKind::CobCannon,
        pe_rs::PlantType::Imitater => PlantKind::Imitator,
        pe_rs::PlantType::None => {
            return Err(PeBackendError::InvalidKind { kind: "plant", raw: -1 });
        }
    })
}

pub const fn plant_to_pe(kind: PlantKind) -> pe_rs::PlantType {
    match kind {
        PlantKind::Peashooter => pe_rs::PlantType::PeaShooter,
        PlantKind::Sunflower => pe_rs::PlantType::Sunflower,
        PlantKind::CherryBomb => pe_rs::PlantType::CherryBomb,
        PlantKind::WallNut => pe_rs::PlantType::WallNut,
        PlantKind::PotatoMine => pe_rs::PlantType::PotatoMine,
        PlantKind::SnowPea => pe_rs::PlantType::SnowPea,
        PlantKind::Chomper => pe_rs::PlantType::Chomper,
        PlantKind::Repeater => pe_rs::PlantType::Repeater,
        PlantKind::PuffShroom => pe_rs::PlantType::PuffShroom,
        PlantKind::SunShroom => pe_rs::PlantType::SunShroom,
        PlantKind::FumeShroom => pe_rs::PlantType::FumeShroom,
        PlantKind::GraveBuster => pe_rs::PlantType::GraveBuster,
        PlantKind::HypnoShroom => pe_rs::PlantType::HypnoShroom,
        PlantKind::ScaredyShroom => pe_rs::PlantType::ScaredyShroom,
        PlantKind::IceShroom => pe_rs::PlantType::IceShroom,
        PlantKind::DoomShroom => pe_rs::PlantType::DoomShroom,
        PlantKind::LilyPad => pe_rs::PlantType::LilyPad,
        PlantKind::Squash => pe_rs::PlantType::Squash,
        PlantKind::Threepeater => pe_rs::PlantType::Threepeater,
        PlantKind::TangleKelp => pe_rs::PlantType::TangleKelp,
        PlantKind::Jalapeno => pe_rs::PlantType::Jalapeno,
        PlantKind::Spikeweed => pe_rs::PlantType::Spikeweed,
        PlantKind::Torchwood => pe_rs::PlantType::Torchwood,
        PlantKind::TallNut => pe_rs::PlantType::TallNut,
        PlantKind::SeaShroom => pe_rs::PlantType::SeaShroom,
        PlantKind::Plantern => pe_rs::PlantType::Plantern,
        PlantKind::Cactus => pe_rs::PlantType::Cactus,
        PlantKind::Blover => pe_rs::PlantType::Blover,
        PlantKind::SplitPea => pe_rs::PlantType::SplitPea,
        PlantKind::Starfruit => pe_rs::PlantType::Starfruit,
        PlantKind::Pumpkin => pe_rs::PlantType::Pumpkin,
        PlantKind::MagnetShroom => pe_rs::PlantType::MagnetShroom,
        PlantKind::CabbagePult => pe_rs::PlantType::CabbagePult,
        PlantKind::FlowerPot => pe_rs::PlantType::FlowerPot,
        PlantKind::KernelPult => pe_rs::PlantType::KernelPult,
        PlantKind::CoffeeBean => pe_rs::PlantType::CoffeeBean,
        PlantKind::Garlic => pe_rs::PlantType::Garlic,
        PlantKind::UmbrellaLeaf => pe_rs::PlantType::UmbrellaLeaf,
        PlantKind::Marigold => pe_rs::PlantType::Marigold,
        PlantKind::MelonPult => pe_rs::PlantType::MelonPult,
        PlantKind::GatlingPea => pe_rs::PlantType::GatlingPea,
        PlantKind::TwinSunflower => pe_rs::PlantType::TwinSunflower,
        PlantKind::GloomShroom => pe_rs::PlantType::GloomShroom,
        PlantKind::Cattail => pe_rs::PlantType::Cattail,
        PlantKind::WinterMelon => pe_rs::PlantType::WinterMelon,
        PlantKind::GoldMagnet => pe_rs::PlantType::GoldMagnet,
        PlantKind::Spikerock => pe_rs::PlantType::Spikerock,
        PlantKind::CobCannon => pe_rs::PlantType::CobCannon,
        PlantKind::Imitator => pe_rs::PlantType::Imitater,
    }
}

pub const fn zombie_from_pe(kind: pe_rs::ZombieType) -> Result<ZombieKind> {
    Ok(match kind {
        pe_rs::ZombieType::Normal => ZombieKind::Normal,
        pe_rs::ZombieType::Flag => ZombieKind::Flag,
        pe_rs::ZombieType::Conehead => ZombieKind::Conehead,
        pe_rs::ZombieType::PoleVaulting => ZombieKind::PoleVaulting,
        pe_rs::ZombieType::Buckethead => ZombieKind::Buckethead,
        pe_rs::ZombieType::Newspaper => ZombieKind::Newspaper,
        pe_rs::ZombieType::ScreenDoor => ZombieKind::ScreenDoor,
        pe_rs::ZombieType::Football => ZombieKind::Football,
        pe_rs::ZombieType::Dancing => ZombieKind::Dancing,
        pe_rs::ZombieType::BackupDancer => ZombieKind::BackupDancer,
        pe_rs::ZombieType::DuckyTube => ZombieKind::DuckyTube,
        pe_rs::ZombieType::Snorkel => ZombieKind::Snorkel,
        pe_rs::ZombieType::Zomboni => ZombieKind::Zomboni,
        pe_rs::ZombieType::DolphinRider => ZombieKind::DolphinRider,
        pe_rs::ZombieType::JackInTheBox => ZombieKind::JackInTheBox,
        pe_rs::ZombieType::Balloon => ZombieKind::Balloon,
        pe_rs::ZombieType::Digger => ZombieKind::Digger,
        pe_rs::ZombieType::Pogo => ZombieKind::Pogo,
        pe_rs::ZombieType::Yeti => ZombieKind::Yeti,
        pe_rs::ZombieType::Bungee => ZombieKind::Bungee,
        pe_rs::ZombieType::Ladder => ZombieKind::Ladder,
        pe_rs::ZombieType::Catapult => ZombieKind::Catapult,
        pe_rs::ZombieType::Gargantuar => ZombieKind::Gargantuar,
        pe_rs::ZombieType::Imp => ZombieKind::Imp,
        pe_rs::ZombieType::GigaGargantuar => ZombieKind::GigaGargantuar,
        pe_rs::ZombieType::None => {
            return Err(PeBackendError::InvalidKind {
                kind: "zombie",
                raw: -1,
            });
        }
    })
}

pub fn zombie_to_pe(kind: ZombieKind) -> Result<pe_rs::ZombieType> {
    Ok(match kind {
        ZombieKind::Normal => pe_rs::ZombieType::Normal,
        ZombieKind::Flag => pe_rs::ZombieType::Flag,
        ZombieKind::Conehead => pe_rs::ZombieType::Conehead,
        ZombieKind::PoleVaulting => pe_rs::ZombieType::PoleVaulting,
        ZombieKind::Buckethead => pe_rs::ZombieType::Buckethead,
        ZombieKind::Newspaper => pe_rs::ZombieType::Newspaper,
        ZombieKind::ScreenDoor => pe_rs::ZombieType::ScreenDoor,
        ZombieKind::Football => pe_rs::ZombieType::Football,
        ZombieKind::Dancing => pe_rs::ZombieType::Dancing,
        ZombieKind::BackupDancer => pe_rs::ZombieType::BackupDancer,
        ZombieKind::DuckyTube => pe_rs::ZombieType::DuckyTube,
        ZombieKind::Snorkel => pe_rs::ZombieType::Snorkel,
        ZombieKind::Zomboni => pe_rs::ZombieType::Zomboni,
        ZombieKind::DolphinRider => pe_rs::ZombieType::DolphinRider,
        ZombieKind::JackInTheBox => pe_rs::ZombieType::JackInTheBox,
        ZombieKind::Balloon => pe_rs::ZombieType::Balloon,
        ZombieKind::Digger => pe_rs::ZombieType::Digger,
        ZombieKind::Pogo => pe_rs::ZombieType::Pogo,
        ZombieKind::Yeti => pe_rs::ZombieType::Yeti,
        ZombieKind::Bungee => pe_rs::ZombieType::Bungee,
        ZombieKind::Ladder => pe_rs::ZombieType::Ladder,
        ZombieKind::Catapult => pe_rs::ZombieType::Catapult,
        ZombieKind::Gargantuar => pe_rs::ZombieType::Gargantuar,
        ZombieKind::Imp => pe_rs::ZombieType::Imp,
        ZombieKind::Boss => {
            return Err(PeBackendError::unsupported_kind("zombie", "Boss"));
        }
        ZombieKind::GigaGargantuar => pe_rs::ZombieType::GigaGargantuar,
        ZombieKind::Bobsled => {
            return Err(PeBackendError::unsupported_kind("zombie", "Bobsled"));
        }
        ZombieKind::PeaHead => {
            return Err(PeBackendError::unsupported_kind("zombie", "PeaHead"));
        }
        ZombieKind::WallNutHead => {
            return Err(PeBackendError::unsupported_kind("zombie", "WallNutHead"));
        }
        ZombieKind::JalapenoHead => {
            return Err(PeBackendError::unsupported_kind("zombie", "JalapenoHead"));
        }
        ZombieKind::GatlingHead => {
            return Err(PeBackendError::unsupported_kind("zombie", "GatlingHead"));
        }
        ZombieKind::SquashHead => {
            return Err(PeBackendError::unsupported_kind("zombie", "SquashHead"));
        }
        ZombieKind::TallNutHead => {
            return Err(PeBackendError::unsupported_kind("zombie", "TallNutHead"));
        }
    })
}

pub const fn card_to_pe(selection: CheckedCardSelection) -> (pe_rs::PlantType, pe_rs::PlantType) {
    let packet = plant_to_pe(selection.packet_kind());
    let target = match selection.imitator_target() {
        Some(kind) => plant_to_pe(kind),
        None => pe_rs::PlantType::None,
    };
    (packet, target)
}

pub fn card_from_pe(plant_type: pe_rs::PlantType, imitater_type: pe_rs::PlantType) -> Result<CheckedCardSelection> {
    let packet = plant_from_pe(plant_type)?;
    let target = if matches!(imitater_type, pe_rs::PlantType::None) {
        None
    } else {
        Some(plant_from_pe(imitater_type)?)
    };
    CardSelection::from_packet_parts(packet, target)
        .map_err(|_error| PeBackendError::InvalidCardSelection("invalid packet/imitator relation"))
}

pub fn grid_item_from_pe(kind: pe_rs::GridItemType) -> Result<GridItemKind> {
    Ok(match kind {
        pe_rs::GridItemType::Grave => GridItemKind::Grave,
        pe_rs::GridItemType::Crater => GridItemKind::Crater,
        pe_rs::GridItemType::Ladder => GridItemKind::Ladder,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pe_plant_subset_round_trips_every_canonical_plant() {
        for code in 0..=48 {
            let kind = PlantKind::try_from_code(code).expect("canonical plant code");
            assert!(matches!(plant_from_pe(plant_to_pe(kind)), Ok(actual) if actual == kind));
        }
    }

    #[test]
    fn pe_zombie_subset_round_trips_and_keeps_unsupported_gaps_typed() {
        for code in 0..=32 {
            let kind = ZombieKind::try_from_code(code).expect("canonical zombie code");
            let unsupported = matches!(
                kind,
                ZombieKind::Bobsled
                    | ZombieKind::Boss
                    | ZombieKind::PeaHead
                    | ZombieKind::WallNutHead
                    | ZombieKind::JalapenoHead
                    | ZombieKind::GatlingHead
                    | ZombieKind::SquashHead
                    | ZombieKind::TallNutHead
            );
            match zombie_to_pe(kind) {
                Ok(pe) => {
                    assert!(!unsupported, "{kind:?} unexpectedly became supported");
                    assert!(matches!(zombie_from_pe(pe), Ok(actual) if actual == kind));
                }
                Err(PeBackendError::UnsupportedKind { kind: "zombie", .. }) => {
                    assert!(unsupported, "{kind:?} unexpectedly became unsupported");
                }
                Err(error) => panic!("unexpected conversion error for {kind:?}: {error}"),
            }
        }
    }

    #[test]
    fn pe_grid_item_subset_does_not_invent_missing_native_kinds() {
        assert!(matches!(
            grid_item_from_pe(pe_rs::GridItemType::Grave),
            Ok(GridItemKind::Grave)
        ));
        assert!(matches!(
            grid_item_from_pe(pe_rs::GridItemType::Crater),
            Ok(GridItemKind::Crater)
        ));
        assert!(matches!(
            grid_item_from_pe(pe_rs::GridItemType::Ladder),
            Ok(GridItemKind::Ladder)
        ));
        assert_eq!(GridItemKind::Rake.code(), 11);
    }
}
