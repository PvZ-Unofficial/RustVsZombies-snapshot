//! Current-runtime state-hook user API.

pub use rsvz_schedule::state_hook::{StateEvent, StateHookCommandOutcome, StateHookDispatchResult, StateHookHandle};

pub use rsvz_game::state_hook::run_installer as __run_state_hook_installer;

macro_rules! state_hook_api {
    ($(#[$attr:meta])* $value:ident, $callable:ident, $event:ident) => {
        crate::callable::callable_api! {
            $(#[$attr])*
            pub $value: $callable;

            where {}

            impl<F>
            where {
                F: FnMut() + 'static,
            }
            call(callback: F) -> StateHookHandle {
                rsvz_game::state_hook::register_user_hook::<F>(StateEvent::$event, 0, callback)
            }

            impl<F>
            where {
                F: FnMut() + 'static,
            }
            call(order: i32, callback: F) -> StateHookHandle {
                rsvz_game::state_hook::register_user_hook::<F>(StateEvent::$event, order, callback)
            }
        }
    };
}

state_hook_api!(
    /// Registers a session hook after runtime attachment.
    on_after_attach,
    OnAfterAttach,
    AfterAttach
);
state_hook_api!(
    /// Registers a session hook before each script generation.
    on_before_script,
    OnBeforeScript,
    BeforeScript
);
state_hook_api!(
    /// Registers a session hook after successful script registration.
    on_after_script,
    OnAfterScript,
    AfterScript
);
state_hook_api!(
    /// Registers a session hook when a fight attempt becomes playable.
    on_enter_fight,
    OnEnterFight,
    EnterFight
);
state_hook_api!(
    /// Registers a session hook before a chooser/fight attempt is cleared.
    on_exit_fight,
    OnExitFight,
    ExitFight
);
state_hook_api!(
    /// Registers a session hook before each runtime logic tick.
    on_before_tick,
    OnBeforeTick,
    BeforeTick
);
state_hook_api!(
    /// Registers a session hook after each non-fatal runtime logic tick.
    on_after_tick,
    OnAfterTick,
    AfterTick
);
state_hook_api!(
    /// Registers a session hook before controlled session teardown.
    on_before_exit,
    OnBeforeExit,
    BeforeExit
);

pub use rsvz_game::state_hook::{register_fallible, remove_state_hook};
