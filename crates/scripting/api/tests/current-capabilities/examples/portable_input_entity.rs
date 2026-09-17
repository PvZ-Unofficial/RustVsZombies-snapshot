use rsvz_backend_api::*;
use rsvz::core::model::{PixelPos, MouseButton};
use rsvz_current::CurrentBackend;

fn borrowed(backend: &mut CurrentBackend) {
    let plant = backend.plants().unwrap().next().unwrap();
    backend.mouse_down(PixelPos { x: 10, y: 10 }, MouseButton::Left).unwrap();
    let _ = backend.plant_hp(plant);
}
fn main() {}
