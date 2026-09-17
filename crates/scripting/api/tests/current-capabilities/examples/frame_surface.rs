use rsvz::prelude::*;
use rsvz::tick::{TickControl, TickOptions};

fn main() {
    at_frame(1, 401, |frame| {
        for zombie in frame.zombies() {
            let _ = (zombie.id(), zombie.kind(), zombie.row(), zombie.age());
            let _ = (zombie.motion_state(), zombie.stable_x_at(8), zombie.stable_x_trace(8));
            if zombie.hp() < 100 {
                zombie.remove();
                fire(2, 9);
                let _ = zombie.is_alive();
            }
        }
        frame.plants().map(|plant| plant.id()).collect::<Vec<_>>()
    });
    let _ = try_at_frame(1, 402, |frame| frame.plants().next().map(|plant| plant.hp()));
    tick::on_frame(|frame| {
        for plant in frame.plants() {
            let _ = (plant.kind(), plant.grid(), plant.is_alive());
            if plant.hp() < 10 {
                plant.remove();
            }
        }
    });
    let _ = tick::spawn_frame(TickOptions::active_phase(), |_| -> RuntimeResult<TickControl> {
        Ok(TickControl::Pause)
    });
}
