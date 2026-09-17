use std::cell::Cell;

thread_local! {
    static TRIAL: Cell<usize> = const { Cell::new(0) };
}

fn runtime_error(error: impl std::fmt::Display) -> rsvz::runtime::RuntimeError {
    rsvz::runtime::RuntimeError::new(error.to_string())
}

#[rsvz::state_hooks]
fn hooks() {
    use rsvz::core::model::{I32RepresentableF32, NonNegativeI32, PositiveHp};
    use rsvz::prelude::*;

    on_enter_fight(|| {
        let trial = TRIAL.replace(TRIAL.get() + 1) % 10;
        let result = rsvz::with_backend(|backend| -> rsvz::runtime::RuntimeResult<()> {
            let selection = if trial < 2 {
                CardSelection::Plant(PlantKind::WallNut)
            } else {
                CardSelection::Imitator(PlantKind::IceShroom)
            };
            backend
                .new_plant(
                    selection.checked().expect("valid card"),
                    Grid::new(0, if trial == 8 { 4 } else { 8 }).expect("grid"),
                )
                .map_err(runtime_error)?;
            if trial >= 5 {
                // Hold the unfinished imitator for slow native attack animations.
                // The fifth trial above independently checks normal transformation.
                for plant in backend.plants().map_err(runtime_error)? {
                    backend
                        .set_plant_state_countdown(plant, 10_000)
                        .map_err(runtime_error)?;
                    backend
                        .set_plant_hp(plant, PositiveHp::new(1).expect("hp"))
                        .map_err(runtime_error)?;
                }
            }
            // Fifth trial is the normal morph counterexample and reaches the declared window.
            if trial != 4 {
                let kind = match trial {
                    0 | 2 => ZombieKind::Zomboni,
                    1 | 3 | 8 => ZombieKind::Catapult,
                    5 => ZombieKind::Normal,
                    6 => ZombieKind::JackInTheBox,
                    7 => ZombieKind::Gargantuar,
                    9 => ZombieKind::Bungee,
                    _ => unreachable!(),
                };
                let zombie = backend
                    .add_zombie_in_row(kind, 0, 0)
                    .map_err(runtime_error)?
                    .ok_or_else(|| runtime_error("vehicle creation failed"))?;
                if backend
                    .set_zombie_x(
                        zombie,
                        I32RepresentableF32::new(match trial {
                            8 => 650.0,
                            6 => 750.0,
                            _ => 680.0,
                        })
                        .expect("x"),
                    )
                    .map_err(runtime_error)?
                    != ObjectEditOutcome::Applied
                {
                    return Err(runtime_error("vehicle placement failed"));
                }
                if trial == 6 {
                    backend
                        .set_zombie_phase_countdown(zombie, NonNegativeI32::new(1).expect("countdown"))
                        .map_err(runtime_error)?;
                }
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
    use rsvz::event::{EventLifetime, EventOptions, PlantEffectOutcome};
    use rsvz::prelude::*;
    reload(MainUiOrFightUi);
    skip_seed_chooser();
    rsvz::with_backend(|backend| backend.set_zombie_spawn_stopped(true).map_err(runtime_error))?;
    set_zombies("普");
    select_cards("P");
    rsvz::measure::completed_rounds(63);
    rsvz::measure::protect_only([rsvz::measure::protect::grid(1, 9)]);
    rsvz::measure::imp_leak_detection(false);
    rsvz::event::on_plant_effect(EventOptions::new().lifetime(EventLifetime::Session), |effect| {
        if matches!(
            effect.outcome,
            PlantEffectOutcome::Killed | PlantEffectOutcome::Squished | PlantEffectOutcome::Stolen
        ) {
            let trial = (TRIAL.get() - 1) % 10;
            let expected = match trial {
                0 | 2 => "zomboni_crush",
                1 | 3 => "catapult_crush",
                5 => "zombie_chew",
                6 => "jack_explosion",
                7 => "gargantuar_smash",
                8 => "catapult_basket",
                9 => "bungee_steal",
                _ => "no destructive event",
            };
            if effect.source.as_str() != expected {
                fail_script(runtime_error(format!(
                    "trial {trial}: expected {expected}, got {:?}",
                    effect.source
                )));
            }
        }
    })
    .map_err(runtime_error)?;
    assume_wavelength(1, 2_500);
    rsvz::measure::end_at((1, 3_000));
    rsvz::measure::damage_narrow_trials(10);
    Ok(())
}
