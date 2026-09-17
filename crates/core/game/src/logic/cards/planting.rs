use super::*;

#[derive(Clone, Copy)]
struct LawnPlant {
    id: PlantId,
    kind: PlantKind,
}

#[derive(Clone, Copy, Default)]
struct PlantsOnLawn {
    under: Option<LawnPlant>,
    pumpkin: Option<LawnPlant>,
    normal: Option<LawnPlant>,
}

#[derive(Clone, Copy, Default)]
struct GloomSleepCarry {
    was_awake: bool,
    wake_up_counter: i32,
    active: bool,
}

#[derive(Clone, Copy, Default)]
struct PlantReplacements {
    ids: [Option<PlantId>; 2],
}

impl PlantReplacements {
    fn push(&mut self, id: PlantId) {
        if self.ids.contains(&Some(id)) {
            return;
        }
        if let Some(slot) = self.ids.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(id);
        }
    }
}

fn plants_on_lawn(grid: Grid) -> Result<PlantsOnLawn, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: PlantReadBackend,
{
    crate::access::with_backend(|backend| {
        let mut result = PlantsOnLawn::default();
        for plant in crate::live_value::read_or_abort(backend.plants(), "plants") {
            if !plant_counts_as_on_lawn(backend, plant)? {
                continue;
            }
            let anchor = crate::plant::grid_from_handle(backend, plant);
            let kind = crate::live_value::read_or_abort(backend.plant_kind(plant), "plant_kind");
            let occupies_grid = anchor == grid
                || (kind == PlantKind::CobCannon
                    && anchor.row == grid.row
                    && anchor.col.checked_add(1) == Some(grid.col));
            if !occupies_grid {
                continue;
            }

            let current = LawnPlant {
                id: backend.plant_id(plant),
                kind,
            };
            match kind {
                PlantKind::CoffeeBean => {}
                PlantKind::LilyPad | PlantKind::FlowerPot => {
                    result.under.get_or_insert(current);
                }
                PlantKind::Pumpkin => {
                    result.pumpkin.get_or_insert(current);
                }
                _ => {
                    result.normal.get_or_insert(current);
                }
            }
        }
        Ok(result)
    })
}

pub(crate) fn plant_counts_as_on_lawn<'a>(
    backend: &'a rsvz_current::CurrentBackend,
    plant: <rsvz_current::CurrentBackend as PlantReadBackend>::PlantHandle<'a>,
) -> Result<bool, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: PlantReadBackend,
{
    let raw_kind = crate::live_value::read_or_abort(backend.plant_raw_kind(plant), "plant_raw_kind");
    let state = backend.plant_state(plant);
    let squash_in_air = raw_kind == PlantKind::Squash && matches!(state, 5..=7);
    Ok(!squash_in_air && !backend.plant_is_squished(plant) && backend.plant_on_bungee_state(plant) != 2)
}

const fn is_upgrade_pair_after_can_plant(base: PlantKind, upgrade: PlantKind) -> bool {
    matches!(
        (base, upgrade),
        (PlantKind::Repeater, PlantKind::GatlingPea)
            | (PlantKind::MelonPult, PlantKind::WinterMelon)
            | (PlantKind::Sunflower, PlantKind::TwinSunflower)
            | (PlantKind::Spikeweed, PlantKind::Spikerock)
            | (PlantKind::KernelPult, PlantKind::CobCannon)
            | (PlantKind::MagnetShroom, PlantKind::GoldMagnet)
            | (PlantKind::FumeShroom, PlantKind::GloomShroom)
            | (PlantKind::LilyPad, PlantKind::Cattail)
    )
}

