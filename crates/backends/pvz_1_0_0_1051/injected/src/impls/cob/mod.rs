use rsvz_backend_api::backend::CobFireBackend;
use rsvz_model::model::PixelPos;

use crate::error::{Pvz1051Error, Result};
use crate::ops::plant::effective_seed_type as plant_effective_seed_type;
use crate::raw::kind::PvzPlantType;
use crate::raw::layout as ptrs;
use crate::runtime::Pvz1051Backend;

mod fire;
