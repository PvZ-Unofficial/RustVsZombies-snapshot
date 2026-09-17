use rsvz::dsl::prelude::*;

// Type-check actual selected backends, including the native backends without
// Refresh measurement capabilities. Running these operations needs a game scope.
fn main() {
    let _: fn(i32, i32, i32, f32) -> Expr = rp;
    let _: fn() -> Expr = auto_cobs;
    let _: fn(&CobManager, i32) -> Expr = auto_cobs_col;
    let _: fn([(i32, i32); 1]) -> Expr = set_ice;
    let _: fn() -> Expr = dancing;
    let _: fn(PlantKind, i32, i32) -> Expr = rm;
    let _: fn(ZombieKind, i32) = ensure_exist;
    let manager = CobManager::new();
    let expression = p(2, 9)
        + pp()
        + p(&manager, [(2, 8.8), (5, 8.8)])
        + recover_p([2, 5], 9)
        + rp(1, 1, 2, 9)
        + d(100)
        + card(PlantKind::FumeShroom, 2, 9)
        + try_card(PlantKind::PuffShroom, [(2, 8), (2, 9)])
        + ice(2, 9)
        + doom(2, 8)
        + shovel(2, 9, PlantKind::Pumpkin)
        + act(|| {})
        + try_act(|| Ok(()));
    let _ = rsvz::__private::run_script(|| {
        waves([1, 2]);
        1000 << &expression;
        (3, 1200) << expression;
        rsvz::at(1, 401, || rsvz::cob::fire(2, 9));
        rsvz::timeline::try_at(1, 500, || {
            rsvz::cob::try_fire(2, 9)?;
            Ok(())
        })?;
        Ok(())
    });
}
