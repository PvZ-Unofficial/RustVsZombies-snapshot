use std::ffi::c_char;

pub const fn plugin_abi_version() -> u32 {
    5
}

pub fn plugin_initialize(dispatch: crate::DispatchEntry) -> i32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // SAFETY: only fixed layout ABI data is exchanged before this succeeds.
        if unsafe { pvzp_rs::raw::pvzp_rs_validate_abi() } == 0 {
            return Err("EXE/Clang native layout mismatch".to_owned());
        }
        let config = std::env::var("RSVZ_PORTABLE_CONFIG").map_err(|error| error.to_string())?;
        crate::host::initialize(config.as_bytes(), dispatch)?;
        // SAFETY: both layout checks succeeded; callbacks remain in this DLL until shutdown.
        if unsafe {
            pvzp_rs::raw::pvzp_rs_register_update(
                Some(callbacks::rsvz_pvzp_dispatch),
                Some(callbacks::board_destroying),
            )
        } == 0
        {
            return Err("native callback registration rejected".to_owned());
        }
        Ok(())
    }));
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            log(&format!("Portable initialization failed: {error}"));
            1
        }
        Err(_) => 1,
    }
}

pub fn plugin_shutdown() -> i32 {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(crate::host::shutdown)).map_or(1, |result| result)
}

unsafe extern "C" {
    fn rsvz_pvzp_log(message: *const c_char, length: usize);
}

pub(crate) fn log(message: &str) {
    // SAFETY: the bridge copies exactly `length` bytes before returning.
    unsafe { rsvz_pvzp_log(message.as_ptr().cast(), message.len()) };
}

pub(crate) mod callbacks {
    use std::ffi::c_void;
    use std::panic::{self, AssertUnwindSafe};

    fn native_event<R>(fallback: R, event: impl FnOnce() -> R) -> R {
        match panic::catch_unwind(AssertUnwindSafe(event)) {
            Ok(value) => value,
            Err(_) => {
                crate::host::native_event_panicked();
                fallback
            }
        }
    }

    pub extern "C" fn rsvz_pvzp_dispatch(world_replaced: u8, completed_rounds: u64) -> i32 {
        native_event(2, || crate::host::dispatch(world_replaced != 0, completed_rounds))
    }

    pub extern "C" fn rsvz_pvzp_begin_logic_frame(board_epoch: u64, main_counter: i32) {
        native_event((), || crate::event::begin_logic_frame(board_epoch, main_counter));
    }

    pub extern "C" fn rsvz_pvzp_end_logic_frame(status: i32) {
        native_event((), || crate::event::end_logic_frame(status));
    }

    pub unsafe extern "C" fn rsvz_pvzp_begin_plant_effect(
        source_kind: i32, actor: *mut c_void, plant: *mut c_void, effect_kind: i32, requested: i32,
    ) -> u64 {
        native_event(0, || {
            // SAFETY: the native hook identifies the concrete live Plant pointer type.
            let begin = unsafe {
                crate::event::begin_plant_effect(
                    source_kind,
                    actor,
                    plant.cast::<pvzp_rs::raw::pvzp_rs_plant>(),
                    effect_kind,
                    requested,
                )
            };
            u64::from(begin.token.raw())
                | (u64::from(begin.decision == rsvz_model::model::EventDecision::SuppressByMeasurement) << 32)
        })
    }

    pub extern "C" fn rsvz_pvzp_finish_plant_effect(token: u32, outcome: i32, applied: i32) {
        native_event((), || crate::event::finish_plant_effect(token, outcome, applied));
    }

    pub unsafe extern "C" fn rsvz_pvzp_emit_home_entry(zombie: *mut c_void) {
        native_event((), || {
            // SAFETY: the native hook identifies a live occupied Zombie pointer for this call only.
            unsafe { crate::event::emit_home_entry(zombie.cast::<pvzp_rs::raw::pvzp_rs_zombie>()) };
        });
    }

    pub unsafe extern "C" fn rsvz_pvzp_emit_gargantuar_spawned(zombie: *mut c_void) {
        native_event((), || {
            // SAFETY: Portable supplies a live occupied Zombie pointer for this call only.
            unsafe { crate::event::emit_gargantuar_spawned(zombie.cast()) };
        });
    }

    pub unsafe extern "C" fn rsvz_pvzp_emit_imp_thrown(parent: *mut c_void, imp: *mut c_void) {
        native_event((), || {
            // SAFETY: Portable supplies both live occupied Zombie pointers synchronously.
            unsafe { crate::event::emit_imp_thrown(parent.cast(), imp.cast()) };
        });
    }

    pub unsafe extern "C" fn rsvz_pvzp_emit_gargantuar_ash_hit(zombie: *mut c_void) {
        native_event((), || {
            // SAFETY: Portable supplies a live occupied Gargantuar pointer before burn routing.
            unsafe { crate::event::emit_gargantuar_ash_hit(zombie.cast()) };
        });
    }

    pub extern "C" fn board_destroying() {
        native_event((), crate::event::clear_native_event_sink);
    }
}
