//! bench operations for the selected backend.

use crate::access::with_backend;
use crate::bench::{BenchOptions, BenchTask, DEFAULT_FAST_FORWARD_OPTIONS};
use crate::runtime::{RuntimeError, RuntimeResult};
use crate::session::{
    SessionJobKey, claim_session_job, completed_rounds, fail_script, set_session_artifact, stop_script,
};
use crate::setup::with_script_setup;
use rsvz_backend_api::backend::FastForwardBackend;
use rsvz_schedule::state_hook::{StateEvent, with_state_hooks};
use rsvz_schedule::tick::{TickControl, TickLane, TickLifetime, TickOptions, TickPriority, with_scheduler};

static BENCH_JOB_ID: u8 = 1;

pub fn install_bench_session(options: BenchOptions) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: FastForwardBackend,
{
    let options = options.validate()?;
    with_script_setup(|setup| {
        setup.reload_mode = rsvz_model::ReloadMode::MainUiOrFightUi;
        if options.fast_forward {
            setup.seed_chooser_fast_forward = Some(crate::logic::SeedChooserFastForwardOptions::default());
        }
    });
    if !claim_session_job(SessionJobKey::new(&BENCH_JOB_ID))? {
        return Ok(());
    }
    if options.fast_forward {
        with_state_hooks(|hooks| {
            hooks.register(StateEvent::EnterFight, i32::MIN, || {
                with_backend(|backend| backend.start_fast_forward(DEFAULT_FAST_FORWARD_OPTIONS))
                    .map_err(|error| RuntimeError::new(error.to_string()))
            });
        });
    }

    let mut task = BenchTask::new(options)?;
    with_scheduler(|scheduler| {
        scheduler.spawn(
            TickOptions::any_dispatch()
                .lifetime(TickLifetime::Session)
                .lane(TickLane::After)
                .priority(TickPriority::LOW)
                .idle_neutral()
                .name("session_bench"),
            move |meta| {
                let Some(report) = task.tick(meta, completed_rounds()) else {
                    return Ok(TickControl::Continue);
                };
                if let Err(error) = set_session_artifact(report.artifact()) {
                    fail_script(error);
                } else {
                    stop_script();
                }
                Ok(TickControl::Stop)
            },
        );
    });
    Ok(())
}

/// Declares the ordinary session benchmark task.
pub fn start(options: BenchOptions)
where
    rsvz_current::CurrentBackend: FastForwardBackend,
{
    if let Err(error) = install_bench_session(options) {
        crate::registration::record_error(error);
    }
}
