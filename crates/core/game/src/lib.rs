#![feature(trivial_bounds)]
#![allow(
    incomplete_features,
    trivial_bounds,
    reason = "concrete current-backend capability bounds are checked at use"
)]

//! Reusable game logic built on safe backend traits.

pub mod backend {
    pub use rsvz_backend_api::*;
}

pub mod model {
    pub use rsvz_model::*;
}

mod access;
pub use access::with_exclusive_backend as with_backend;

pub mod artifact;
pub mod auto_collect;
pub mod bench;
pub mod cards;
pub mod cob;
pub mod diagnostics;
pub mod dispatch;
pub mod event;
pub mod event_measure;
pub mod fast_forward;
pub mod frame;
pub mod ice_filler;
pub mod key;
pub mod lifecycle;
pub mod lineup;
pub mod live_value;
pub mod measure;
pub mod plant;
pub mod plant_fixer;
pub mod registration;
pub mod resources;
pub mod runtime;
pub mod script;
pub mod session;
pub mod setup;
pub mod shovel;
pub mod smart_fodder;
pub mod smart_remove;
pub mod state_hook;
pub mod tick;
pub mod timeline;
pub mod zombie;

pub mod logic {
    pub mod active_time;
    pub mod blover;
    pub mod card_timing;
    pub mod cards;
    pub mod cleanup {
        pub use crate::modifier::cleanup::{clear_plants, clear_zombies, kill_all_zombies};
    }
    pub mod cob;
    pub mod contact;
    pub mod defense;
    pub mod fast_forward;
    pub mod grid;
    pub mod ice_filler;
    pub mod imitator_ice;
    pub mod item_collector;
    pub mod lineup {
        pub use crate::lineup::*;
    }
    pub mod plant_fixer;
    pub mod selectors;
    pub mod shovel;
    pub mod smart_fodder;
    pub mod smart_remove;
    pub mod tactics;
    pub mod timing;
    pub mod waves;
    pub mod zombie_geometry;
    pub mod zombie_motion;
    pub mod zombies;

    pub use crate::lineup::apply_lineup;
    pub use crate::lineup::{ApplyLineupError, LineupApplyBackend};
    pub use card_timing::{
        MushroomEffectTiming, SimpleCardEffectTiming, mushroom_effect_timing, simple_card_effect_timing,
    };
    pub use contact::zombie_state;
    pub use contact::{
        ContactGeometryBackend, ContactGeometryQueryError, PLANT_THREAT_HORIZONTAL_OVERLAP, attack_reaches_defense,
        circle_hits_rect, native_horizontal_rect_overlap, plant_threat_hits_zombie_geometry,
        same_row_and_attack_reaches_plant_geometry, zombie_threat_hits_plant_geometry,
    };
    pub use fast_forward::{
        AdvancedPauseMaskColor, AdvancedPauseOptions, FastForwardOptions, FastForwardPerformance, FastForwardRequest,
        FastForwardRequestExt, FastForwardStopReason, FastForwardUntil, FastForwardWindow, FastForwardWindowExt,
        SeedChooserFastForwardOptions,
    };
    pub use grid::IntoGrid;
    pub use plant_fixer::{PlantFixer, PlantFixerBackend, PlantFixerError, PlantFixerGridError};
    pub use smart_fodder::{
        CapOneDiagnostics, CapOneWeights, DamageTables, ExplosionPmf, FirstExplosionKernel, FodderMorph,
        JackBlastEvent, JackDistributionInput, JackGeometry, ReleaseTailKind, SmartFodderChoice, SmartFodderDomain,
        SmartFodderDomainError, SmartFodderModel, SmartFodderSolveError, SmartFodderSpec, SmartFodderTableError,
        SmartFodderThreat, SmartFodderTimes, cap_one_weights, critical_remove_candidates, smart_fodder_tables,
    };
    pub use smart_fodder::{predict_c9_remove_by_at, predict_smart_fodder_at};
    pub use smart_remove::tick_smart_remove;
    pub use smart_remove::{SmartRemoveError, SmartRemoveState};
    pub use waves::{IntoWaveSet, WaveSet};
    pub use zombie_geometry::{
        default_body_width, is_profile_compatible, predicted_zombie_attack_bounds, pvz_trunc_f32_to_i32,
        zombie_defense_draw_pose, zombie_defense_geometry, zombie_defense_geometry_from_state,
        zombie_geometry_input_from_profile,
    };
    pub use zombie_motion::{
        MAX_ZOMBIE_MOTION_HORIZON, VanillaZombieMotionRules, ZombieMotionRules, predict_stable_zombie_x_at,
        predict_stable_zombie_x_at_with_rules, predict_stable_zombie_x_trace, predict_stable_zombie_x_trace_with_rules,
        vanilla_track_frames,
    };
    pub use zombie_motion::{unopened_jack_motion_state, zombie_motion_state, zombie_stable_x_trace};
    pub use zombies::{
        ApplySpawnListError, ApplyZombieSpawnRequestError, ZombieSpawnRequest, ZombieSpawnRequestError,
        ZombieTypeSelection, average_spawn_list, is_aquatic_zombie, random_zombie_types, resolve_zombie_spawn_types,
        zombie_row_allowed_for_kind, zombie_row_allowed_for_source,
    };
    pub use zombies::{apply_spawn_list, apply_zombie_spawn_request};
}

pub mod modifier {
    mod cheats;
    pub use cheats::{set_dance_mode, set_maid_cheat};
    pub mod automation;
    pub mod cleanup;
    pub mod plant;
    pub mod resource;
    mod value;
    pub mod zombie;

    pub use automation::{keep_plant_hp_tick, keep_sun_tick, pin_zombie_x_tick};
    pub use cleanup::{clear_plants, clear_zombies, kill_all_zombies, remove_grid_item_by_id};
    pub use plant::PlantEffectCountdownError;
    pub use plant::{
        normalize_effect_countdown_by_grid, normalize_effect_countdown_by_id, normalize_first_effect_countdown,
        remove_plant_by_id, remove_plant_kind_at, set_all_plants_hp, set_plant_hp, set_plant_hp_by_grid,
    };
    pub use resource::{STABLE_NATURAL_SUN_COUNTDOWN, STABLE_NATURAL_SUN_GENERATED};
    pub use resource::{set_sun, set_sun_cost_ignored, stabilize_natural_sun_drop, stabilize_natural_sun_drop_aging};
    pub use value::ModifierValueError;
    pub use zombie::SpawnZombieError;
    pub use zombie::{
        set_all_zombies_body_hp, set_zombie_body_hp, set_zombie_body_hp_by_kind, set_zombie_x, spawn_zombie,
    };
}

pub use artifact::SessionArtifact;
pub use lineup::{
    Lineup, LineupApplyOptions, LineupBase, LineupCell, LineupParseError, LineupPlant, LineupReloadPolicy,
    PendingLineup,
};
pub use logic::*;
pub use setup::{
    ApplyScriptOpeningError, OpeningState, ScriptSetup, VerifySelectedCardsError, classify_reload_boundary,
    registered_board_setup_pending,
};
pub use setup::{apply_registered_board_setup, finish_script_opening, prepare_script_opening, verify_selected_cards};

pub use frame::{Frame, PlantRef, ZombieRef};

pub mod timing;
