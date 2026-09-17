//! Script spellings and conversion into canonical core setup types.
use rsvz_game::logic::zombies::ZombieTypeSelection;
use rsvz_model::{CardSelection, PlantKind, ZombieKind};

/// 解析卡片缩写时返回的错误。
#[derive(Clone, Copy, Debug, thiserror::Error, PartialEq, Eq)]
pub enum CardSelectionParseError {
    #[error("unknown card abbreviation '{0}'")]
    UnknownAbbreviation(char),
    #[error("too many automatic fodder cards requested with C")]
    TooManyAutoFodders,
}

/// 解析卡片缩写，包括用重复字母表示模仿者和用 `C` 自动选择垫材。
///
/// 本函数只解析缩写；完整的重复项与卡片数量校验仍由调用者负责，因此可以先把
/// 解析结果追加到其他卡片，再统一校验最终选卡列表。
pub fn parse_card_abbreviations(input: &str) -> Result<Vec<CardSelection>, CardSelectionParseError> {
    const AUTO_FODDERS: [PlantKind; 5] = [
        PlantKind::PuffShroom,
        PlantKind::FlowerPot,
        PlantKind::ScaredyShroom,
        PlantKind::SunShroom,
        PlantKind::Sunflower,
    ];

    let mut cards = Vec::new();
    let mut fodder_index = 0;
    for ch in input.chars() {
        if matches!(ch, ' ' | ',' | ';' | '　' | '，' | '；') {
            continue;
        }
        if matches!(ch, 'C' | 'c') {
            while AUTO_FODDERS
                .get(fodder_index)
                .is_some_and(|kind| cards.contains(&CardSelection::Plant(*kind)))
            {
                fodder_index += 1;
            }
            let Some(kind) = AUTO_FODDERS.get(fodder_index).copied() else {
                return Err(CardSelectionParseError::TooManyAutoFodders);
            };
            cards.push(CardSelection::Plant(kind));
            fodder_index += 1;
            continue;
        }

        let kind = match ch {
            'A' => PlantKind::CherryBomb,
            'B' => PlantKind::Blover,
            'F' => PlantKind::FlowerPot,
            'G' => PlantKind::GraveBuster,
            'I' => PlantKind::IceShroom,
            'J' => PlantKind::Jalapeno,
            'K' => PlantKind::CoffeeBean,
            'L' => PlantKind::LilyPad,
            'M' => PlantKind::PotatoMine,
            'N' => PlantKind::DoomShroom,
            'P' => PlantKind::Pumpkin,
            'T' => PlantKind::TallNut,
            'U' => PlantKind::UmbrellaLeaf,
            'W' => PlantKind::Squash,
            '_' => PlantKind::Spikeweed,
            other => return Err(CardSelectionParseError::UnknownAbbreviation(other)),
        };
        let ordinary = CardSelection::Plant(kind);
        cards.push(if cards.contains(&ordinary) {
            CardSelection::Imitator(kind)
        } else {
            ordinary
        });
    }
    Ok(cards)
}

/// Error returned while parsing or validating a requested zombie set.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ZombieSelectionError {
    #[error("unknown zombie abbreviation '{0}'")]
    UnknownAbbreviation(char),
    #[error("zombie list cannot be empty")]
    Empty,
}

/// User-facing exact zombie-list inputs accepted by `set_zombies`.
pub trait IntoZombieTypeSelection {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError>;
}

impl IntoZombieTypeSelection for ZombieTypeSelection {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        Ok(self)
    }
}

impl IntoZombieTypeSelection for ZombieKind {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        Ok(ZombieTypeSelection::Exact(vec![self]))
    }
}

impl IntoZombieTypeSelection for &str {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        parse_zombie_abbreviations(self).map(ZombieTypeSelection::Exact)
    }
}

impl IntoZombieTypeSelection for String {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        self.as_str().into_zombie_type_selection()
    }
}

impl IntoZombieTypeSelection for &String {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        self.as_str().into_zombie_type_selection()
    }
}

impl IntoZombieTypeSelection for Vec<ZombieKind> {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        Ok(ZombieTypeSelection::Exact(self))
    }
}

impl IntoZombieTypeSelection for &[ZombieKind] {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        Ok(ZombieTypeSelection::Exact(self.to_vec()))
    }
}

impl IntoZombieTypeSelection for &Vec<ZombieKind> {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        self.as_slice().into_zombie_type_selection()
    }
}

impl<const N: usize> IntoZombieTypeSelection for [ZombieKind; N] {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        Ok(ZombieTypeSelection::Exact(self.to_vec()))
    }
}

impl<const N: usize> IntoZombieTypeSelection for &[ZombieKind; N] {
    fn into_zombie_type_selection(self) -> Result<ZombieTypeSelection, ZombieSelectionError> {
        self.as_slice().into_zombie_type_selection()
    }
}

