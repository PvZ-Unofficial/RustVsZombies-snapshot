use rsvz_backend_api::*;
use rsvz::core::model::PixelPos;
use rsvz_current::CurrentBackend;

// Frame stores this same shared backend borrow; it cannot supply mutable input.
fn shared(backend: &CurrentBackend) {
    backend.mouse_move(PixelPos { x: 10, y: 10 }).unwrap();
}
fn main() {}
