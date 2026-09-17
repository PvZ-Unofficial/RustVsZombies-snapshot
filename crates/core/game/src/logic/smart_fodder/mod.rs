//! Backend-neutral prediction, probability, and scoring for smart C9 fodder.

mod domain;
mod prediction;
mod probability;
mod solver;
mod tables;

pub use domain::{SmartFodderDomain, SmartFodderDomainError, SmartFodderTimes, critical_remove_candidates};
#[doc(hidden)]
pub use prediction::SmartFodderReplanContext;
pub use prediction::SmartFodderSpec;
pub use prediction::predict_smart_fodder_with_context_at;
pub use prediction::{predict_c9_remove_by_at, predict_smart_fodder_at};
pub use probability::{
    CapOneDiagnostics, CapOneWeights, ExplosionPmf, FirstExplosionKernel, JackDistributionInput, cap_one_weights,
};
pub use solver::{
    FodderBehavior, FodderContactKind, FodderMorph, JackBlastEvent, OrderedExplosionPmf, SmartFodderChoice,
    SmartFodderModel, SmartFodderSolveError, SmartFodderThreat,
};
pub use tables::{DamageTables, JackGeometry, ReleaseTailKind, SmartFodderTableError, smart_fodder_tables};