fn plan_replacements(
    grid: Grid, planting: PlantKind,
) -> Result<(PlantReplacements, GloomSleepCarry), rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: PlantReadBackend,
{
    crate::access::with_backend(|backend| {
        let plants = plants_on_lawn(grid)?;
        let mut replacements = PlantReplacements::default();
        let mut gloom_sleep = GloomSleepCarry::default();

        if let Some(normal) = plants.normal {
            if is_upgrade_pair_after_can_plant(normal.kind, planting) {
                if planting == PlantKind::GloomShroom
                    && let Some(handle) = crate::live_value::read_or_abort(backend.plant(normal.id), "plant")
                {
                    gloom_sleep = GloomSleepCarry {
                        was_awake: !backend.plant_is_sleeping(handle),
                        wake_up_counter: backend.plant_wake_up_counter(handle),
                        active: true,
                    };
                }
                replacements.push(normal.id);
            }
            if matches!(planting, PlantKind::WallNut | PlantKind::TallNut) && normal.kind == planting {
                replacements.push(normal.id);
            }
        }

        if planting == PlantKind::Pumpkin
            && let Some(pumpkin) = plants.pumpkin
            && pumpkin.kind == PlantKind::Pumpkin
        {
            replacements.push(pumpkin.id);
        }

        if planting == PlantKind::CobCannon
            && let Some(right_col) = grid.col.checked_add(1)
            && let Some(right_kernel) = plants_on_lawn(Grid {
                row: grid.row,
                col: right_col,
            })?
            .normal
        {
            replacements.push(right_kernel.id);
        }

        if planting == PlantKind::Cattail {
            if let Some(under) = plants.under {
                replacements.push(under.id);
            }
            if let Some(normal) = plants.normal {
                replacements.push(normal.id);
            }
        }

        Ok((replacements, gloom_sleep))
    })
}

const fn can_use_auto_flower_pot(kind: PlantKind) -> bool {
    match kind {
        PlantKind::Peashooter
        | PlantKind::Sunflower
        | PlantKind::CherryBomb
        | PlantKind::WallNut
        | PlantKind::PotatoMine
        | PlantKind::SnowPea
        | PlantKind::Chomper
        | PlantKind::Repeater
        | PlantKind::PuffShroom
        | PlantKind::SunShroom
        | PlantKind::FumeShroom
        | PlantKind::HypnoShroom
        | PlantKind::ScaredyShroom
        | PlantKind::IceShroom
        | PlantKind::DoomShroom
        | PlantKind::Squash
        | PlantKind::Threepeater
        | PlantKind::Jalapeno
        | PlantKind::Torchwood
        | PlantKind::TallNut
        | PlantKind::Plantern
        | PlantKind::Cactus
        | PlantKind::Blover
        | PlantKind::SplitPea
        | PlantKind::Starfruit
        | PlantKind::Pumpkin
        | PlantKind::MagnetShroom
        | PlantKind::CabbagePult
        | PlantKind::KernelPult
        | PlantKind::Garlic
        | PlantKind::UmbrellaLeaf
        | PlantKind::Marigold
        | PlantKind::MelonPult => true,
        PlantKind::GraveBuster
        | PlantKind::LilyPad
        | PlantKind::TangleKelp
        | PlantKind::Spikeweed
        | PlantKind::SeaShroom
        | PlantKind::FlowerPot
        | PlantKind::CoffeeBean
        | PlantKind::GatlingPea
        | PlantKind::TwinSunflower
        | PlantKind::GloomShroom
        | PlantKind::Cattail
        | PlantKind::WinterMelon
        | PlantKind::GoldMagnet
        | PlantKind::Spikerock
        | PlantKind::CobCannon
        | PlantKind::Imitator => false,
    }
}

const fn can_use_auto_lily_pad(kind: PlantKind) -> bool {
    can_use_auto_flower_pot(kind) && !matches!(kind, PlantKind::PotatoMine)
}

const fn normalize_plant_reject_reason(kind: PlantKind, reason: PlantRejectReason) -> PlantRejectReason {
    match reason {
        PlantRejectReason::GameRule(5) if can_use_auto_flower_pot(kind) => PlantRejectReason::RequiresPot,
        PlantRejectReason::GameRule(11) if can_use_auto_lily_pad(kind) => PlantRejectReason::RequiresLilypad,
        PlantRejectReason::GameRule(8) if matches!(kind, PlantKind::Cattail) => PlantRejectReason::RequiresLilypad,
        reason => reason,
    }
}

/// 组合 backend 原子事实，查询指定 `0-based` 卡槽能否种在 core 格。
pub fn can_plant_seed(slot: SeedSlot, grid: Grid) -> Plantability
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
{
    crate::live_value::read_or_abort(
        crate::access::with_backend(|backend| -> Result<_, rsvz_current::CurrentBackendError> {
            let Some(seed) = seed_for_slot(backend, slot) else {
                return Ok(Plantability::Rejected(PlantRejectReason::GameRule(-1)));
            };
            let selection = crate::live_value::read_or_abort(backend.seed_selection(seed), "seed_selection");
            let planting = selection.effective_kind();
            let native = crate::live_value::read_or_abort(backend.can_plant_at(selection, grid), "can_plant_at");
            match native {
                Plantability::Allowed => Ok(Plantability::Allowed),
                Plantability::Rejected(reason) => {
                    Ok(Plantability::Rejected(normalize_plant_reject_reason(planting, reason)))
                }
            }
        }),
        "failed to can_plant_seed",
    )
}

