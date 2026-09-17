use crate::backend::{CobFireBackend, GridGeometryBackend, PlantReadBackend};
use crate::model::{CobTarget, PixelPos};
use crate::model::{Grid, PlantId, PlantKind};
use rsvz_current::CurrentBackend;

/// 查找锚点为 `grid` 的第一株存活 `kind` 植物。
pub fn find_plant_at_kind(grid: Grid, kind: PlantKind) -> Option<PlantId>
where
    CurrentBackend: PlantReadBackend,
{
    crate::live_value::read_or_abort(
        crate::access::with_backend(|backend| -> Result<_, rsvz_current::CurrentBackendError> {
            let mut found = None;
            backend.for_each_plant_at_anchor_grid(grid, |plant| {
                if crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") == kind && found.is_none()
                {
                    found = Some(backend.plant_id(plant));
                }
                Ok(())
            })?;
            Ok(found)
        }),
        "failed to read find_plant_at_kind",
    )
}

/// 查找锚点为 `grid` 的存活玉米加农炮。
pub fn find_cob_at(grid: Grid) -> Option<PlantId>
where
    CurrentBackend: PlantReadBackend,
{
    find_plant_at_kind(grid, PlantKind::CobCannon)
}

fn cob_recover_time_handle<'a>(
    backend: &'a CurrentBackend, plant: <CurrentBackend as PlantReadBackend>::PlantHandle<'a>,
) -> i32
where
    CurrentBackend: PlantReadBackend,
{
    match backend.plant_state(plant) {
        35 => 125 + backend.plant_state_countdown(plant),
        36 => crate::live_value::read_or_abort(backend.plant_reanim_circulation(plant), "plant_reanim_circulation")
            .map_or(-1, |progress| (125.0 * (1.0 - progress) + 0.5) as i32 + 1),
        37 => 0,
        38 => crate::live_value::read_or_abort(backend.plant_reanim_circulation(plant), "plant_reanim_circulation")
            .map_or(-1, |progress| 3125 + (350.0 * (1.0 - progress) + 0.5) as i32),
        _ => -1,
    }
}

/// 根据 P01/P04/P07 借用事实计算炮恢复时间。
pub fn cob_recover_time(cob: PlantId) -> i32
where
    CurrentBackend: PlantReadBackend,
{
    crate::access::with_backend(|backend| {
        let Some(plant) = crate::live_value::read_or_abort(backend.plant(cob), "plant") else {
            return -1;
        };
        if crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") != PlantKind::CobCannon {
            return -1;
        }
        cob_recover_time_handle(backend, plant)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CobFireAttempt {
    Fired,
    NotReady,
    Unavailable,
}

pub(super) fn fire_cob_attempt(
    cob: PlantId, target: PixelPos,
) -> Result<CobFireAttempt, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: CobFireBackend,
{
    crate::access::with_backend(|backend| {
        let Some(plant) = crate::live_value::read_or_abort(backend.plant(cob), "plant") else {
            return Ok(CobFireAttempt::Unavailable);
        };
        if crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind") != PlantKind::CobCannon {
            return Ok(CobFireAttempt::Unavailable);
        }
        match cob_recover_time_handle(backend, plant) {
            ..=-1 => Ok(CobFireAttempt::Unavailable),
            0 => {
                backend.fire_cob(plant, target).map_err(crate::access::rejected_error)?;
                Ok(CobFireAttempt::Fired)
            }
            1.. => Ok(CobFireAttempt::NotReady),
        }
    })
}

/// 把 `0-based` 行与脚本浮点落点列转换为 backend 发炮原语使用的、已夹紧
/// PvZ 像素落点。
pub fn cob_target_to_pixel(target: CobTarget) -> Result<PixelPos, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: GridGeometryBackend,
{
    crate::access::with_backend(|backend| {
        // AvZ permits a right-edge pseudo-column 10, but PvZ's safe grid geometry
        // only has columns 1..=9. Both columns share the same vertical baseline.
        let target_col = ((target.drop_col + 0.5) as i32).clamp(1, 9);
        let x = ((target.drop_col * 80.0 + 1.0e-3) as i32).clamp(0, 799);
        let y =
            (crate::live_value::read_or_abort(backend.grid_to_pixel_y(target_col - 1, target.row), "grid_to_pixel_y")
                + 40)
                .clamp(0, 599);
        Ok(PixelPos { x, y })
    })
}

/// 当 `cob` 存活、确为玉米加农炮且已经恢复时向 `target` 发炮。
///
/// 植物不存在、类型不对或尚不可用时返回 `false`；只有 backend 故障使用
/// `Err`。
pub fn fire_cob(cob: PlantId, target: PixelPos) -> Result<bool, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: CobFireBackend,
{
    fire_cob_attempt(cob, target).map(|attempt| attempt == CobFireAttempt::Fired)
}
