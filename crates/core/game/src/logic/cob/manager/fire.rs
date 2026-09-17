use super::*;

impl CobManagerState {
    pub(super) fn fire_one(&mut self, target: CobTarget) -> Result<usize, CobManagerCallError>
    where
        CurrentBackend: ClockBackend + CobBackend,
    {
        crate::access::with_backend(|backend| {
            validate_target(target)?;
            let mut fire_target = None;
            let selected = self
                .select_cob_for_fire(false, |_grid| {
                    Ok((
                        0,
                        *fire_target.get_or_insert_with(|| {
                            cob_target_to_pixel(target)
                                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
                        }),
                    ))
                })
                .map_err(CobManagerCallError::Manager)?;
            if !fire_cob(selected.cob, selected.fire_target)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
            {
                return Err(CobManagerError::NoReadyCob.into());
            }
            self.next = selected.index;
            self.record_latest(
                backend
                    .clock()
                    .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into())),
                selected.index,
            );
            self.skip_unchecked(1).map_err(CobManagerCallError::Manager)?;
            Ok(selected.index)
        })
    }

    pub(super) fn raw_fire_one(&mut self, drop: CobFireDrop, roof: bool) -> Result<(), CobManagerCallError>
    where
        CurrentBackend: ClockBackend
            + CobBackend
            + 'static
            + rsvz_backend_api::BoardReadinessBackend
            + rsvz_backend_api::GameUiBackend,
    {
        {
            validate_grid(drop.cob_grid)?;
            validate_target(drop.target)?;
            let Some(cob) = find_cob_at(drop.cob_grid) else {
                return Err(CobManagerError::CobNotFound(drop.cob_grid).into());
            };
            if self.runtime.contains(cob) {
                return Err(CobManagerError::NoReadyCob.into());
            }
            let fire_target = cob_target_to_pixel(drop.target).unwrap_or_else(|error| {
                crate::diagnostics::abort_operation(RuntimeError::new(format!("底层操作失败：{error}")))
            });
            if roof {
                let recover = cob_recover_time(cob);
                if recover < 0 {
                    return Err(CobManagerError::NoReadyCob.into());
                }
                let delay = classic_roof_fire_delay(drop.cob_grid.col + 1, drop.target.drop_col)?;
                if recover > delay {
                    return Err(CobManagerError::NoReadyCob.into());
                }
                schedule_cob_fire(&self.runtime, delay, cob, fire_target)?;
                return Ok(());
            }
            if !fire_cob(cob, fire_target).unwrap_or_else(|error| {
                crate::diagnostics::abort_operation(RuntimeError::new(format!("底层操作失败：{error}")))
            }) {
                return Err(CobManagerError::NoReadyCob.into());
            }
            Ok(())
        }
    }

    pub(super) fn delay_fire(
        &mut self, target: CobTarget, roof: bool, recover: bool,
    ) -> Result<usize, CobManagerCallError>
    where
        CurrentBackend: ClockBackend
            + CobBackend
            + 'static
            + rsvz_backend_api::BoardReadinessBackend
            + rsvz_backend_api::GameUiBackend,
    {
        crate::access::with_backend(|backend| {
            validate_target(target)?;
            let allow_wait = recover;
            let mut fire_target = None;
            let selected = match self.select_cob_for_fire(allow_wait, |grid| {
                let fire_target = *fire_target.get_or_insert_with(|| {
                    cob_target_to_pixel(target).unwrap_or_else(|error| {
                        crate::diagnostics::abort_operation(RuntimeError::new(format!("底层操作失败：{error}")))
                    })
                });
                if roof {
                    classic_roof_fire_delay(grid.col + 1, target.drop_col).map(|lead| (lead, fire_target))
                } else {
                    Ok((0, fire_target))
                }
            }) {
                Ok(selected) => selected,
                Err(error) => return Err(CobManagerCallError::Manager(error)),
            };
            let due_clock = backend
                .clock()
                .unwrap_or_else(|error| {
                    crate::diagnostics::abort_operation(RuntimeError::new(format!("底层操作失败：{error}")))
                })
                .saturating_add(selected.delay);
            schedule_cob_fire(&self.runtime, selected.delay, selected.cob, selected.fire_target)?;
            self.next = selected.index;
            self.skip_unchecked(1).map_err(CobManagerCallError::Manager)?;
            self.record_latest(due_clock, selected.index);
            Ok(selected.index)
        })
    }

    pub(super) fn select_cob_for_fire<L>(
        &mut self, allow_wait: bool, mut profile_for: L,
    ) -> Result<SelectedFireCob, CobManagerError>
    where
        CurrentBackend: CobFireBackend,
        L: FnMut(Grid) -> Result<(i32, PixelPos), CobManagerError>,
    {
        if self.entries.is_empty() {
            return Err(CobManagerError::EmptyList);
        }
        let start_index = if self.mode == CobSequentialMode::Priority {
            0
        } else {
            self.current_index()?
        };

        let scan = if self.mode == CobSequentialMode::Space {
            1
        } else {
            self.entries.len()
        };
        let runtime = self.runtime.clone();
        let runtime = runtime.0.borrow();
        let mut best: Option<(i32, SelectedFireCob)> = None;

        for offset in 0..scan {
            let index = (start_index + offset) % self.entries.len();
            let grid = self.entries[index].grid;
            if let Some(cob) = self.refresh_entry_at(index)
                && !runtime.contains(&cob)
            {
                let wait = cob_recover_time(cob);
                if wait >= 0 {
                    let (lead, fire_target) = profile_for(grid)?;
                    let ready_in = wait - lead;
                    if ready_in <= 0 {
                        let Ok(_) = i32::try_from(index) else {
                            return Err(CobManagerError::InvalidNext);
                        };
                        return Ok(SelectedFireCob {
                            index,
                            cob,
                            delay: lead,
                            fire_target,
                        });
                    }
                    let selected = SelectedFireCob {
                        index,
                        cob,
                        delay: wait,
                        fire_target,
                    };
                    if best.as_ref().is_none_or(|(best_ready_in, _)| ready_in < *best_ready_in) {
                        best = Some((ready_in, selected));
                    }
                }
            }
        }

        let Some((_, selected)) = best else {
            return Err(if allow_wait {
                CobManagerError::NoRecoverableCob
            } else {
                CobManagerError::NoReadyCob
            });
        };
        let Ok(_) = i32::try_from(selected.index) else {
            return Err(CobManagerError::InvalidNext);
        };
        if allow_wait {
            Ok(selected)
        } else {
            self.next = selected.index;
            Err(CobManagerError::NoReadyCob)
        }
    }
}