/// 脚本错误解释之前，一次共享卡槽提交的结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlantSeedOutcome {
    /// 已种下植物并返回其跨帧 ID。
    Planted(PlantId),
    /// 卡槽不存在或当前不可用。
    Unusable,
    /// 游戏规则拒绝该落点。
    Rejected(PlantRejectReason),
}

fn try_plant_seed_recording<F>(
    slot: SeedSlot, grid: Grid, component: PlantingComponent, recorder: &mut F,
) -> Result<PlantSeedOutcome, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
    F: FnMut(PlantingComponent, PlantId),
{
    crate::access::with_backend(|backend| {
        let Some(seed) = seed_for_slot(backend, slot) else {
            return Ok(PlantSeedOutcome::Unusable);
        };
        let selection = crate::live_value::read_or_abort(backend.seed_selection(seed), "seed_selection");
        let planting = selection.effective_kind();

        if !crate::live_value::read_or_abort(backend.seed_can_pick_up(seed), "seed_can_pick_up") {
            return Ok(PlantSeedOutcome::Unusable);
        }
        if let Plantability::Rejected(reason) = can_plant_seed(slot, grid) {
            return Ok(PlantSeedOutcome::Rejected(reason));
        }

        let cost = crate::live_value::read_or_abort(backend.current_plant_cost(selection), "current_plant_cost");
        if !crate::live_value::read_or_abort(backend.can_take_sun_money(cost), "can_take_sun_money") {
            return Ok(PlantSeedOutcome::Rejected(PlantRejectReason::NotEnoughSun));
        }
        if !pool_has_free_slot(
            crate::live_value::read_or_abort(backend.plant_pool_active_count(), "plant_pool_active_count"),
            crate::live_value::read_or_abort(backend.plant_pool_capacity(), "plant_pool_capacity"),
        ) {
            return Ok(PlantSeedOutcome::Rejected(PlantRejectReason::PoolFull));
        }

        let (replacements, gloom_sleep) = plan_replacements(grid, planting)?;
        if !backend.take_sun_money(cost).map_err(crate::access::rejected_error)? {
            return Ok(PlantSeedOutcome::Rejected(PlantRejectReason::NotEnoughSun));
        }

        for id in replacements.ids.into_iter().flatten() {
            if let Some(plant) = crate::live_value::read_or_abort(backend.plant(id), "plant") {
                backend.remove_plant(plant).map_err(crate::access::rejected_error)?;
            }
        }

        let planted = backend
            .add_plant(selection, grid)
            .map_err(crate::access::rejected_error)?;
        let plant_id = backend.plant_id(planted);
        recorder(component, plant_id);
        if planting == PlantKind::GloomShroom && gloom_sleep.active {
            if gloom_sleep.was_awake {
                backend
                    .set_plant_sleeping(planted, false)
                    .map_err(crate::access::rejected_error)?;
            } else {
                backend
                    .set_plant_wake_up_counter(planted, gloom_sleep.wake_up_counter)
                    .map_err(crate::access::rejected_error)?;
            }
        }

        crate::live_value::read_or_abort(backend.seed_was_planted(seed), "seed_was_planted");
        Ok(PlantSeedOutcome::Planted(plant_id))
    })
}

/// 使用明确的 `0-based` 卡槽与 core 格完成一次共享语义种植。
///
/// 本函数不会自动补种荷叶或花盆；它组合所有 backend 的共同原子能力，并把
/// 普通拒绝保留为 [`PlantSeedOutcome`]。
pub fn try_plant_seed(slot: SeedSlot, grid: Grid) -> PlantSeedOutcome
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
{
    crate::live_value::read_or_abort(
        try_plant_seed_recording(slot, grid, PlantingComponent::Main, &mut |_component, _id| {}),
        "failed to try_plant_seed",
    )
}

/// 使用明确卡槽种植并把普通拒绝提升为详细 [`CardLogicError`]。
pub fn plant_seed(slot: SeedSlot, grid: Grid) -> Result<PlantId, CardLogicError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
{
    match try_plant_seed(slot, grid) {
        PlantSeedOutcome::Planted(id) => Ok(id),
        PlantSeedOutcome::Unusable => Err(CoreLogicError::NoUsableSeed.into()),
        PlantSeedOutcome::Rejected(reason) => Err(CoreLogicError::PlantRejected(reason).into()),
    }
}

