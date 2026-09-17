use rsvz::core::model::PlantId;
use rsvz::plant::Plant;

fn main() {
    assert!(std::any::type_name::<rsvz_current::CurrentBackend>().ends_with("NoBackend"));

    let id = PlantId::from_raw(7);
    assert_eq!(Plant::from_id(id).id(), id);
    let mut fixer = rsvz::plant_fixer::PlantFixer::new();
    assert!(fixer.set_run_interval(0).is_err());
    fixer.set_list([(1, 1), (2, 2)]);
    assert_eq!(fixer.list().len(), 2);

    rsvz::with_scheduler(|scheduler| {
        let handle = scheduler.spawn(rsvz::tick::TickOptions::any_dispatch(), |_| {
            Ok(rsvz::tick::TickControl::Stop)
        });
        assert_eq!(scheduler.state(handle), rsvz::tick::TickTaskState::Running);
        scheduler.clear_all();
    });
    let _callable_without_invocation = rsvz::cob::fire;
}
