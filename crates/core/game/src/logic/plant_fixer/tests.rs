use super::*;

#[test]
fn configuration_rejects_invalid_thresholds_grids_and_zero_interval() {
    assert_eq!(threshold_to_hp(PlantKind::Pumpkin, 0.5).unwrap(), 2000);
    assert_eq!(threshold_to_hp(PlantKind::Sunflower, 200.0).unwrap(), 200);
    for threshold in [f32::NAN, f32::INFINITY, -1.0] {
        assert!(matches!(
            threshold_to_hp(PlantKind::Sunflower, threshold),
            Err(PlantFixerError::InvalidThreshold(_))
        ));
    }
    assert!(matches!(
        parse_grids([(0, 1)]),
        Err(PlantFixerError::InvalidGrid { .. })
    ));
    let mut fixer = PlantFixer::new();
    assert!(matches!(
        fixer.set_run_interval(0),
        Err(PlantFixerError::InvalidRunInterval(0))
    ));
    assert_eq!(fixer.run_interval, 1);
    fixer.set_run_interval(u32::MAX).unwrap();
    assert_eq!(fixer.run_interval, u32::MAX);
}

#[test]
fn list_operations_preserve_requested_order() {
    let a = Grid { row: 0, col: 0 };
    let b = Grid { row: 0, col: 1 };
    let c = Grid { row: 0, col: 2 };
    let mut fixer = PlantFixer::new();
    fixer.set_list([a, b]);
    fixer.move_to_list_top([c]);
    assert_eq!(fixer.list(), [c, a, b]);
    fixer.move_to_list_bottom([a]);
    assert_eq!(fixer.list(), [c, b, a]);
    fixer.erase_from_list([b]);
    assert_eq!(fixer.list(), [c, a]);
}
