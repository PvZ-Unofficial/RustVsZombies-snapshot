use rsvz_model::model::SceneKind;

pub const fn scene_from_pe(scene: pe_rs::SceneType) -> SceneKind {
    match scene {
        pe_rs::SceneType::Day => SceneKind::Day,
        pe_rs::SceneType::Night => SceneKind::Night,
        pe_rs::SceneType::Pool => SceneKind::Pool,
        pe_rs::SceneType::Fog => SceneKind::Fog,
        pe_rs::SceneType::Roof => SceneKind::Roof,
        pe_rs::SceneType::MoonNight => SceneKind::MoonNight,
        pe_rs::SceneType::MushroomGarden => SceneKind::MushroomGarden,
    }
}

pub fn scene_to_pe(scene: SceneKind) -> Result<pe_rs::SceneType, crate::PeBackendError> {
    Ok(match scene {
        SceneKind::Day => pe_rs::SceneType::Day,
        SceneKind::Night => pe_rs::SceneType::Night,
        SceneKind::Pool => pe_rs::SceneType::Pool,
        SceneKind::Fog => pe_rs::SceneType::Fog,
        SceneKind::Roof => pe_rs::SceneType::Roof,
        SceneKind::MoonNight => pe_rs::SceneType::MoonNight,
        SceneKind::MushroomGarden => pe_rs::SceneType::MushroomGarden,
        SceneKind::Greenhouse => {
            return Err(crate::PeBackendError::unsupported_kind("scene", "Greenhouse"));
        }
        SceneKind::Zombiquarium => {
            return Err(crate::PeBackendError::unsupported_kind("scene", "Zombiquarium"));
        }
        SceneKind::TreeOfWisdom => {
            return Err(crate::PeBackendError::unsupported_kind("scene", "TreeOfWisdom"));
        }
    })
}
