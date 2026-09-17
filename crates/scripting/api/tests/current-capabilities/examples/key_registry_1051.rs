use rsvz::core::model::KeyCode;
use rsvz::core::tick::{TickCommandOutcome, TickTaskState};
use rsvz::key::{self, KeyBindError};

// These checks register and edit tasks; they never poll native keyboard input.
fn main() {
    rsvz::reset_runtime_state_preserving_backend();
    let first = key::on_press(KeyCode::Q, || Ok(()));
    let second = key::on_press(KeyCode::Q, || Ok(()));
    assert_ne!(first, second);
    rsvz::with_scheduler(|scheduler| {
        assert_eq!(scheduler.stop(first), TickCommandOutcome::Applied);
        assert_eq!(scheduler.state(first), TickTaskState::Stopped);
        assert_eq!(scheduler.state(second), TickTaskState::Running);
    });

    let unique = key::try_on_press_unique('q', || Ok(())).unwrap();
    assert!(matches!(
        key::try_on_release_unique(KeyCode::Q, || Ok(())),
        Err(KeyBindError::DuplicateBinding { key: KeyCode::Q })
    ));
    rsvz::with_scheduler(|scheduler| {
        scheduler.pause(unique);
    });
    assert!(matches!(
        key::try_on_press_unique(KeyCode::Q, || Ok(())),
        Err(KeyBindError::DuplicateBinding { .. })
    ));
    rsvz::with_scheduler(|scheduler| {
        scheduler.stop(unique);
    });
    let rebound = key::try_on_release_unique(KeyCode::Q, || Ok(())).unwrap();
    assert_eq!(
        rsvz::with_scheduler(|scheduler| scheduler.state(rebound)),
        TickTaskState::Running
    );

    rsvz::with_scheduler(|scheduler| scheduler.clear_all());
    let stale_rebound = key::try_on_press_unique(KeyCode::Q, || Ok(())).unwrap();
    assert_eq!(
        rsvz::with_scheduler(|scheduler| scheduler.state(stale_rebound)),
        TickTaskState::Running
    );
    assert!(key::try_on_press_unique(KeyCode::W, || Ok(())).is_ok());
    assert!(matches!(
        key::try_on_press('中', || Ok(())),
        Err(KeyBindError::KeyCode(_))
    ));
    assert!(key::try_on_press('q', || Ok(())).is_ok());
    let raw = KeyCode::try_from_raw_virtual_key(0x07).unwrap();
    assert!(matches!(
        key::try_on_press_unique(raw, || Ok(())),
        Err(KeyBindError::UnknownKey { .. })
    ));
    let shared_raw = key::on_press(raw, || Ok(()));
    assert_eq!(
        rsvz::with_scheduler(|scheduler| scheduler.state(shared_raw)),
        TickTaskState::Running
    );
    println!("1051 key registration, uniqueness, and handle-state checks passed");
}
