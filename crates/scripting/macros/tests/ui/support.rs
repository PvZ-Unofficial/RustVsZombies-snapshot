extern crate self as rsvz;

pub mod core {
    pub mod runtime {
        #[derive(Debug)]
        pub struct RuntimeError;

        pub type RuntimeResult<T = ()> = Result<T, RuntimeError>;
    }
}

pub mod __private {
    pub struct CurrentBackend;
    pub struct DispatchInput;
    pub struct DispatchResult;

    pub fn run_script(
        body: impl FnOnce() -> crate::core::runtime::RuntimeResult<()>,
    ) -> crate::core::runtime::RuntimeResult<()> {
        body()
    }

    pub fn run_state_hook_installer(
        body: impl FnOnce() -> crate::core::runtime::RuntimeResult<()>,
    ) -> crate::core::runtime::RuntimeResult<()> {
        body()
    }

    pub fn install_framework_state_hooks() -> crate::core::runtime::RuntimeResult<()> {
        Ok(())
    }

    pub fn runtime_dispatch(
        _backend: &mut CurrentBackend,
        _input: DispatchInput,
        script: fn() -> crate::core::runtime::RuntimeResult<()>,
        install: fn() -> crate::core::runtime::RuntimeResult<()>,
    ) -> DispatchResult {
        let _ = install();
        let _ = script();
        DispatchResult
    }
}

pub mod dsl {
    pub mod prelude {
        use std::ops::{Add, Shl};

        #[derive(Clone, Copy)]
        pub enum ReloadMode {
            MainUiOrFightUi,
        }

        pub use ReloadMode::*;

        #[derive(Clone, Copy)]
        pub struct Retention;

        pub const fn to(_time: i32) -> Retention {
            Retention
        }

        #[derive(Clone)]
        pub struct LowExpr;

        impl Add for LowExpr {
            type Output = Self;

            fn add(self, _rhs: Self) -> Self::Output {
                Self
            }
        }

        impl Add<&LowExpr> for LowExpr {
            type Output = Self;

            fn add(self, _rhs: &LowExpr) -> Self::Output {
                Self
            }
        }

        impl Shl<LowExpr> for i32 {
            type Output = ();

            fn shl(self, _rhs: LowExpr) -> Self::Output {
                let _ = self;
            }
        }

        impl Shl<&LowExpr> for i32 {
            type Output = ();

            fn shl(self, _rhs: &LowExpr) -> Self::Output {
                let _ = self;
            }
        }

        pub fn pp() -> LowExpr {
            LowExpr
        }

        pub fn d(_delay: i32) -> LowExpr {
            LowExpr
        }

        pub fn dancing() -> LowExpr {
            LowExpr
        }

        #[allow(non_upper_case_globals)]
        pub const pot: fn(Retention, i32, i32) -> LowExpr =
            |_retention, _row, _col| LowExpr;

        pub fn reload(_mode: ReloadMode) {}

        pub fn set_zombies(_input: &str) {}

        pub fn assume_wavelength<T>(_waves: T, _length: i32) {}

        pub fn skip_until<T>(_time: T) {}

        pub fn wave(_wave: i32) {}
    }
}

pub mod timeline {
    #[derive(Clone, Copy)]
    pub struct TimeHandle;

    pub fn try_at<F>(
        _wave: i32, _time: i32,
        _f: F,
    ) -> crate::core::runtime::RuntimeResult<TimeHandle>
    where
        F: FnMut() -> crate::core::runtime::RuntimeResult<()> + 'static,
    {
        Ok(TimeHandle)
    }
}
