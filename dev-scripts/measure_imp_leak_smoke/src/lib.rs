fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn install_hooks() {
    use rsvz::core::model::{I32RepresentableF32, PositiveHp};
    use rsvz::prelude::{
        CardSelection, Grid, ObjectEditOutcome, PlantCreateBackend as _, PlantKind, ZombieBodyHealthWriteBackend as _,
        ZombieCreateBackend as _, ZombieKind, ZombieXWriteBackend as _, fail_script, on_enter_fight,
    };

    on_enter_fight(|| {
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            for col in 0..5 {
                backend
                    .new_plant(
                        CardSelection::Plant(PlantKind::WallNut)
                            .checked()
                            .expect("WallNut is a valid card selection"),
                        Grid::new(0, col).expect("R1 plant grid is valid"),
                    )
                    .map_err(runtime_error)?;
            }
            let gargantuar = backend
                .add_zombie_in_row(ZombieKind::Gargantuar, 0, 0)
                .map_err(runtime_error)?
                .ok_or_else(|| rsvz::runtime::RuntimeError::new("failed to create Gargantuar"))?;
            if backend
                .set_zombie_x(
                    gargantuar,
                    I32RepresentableF32::new(800.0).expect("800 is exactly representable"),
                )
                .map_err(runtime_error)?
                != ObjectEditOutcome::Applied
            {
                return Err(rsvz::runtime::RuntimeError::new("failed to place Gargantuar"));
            }
            if backend
                .set_zombie_body_hp(gargantuar, PositiveHp::new(1_000).expect("positive HP"))
                .map_err(runtime_error)?
                != ObjectEditOutcome::Applied
            {
                return Err(rsvz::runtime::RuntimeError::new("failed to lower Gargantuar HP"));
            }
            Ok(())
        });
        if let Err(error) = result {
            fail_script(error);
        }
    });
}

#[rsvz::script]
fn script() -> rsvz::runtime::RuntimeResult<()> {
    use rsvz::prelude::*;

    reload(MainUiOrFightUi);
    skip_seed_chooser();
    rsvz::with_backend(|backend| backend.set_zombie_spawn_stopped(true).map_err(runtime_error))?;
    set_zombies("普");
    select_cards("P");
    rsvz::measure::completed_rounds(63);
    rsvz::measure::protect_only((1..=5).map(|col| rsvz::measure::protect::grid(1, col)));
    assume_wavelength(1, 2_500);
    rsvz::measure::end_at((1, 4_000));
    rsvz::measure::damage_narrow_trials(1);
    Ok(())
}
