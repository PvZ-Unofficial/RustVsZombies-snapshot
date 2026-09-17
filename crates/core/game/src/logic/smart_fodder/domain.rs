use std::ops::RangeInclusive;

pub const MIN_PLANT_AT: i32 = 658;
pub const MAX_PLANT_AT: i32 = 1_290;
pub const MIN_ACTIVATION_AT: i32 = 1_100;
pub const MAX_ACTIVATION_AT: i32 = 1_800;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartFodderTimes {
    pub now: i32,
    pub plant_window: RangeInclusive<i32>,
    pub remove_by: Option<i32>,
    pub activation_at: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SmartFodderDomain {
    pub now: i32,
    pub plant_candidates: RangeInclusive<i32>,
    pub remove_by: Option<i32>,
    pub activation_at: i32,
}

impl SmartFodderDomain {
    pub fn validate(times: SmartFodderTimes) -> Result<Self, SmartFodderDomainError> {
        if !(MIN_ACTIVATION_AT..=MAX_ACTIVATION_AT).contains(&times.activation_at) {
            return Err(SmartFodderDomainError::ActivationOutOfRange);
        }
        let first = *times.plant_window.start();
        let last = *times.plant_window.end();
        if first > last {
            return Err(SmartFodderDomainError::ReversedPlantWindow);
        }
        if first <= times.now {
            return Err(SmartFodderDomainError::PlantWindowNotAfterCall);
        }
        if last >= times.activation_at {
            return Err(SmartFodderDomainError::PlantWindowNotBeforeActivation);
        }
        if !(MIN_PLANT_AT..=MAX_PLANT_AT).contains(&first) || !(MIN_PLANT_AT..=MAX_PLANT_AT).contains(&last) {
            return Err(SmartFodderDomainError::PlantWindowOutOfRange);
        }
        if times.remove_by.is_some_and(|remove_by| remove_by > times.activation_at) {
            return Err(SmartFodderDomainError::RemoveDeadlineAfterActivation);
        }

        let candidate_first = first;
        let deadline = times.remove_by.unwrap_or(i32::MAX);
        let candidate_last = last.min(deadline);
        if candidate_first > candidate_last {
            return Err(SmartFodderDomainError::EmptyCandidateWindow);
        }
        Ok(Self {
            now: times.now,
            plant_candidates: candidate_first..=candidate_last,
            remove_by: times.remove_by,
            activation_at: times.activation_at,
        })
    }
}

/// Builds `C(F,S)`. `threat_boundaries` already use the normalized v1
/// integer endpoint oracle; this helper only performs the contract filtering
/// and deduplication.
#[must_use]
pub fn critical_remove_candidates(
    plant_at: i32, remove_by: Option<i32>, threat_boundaries: impl IntoIterator<Item = i32>,
) -> Vec<Option<i32>> {
    let mut candidates = Vec::new();
    critical_remove_candidates_into(&mut candidates, plant_at, remove_by, threat_boundaries);
    candidates
}

pub(crate) fn critical_remove_candidates_into(
    candidates: &mut Vec<Option<i32>>, plant_at: i32, remove_by: Option<i32>,
    threat_boundaries: impl IntoIterator<Item = i32>,
) {
    candidates.clear();
    let Some(deadline) = remove_by else {
        candidates.push(None);
        return;
    };
    candidates.extend(
        threat_boundaries
            .into_iter()
            .filter(|boundary| (plant_at..=deadline).contains(boundary))
            .map(Some),
    );
    candidates.push(Some(deadline));
    candidates.sort_unstable();
    candidates.dedup();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SmartFodderDomainError {
    #[error("activation time must be in 1100..=1800")]
    ActivationOutOfRange,
    #[error("plant window start must not exceed its end")]
    ReversedPlantWindow,
    #[error("plant window must start strictly after the call time")]
    PlantWindowNotAfterCall,
    #[error("plant window must end strictly before activation")]
    PlantWindowNotBeforeActivation,
    #[error("plant window must be contained in the supported 658..=1290 domain")]
    PlantWindowOutOfRange,
    #[error("remove deadline must not be after activation")]
    RemoveDeadlineAfterActivation,
    #[error("plant window has no candidate in the supported 658..=1290 domain")]
    EmptyCandidateWindow,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_the_fixed_v1_domain_and_applies_the_remove_deadline() {
        let domain = SmartFodderDomain::validate(SmartFodderTimes {
            now: 600,
            plant_window: 658..=1_290,
            remove_by: Some(1_200),
            activation_at: 1_500,
        })
        .expect("domain");
        assert_eq!(domain.plant_candidates, 658..=1_200);
    }

    #[test]
    fn rejects_each_public_time_boundary_without_clamping() {
        let valid = || SmartFodderTimes {
            now: 600,
            plant_window: 658..=1_000,
            remove_by: None,
            activation_at: 1_100,
        };
        assert!(matches!(
            SmartFodderDomain::validate(SmartFodderTimes {
                activation_at: 1_099,
                ..valid()
            }),
            Err(SmartFodderDomainError::ActivationOutOfRange)
        ));
        assert!(matches!(
            SmartFodderDomain::validate(SmartFodderTimes { now: 658, ..valid() }),
            Err(SmartFodderDomainError::PlantWindowNotAfterCall)
        ));
        assert!(matches!(
            SmartFodderDomain::validate(SmartFodderTimes {
                plant_window: 658..=1_100,
                ..valid()
            }),
            Err(SmartFodderDomainError::PlantWindowNotBeforeActivation)
        ));
        assert!(matches!(
            SmartFodderDomain::validate(SmartFodderTimes {
                remove_by: Some(1_101),
                ..valid()
            }),
            Err(SmartFodderDomainError::RemoveDeadlineAfterActivation)
        ));
        assert!(matches!(
            SmartFodderDomain::validate(SmartFodderTimes {
                plant_window: 657..=1_000,
                ..valid()
            }),
            Err(SmartFodderDomainError::PlantWindowOutOfRange)
        ));
        assert!(matches!(
            SmartFodderDomain::validate(SmartFodderTimes {
                plant_window: 658..=1_291,
                activation_at: 1_500,
                ..valid()
            }),
            Err(SmartFodderDomainError::PlantWindowOutOfRange)
        ));
    }

    #[test]
    fn critical_remove_set_is_deadline_plus_filtered_unique_boundaries() {
        assert_eq!(critical_remove_candidates(700, None, [710, 730]), vec![None]);
        assert_eq!(
            critical_remove_candidates(700, Some(750), [699, 710, 710, 760]),
            vec![Some(710), Some(750)]
        );
    }
}
