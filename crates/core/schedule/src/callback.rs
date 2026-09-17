//! Local callback control flow, distinct from a fatal timeline termination.
use rsvz_backend_api::error::RuntimeError;
use std::any::Any;

struct CallbackAbort(RuntimeError);

/// Ends the nearest callback boundary. Reporting is deferred until its guards unwind.
#[doc(hidden)]
pub fn abort(error: RuntimeError) -> ! {
    std::panic::resume_unwind(Box::new(CallbackAbort(error)))
}

#[doc(hidden)]
pub fn is_abort(payload: &(dyn Any + Send)) -> bool {
    payload.is::<CallbackAbort>()
}

#[doc(hidden)]
pub fn into_error(payload: Box<dyn Any + Send>) -> Result<RuntimeError, Box<dyn Any + Send>> {
    payload.downcast::<CallbackAbort>().map(|abort| abort.0)
}
