use std::cell::{Cell, RefCell};

use rsvz_model::{AdvancedPauseOptions, FastForwardOptions, FastForwardStopReason, SeedChooserFastForwardOptions};

use crate::error::{Pvz1051Error, Result};
use crate::patches::ImitatorMorphHookGuard;
use crate::runtime::Pvz1051Backend;
use crate::runtime::advanced_pause::AdvancedPauseRuntime;
use crate::runtime::fast_forward::{
    FastForwardLoopDecision, FastForwardRuntime, SeedChooserFastForwardLoopDecision, SeedChooserFastForwardRuntime,
    SeedChooserFastForwardStopReason,
};

use super::{lifecycle, profiler};

thread_local! {
    static FAST_FORWARD: RefCell<FastForwardRuntime> = const { RefCell::new(FastForwardRuntime::new()) };
    static SEED_CHOOSER_FAST_FORWARD: RefCell<SeedChooserFastForwardRuntime> =
        const { RefCell::new(SeedChooserFastForwardRuntime::new()) };
    static ADVANCED_PAUSE: RefCell<AdvancedPauseRuntime> =
        const { RefCell::new(AdvancedPauseRuntime::new()) };
    static IMITATOR_MORPH_HOOK: RefCell<Option<ImitatorMorphHookGuard>> =
        const { RefCell::new(None) };
    static ORIGINAL_GAME_SPEED: Cell<Option<(i32, f64)>> = const { Cell::new(None) };
}

fn ensure_accepting_requests() -> Result<()> {
    if lifecycle::accepting_dispatch() {
        Ok(())
    } else {
        Err(Pvz1051Error::AbiPreconditionFailed(
            "1051 runtime is not accepting control requests",
        ))
    }
}

pub(crate) fn start_fast_forward(options: FastForwardOptions) -> Result<()> {
    ensure_accepting_requests()?;
    if advanced_pause_active() {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "fast-forward cannot start while advanced pause is active",
        ));
    }
    let requested = FAST_FORWARD.with(|state| state.borrow_mut().start(options))?;
    if requested {
        profiler::reset();
    }
    Ok(())
}

pub(crate) fn stop_fast_forward(reason: FastForwardStopReason) -> Result<()> {
    FAST_FORWARD.with(|state| {
        let mut state = state.borrow_mut();
        let frames = state.frames_advanced();
        let was_active = state.is_active();
        state.stop(reason)?;
        if was_active {
            finish_profile(reason, frames);
        }
        Ok(())
    })
}

pub(crate) fn fast_forward_active() -> bool {
    FAST_FORWARD.with(|state| state.borrow().is_active())
}

pub(crate) fn request_seed_chooser_fast_forward(options: SeedChooserFastForwardOptions) -> Result<()> {
    ensure_accepting_requests()?;
    if advanced_pause_active() || fast_forward_active() {
        return Err(Pvz1051Error::AbiPreconditionFailed(
            "seed chooser fast-forward conflicts with another physical loop",
        ));
    }
    SEED_CHOOSER_FAST_FORWARD.with(|state| {
        let _requested = state.borrow_mut().request(options);
    });
    Ok(())
}

pub(crate) fn set_advanced_pause(backend: &Pvz1051Backend, enabled: bool, options: AdvancedPauseOptions) -> Result<()> {
    if enabled {
        ensure_accepting_requests()?;
        if fast_forward_active() {
            return Err(Pvz1051Error::AbiPreconditionFailed(
                "advanced pause cannot start while fast-forward is active",
            ));
        }
        if options.draw_mask {
            return Err(Pvz1051Error::KindUnavailable("advanced pause mask drawing"));
        }
        if options.play_sound {
            return Err(Pvz1051Error::KindUnavailable("advanced pause sound feedback"));
        }
        crate::runtime::advanced_pause::ensure_can_enable(backend)?;
    }
    ADVANCED_PAUSE.with(|state| state.borrow_mut().set_enabled(enabled, options))
}

pub(crate) fn advanced_pause_active() -> bool {
    ADVANCED_PAUSE.with(|state| state.borrow().is_active())
}

pub(super) fn update_advanced_pause_frame() -> Result<()> {
    ADVANCED_PAUSE.with(|state| state.borrow_mut().update_frame())
}

pub(super) fn ensure_native_observers() -> Result<()> {
    IMITATOR_MORPH_HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(ImitatorMorphHookGuard::install()?);
        }
        Ok(())
    })
}

pub(super) fn restore_observers() -> Result<()> {
    let result = IMITATOR_MORPH_HOOK.with(|slot| {
        let mut slot = slot.borrow_mut();
        let Some(hook) = slot.as_mut() else {
            return Ok(());
        };
        hook.restore()?;
        *slot = None;
        Ok(())
    });
    crate::runtime::imitator_morph::reset();
    result
}

