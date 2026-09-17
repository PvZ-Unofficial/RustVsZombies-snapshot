#[test]
fn invalid_setters_preserve_cursor_and_extreme_skips_wrap() {
    let manager = CobManager::new_root();
    manager.set_list_unchecked([
        Grid { row: 0, col: 0 },
        Grid { row: 1, col: 0 },
        Grid { row: 2, col: 0 },
    ]);
    manager.set_next_slot(2).unwrap();
    for slot in [0, 4, i32::MIN, i32::MAX] {
        assert!(manager.set_next_slot(slot).is_err());
        assert_eq!(manager.next_index_raw(), 1);
    }
    for delta in [i32::MIN, i32::MAX, -1, 1] {
        let expected = (i64::from(manager.next_index_raw()) + i64::from(delta)).rem_euclid(3) as i32;
        manager.skip(delta).unwrap();
        assert_eq!(manager.next_index_raw(), expected);
    }
    manager.set_sequential_mode(CobSequentialMode::Priority);
    let before = manager.next_index_raw();
    assert!(manager.set_next_slot(1).is_err());
    assert_eq!(manager.next_index_raw(), before);
    manager.set_list_unchecked([] as [Grid; 0]);
    assert!(manager.skip(1).is_err());
}

#[test]
fn repair_lock_releases_when_callback_is_cancelled_or_aborts() {
    let manager = CobManager::new_root();
    manager.0.borrow_mut().latest.writable = false;
    let guard = LatestUnlock {
        manager: manager.clone(),
        armed: true,
    };
    let mut scheduler = TickScheduler::new();
    scheduler.spawn(TickOptions::playing_frame(), move |_| {
        let _keep_alive = &guard;
        Ok(TickControl::Continue)
    });
    drop(scheduler);
    assert!(manager.0.borrow().latest.writable);

    manager.0.borrow_mut().latest.writable = false;
    let _failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let guard = LatestUnlock {
            manager: manager.clone(),
            armed: true,
        };
        let _state = manager.0.borrow_mut();
        let _keep_alive = &guard;
        panic!("abort while borrowed");
    }));
    assert!(manager.0.borrow().latest.writable);
}

use super::*;

#[test]
fn fix_latest_uses_fire_time_and_execution_time_list_index() {
    let manager = CobManager::new();
    let mut state = manager.0.borrow_mut();
    state.set_list_unchecked([Grid { row: 0, col: 0 }, Grid { row: 0, col: 1 }]);
    state.record_latest(100, 0);
    let (index, delay) = state.fix_latest_index_and_delay(150).unwrap();
    assert_eq!(delay, 155);
    state.set_list_unchecked([Grid { row: 4, col: 4 }, Grid { row: 3, col: 3 }]);
    assert_eq!(state.finish_fix_latest(index), Ok(Grid { row: 4, col: 4 }));
}
