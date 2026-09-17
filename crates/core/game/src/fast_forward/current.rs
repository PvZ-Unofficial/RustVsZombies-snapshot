//! Current fast-forward window task installation.
use super::with_fast_forward_task;
use crate::logic::fast_forward::{FastForwardWindow, active_window_request};
use rsvz_backend_api::{FastForwardBackend, WaveTimingBackend};
use rsvz_model::{FastForwardStopReason, WaveClockState};
use rsvz_schedule::{TickControl, TickLane, TickOptions, TickPriority, TickScheduler, TickTaskState};
fn register_fast_forward_window_tick(
    scheduler: &mut TickScheduler, initial_wave_clocks: WaveClockState, windows: Vec<FastForwardWindow>,
) where
    rsvz_current::CurrentBackend: FastForwardBackend + WaveTimingBackend + 'static,
{
    with_fast_forward_task(|task| {
        if task.is_some_and(|handle| scheduler.state(handle) != TickTaskState::Stopped) {
            return;
        }
        let mut wave_clocks = initial_wave_clocks;
        let mut last_request = None;
        *task = Some(
            scheduler.spawn(
                TickOptions::playing_frame()
                    .lane(TickLane::After)
                    .priority(TickPriority::LOW)
                    .idle_neutral()
                    .name("fast_forward_windows"),
                move |_meta| {
                    crate::access::with_backend(|backend| {
                        let snapshot = crate::timing::wave_timing()
                            .map_err(|error| crate::runtime::RuntimeError::from(error.to_string()))?;
                        let request = active_window_request(&windows, &mut wave_clocks, snapshot);
                        match request {
                            Some((identity, options)) if last_request != Some(identity) => {
                                if last_request.is_some() {
                                    backend
                                        .stop_fast_forward(FastForwardStopReason::ConditionMet)
                                        .map_err(|error| crate::runtime::RuntimeError::from(error.to_string()))?;
                                }
                                backend
                                    .start_fast_forward(options)
                                    .map_err(|error| crate::runtime::RuntimeError::from(error.to_string()))?;
                                last_request = Some(identity);
                            }
                            Some(_) => {}
                            None if last_request.take().is_some() => {
                                backend
                                    .stop_fast_forward(FastForwardStopReason::ConditionMet)
                                    .map_err(|error| crate::runtime::RuntimeError::from(error.to_string()))?;
                            }
                            None => {}
                        }
                        Ok(TickControl::Continue)
                    })
                },
            ),
        );
    });
}

pub fn register_fast_forward_window_task() -> crate::runtime::RuntimeResult<()>
where
    rsvz_current::CurrentBackend: FastForwardBackend + WaveTimingBackend + 'static,
{
    let wave_clocks = crate::timeline::runtime_wave_clocks();
    let windows = crate::setup::with_script_setup(|setup| setup.fast_forward_windows.clone());
    rsvz_schedule::tick::with_scheduler(|scheduler| register_fast_forward_window_tick(scheduler, wave_clocks, windows));
    Ok(())
}