enum CardPlantingPlan {
    Direct,
    WithContainer {
        slot: SeedSlot,
    },
    Unavailable {
        component: PlantingComponent,
        error: CoreLogicError,
    },
}

fn plan_card_planting(slot: SeedSlot, grid: Grid) -> Result<CardPlantingPlan, rsvz_current::CurrentBackendError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
{
    let container_kind = match can_plant_seed(slot, grid) {
        Plantability::Allowed => return Ok(CardPlantingPlan::Direct),
        Plantability::Rejected(PlantRejectReason::RequiresPot) => PlantKind::FlowerPot,
        Plantability::Rejected(PlantRejectReason::RequiresLilypad) => PlantKind::LilyPad,
        Plantability::Rejected(reason) => {
            return Ok(CardPlantingPlan::Unavailable {
                component: PlantingComponent::Main,
                error: CoreLogicError::PlantRejected(reason),
            });
        }
    };
    let selection = CardSelection::Plant(container_kind);
    let Some((container_slot, usable)) = find_seed_for_selection(selection) else {
        return Ok(CardPlantingPlan::Unavailable {
            component: PlantingComponent::AutoContainer,
            error: CoreLogicError::SeedSlotNotFound(selection),
        });
    };
    if !usable {
        return Ok(CardPlantingPlan::Unavailable {
            component: PlantingComponent::AutoContainer,
            error: CoreLogicError::NoUsableSeed,
        });
    }
    if let Plantability::Rejected(reason) = can_plant_seed(container_slot, grid) {
        return Ok(CardPlantingPlan::Unavailable {
            component: PlantingComponent::AutoContainer,
            error: CoreLogicError::PlantRejected(reason),
        });
    }
    Ok(CardPlantingPlan::WithContainer { slot: container_slot })
}

fn commit_component<F>(
    slot: SeedSlot, grid: Grid, component: PlantingComponent, recorder: &mut F,
) -> Result<PlantId, CardLogicError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
    F: FnMut(PlantingComponent, PlantId),
{
    match try_plant_seed_recording(slot, grid, component, recorder).map_err(|error| {
        CardLogicError::PlantingBackend {
            component,
            error: RuntimeError::new(error.to_string()),
        }
    })? {
        PlantSeedOutcome::Planted(id) => Ok(id),
        PlantSeedOutcome::Unusable => Err(CardLogicError::PlantingCore {
            component,
            error: CoreLogicError::NoUsableSeed,
        }),
        PlantSeedOutcome::Rejected(reason) => Err(CardLogicError::PlantingCore {
            component,
            error: CoreLogicError::PlantRejected(reason),
        }),
    }
}

fn commit_card_planting<F>(
    selection: CardSelection, slot: SeedSlot, grid: Grid, plan: CardPlantingPlan, recorder: &mut F,
) -> Result<PlantingReceipt, CardLogicError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
    F: FnMut(PlantingComponent, PlantId),
{
    let (main, auto_container) = match plan {
        CardPlantingPlan::Direct => {
            let main = match try_plant_seed_recording(slot, grid, PlantingComponent::Main, recorder)
                .unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()))
            {
                PlantSeedOutcome::Planted(id) => id,
                PlantSeedOutcome::Unusable => return Err(CoreLogicError::NoUsableSeed.into()),
                PlantSeedOutcome::Rejected(reason) => {
                    return Err(CoreLogicError::PlantRejected(reason).into());
                }
            };
            (main, None)
        }
        CardPlantingPlan::WithContainer { slot: container_slot } => {
            let container = commit_component(container_slot, grid, PlantingComponent::AutoContainer, recorder)?;
            if let Plantability::Rejected(reason) = can_plant_seed(slot, grid) {
                return Err(CardLogicError::PlantingCore {
                    component: PlantingComponent::Main,
                    error: CoreLogicError::PlantRejected(reason),
                });
            }
            let main = commit_component(slot, grid, PlantingComponent::Main, recorder)?;
            (main, Some(container))
        }
        CardPlantingPlan::Unavailable { component, error } => {
            return Err(if component == PlantingComponent::Main {
                CardLogicError::Core(error)
            } else {
                CardLogicError::PlantingCore { component, error }
            });
        }
    };
    Ok(PlantingReceipt {
        selection,
        slot,
        grid,
        main,
        auto_container,
    })
}