impl CobManagerState
where
    CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
{
    pub(super) fn try_fire<C>(&mut self, row: i32, col: C) -> RuntimeResult<Option<i32>>
    where
        (i32, C): IntoCobTarget,
    {
        let mut outcome = None;
        self.visit_fire_targets([(row, col)], false, "发炮", &mut |index, result| {
            outcome = optional_fire_outcome("发炮", index, result)?;
            Ok(())
        })?;
        Ok(outcome)
    }

    pub(super) fn try_fire_many<T>(&mut self, targets: T) -> RuntimeResult<Vec<Option<i32>>>
    where
        T: IntoCobTargets,
    {
        self.try_fire_many_with(targets, false, "发炮")
    }

    pub(super) fn try_recover_fire<C>(&mut self, row: i32, col: C) -> RuntimeResult<Option<i32>>
    where
        (i32, C): IntoCobTarget,
    {
        let mut outcome = None;
        self.visit_fire_targets([(row, col)], true, "恢复发炮", &mut |index, result| {
            outcome = optional_fire_outcome("恢复发炮", index, result)?;
            Ok(())
        })?;
        Ok(outcome)
    }

    pub(super) fn try_recover_fire_many<T>(&mut self, targets: T) -> RuntimeResult<Vec<Option<i32>>>
    where
        T: IntoCobTargets,
    {
        self.try_fire_many_with(targets, true, "恢复发炮")
    }

    pub(super) fn try_fire_many_with<T>(
        &mut self, targets: T, recover: bool, operation: &'static str,
    ) -> RuntimeResult<Vec<Option<i32>>>
    where
        T: IntoCobTargets,
    {
        let mut outcomes = Vec::new();
        self.visit_fire_targets(targets, recover, operation, &mut |index, result| {
            outcomes.push(optional_fire_outcome(operation, index, result)?);
            Ok(())
        })?;
        Ok(outcomes)
    }

    pub(super) fn visit_fire_targets<T, F>(
        &mut self, targets: T, recover: bool, operation: &'static str, visit: &mut F,
    ) -> RuntimeResult<()>
    where
        T: IntoCobTargets,
        F: FnMut(usize, Result<i32, CobManagerError>) -> RuntimeResult<()>,
    {
        crate::access::with_backend(|backend| {
            let mut roof = None;
            let mut target_index = 0;

            targets.try_for_each_cob_target_result(&mut |target| -> RuntimeResult<()> {
                let current_index = target_index;
                target_index += 1;
                let target = match target {
                    Ok(target) => target,
                    Err(error) => return visit(current_index, Err(error)),
                };

                let roof = match roof {
                    Some(roof) => roof,
                    None => {
                        let current = backend.scene().unwrap_or_else(|error| {
                            crate::diagnostics::abort_operation(RuntimeError::new(format!(
                                "{operation}第 {} 个目标失败：读取场景失败：{error}",
                                current_index + 1
                            )))
                        });
                        let current = current.has_roof();
                        roof = Some(current);
                        current
                    }
                };

                if recover || roof {
                    match self.delay_fire(target, roof, recover) {
                        Ok(index) => visit(current_index, Ok(index as i32)),
                        Err(CobManagerCallError::Manager(error)) => visit(current_index, Err(error)),
                        Err(CobManagerCallError::Backend(error)) => Err(RuntimeError::new(format!(
                            "{operation}第 {} 个目标失败：{error}",
                            current_index + 1
                        ))),
                    }
                } else {
                    match self.fire_one(target) {
                        Ok(index) => visit(current_index, Ok(index as i32)),
                        Err(CobManagerCallError::Manager(error)) => visit(current_index, Err(error)),
                        Err(CobManagerCallError::Backend(error)) => Err(RuntimeError::new(format!(
                            "{operation}第 {} 个目标失败：底层操作失败：{error}",
                            current_index + 1
                        ))),
                    }
                }
            })?;

            Ok(())
        })
    }

    pub(super) fn try_raw_fire<D>(&mut self, drops: D) -> RuntimeResult<()>
    where
        D: IntoCobFireDrops,
    {
        crate::access::with_backend(|backend| {
            let roof = backend
                .scene()
                .unwrap_or_else(|error| {
                    crate::diagnostics::abort_operation(RuntimeError::new(format!(
                        "明确发炮失败：读取场景失败：{error}"
                    )))
                })
                .has_roof();
            let mut errors = None;
            let mut drop_index = 0;

            match drops.try_for_each_cob_fire_drop_result(&mut |drop| {
                let current_index = drop_index;
                drop_index += 1;
                let drop = match drop {
                    Ok(drop) => drop,
                    Err(error) => {
                        push_raw_fire_error(&mut errors, current_index, error);
                        return Ok(());
                    }
                };

                match self.raw_fire_one(drop, roof) {
                    Ok(()) => Ok(()),
                    Err(CobManagerCallError::Manager(error)) => {
                        push_raw_fire_error(&mut errors, current_index, error);
                        Ok(())
                    }
                    Err(CobManagerCallError::Backend(error)) => {
                        push_raw_fire_error(&mut errors, current_index, error);
                        Err(())
                    }
                }
            }) {
                Ok(()) | Err(()) => {}
            }

            errors.map_or_else(|| Ok(()), |message| Err(RuntimeError::new(message)))
        })
    }

    pub(super) fn fire_prepared(
        &mut self, targets: &[CobTarget], recover: bool, operation: &'static str,
    ) -> RuntimeResult<()> {
        let mut errors = None;
        if let Err(error) = self.visit_fire_targets(targets, recover, operation, &mut |index, result| {
            if let Err(error) = result {
                push_indexed_fire_error(&mut errors, operation, index, error);
            }
            Ok(())
        }) {
            push_error_line(&mut errors, error);
        }
        errors.map_or_else(|| Ok(()), |message| Err(RuntimeError::new(message)))
    }
}
