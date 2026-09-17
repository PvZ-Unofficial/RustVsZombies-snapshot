//! Script-registration state shared by setup helpers and the low DSL.

use std::cell::RefCell;
use std::fmt::Display;

use crate::runtime::{RuntimeError, RuntimeResult};

thread_local! {
    static CURRENT: RefCell<Option<RegistrationContext>> = const { RefCell::new(None) };
}

#[derive(Default)]
struct RegistrationContext {
    errors: Vec<String>,
}

struct RegistrationGuard {
    previous: Option<RegistrationContext>,
    active: bool,
}

impl RegistrationGuard {
    fn enter() -> Self {
        let previous = CURRENT.with(|slot| slot.replace(Some(RegistrationContext::default())));
        Self { previous, active: true }
    }

    fn finish(mut self, body_result: RuntimeResult<()>) -> RuntimeResult<()> {
        let mut current = CURRENT
            .with(|slot| slot.replace(self.previous.take()))
            .expect("script registration context disappeared");
        self.active = false;
        if let Err(error) = body_result {
            current.errors.push(error.to_string());
        }
        if current.errors.is_empty() {
            Ok(())
        } else {
            Err(RuntimeError::new(current.errors.join("; ")))
        }
    }
}

impl Drop for RegistrationGuard {
    fn drop(&mut self) {
        if self.active {
            CURRENT.with(|slot| {
                slot.replace(self.previous.take());
            });
        }
    }
}

pub fn record_error(error: impl Display) {
    with_current(|context| context.errors.push(error.to_string()));
}

pub fn is_active() -> bool {
    CURRENT.with(|slot| slot.borrow().is_some())
}

fn with_current<T>(f: impl FnOnce(&mut RegistrationContext) -> T) -> T {
    CURRENT.with(|slot| {
        let mut slot = slot.borrow_mut();
        let context = slot
            .as_mut()
            .expect("operation has no current script registration context");
        f(context)
    })
}

#[doc(hidden)]
pub fn run_script(body: impl FnOnce() -> RuntimeResult<()>) -> RuntimeResult<()> {
    let guard = RegistrationGuard::enter();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).unwrap_or_else(|payload| {
        match rsvz_schedule::callback::into_error(payload) {
            Ok(error) => Err(error),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    });
    guard.finish(result)
}

mod current;
pub use current::{ScriptGeneration, abort_script_generation, begin_script_generation, finish_script_generation};
