use super::*;
use std::time::Instant;

/// Number of trial indices from `0..total` assigned to one strided shard.
#[must_use]
pub const fn local_trial_quota(total: u64, shard: SessionShard) -> u64 {
    let index = shard.index as u64;
    if index >= total || shard.count == 0 {
        0
    } else {
        (total - 1 - index) / shard.count as u64 + 1
    }
}

/// Stable global trial sequence for one shard-local index.
#[must_use]
pub const fn global_trial_sequence(shard: SessionShard, local_index: u64) -> u64 {
    shard.index as u64 + local_index.wrapping_mul(shard.count as u64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MeasurementRunConfig {
    pub(crate) limit: MeasureLimit,
    pub(crate) completed_rounds: u32,
    pub(super) shard_count: u32,
    pub(super) seed_base: u32,
}

impl MeasurementRunConfig {
    pub(crate) const fn new(limit: MeasureLimit, completed_rounds: u32, shard: SessionShard) -> Self {
        Self {
            limit,
            completed_rounds,
            shard_count: shard.count,
            seed_base: shard.seed_base,
        }
    }
}

pub(crate) struct TrialRun {
    pub(crate) limit: MeasureLimit,
    pub(crate) artifact_limit: MeasureLimit,
    completed_rounds: u32,
    shard: SessionShard,
    local_index: u64,
    pub(crate) started: Instant,
}

impl TrialRun {
    pub(crate) fn new(limit: MeasureLimit, completed_rounds: u32, shard: SessionShard) -> Self {
        Self {
            limit,
            artifact_limit: limit,
            completed_rounds,
            shard,
            local_index: 0,
            started: Instant::now(),
        }
    }

    pub(crate) fn sequence(&self) -> u64 {
        global_trial_sequence(self.shard, self.local_index)
    }

    pub(crate) fn next_reset_config(&self) -> WorldResetConfig {
        WorldResetConfig {
            completed_rounds: self.completed_rounds,
            seed: self.shard.seed_base.wrapping_add(self.sequence() as u32),
            ..WorldResetConfig::default()
        }
    }

    pub(crate) fn finish_trial(&mut self, attempted: u64) -> bool {
        self.local_index = self.local_index.saturating_add(1);
        self.limit.reached_at_trial_boundary(attempted, self.started.elapsed())
    }

    pub(crate) fn artifact_config(&self) -> MeasurementRunConfig {
        MeasurementRunConfig::new(self.artifact_limit, self.completed_rounds, self.shard)
    }
}
