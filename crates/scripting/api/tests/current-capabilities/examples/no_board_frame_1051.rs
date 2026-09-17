//! Non-live concrete-backend fixture. No game process, native actions or injection.
use rsvz::core::model::{GameUi, PlantId, Wave, WaveTimingSnapshot};
use rsvz::tick::{TickControl, TickMeta, TickOptions, TickPhase, TickTaskState};
use std::{cell::Cell, ffi::c_void, rc::Rc};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn VirtualAlloc(address: *mut c_void, size: usize, allocation: u32, protect: u32) -> *mut c_void;
    fn VirtualFree(address: *mut c_void, size: usize, kind: u32) -> i32;
}

struct RootPage(*mut c_void);
impl Drop for RootPage {
    fn drop(&mut self) {
        // SAFETY: this is exactly the allocation owned by this test process.
        assert_ne!(unsafe { VirtualFree(self.0, 0, 0x8000) }, 0);
    }
}

fn main() {
    assert_eq!(std::mem::size_of::<rsvz_current::CurrentBackend>(), 0);
    // The example is linked at 0x10000000. Refuse to overwrite any existing mapping.
    let page = unsafe { VirtualAlloc(0x006a0000 as *mut c_void, 0x10000, 0x3000, 0x04) };
    if page.is_null() {
        std::process::exit(77);
    }
    assert_eq!(page as usize, 0x006a0000);
    let _page = RootPage(page);
    let mut app = [0u32; 0x900 / 4];
    // SAFETY: owned writable page; LawnApp + 0x768 remains null. The array stays
    // alive throughout the unique token scope. No other backend token is created.
    unsafe {
        (0x006a9ec0 as *mut *mut u32).write(app.as_mut_ptr());
    }
    // SAFETY: CurrentBackend is an inhabited ZST with only a PhantomData marker.
    // This isolated process supplies its unique token and a live synthetic App.
    let mut token: rsvz_current::CurrentBackend = unsafe { std::mem::zeroed() };
    let entered = Rc::new(Cell::new(0));
    let after_query = Rc::new(Cell::new(false));
    let logs = Rc::new(Cell::new(0));
    let report = Rc::clone(&logs);
    let logger = rsvz::replace_logger(move |_: &rsvz::LogRecord<'_>| report.set(report.get() + 1));
    rsvz::reset_runtime_state_preserving_backend();
    rsvz_current::scope_backend(&mut token, || {
        let pure = Rc::clone(&entered);
        let reached = Rc::clone(&after_query);
        rsvz::__run_script(|| {
            rsvz::at_frame(1, 0, move |_frame| pure.set(pure.get() + 1));
            rsvz::at_frame(1, 0, move |frame| {
                let _ = frame.plants().count();
                reached.set(true);
            });
            rsvz::at(1, 0, || ());
            Ok(())
        })
        .unwrap();
        let mut errors = Vec::new();
        let meta = TickMeta {
            phase: TickPhase::Playing,
            game_ui: Some(GameUi::Playing),
            clock: Some(1),
            is_new_frame: true,
        };
        rsvz_game::timeline::dispatch_timeline_tick_reporting(
            WaveTimingSnapshot::minimal(1, Wave(1)),
            meta,
            &mut |e| errors.push(e),
        );
        assert_eq!(entered.get(), 1, "pure Frame must run with a token but no Board");
        assert!(!after_query.get(), "first real Board query must abort locally");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].to_string().contains("Board"));
        // A primed, already-due registration executes immediately with the same rules.
        rsvz::at_frame(1, 0, |_frame| ());
        rsvz::at_frame(1, 0, |frame| {
            let _ = frame.zombies().count();
        });
        assert_eq!(logs.get(), 1);
        let repeating = rsvz::tick::spawn(TickOptions::any_dispatch(), |_| {
            let _ = rsvz::Plant::from_id(PlantId::from_raw(0)).hp();
            panic!("missing Board must not become a missing object");
            #[allow(unreachable_code)]
            Ok(TickControl::Stop)
        });
        let pure_tick = Rc::clone(&entered);
        rsvz::tick::spawn(TickOptions::any_dispatch(), move |_| {
            pure_tick.set(pure_tick.get() + 1);
            Ok(TickControl::Continue)
        });
        for _ in 0..2 {
            rsvz_game::tick::dispatch_scheduler_tick_reporting(meta, &mut rsvz_game::diagnostics::report_runtime_error);
        }
        assert_eq!(entered.get(), 3, "later pure callbacks must continue");
        rsvz::with_scheduler(|scheduler| assert_eq!(scheduler.state(repeating), TickTaskState::Running));
        assert_eq!(logs.get(), 3, "repeating failed callbacks remain registered");
    });
    assert!(rsvz_current::with_backend_shared(|_| panic!("no owner")).is_err());
    let missing_owner = Rc::clone(&entered);
    rsvz::at_frame(1, 0, move |_| missing_owner.set(99));
    assert_eq!(entered.get(), 3, "no owner must not manufacture a Frame");
    assert_eq!(logs.get(), 4);
    rsvz::restore_logger(logger);
    rsvz::reset_runtime_state_preserving_backend();
}
