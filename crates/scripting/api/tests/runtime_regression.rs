#![cfg(feature = "pvz-emulator")]
// Existing regressions now call the owning core modules directly.
use rsvz_current::{CurrentBackend, scope_backend, with_backend};
use rsvz_game::bench::install_bench_session;
use rsvz_game::diagnostics::{runtime_report_time, with_runtime_report_time};
use rsvz_game::frame::{
    dispatch_current_frame_sample, dispatch_runtime_tick_reporting, reset_runtime_state_preserving_backend,
};
use rsvz_game::lifecycle::{
    begin_logic_tick, close_attempt, enter_fight, finalize_session, finish_hook_installation, finish_logic_tick,
    reset_session_resources, with_lifecycle,
};
use rsvz_game::registration::{abort_script_generation, begin_script_generation, finish_script_generation};
use rsvz_game::runtime::{CurrentFrameSample, RuntimeError, RuntimeFrameDispatch, RuntimeResult};
use rsvz_game::session::{
    DispatchOutcome, RuntimeDispatchState, SessionJobKey, claim_session_job, execute_pending_reset, fail_script,
    forbid_additional_world_resets, record_dispatch_outcome, request_world_reset_with as request_world_reset,
    reset_session_control, script_stop_requested, session_shard, set_session_artifact, stop_script,
    take_dispatch_outcome, take_fatal_session_error, take_session_artifact, world_epoch, world_reset_pending,
};
use rsvz_game::setup::{auto_enter_enabled, set_auto_enter, with_script_setup};
use rsvz_game::state_hook::clear_state_hooks;
use rsvz_game::timeline::dispatch_timeline_tick;
use rsvz_schedule::event::with_events;
use rsvz_schedule::state_hook::with_state_hooks;
use rsvz_schedule::tick::with_scheduler;
use rsvz_schedule::timeline::with_timeline;

#[path = "runtime_regression/current.rs"]
mod current;

#[path = "runtime_regression/dispatch.rs"]
mod dispatch;
