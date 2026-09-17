use super::*;
use rsvz_model::Grid;

const fn grid(row: i32, col: i32) -> Grid {
    Grid { row, col }
}
fn list(manager: &CobManager) -> Vec<Grid> {
    let mut grids = Vec::new();
    manager.for_each_grid(|grid| grids.push(grid));
    grids
}

#[test]
fn set_list_and_move_operations_preserve_requested_ordering() {
    let a = grid(1, 1);
    let b = grid(2, 2);
    let manager = CobManager::with_mode(CobSequentialMode::Priority);
    manager.set_list_unchecked([a, b, a]);
    assert_eq!(list(&manager), vec![a, b, a]);

    manager.move_to_list_top(&[b, a]).expect("priority move top");
    assert_eq!(list(&manager), vec![b, a]);

    manager.move_to_list_bottom(&[a, a]).expect("priority move bottom");
    assert_eq!(list(&manager), vec![b, a, a]);

    manager.erase_from_list(&[a]);
    assert_eq!(list(&manager), vec![b]);

    let time = CobManager::with_mode(CobSequentialMode::Time);
    time.set_list_unchecked([a, b]);
    assert_eq!(
        time.move_to_list_top(&[b]),
        Err(CobManagerError::InvalidModeForOperation)
    );
    assert_eq!(
        time.move_to_list_bottom(&[b]),
        Err(CobManagerError::InvalidModeForOperation)
    );
}

#[test]
fn roof_fire_delay_matches_avz_calibration_table() {
    assert_eq!(timing::classic_roof_fire_delay(2, 9.0), Ok(25));
    assert_eq!(timing::classic_roof_cob_fly_time(1, 9.0), Ok(359));
    assert_eq!(timing::classic_roof_cob_fly_time(8, 9.0), Ok(373));
    assert_eq!(timing::classic_roof_cob_fly_time(1, 1.0), Ok(373));
    assert_eq!(
        timing::classic_roof_cob_fly_time(0, 9.0),
        Err(CobManagerError::InvalidTarget)
    );
    assert_eq!(
        timing::classic_roof_cob_fly_time(i32::MIN, 9.0),
        Err(CobManagerError::InvalidTarget)
    );
}
