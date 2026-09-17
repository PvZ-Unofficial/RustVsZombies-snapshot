use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Default)]
struct Keyboard {
    down: bool,
    focused: bool,
}
#[test]
fn focused_edges_and_focus_regain_preserve_the_previous_contract() {
    let inputs = [
        (false, true),
        (true, true),
        (true, true),
        (false, true),
        (true, false),
        (true, true),
        (false, false),
        (false, true),
        (true, true),
    ];
    for (edge, focus, expected) in [
        (
            EdgeKind::Press,
            true,
            vec![false, true, false, false, false, false, false, false, true],
        ),
        (
            EdgeKind::Release,
            true,
            vec![false, false, false, true, false, false, false, false, false],
        ),
        (
            EdgeKind::Press,
            false,
            vec![false, true, false, false, true, false, false, false, true],
        ),
    ] {
        let mut was_down = false;
        let actual: Vec<_> = inputs
            .into_iter()
            .map(|(down, focused)| edge_triggered(down, !focus || focused, edge, &mut was_down))
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn subscriptions_have_independent_edges_and_release_input_before_callbacks() {
    let input = Rc::new(RefCell::new(Keyboard {
        down: true,
        focused: true,
    }));
    let count = Rc::new(Cell::new(0));
    let make = |weight| {
        let read = input.clone();
        let write = input.clone();
        let count = count.clone();
        binding_callback(
            move |previous| {
                let input = read.borrow();
                Ok(edge_triggered(input.down, input.focused, EdgeKind::Press, previous))
            },
            move || {
                let _borrow = write.borrow_mut();
                count.set(count.get() + weight);
                Ok(())
            },
        )
    };
    let mut first = make(1);
    let mut second = make(10);
    let meta = rsvz_schedule::TickMeta::unavailable();
    first(meta).unwrap();
    second(meta).unwrap();
    assert_eq!(count.get(), 11);
    first(meta).unwrap();
    second(meta).unwrap();
    assert_eq!(count.get(), 11);
    input.borrow_mut().down = false;
    second(meta).unwrap();
    input.borrow_mut().down = true;
    second(meta).unwrap();
    assert_eq!(count.get(), 21);
    assert_eq!(
        KeyBindOptions::new()
            .tick_options(TickOptions::once_playing_ready())
            .repeating_tick_options()
            .trigger,
        TickTrigger::Repeating
    );
}
