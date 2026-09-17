fn main() {
    let plant = rsvz::plant::Plant::from_id(rsvz::core::model::PlantId::from_raw(7));
    let _ = rsvz::plant::Plant::is_alive(&plant);
}
