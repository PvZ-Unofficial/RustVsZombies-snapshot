use rsvz::prelude::*;

fn callback() -> RuntimeResult<()> {
    let _: Option<RuntimeError> = None;
    Ok(())
}

fn main() {
    let _fire = rsvz::cob::fire;
    let _at: fn(i32, i32, fn()) -> Option<rsvz::timeline::TimeHandle> = rsvz::at::<()>;
    let _callback: fn() -> rsvz::RuntimeResult<()> = callback;
    let _: Option<rsvz::RuntimeError> = None;
}
