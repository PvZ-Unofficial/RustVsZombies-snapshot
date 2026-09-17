//! Registration-time preparation of cannon impact timing.
use crate::logic::cob::{
    self as logic, CLASSIC_POOL_LAND_COB_LEAD, CLASSIC_POOL_WATER_COB_LEAD, CLASSIC_ROOF_COB_REFERENCE_TIME,
    CobBackend, CobFireDrop, CobManager,
};
use crate::runtime::{RuntimeError, RuntimeResult};
use rsvz_backend_api::{ClockBackend, SceneBackend};
use rsvz_current::CurrentBackend;
use rsvz_model::CobTarget;
use std::rc::Rc;

type ImpactCallback = Box<dyn FnMut() -> RuntimeResult<()> + 'static>;

/// Emits command leads while preserving execution-time scene selection.
pub fn prepare_raw(manager: CobManager, drop: CobFireDrop, mut push: impl FnMut(i32, ImpactCallback))
where
    CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
{
    let flat_manager = manager.clone();
    let flat_drop = drop;
    push(
        CLASSIC_POOL_LAND_COB_LEAD,
        Box::new(move || {
            let scene = crate::access::with_backend(|backend| backend.scene()).map_err(RuntimeError::from)?;
            let water_row = matches!(flat_drop.target.row + 1, 3 | 4);
            if !scene.has_roof() && !(scene.has_pool() && water_row) {
                logic::raw_fire(&flat_manager, flat_drop)
            } else {
                Ok(())
            }
        }),
    );

    let pool_manager = manager.clone();
    let pool_drop = drop;
    push(
        CLASSIC_POOL_WATER_COB_LEAD,
        Box::new(move || {
            let scene = crate::access::with_backend(|backend| backend.scene()).map_err(RuntimeError::from)?;
            if !scene.has_roof() && scene.has_pool() && matches!(pool_drop.target.row + 1, 3 | 4) {
                logic::raw_fire(&pool_manager, pool_drop)
            } else {
                Ok(())
            }
        }),
    );

    let roof_drop = drop;
    push(
        CLASSIC_ROOF_COB_REFERENCE_TIME,
        Box::new(move || {
            let scene = crate::access::with_backend(|backend| backend.scene()).map_err(RuntimeError::from)?;
            if scene.has_roof() {
                logic::raw_fire(&manager, roof_drop)
            } else {
                Ok(())
            }
        }),
    );
}

/// Partitions targets once during registration, retaining original order in each lane.
pub fn prepare(manager: CobManager, targets: &Rc<[CobTarget]>, recover: bool, mut push: impl FnMut(i32, ImpactCallback))
where
    CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
{
    let all_targets = Rc::clone(targets);
    let (pool_land_targets, pool_water_targets): (Vec<_>, Vec<_>) = targets
        .iter()
        .copied()
        .partition(|target| !matches!(target.row + 1, 3 | 4));
    let pool_land_targets: Rc<[CobTarget]> = Rc::from(pool_land_targets.into_boxed_slice());
    let pool_water_targets: Rc<[CobTarget]> = Rc::from(pool_water_targets.into_boxed_slice());

    let flat_manager = manager.clone();
    let flat_all = Rc::clone(&all_targets);
    let flat_land = Rc::clone(&pool_land_targets);
    push(
        CLASSIC_POOL_LAND_COB_LEAD,
        Box::new(move || {
            let scene = crate::access::with_backend(|backend| backend.scene()).map_err(RuntimeError::from)?;
            if scene.has_roof() {
                return Ok(());
            }
            let targets = if scene.has_pool() {
                flat_land.as_ref()
            } else {
                flat_all.as_ref()
            };
            fire_cobs(&flat_manager, targets, recover)
        }),
    );

    if !pool_water_targets.is_empty() {
        let pool_manager = manager.clone();
        push(
            CLASSIC_POOL_WATER_COB_LEAD,
            Box::new(move || {
                let scene = crate::access::with_backend(|backend| backend.scene()).map_err(RuntimeError::from)?;
                if scene.has_pool() && !scene.has_roof() {
                    fire_cobs(&pool_manager, &pool_water_targets, recover)
                } else {
                    Ok(())
                }
            }),
        );
    }

    push(
        CLASSIC_ROOF_COB_REFERENCE_TIME,
        Box::new(move || {
            let scene = crate::access::with_backend(|backend| backend.scene()).map_err(RuntimeError::from)?;
            if scene.has_roof() {
                fire_cobs(&manager, &all_targets, recover)
            } else {
                Ok(())
            }
        }),
    );
}

fn fire_cobs(manager: &CobManager, targets: &[CobTarget], recover: bool) -> RuntimeResult<()>
where
    rsvz_current::CurrentBackend: crate::logic::cob::CobBackend
        + rsvz_backend_api::ClockBackend
        + rsvz_backend_api::SceneBackend
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
{
    if targets.is_empty() {
        return Ok(());
    }
    if recover {
        logic::recover_fire_prepared(manager, targets)
    } else {
        logic::fire_prepared(manager, targets)
    }
}

/// Prepares the conventional pair, selecting rows from the execution-time scene.
pub fn prepare_default_pair(manager: CobManager, col: f32, mut push: impl FnMut(i32, ImpactCallback))
where
    CurrentBackend: ClockBackend
        + CobBackend
        + SceneBackend
        + 'static
        + rsvz_backend_api::BoardReadinessBackend
        + rsvz_backend_api::GameUiBackend,
{
    let flat_manager = manager.clone();
    let flat_time = CLASSIC_POOL_LAND_COB_LEAD;
    push(
        flat_time,
        Box::new(move || {
            let scene = crate::access::with_backend(|backend| backend.scene()).map_err(RuntimeError::from)?;
            if scene.has_roof() {
                return Ok(());
            }
            let second_row = if scene.has_pool() { 5 } else { 4 };
            let targets = [
                CobTarget::from_one_based_row(2, col)
                    .map_err(|_error| RuntimeError::new("invalid default PP target"))?,
                CobTarget::from_one_based_row(second_row, col)
                    .map_err(|_error| RuntimeError::new("invalid default PP target"))?,
            ];
            fire_cobs(&flat_manager, &targets, false)
        }),
    );

    let roof_time = CLASSIC_ROOF_COB_REFERENCE_TIME;
    push(
        roof_time,
        Box::new(move || {
            let scene = crate::access::with_backend(|backend| backend.scene()).map_err(RuntimeError::from)?;
            if !scene.has_roof() {
                return Ok(());
            }
            let targets = [
                CobTarget::from_one_based_row(2, col)
                    .map_err(|_error| RuntimeError::new("invalid default PP target"))?,
                CobTarget::from_one_based_row(4, col)
                    .map_err(|_error| RuntimeError::new("invalid default PP target"))?,
            ];
            fire_cobs(&manager, &targets, false)
        }),
    );
}
