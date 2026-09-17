//! 即时语义铲除 API。
//!
//! [`Shovel`] (`shovel`) 和 [`TryShovel`] (`try_shovel`) 在被调用时立即执行；
//! low DSL 的 [`crate::dsl::DslShovel`] (`dsl::shovel`) 则只构造待连接的表达式。脚本 `(row, col)` 从
//! `1` 开始。操作直接通过 backend 删除目标植物，不移动鼠标、不点击铲子，
//! 因而不会干扰用户的鼠标操作。
//!
//! 省略目标或传入 `false` 时，使用与普通中心铲除对应的优先级：主体植物、
//! 南瓜、荷叶/花盆、咖啡豆。传入 `true` 只铲南瓜；传入
//! [`PlantKind`](rsvz_model::model::PlantKind) 或
//! [`CardSelection`](rsvz_model::model::CardSelection) 只铲精确目标，并可
//! 区分普通植物与模仿者。目标不存在时是成功的空操作，不会退而铲除其他层。
//!
//! 铲除香蒲且仍有南瓜保护时会补回荷叶，以保留原生铲子路径的可见结果。
//!
//! 批量操作按输入顺序执行。无效坐标会被汇总，之后的有效操作仍会执行；
//! backend 故障会停止后续 backend 访问，此前完成的铲除不会回滚。

use crate::runtime::RuntimeResult;
use rsvz_game::logic::shovel::{IntoShovelOp, IntoShovelTarget, ShovelContext};

crate::callable::callable_api! {
    #[doc(alias = "AShovel")]
    /// 立即尝试铲除一个或多个格子的目标植物，不自动报告错误。
    ///
    /// # 调用形式
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::prelude::*;
    /// # use rsvz::core::runtime::RuntimeResult;
    /// let ordinary: RuntimeResult<()> = try_shovel(4, 6);
    /// let pumpkin: RuntimeResult<()> = try_shovel(4, 6, true);
    /// let coffee: RuntimeResult<()> = try_shovel(4, 6, PlantKind::CoffeeBean);
    /// let imitator: RuntimeResult<()> = try_shovel(
    ///     4,
    ///     6,
    ///     CardSelection::Imitator(PlantKind::PuffShroom),
    /// );
    /// let batch: RuntimeResult<()> = try_shovel([(3, 6), (4, 6)]);
    /// # let _ = (ordinary, pumpkin, coffee, imitator, batch);
    /// # }
    /// ```
    ///
    /// # 错误
    ///
    /// 无效脚本坐标和 backend 故障返回 `Err(RuntimeError)`。指定格没有对应
    /// 植物时返回 `Ok(())`。批量中的无效坐标会被汇总且不阻止后续有效项；
    /// backend 故障会停止后续访问，已完成的铲除不会回滚。
    ///
    /// 需要自动报告错误并继续当前 callback 时使用 [`shovel`]。
    pub try_shovel: TryShovel;

    where {
        rsvz_current::CurrentBackend: ShovelContext,
    }


    impl<>
    where {}
    call(row: i32, col: i32) -> RuntimeResult<()> {
        rsvz_game::shovel::try_shovel(row, col)
    }

    impl<T>
    where {
        T: IntoShovelTarget,
    }
    call(row: i32, col: i32, target: T) -> RuntimeResult<()> {
        rsvz_game::shovel::try_shovel_target(row, col, target)
    }

    impl<I, O>
    where {
        I: IntoIterator<Item = O>,
        O: IntoShovelOp,
    }
    call(ops: I) -> RuntimeResult<()> {
        rsvz_game::shovel::try_shovel_ops(ops)
    }
}

crate::callable::callable_api! {
    #[doc(alias = "AShovel")]
    /// 立即铲除一个或多个目标，报告错误后返回。
    ///
    /// 调用形式与 [`try_shovel`] 相同。无效坐标或 backend 错误会被报告，
    /// 但不会通过 `RuntimeResult` 传播，也不会 panic；当前 callback 中位于
    /// 本次调用之后的语句仍会继续执行。目标不存在是成功的空操作，不报告。
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # #[cfg(any(feature = "pvz-emulator", feature = "pvz-1-0-0-1051", feature = "pvz-portable"))]
    /// # {
    /// # use rsvz::prelude::*;
    /// shovel(4, 6); // 默认主体层
    /// shovel(4, 6, true); // 只铲南瓜
    /// shovel(4, 6, PlantKind::CoffeeBean); // 只铲咖啡豆
    /// shovel([(3, 6), (4, 6)]); // 按顺序批量铲除
    /// # }
    /// ```
    pub shovel: Shovel;

    where {
        rsvz_current::CurrentBackend: ShovelContext,
    }

    impl<>
    where {}
    call(row: i32, col: i32) -> () {
        rsvz_game::shovel::shovel(row, col);
    }

    impl<T>
    where {
        T: IntoShovelTarget,
    }
    call(row: i32, col: i32, target: T) -> () {
        rsvz_game::shovel::shovel_target(row, col, target);
    }

    impl<I, O>
    where {
        I: IntoIterator<Item = O>,
        O: IntoShovelOp,
    }
    call(ops: I) -> () {
        rsvz_game::shovel::shovel_ops(ops);
    }
}
