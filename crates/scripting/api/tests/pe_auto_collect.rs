#![cfg(feature = "pvz-emulator")]

use rsvz::auto_collect;
use rsvz_game::{frame, lifecycle, registration, session};
use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};
use rsvz_schedule::tick::{TickMeta, TickPhase, with_scheduler};

fn assert_no_collection_tasks(expected_other_tasks: usize) {
    with_scheduler(|scheduler| {
        let order = scheduler.begin_runtime_dispatch_tick(TickMeta {
            phase: TickPhase::Playing,
            game_ui: Some(rsvz_model::GameUi::Playing),
            clock: Some(0),
            is_new_frame: true,
        });
        assert_eq!(order.queued_len(), expected_other_tasks);
        scheduler.end_runtime_dispatch_tick(order);
    });
    rsvz_game::auto_collect::with_item_collector(|resource| {
        assert!(resource.native_task.is_none());
        assert!(resource.click_task.is_none());
    });
}

#[test]
fn pe_collection_is_a_validated_noop_in_every_mode_and_registration_path() {
    frame::reset_runtime_state_preserving_backend();
    session::reset_session_control();
    lifecycle::reset_session_resources();
    rsvz_game::state_hook::clear_state_hooks();
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let reports = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
            let captured = reports.clone();
            let logger = rsvz::replace_logger(move |record: &rsvz::LogRecord<'_>| {
                captured.borrow_mut().push(record.to_string());
            });
            assert!(auto_collect::set_interval(0).is_err());
            assert!(auto_collect::set_interval(i32::MAX as u32 + 1).is_err());
            assert!(auto_collect::set_type_list([i32::MAX]).is_err());
            auto_collect::set_interval(1).unwrap();
            for mode in [
                rsvz_model::AutoCollectMode::Normal,
                rsvz_model::AutoCollectMode::Click,
                rsvz_model::AutoCollectMode::Off,
            ] {
                auto_collect::set_mode(mode).unwrap();
                auto_collect::tick().unwrap();
                auto_collect::tick_click().unwrap();
                auto_collect::register_tick().unwrap();
                auto_collect::register_click_tick().unwrap();
                assert_no_collection_tasks(0);
            }
            // The framework installer also adds no empty collection task.
            rsvz::install_framework_state_hooks().unwrap();
            lifecycle::finish_hook_installation().unwrap();
            let generation = registration::begin_script_generation().unwrap();
            rsvz::__run_script(|| {
                auto_collect::click()?;
                Ok(())
            })
            .unwrap();
            registration::finish_script_generation(generation).unwrap();
            // The pre-existing fast-forward tracker is the only frame task installed here.
            assert!(rsvz_game::fast_forward::with_fast_forward_task(|task| task.is_some()));
            assert_no_collection_tasks(1);
            assert!(session::take_dispatch_outcome().is_none());
            lifecycle::finalize_session().unwrap();
            rsvz::restore_logger(logger);
            assert!(reports.borrow().is_empty(), "{:?}", reports.borrow());
        })
    });
}
