#![cfg(feature = "pvz-emulator")]
use rsvz::dsl::{auto_cobs, p};
use rsvz_backend_api::{PlantCreateBackend, PlantReadBackend, PlantStateWriteBackend};
use rsvz_model::{CardSelection, Grid, PlantKind, Wave, WaveTimingSnapshot};
use rsvz_pvz_emulator_backend::{PeWorldConfig, runner_internal::PeWorldOwner};
use rsvz_schedule::tick::{TickMeta, TickPhase};

#[test]
fn auto_cobs_runs_before_p_at_the_same_actual_due_time() {
    rsvz::reset_runtime_state_preserving_backend();
    rsvz::cob::__current_core().set_list_unchecked(std::iter::empty::<Grid>());
    let mut world = PeWorldOwner::new_reset(PeWorldConfig::default())
        .unwrap()
        .install_current()
        .unwrap();
    world.with_backend(|backend| {
        rsvz_pvz_emulator_backend::scope_backend(backend, || {
            let id = rsvz::with_backend(|backend| {
                let plant = backend
                    .add_plant(
                        CardSelection::Plant(PlantKind::CobCannon).checked().unwrap(),
                        Grid { row: 0, col: 0 },
                    )
                    .unwrap();
                backend.set_plant_state(plant, 37).unwrap();
                backend.plant_id(plant)
            });
            rsvz::__run_script(|| {
                (1, 0) << auto_cobs();
                (1, 373) << p(1, 8.0);
                Ok(())
            })
            .unwrap();
            let result = rsvz_game::timeline::dispatch_timeline_tick(
                WaveTimingSnapshot::minimal(100, Wave(1)),
                TickMeta {
                    clock: Some(100),
                    phase: TickPhase::Playing,
                    game_ui: Some(rsvz_model::GameUi::Playing),
                    is_new_frame: true,
                },
            );
            assert_eq!(result, rsvz_schedule::TimelineDispatchResult::Continue);
            assert_eq!(
                rsvz_current::with_backend_shared(|access| {
                    let backend = access;
                    backend.plant_state(backend.plant(id).unwrap().unwrap())
                })
                .unwrap(),
                38
            );
        })
    });
}
