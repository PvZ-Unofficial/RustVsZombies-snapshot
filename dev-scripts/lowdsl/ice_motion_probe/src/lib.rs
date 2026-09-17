use std::fmt::Display;

use rsvz::core::runtime::{RuntimeError, RuntimeResult};
use rsvz::prelude::{clear_plants, kill_all_zombies};

const ICE_EFFECT_DELAY: i32 = 100;

#[rsvz::script]
fn script() {
    reload(MainUiOrFightUi);
    rsvz::setup::set_game_speed(1.0);
    set_zombies("普");
    select_cards("IIKAWPCCCC");

    rsvz::try_at(1, -590, || {
        rsvz::__private::with_board_access(|_access| {
            clear_plants().map_err(runtime_error)?;
            kill_all_zombies().map_err(runtime_error)?;
            rsvz::core::logic::cards::new_plant(
                rsvz::prelude::PlantKind::IceShroom,
                rsvz::prelude::Grid::from_one_based(2, 2).map_err(runtime_error)?,
            )
            .map_err(runtime_error)?;
            let target_countdown = ICE_EFFECT_DELAY
                .checked_sub(10)
                .ok_or_else(|| RuntimeError::new("ice effect delay must be at least 10 frames"))?;
            rsvz::core::modifier::normalize_first_effect_countdown(
                rsvz::prelude::PlantKind::IceShroom,
                target_countdown,
            )
            .map_err(runtime_error)?;
            let zombie = rsvz::core::modifier::spawn_zombie(
                rsvz::prelude::ZombieKind::Normal,
                rsvz::prelude::Grid { row: 0, col: 7 },
            )
            .map_err(runtime_error)?;
            let motion = rsvz::core::logic::zombie_motion_state(zombie);
            ensure(motion.is_some(), "normal zombie motion state missing")?;
            clear_plants().map_err(runtime_error)?;
            kill_all_zombies().map(|_removed| ()).map_err(runtime_error)
        })
        .and_then(|result| result)
    })?;
}

fn ensure(condition: bool, message: &'static str) -> RuntimeResult<()> {
    if condition {
        Ok(())
    } else {
        Err(RuntimeError::new(message))
    }
}

fn runtime_error(error: impl Display) -> RuntimeError {
    RuntimeError::new(error.to_string())
}
