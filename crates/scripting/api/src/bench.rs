//! Script-declared benchmark session job.

use std::time::Duration;

pub use rsvz_game::bench::{BenchOptions, BenchReport};

crate::callable::callable_api! {
    /// Declares a session benchmark with default 10ms stop precision.
    ///
    /// Passing only a [`Duration`] enables seed-chooser fast-forward and
    /// aggressive battle fast-forward by default. To benchmark normal-speed gameplay, pass
    /// `BenchOptions::new(duration).fast_forward(false)` instead.
    pub start: StartBench;

    where {
        rsvz_current::CurrentBackend: rsvz_backend_api::FastForwardBackend,
    }

    impl<>
    where {}
    call(duration: Duration) -> () {
        rsvz_game::bench::start(BenchOptions::new(duration));
    }

    impl<>
    where {}
    call(options: BenchOptions) -> () {
        rsvz_game::bench::start(options);
    }
}
