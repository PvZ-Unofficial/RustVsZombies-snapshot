use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::convert::Infallible;
use std::sync::atomic::{AtomicUsize, Ordering};

use rsvz_schedule::event::{DEFAULT_PUBLIC_EVENT_CAPACITY, EventDispatcher, EventOptions, InternalEventInterceptor};
use rsvz_schedule::model::{
    EventDecision, EventFrameStatus, EventInterest, GameUi, Grid, PlantEffect, PlantEffectAttemptFact,
    PlantEffectOutcome, PlantEffectSource, PlantId, PlantKind, Wave, WaveTimingSnapshot, ZombieId,
};
use rsvz_schedule::state_hook::{StateEvent, StateHookDispatchResult, StateHookRegistry};
use rsvz_schedule::tick::{TickControl, TickMeta, TickOptions, TickPhase, TickScheduler};
use rsvz_schedule::timeline::{Timeline, TimelineDispatchResult};

struct CountingAllocator;

thread_local! {
    static COUNT_ALLOCATIONS: Cell<bool> = const { Cell::new(false) };
    static ALLOCATION_COUNT: Cell<usize> = const { Cell::new(0) };
}

fn record_allocation() {
    COUNT_ALLOCATIONS.with(|enabled| {
        if enabled.get() {
            ALLOCATION_COUNT.with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: Every operation delegates unchanged to `System`; the TLS bookkeeping
// only counts allocations made by the current test thread.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: delegated with the allocator contract received from the caller.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record_allocation();
        // SAFETY: delegated with the allocator contract received from the caller.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record_allocation();
        // SAFETY: delegated with the allocator contract received from the caller.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: delegated with the allocator contract received from the caller.
        unsafe { System.dealloc(ptr, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocations_during(f: impl FnOnce()) -> usize {
    // Initialize both TLS cells before accounting starts.
    COUNT_ALLOCATIONS.with(|enabled| enabled.set(false));
    ALLOCATION_COUNT.with(|count| count.set(0));
    COUNT_ALLOCATIONS.with(|enabled| enabled.set(true));
    f();
    COUNT_ALLOCATIONS.with(|enabled| enabled.set(false));
    ALLOCATION_COUNT.with(Cell::get)
}

fn playing_meta(clock: i32) -> TickMeta {
    TickMeta {
        phase: TickPhase::Playing,
        game_ui: Some(GameUi::Playing),
        clock: Some(clock),
        is_new_frame: true,
    }
}

static TIMELINE_RUNS: AtomicUsize = AtomicUsize::new(0);
static TICK_RUNS: AtomicUsize = AtomicUsize::new(0);
static HOOK_RUNS: AtomicUsize = AtomicUsize::new(0);
static EVENT_RUNS: AtomicUsize = AtomicUsize::new(0);
static PUBLIC_EVENT_RUNS: AtomicUsize = AtomicUsize::new(0);

struct EventProbe;

impl InternalEventInterceptor for EventProbe {
    fn interest(&self) -> EventInterest {
        EventInterest::BITE
    }

    fn begin_plant_effect(&mut self, _fact: PlantEffectAttemptFact) -> EventDecision {
        EVENT_RUNS.fetch_add(1, Ordering::Relaxed);
        EventDecision::Apply
    }
}

fn event_attempt(main_counter: i32) -> PlantEffectAttemptFact {
    PlantEffectAttemptFact {
        source: PlantEffectSource::Bite(ZombieId::from_raw(1)),
        plant_id: PlantId::from_raw(2),
        raw_kind: PlantKind::WallNut,
        effective_kind: PlantKind::WallNut,
        grid: Grid::new(0, 0).expect("valid grid"),
        hp_before: 100,
        max_hp: 4_000,
        effect: PlantEffect::HpDamage { native_requested: 4 },
        main_counter,
    }
}

#[test]
fn timeline_dispatch_allocates_nothing_after_warmup() {
    TIMELINE_RUNS.store(0, Ordering::Relaxed);
    let mut timeline = Timeline::new();

    assert_eq!(
        timeline.dispatch_tick_catching(WaveTimingSnapshot::minimal(100, Wave(1)), playing_meta(100),),
        TimelineDispatchResult::Continue
    );
    let _registration = timeline
        .at(Wave(1), 2, || {
            TIMELINE_RUNS.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
        .expect("timeline registration should succeed");

    let allocations = allocations_during(|| {
        assert_eq!(
            timeline.dispatch_tick_catching(WaveTimingSnapshot::minimal(102, Wave(1)), playing_meta(102),),
            TimelineDispatchResult::Continue
        );
    });

    assert_eq!(TIMELINE_RUNS.load(Ordering::Relaxed), 1);
    assert_eq!(allocations, 0, "warmed Timeline dispatch allocated");
}

#[test]
fn tick_scheduler_dispatch_allocates_nothing_after_warmup() {
    TICK_RUNS.store(0, Ordering::Relaxed);
    let mut scheduler = TickScheduler::new();
    scheduler.spawn(TickOptions::playing_frame(), |_backend| {
        TICK_RUNS.fetch_add(1, Ordering::Relaxed);
        Ok(TickControl::Continue)
    });

    assert!(
        scheduler.dispatch_tick(playing_meta(1)).is_ok(),
        "warmup dispatch should succeed"
    );
    let allocations = allocations_during(|| {
        assert!(
            scheduler.dispatch_tick(playing_meta(2)).is_ok(),
            "measured dispatch should succeed"
        );
    });

    assert_eq!(TICK_RUNS.load(Ordering::Relaxed), 2);
    assert_eq!(allocations, 0, "warmed TickScheduler dispatch allocated");
}

#[test]
fn state_hook_dispatch_allocates_nothing_after_warmup() {
    HOOK_RUNS.store(0, Ordering::Relaxed);
    let mut registry = StateHookRegistry::<Infallible>::new();
    registry.register(StateEvent::BeforeTick, 0, || {
        HOOK_RUNS.fetch_add(1, Ordering::Relaxed);
        Ok(())
    });

    assert_eq!(
        registry.dispatch_catching(StateEvent::BeforeTick),
        StateHookDispatchResult::Continue
    );
    let allocations = allocations_during(|| {
        assert_eq!(
            registry.dispatch_catching(StateEvent::BeforeTick),
            StateHookDispatchResult::Continue
        );
    });

    assert_eq!(HOOK_RUNS.load(Ordering::Relaxed), 2);
    assert_eq!(allocations, 0, "warmed StateHookRegistry dispatch allocated");
}

#[test]
fn event_interceptor_hot_path_allocates_nothing_after_registration() {
    EVENT_RUNS.store(0, Ordering::Relaxed);
    let mut events = EventDispatcher::new();
    events.begin_script_generation();
    events
        .install_internal_interceptor(Box::new(EventProbe))
        .expect("event registration should succeed");
    events.freeze().expect("freeze");

    let allocations = allocations_during(|| {
        events.begin_logic_frame(1, 10);
        let begin = events.begin_plant_effect(event_attempt(10));
        events.finish_plant_effect(begin.token, PlantEffectOutcome::HpDelta { applied: 4 });
        events.end_logic_frame(EventFrameStatus::Running);
    });

    assert_eq!(EVENT_RUNS.load(Ordering::Relaxed), 1);
    assert_eq!(events.take_fault(), None);
    assert_eq!(allocations, 0, "EventDispatcher hot path allocated");
}

#[test]
fn public_event_queue_reserves_only_at_first_freeze_and_not_on_the_hot_path() {
    PUBLIC_EVENT_RUNS.store(0, Ordering::Relaxed);
    let mut events = EventDispatcher::new();
    events.begin_script_generation();
    events
        .register_public_handler(
            EventInterest::BITE,
            EventOptions::new(),
            Box::new(|_| {
                PUBLIC_EVENT_RUNS.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .expect("observer");

    let cold_allocations = allocations_during(|| {
        events.freeze().expect("freeze");
    });
    assert!(cold_allocations > 0, "first freeze should reserve the public queue");
    assert!(events.public_queue_capacity() >= DEFAULT_PUBLIC_EVENT_CAPACITY);
    assert_eq!(
        allocations_during(|| {
            events.freeze().expect("repeated freeze");
        }),
        0,
        "an already reserved queue should be reused"
    );

    let hot_allocations = allocations_during(|| {
        events.begin_logic_frame(1, 10);
        let begin = events.begin_plant_effect(event_attempt(10));
        events.finish_plant_effect(begin.token, PlantEffectOutcome::HpDelta { applied: 4 });
        events.end_logic_frame(EventFrameStatus::Running);
        let mut dispatch = events.begin_public_dispatch().expect("pending event");
        while let Some(callback) = events.take_next_public_callback(&mut dispatch) {
            let outcome = EventDispatcher::run_public_callback_catching(callback);
            assert_eq!(
                events.finish_public_callback(outcome),
                rsvz_schedule::event::PublicEventCallbackResult::Continue
            );
        }
        events.end_public_dispatch(dispatch);
    });

    assert_eq!(PUBLIC_EVENT_RUNS.load(Ordering::Relaxed), 1);
    assert_eq!(hot_allocations, 0, "public event hot path allocated");
}
