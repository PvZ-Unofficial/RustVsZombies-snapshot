use rsvz_model::model::{GameUi, ItemKind, PlantKind, SceneKind, ZombieKind, ZombiePhase};

use crate::error::{Pvz1051Error, Result};

macro_rules! define_pvz_kind {
    ($name:ident, $core:ident, $category:literal, [$($variant:ident),+ $(,)?]) => {
        #[repr(i32)]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub(crate) enum $name {
            $($variant = $core::$variant.code(),)+
        }
        impl $name {
            #[allow(dead_code, reason = "not every ABI wrapper kind needs an outbound raw conversion")]
            #[inline(always)]
            pub(crate) const fn raw(self) -> i32 {
                self as i32
            }
            pub(crate) fn from_raw(raw: i32) -> Result<Self> {
                let core = $core::try_from_code(raw).map_err(|_error| Pvz1051Error::UnknownRawKind {
                    category: $category,
                    raw,
                })?;
                Ok(Self::from(core))
            }
            pub(crate) const fn to_core(self) -> $core {
                match self {
                    $(Self::$variant => $core::$variant,)+
                }
            }
        }
        impl From<$core> for $name {
            fn from(kind: $core) -> Self {
                match kind {
                    $($core::$variant => Self::$variant,)+
                }
            }
        }
    };
}

define_pvz_kind!(
    PvzGameUi,
    GameUi,
    "game_ui",
    [Loading, Menu, LevelIntro, Playing, ZombiesWon, Award, Credit, Challenge,]
);

define_pvz_kind!(
    PvzSceneKind,
    SceneKind,
    "scene",
    [
        Day,
        Night,
        Pool,
        Fog,
        Roof,
        MoonNight,
        MushroomGarden,
        Greenhouse,
        Zombiquarium,
        TreeOfWisdom,
    ]
);

define_pvz_kind!(
    PvzPlantType,
    PlantKind,
    "plant",
    [
        Peashooter,
        Sunflower,
        CherryBomb,
        WallNut,
        PotatoMine,
        SnowPea,
        Chomper,
        Repeater,
        PuffShroom,
        SunShroom,
        FumeShroom,
        GraveBuster,
        HypnoShroom,
        ScaredyShroom,
        IceShroom,
        DoomShroom,
        LilyPad,
        Squash,
        Threepeater,
        TangleKelp,
        Jalapeno,
        Spikeweed,
        Torchwood,
        TallNut,
        SeaShroom,
        Plantern,
        Cactus,
        Blover,
        SplitPea,
        Starfruit,
        Pumpkin,
        MagnetShroom,
        CabbagePult,
        FlowerPot,
        KernelPult,
        CoffeeBean,
        Garlic,
        UmbrellaLeaf,
        Marigold,
        MelonPult,
        GatlingPea,
        TwinSunflower,
        GloomShroom,
        Cattail,
        WinterMelon,
        GoldMagnet,
        Spikerock,
        CobCannon,
        Imitator,
    ]
);

define_pvz_kind!(
    PvzZombieType,
    ZombieKind,
    "zombie",
    [
        Normal,
        Flag,
        Conehead,
        PoleVaulting,
        Buckethead,
        Newspaper,
        ScreenDoor,
        Football,
        Dancing,
        BackupDancer,
        DuckyTube,
        Snorkel,
        Zomboni,
        Bobsled,
        DolphinRider,
        JackInTheBox,
        Balloon,
        Digger,
        Pogo,
        Yeti,
        Bungee,
        Ladder,
        Catapult,
        Gargantuar,
        Imp,
        Boss,
        PeaHead,
        WallNutHead,
        JalapenoHead,
        GatlingHead,
        SquashHead,
        TallNutHead,
        GigaGargantuar,
    ]
);

pub(crate) fn game_ui_from_raw(raw: i32) -> Result<GameUi> {
    Ok(PvzGameUi::from_raw(raw)?.to_core())
}

pub(crate) fn scene_from_raw(raw: i32) -> Result<SceneKind> {
    Ok(PvzSceneKind::from_raw(raw)?.to_core())
}

pub(crate) fn plant_kind_from_raw(raw: i32) -> Result<PlantKind> {
    Ok(PvzPlantType::from_raw(raw)?.to_core())
}

pub(crate) fn zombie_kind_from_raw(raw: i32) -> Result<ZombieKind> {
    Ok(PvzZombieType::from_raw(raw)?.to_core())
}

pub(crate) fn zombie_phase_from_raw(raw: i32) -> Result<ZombiePhase> {
    ZombiePhase::try_from_code(raw).map_err(|_error| Pvz1051Error::UnknownRawKind {
        category: "zombie_phase",
        raw,
    })
}

pub(crate) fn item_kind_from_raw(raw: i32) -> Result<ItemKind> {
    ItemKind::try_from(raw).map_err(|raw| Pvz1051Error::UnknownRawKind { category: "item", raw })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_kind_conversions_are_explicit() {
        assert_eq!(PvzPlantType::from_raw(47).expect("cob"), PvzPlantType::CobCannon);
        assert_eq!(PvzPlantType::from_raw(48).expect("imitator"), PvzPlantType::Imitator);
        assert_eq!(PvzZombieType::from_raw(17).expect("digger"), PvzZombieType::Digger);
        assert!(PvzPlantType::from_raw(999).is_err());
        assert!(PvzZombieType::from_raw(999).is_err());
        assert_eq!(plant_kind_from_raw(47).expect("cob"), PlantKind::CobCannon);
        assert_eq!(zombie_kind_from_raw(17).expect("digger"), ZombieKind::Digger);
        assert_eq!(
            zombie_phase_from_raw(91).expect("yeti running"),
            ZombiePhase::YetiRunning
        );
        assert!(plant_kind_from_raw(999).is_err());
        assert!(zombie_kind_from_raw(999).is_err());
        assert!(zombie_phase_from_raw(999).is_err());
        assert_eq!(item_kind_from_raw(4).expect("sun"), ItemKind::Sun);
        assert!(item_kind_from_raw(28).is_err());
        assert!(item_kind_from_raw(999).is_err());
    }

    #[test]
    fn native_zombie_phase_table_is_complete() {
        for raw in 0..=95 {
            assert!(zombie_phase_from_raw(raw).is_ok(), "missing native phase {raw}");
        }
        assert!(zombie_phase_from_raw(-1).is_err());
        assert!(zombie_phase_from_raw(96).is_err());
    }
}
