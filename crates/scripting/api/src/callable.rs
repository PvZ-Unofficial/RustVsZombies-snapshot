//! callable 零大小用户 API 对象的内部辅助宏。

macro_rules! impl_callable {
    (
        impl<$($generic:ident),*> $target:ty
        where { $($where:tt)* }
        call_as($this:ident; $($arg:ident : $arg_ty:ty),* $(,)?) -> $output:ty $body:block
    ) => {
        impl<$($generic),*> FnOnce<($($arg_ty,)*)> for $target
        where
            $($where)*
        {
            type Output = $output;

            extern "rust-call" fn call_once(self, ($($arg,)*): ($($arg_ty,)*)) -> Self::Output {
                let $this = self;
                $body
            }
        }

        impl<$($generic),*> FnMut<($($arg_ty,)*)> for $target
        where
            $($where)*
        {
            extern "rust-call" fn call_mut(&mut self, ($($arg,)*): ($($arg_ty,)*)) -> Self::Output {
                let $this = &*self;
                $body
            }
        }

        impl<$($generic),*> Fn<($($arg_ty,)*)> for $target
        where
            $($where)*
        {
            extern "rust-call" fn call(&self, ($($arg,)*): ($($arg_ty,)*)) -> Self::Output {
                let $this = self;
                $body
            }
        }
    };
    (
        impl<$($generic:ident),*> $target:ty
        where { $($where:tt)* }
        call($($arg:ident : $arg_ty:ty),* $(,)?) -> $output:ty $body:block
    ) => {
        impl<$($generic),*> FnOnce<($($arg_ty,)*)> for $target
        where
            $($where)*
        {
            type Output = $output;

            extern "rust-call" fn call_once(self, ($($arg,)*): ($($arg_ty,)*)) -> Self::Output $body
        }

        impl<$($generic),*> FnMut<($($arg_ty,)*)> for $target
        where
            $($where)*
        {
            extern "rust-call" fn call_mut(&mut self, ($($arg,)*): ($($arg_ty,)*)) -> Self::Output $body
        }

        impl<$($generic),*> Fn<($($arg_ty,)*)> for $target
        where
            $($where)*
        {
            extern "rust-call" fn call(&self, ($($arg,)*): ($($arg_ty,)*)) -> Self::Output $body
        }
    };
}

/// Implements one input overload without runtime selection or TLS acquisition.
macro_rules! __callable_api_plain_impl {
    ($callable:ident; { $($common:tt)* };
        impl<$($generic:ident),*> where { $($specific:tt)* }
        call($($arg:ident : $arg_ty:ty),* $(,)?) -> $output:ty $body:block) => {
        $crate::callable::impl_callable! {
            impl<$($generic),*> $callable
            where { $($common)* $($specific)* }
            call($($arg : $arg_ty),*) -> $output $body
        }
    };
}

macro_rules! callable_api {
    (
        $(#[$attr:meta])* $vis:vis $value:ident: $callable:ident;
        where $common:tt
        $(impl<$($generic:ident),*> where $specific:tt
          call($($arg:ident : $arg_ty:ty),* $(,)?) -> $output:ty $body:block)+
    ) => {
        #[doc(hidden)]
        #[derive(Clone, Copy, Debug, Default)]
        $vis struct $callable;
        impl $callable {
            #[must_use]
            $vis const fn new() -> Self { Self }
        }
        $(#[$attr])*
        #[expect(non_upper_case_globals, reason = "script overloads use function-style names")]
        $vis const $value: $callable = $callable;
        $(
            $crate::callable::__callable_api_plain_impl! {
                $callable; $common;
                impl<$($generic),*> where $specific
                call($($arg : $arg_ty),*) -> $output $body
            }
        )+
    };
}

pub(crate) use __callable_api_plain_impl;
pub(crate) use callable_api;
pub(crate) use impl_callable;