/// Parses AvZ-style single-character Chinese zombie abbreviations.
pub fn parse_zombie_abbreviations(input: &str) -> Result<Vec<ZombieKind>, ZombieSelectionError> {
    let zombies = input
        .chars()
        .filter(|ch| !matches!(ch, ' ' | ',' | ';' | '　' | '，' | '；'))
        .map(|ch| {
            let kind = match ch {
                '普' => ZombieKind::Normal,
                '旗' => ZombieKind::Flag,
                '障' => ZombieKind::Conehead,
                '杆' => ZombieKind::PoleVaulting,
                '桶' => ZombieKind::Buckethead,
                '报' => ZombieKind::Newspaper,
                '门' => ZombieKind::ScreenDoor,
                '橄' => ZombieKind::Football,
                '舞' => ZombieKind::Dancing,
                '伴' => ZombieKind::BackupDancer,
                '鸭' => ZombieKind::DuckyTube,
                '潜' => ZombieKind::Snorkel,
                '车' => ZombieKind::Zomboni,
                '橇' => ZombieKind::Bobsled,
                '豚' => ZombieKind::DolphinRider,
                '丑' => ZombieKind::JackInTheBox,
                '气' => ZombieKind::Balloon,
                '矿' => ZombieKind::Digger,
                '跳' => ZombieKind::Pogo,
                '雪' => ZombieKind::Yeti,
                '偷' => ZombieKind::Bungee,
                '梯' => ZombieKind::Ladder,
                '篮' => ZombieKind::Catapult,
                '白' => ZombieKind::Gargantuar,
                '鬼' => ZombieKind::Imp,
                '博' => ZombieKind::Boss,
                '豌' => ZombieKind::PeaHead,
                '坚' => ZombieKind::WallNutHead,
                '辣' => ZombieKind::JalapenoHead,
                '枪' | '机' => ZombieKind::GatlingHead,
                '窝' | '倭' => ZombieKind::SquashHead,
                '高' => ZombieKind::TallNutHead,
                '红' => ZombieKind::GigaGargantuar,
                other => return Err(ZombieSelectionError::UnknownAbbreviation(other)),
            };
            Ok(kind)
        })
        .collect::<Result<Vec<_>, _>>()?;
    if zombies.is_empty() {
        Err(ZombieSelectionError::Empty)
    } else {
        Ok(zombies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_card_abbreviations_repeats_and_automatic_fodders() {
        assert_eq!(
            parse_card_abbreviations("IIKAWPCCCC").expect("valid abbreviations"),
            vec![
                CardSelection::Plant(PlantKind::IceShroom),
                CardSelection::Imitator(PlantKind::IceShroom),
                CardSelection::Plant(PlantKind::CoffeeBean),
                CardSelection::Plant(PlantKind::CherryBomb),
                CardSelection::Plant(PlantKind::Squash),
                CardSelection::Plant(PlantKind::Pumpkin),
                CardSelection::Plant(PlantKind::PuffShroom),
                CardSelection::Plant(PlantKind::FlowerPot),
                CardSelection::Plant(PlantKind::ScaredyShroom),
                CardSelection::Plant(PlantKind::SunShroom),
            ]
        );
    }

    #[test]
    fn card_abbreviation_errors_are_typed() {
        assert_eq!(
            parse_card_abbreviations("?"),
            Err(CardSelectionParseError::UnknownAbbreviation('?'))
        );
        assert_eq!(
            parse_card_abbreviations("CCCCCC"),
            Err(CardSelectionParseError::TooManyAutoFodders)
        );
    }

    #[test]
    fn parses_avz_zombie_abbreviations_and_formatting() {
        assert_eq!(
            parse_zombie_abbreviations("红白矿梯橄丑杆舞跳篮").expect("valid abbreviations"),
            vec![
                ZombieKind::GigaGargantuar,
                ZombieKind::Gargantuar,
                ZombieKind::Digger,
                ZombieKind::Ladder,
                ZombieKind::Football,
                ZombieKind::JackInTheBox,
                ZombieKind::PoleVaulting,
                ZombieKind::Dancing,
                ZombieKind::Pogo,
                ZombieKind::Catapult,
            ]
        );
        assert_eq!(
            parse_zombie_abbreviations("普旗障杆桶报门橄舞潜车橇豚丑气矿跳雪偷梯篮白博豌坚辣枪窝高红")
                .expect("all canonical abbreviations should parse"),
            vec![
                ZombieKind::Normal,
                ZombieKind::Flag,
                ZombieKind::Conehead,
                ZombieKind::PoleVaulting,
                ZombieKind::Buckethead,
                ZombieKind::Newspaper,
                ZombieKind::ScreenDoor,
                ZombieKind::Football,
                ZombieKind::Dancing,
                ZombieKind::Snorkel,
                ZombieKind::Zomboni,
                ZombieKind::Bobsled,
                ZombieKind::DolphinRider,
                ZombieKind::JackInTheBox,
                ZombieKind::Balloon,
                ZombieKind::Digger,
                ZombieKind::Pogo,
                ZombieKind::Yeti,
                ZombieKind::Bungee,
                ZombieKind::Ladder,
                ZombieKind::Catapult,
                ZombieKind::Gargantuar,
                ZombieKind::Boss,
                ZombieKind::PeaHead,
                ZombieKind::WallNutHead,
                ZombieKind::JalapenoHead,
                ZombieKind::GatlingHead,
                ZombieKind::SquashHead,
                ZombieKind::TallNutHead,
                ZombieKind::GigaGargantuar,
            ]
        );
        assert_eq!(
            parse_zombie_abbreviations("伴 鸭，鬼；机,倭").expect("valid aliases and separators"),
            vec![
                ZombieKind::BackupDancer,
                ZombieKind::DuckyTube,
                ZombieKind::Imp,
                ZombieKind::GatlingHead,
                ZombieKind::SquashHead,
            ]
        );
    }

    #[test]
    fn zombie_abbreviation_errors_are_typed_and_atomic() {
        assert_eq!(
            parse_zombie_abbreviations("普怪红"),
            Err(ZombieSelectionError::UnknownAbbreviation('怪'))
        );
        assert_eq!(parse_zombie_abbreviations(" ，；"), Err(ZombieSelectionError::Empty));
    }
}