pub(super) fn card_detailed_recording<F>(
    selection: CardSelection, row: i32, col: i32, mut recorder: F,
) -> Result<PlantingReceipt, CardLogicError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
    F: FnMut(PlantingComponent, PlantId),
{
    let grid = Grid::from_one_based(row, col).map_err(|_error| CoreLogicError::InvalidGrid)?;
    let slot = require_usable_seed(selection)?;
    let plan = plan_card_planting(slot, grid).unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
    commit_card_planting(selection, slot, grid, plan, &mut recorder)
}

pub(super) fn card_any_prepared_detailed(selection: CardSelection, grids: &[Grid]) -> Result<PlantId, CardLogicError>
where
    rsvz_current::CurrentBackend: CardPlantingBackend,
{
    let slot = require_usable_seed(selection)?;

    for grid in grids.iter().copied() {
        let plan =
            plan_card_planting(slot, grid).unwrap_or_else(|error| crate::diagnostics::abort_operation(error.into()));
        if matches!(&plan, CardPlantingPlan::Unavailable { .. }) {
            continue;
        }
        let receipt = commit_card_planting(selection, slot, grid, plan, &mut |_component, _id| {})?;
        return Ok(receipt.main);
    }

    Err(CoreLogicError::NoPlantableGrid(selection).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_container_reasons_use_an_exhaustive_plant_kind_table() {
        const ALL: [PlantKind; 49] = [
            PlantKind::Peashooter,
            PlantKind::Sunflower,
            PlantKind::CherryBomb,
            PlantKind::WallNut,
            PlantKind::PotatoMine,
            PlantKind::SnowPea,
            PlantKind::Chomper,
            PlantKind::Repeater,
            PlantKind::PuffShroom,
            PlantKind::SunShroom,
            PlantKind::FumeShroom,
            PlantKind::GraveBuster,
            PlantKind::HypnoShroom,
            PlantKind::ScaredyShroom,
            PlantKind::IceShroom,
            PlantKind::DoomShroom,
            PlantKind::LilyPad,
            PlantKind::Squash,
            PlantKind::Threepeater,
            PlantKind::TangleKelp,
            PlantKind::Jalapeno,
            PlantKind::Spikeweed,
            PlantKind::Torchwood,
            PlantKind::TallNut,
            PlantKind::SeaShroom,
            PlantKind::Plantern,
            PlantKind::Cactus,
            PlantKind::Blover,
            PlantKind::SplitPea,
            PlantKind::Starfruit,
            PlantKind::Pumpkin,
            PlantKind::MagnetShroom,
            PlantKind::CabbagePult,
            PlantKind::FlowerPot,
            PlantKind::KernelPult,
            PlantKind::CoffeeBean,
            PlantKind::Garlic,
            PlantKind::UmbrellaLeaf,
            PlantKind::Marigold,
            PlantKind::MelonPult,
            PlantKind::GatlingPea,
            PlantKind::TwinSunflower,
            PlantKind::GloomShroom,
            PlantKind::Cattail,
            PlantKind::WinterMelon,
            PlantKind::GoldMagnet,
            PlantKind::Spikerock,
            PlantKind::CobCannon,
            PlantKind::Imitator,
        ];

        for (raw, kind) in ALL.into_iter().enumerate() {
            assert_eq!(kind as usize, raw);
            let _pot = can_use_auto_flower_pot(kind);
            let _lily = can_use_auto_lily_pad(kind);
        }
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::Peashooter, PlantRejectReason::GameRule(5)),
            PlantRejectReason::RequiresPot
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::Peashooter, PlantRejectReason::GameRule(11)),
            PlantRejectReason::RequiresLilypad
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::PotatoMine, PlantRejectReason::GameRule(5)),
            PlantRejectReason::RequiresPot
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::FlowerPot, PlantRejectReason::GameRule(5)),
            PlantRejectReason::GameRule(5)
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::FlowerPot, PlantRejectReason::GameRule(11)),
            PlantRejectReason::GameRule(11)
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::PotatoMine, PlantRejectReason::GameRule(11)),
            PlantRejectReason::GameRule(11)
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::GatlingPea, PlantRejectReason::GameRule(11)),
            PlantRejectReason::GameRule(11)
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::Cattail, PlantRejectReason::GameRule(8)),
            PlantRejectReason::RequiresLilypad
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::Peashooter, PlantRejectReason::GameRule(1)),
            PlantRejectReason::GameRule(1)
        );
        assert_eq!(
            normalize_plant_reject_reason(PlantKind::Peashooter, PlantRejectReason::GameRule(8)),
            PlantRejectReason::GameRule(8)
        );
    }
}
