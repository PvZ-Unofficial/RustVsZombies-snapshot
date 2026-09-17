use super::*;

impl CobManagerState {
    /// Unlocks latest-fire writes and returns the grid currently stored at the plan index.
    pub(super) fn finish_fix_latest(&mut self, list_index: usize) -> Result<Grid, CobManagerError> {
        self.latest.writable = true;
        self.entries
            .get(list_index)
            .map(|entry| entry.grid)
            .ok_or(CobManagerError::InvalidNext)
    }

    /// Creates latest-fire repair timing and locks latest-fire writes until repair starts.
    pub(super) fn fix_latest_index_and_delay(&mut self, now: i32) -> Result<(usize, i32), CobManagerError> {
        let Some(index) = self.latest.index else {
            return Err(CobManagerError::NoLatestCob);
        };
        self.latest.writable = false;
        let due_delay = (self.latest.time + 205 - now).max(0);
        Ok((index, due_delay))
    }

    pub(super) fn fix_latest_task(&mut self) -> Result<CobFixLatestTask, CobManagerCallError>
    where
        CurrentBackend: ClockBackend,
    {
        crate::access::with_backend(|backend| {
            let start_clock = backend
                .clock()
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
            let (list_index, due_delay) = self
                .fix_latest_index_and_delay(start_clock)
                .map_err(CobManagerCallError::Manager)?;
            Ok(CobFixLatestTask {
                list_index,
                due_delay,
                start_clock,
                plant_grid: None,
            })
        })
    }
}

impl CobFixLatestTask {
    pub(super) fn tick(&mut self, manager: &mut CobManagerState) -> Result<TickControl, CobManagerCallError>
    where
        CurrentBackend: ClockBackend + CardPlantingBackend,
    {
        crate::access::with_backend(|backend| {
            let now = backend
                .clock()
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
            if now < self.start_clock.saturating_add(self.due_delay) {
                return Ok(TickControl::Continue);
            }
            let grid = match self.plant_grid {
                Some(grid) => grid,
                None => {
                    let grid = manager
                        .finish_fix_latest(self.list_index)
                        .map_err(CobManagerCallError::Manager)?;
                    shovel_at(grid).unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
                    self.plant_grid = Some(grid);
                    grid
                }
            };
            if try_plant_cob(grid) {
                Ok(TickControl::Stop)
            } else {
                Ok(TickControl::Continue)
            }
        })
    }
}
