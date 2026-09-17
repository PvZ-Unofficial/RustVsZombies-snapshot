//! Low-frequency facts recorded after a completed round, before the next opening.
use crate::runtime::RuntimeResult;
use rsvz_backend_api::SunQueryBackend;
use rsvz_current::CurrentBackend;
use rsvz_schedule::tick::{TickControl, TickLane, TickOptions, TickPhase, TickTrigger};

/// Logs one completed round's sun balance. Register once per script generation.
/// Manual exits and incomplete failed rounds do not count as completed rounds.
pub fn trace_round_end_sun() -> RuntimeResult<()>
where
    CurrentBackend: SunQueryBackend,
{
    crate::tick::spawn(
        TickOptions::any_dispatch()
            .trigger(TickTrigger::OnceReady(TickPhase::RoundComplete))
            .lane(TickLane::Observe)
            .idle_neutral()
            .name("round_end_sun"),
        |_| {
            let sun =
                crate::access::with_backend(|backend| crate::live_value::read_or_abort(backend.sun(), "round-end sun"));
            crate::diagnostics::log(
                crate::diagnostics::LogLevel::Debug,
                format_args!(
                    "round_end worker={} epoch={} rounds={} sun={}",
                    crate::session::session_shard().index,
                    crate::session::world_epoch(),
                    crate::session::completed_rounds(),
                    sun
                ),
            );
            Ok(TickControl::Stop)
        },
    );
    Ok(())
}
