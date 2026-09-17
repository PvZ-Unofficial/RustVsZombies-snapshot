#[rsvz::script]
fn script() {
    use rsvz::dsl::prelude::*;
    use rsvz::prelude::SeedRuleEditBackend as _;

    reload(MainUiOrFightUi);
    skip_seed_chooser();
    lineup("LI43bJyUlFSWWCROF5C19FdMd1TyXFdFslRUemVaRVY=");
    set_zombies("红白橄丑梯普障气车跳", Average);
    select_cards([
        CardSelection::Plant(PlantKind::IceShroom),
        CardSelection::Imitator(PlantKind::IceShroom),
        CardSelection::Plant(PlantKind::CoffeeBean),
        CardSelection::Plant(PlantKind::Peashooter),
        CardSelection::Plant(PlantKind::Sunflower),
        CardSelection::Plant(PlantKind::Repeater),
        CardSelection::Plant(PlantKind::CabbagePult),
        CardSelection::Plant(PlantKind::LilyPad),
    ]);
    set_sun_cost_ignored(true);
    rsvz::with_backend(|backend| backend.set_seed_recharge_ignored(true))
        .map_err(|error| rsvz::runtime::RuntimeError::new(error.to_string()))?;
    rsvz::prime_runtime_total_waves(20);
    rsvz::with_timeline(|timeline| {
        timeline.set_wavelengths([(rsvz::prelude::Wave(1), 1150), (rsvz::prelude::Wave(2), 1672)])
    })
    .map_err(|error| rsvz::runtime::RuntimeError::new(error.to_string()))?;

    rsvz::measure::completed_rounds(126);
    rsvz::measure::protect_only([
        rsvz::measure::protect::grid(1, 8),
        rsvz::measure::protect::grid(2, 8),
        rsvz::measure::protect::grid(5, 8),
        rsvz::measure::protect::grid(6, 8),
    ]);

    (1, -599) << auto_cobs() + card(PlantKind::LilyPad, 3, 5);
    (1, -598) << set_ice([(3, 5)]);

    wave(1);
    100 << ci(3, 5);
    400 << p([(1, 7.525), (5, 7.525)]);
    950 << pp(9.0);
    1170 << p([(1, 7.7), (5, 7.7)]);

    wave(2);
    12 << ci(3, 5);
    440 << p([(1, 7.4), (5, 7.4)]);
    1472 << pp(9.0);

    let fixed_f = std::env::var("SMART_FODDER_F")
        .ok()
        .and_then(|value| value.parse::<i32>().ok());
    let call_at = std::env::var("SMART_FODDER_CALL_AT")
        .ok()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(613);
    let trace = std::env::var_os("SMART_FODDER_TRACE").is_some();
    call_at
        << try_act(move || {
            let window = fixed_f.map_or(call_at.saturating_add(1).max(660)..=1290, |f| f..=f);
            let started = std::time::Instant::now();
            let mut selected = [0; 4];
            let mut deadlines: [Option<i32>; 4] = [Option::None; 4];
            let mut expected_damage = [0.0; 4];
            for (index, (row, kind)) in [
                (1, PlantKind::Peashooter),
                (2, PlantKind::Sunflower),
                (5, PlantKind::Repeater),
                (6, PlantKind::CabbagePult),
            ]
            .into_iter()
            .enumerate()
            {
                let remove_by = rsvz::smart_fodder::predict_c9_remove_by(row)?.filter(|deadline| *deadline <= 1472);
                let spec = rsvz::smart_fodder::SmartFodderSpec {
                    card: CardSelection::Plant(kind),
                    row,
                    plant_window: window.clone(),
                    remove_by,
                    activation_at: 1472,
                };
                let result = rsvz::smart_fodder::try_smart_fodder(spec)?;
                selected[index] = result.plant_at;
                deadlines[index] = remove_by;
                expected_damage[index] = result.expected_damage;
            }
            if trace {
                println!(
                    "smart_fodder_choice call_at={call_at} remove_by={deadlines:?} plant_at={selected:?} expected_damage={expected_damage:?} elapsed_ns={}",
                    started.elapsed().as_nanos()
                );
            }
            Ok(())
        });

    if std::env::var_os("SMART_FODDER_DIAGNOSTIC").is_some() {
        (2, 1473)
            << try_act(|| {
                rsvz::stop_script();
                Ok(())
            });
        return Ok(());
    }

    rsvz::measure::end_at((2, 1472));
    let trials = std::env::var("SMART_FODDER_TRIALS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(50_000);
    rsvz::measure::damage_narrow_trials(trials);
}