pub(crate) fn set_game_speed(speed: f32) -> Result<()> {
    let app = crate::runtime::backend::current_app()?;
    ORIGINAL_GAME_SPEED.with(|original| {
        if original.get().is_none() {
            // SAFETY: the active LawnApp was validated above and these are
            // process-local scalar timing controls.
            original.set(Some(unsafe {
                (
                    crate::ops::timing::tick_ms(app),
                    crate::ops::timing::update_multiplier(app),
                )
            }));
        }
    });
    // SAFETY: the active LawnApp was validated above. Values are bounded by
    // the public backend implementation before reaching this function.
    unsafe {
        if speed <= 10.0 {
            crate::ops::timing::set_tick_ms(app, (10.0 / speed + 0.5) as i32);
            crate::ops::timing::set_update_multiplier(app, 1.0);
        } else {
            crate::ops::timing::set_tick_ms(app, 10);
            crate::ops::timing::set_update_multiplier(app, f64::from(speed));
        }
    }
    Ok(())
}

pub(crate) fn restore_game_speed() -> Result<()> {
    let Some((tick_ms, update_multiplier)) = ORIGINAL_GAME_SPEED.get() else {
        return Ok(());
    };
    let app = crate::runtime::backend::current_app()?;
    // SAFETY: these exact scalar values were captured before this runner first
    // changed them and unload is executing on the game thread.
    unsafe {
        crate::ops::timing::set_tick_ms(app, tick_ms);
        crate::ops::timing::set_update_multiplier(app, update_multiplier);
    }
    ORIGINAL_GAME_SPEED.set(None);
    Ok(())
}

pub(super) fn seed_chooser_before_update() -> Result<SeedChooserFastForwardLoopDecision> {
    SEED_CHOOSER_FAST_FORWARD.with(|state| {
        let mut state = state.borrow_mut();
        let decision = state.before_tick()?;
        if let SeedChooserFastForwardLoopDecision::Stopped(reason) = decision {
            return Ok(state.stop(reason));
        }
        Ok(decision)
    })
}

pub(super) fn advance_seed_chooser_update() -> Result<SeedChooserFastForwardLoopDecision> {
    SEED_CHOOSER_FAST_FORWARD.with(|state| state.borrow_mut().advance_widget_frame())
}

pub(super) fn fast_forward_before_update() -> Result<FastForwardLoopDecision> {
    FAST_FORWARD.with(|state| {
        let mut state = state.borrow_mut();
        let frames = state.frames_advanced();
        let was_active = state.is_active();
        let decision = state.before_tick()?;
        if matches!(decision, FastForwardLoopDecision::Stopped(_)) && was_active {
            let FastForwardLoopDecision::Stopped(reason) = decision else {
                unreachable!()
            };
            finish_profile(reason, frames);
        }
        Ok(decision)
    })
}

pub(super) fn advance_fast_forward_update() -> Result<FastForwardLoopDecision> {
    FAST_FORWARD.with(|state| {
        let mut state = state.borrow_mut();
        let frames = state.frames_advanced();
        let was_active = state.is_active();
        let decision = state.advance_logic_frame()?;
        if matches!(decision, FastForwardLoopDecision::Stopped(_)) && was_active {
            let FastForwardLoopDecision::Stopped(reason) = decision else {
                unreachable!()
            };
            finish_profile(reason, frames);
        }
        Ok(decision)
    })
}

pub(super) fn restore_all() -> Result<()> {
    let mut first_error = None;
    if let Err(error) = stop_fast_forward(FastForwardStopReason::RuntimeExit)
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    SEED_CHOOSER_FAST_FORWARD.with(|state| {
        let _decision = state.borrow_mut().stop(SeedChooserFastForwardStopReason::RuntimeExit);
    });
    if let Err(error) = ADVANCED_PAUSE.with(|state| state.borrow_mut().restore())
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    if let Err(error) = crate::patches::leases::release_all_owned_patches()
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    if let Err(error) = crate::patches::restore_random()
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    if let Err(error) = crate::patches::restore_sun_production()
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    if let Err(error) = restore_game_speed()
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    first_error.map_or(Ok(()), Err)
}

pub(crate) fn prepare_world_reset() -> Result<()> {
    let mut first_error = None;
    if let Err(error) = stop_fast_forward(FastForwardStopReason::RuntimeExit) {
        first_error = Some(error);
    }
    SEED_CHOOSER_FAST_FORWARD.with(|state| {
        let _decision = state.borrow_mut().stop(SeedChooserFastForwardStopReason::RuntimeExit);
    });
    if let Err(error) = ADVANCED_PAUSE.with(|state| state.borrow_mut().restore())
        && first_error.is_none()
    {
        first_error = Some(error);
    }
    crate::runtime::imitator_morph::reset();
    first_error.map_or(Ok(()), Err)
}

fn finish_profile(reason: FastForwardStopReason, frames: u64) {
    if let Some(summary) = profiler::finish_summary(reason, frames) {
        profiler::write_summary_to_temp(&summary);
    }
}
