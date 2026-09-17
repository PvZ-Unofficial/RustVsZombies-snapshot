#![warn(
    clippy::borrow_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::inline_asm_x86_att_syntax,
    clippy::missing_safety_doc,
    clippy::multiple_unsafe_ops_per_block,
    clippy::ptr_as_ptr,
    clippy::ptr_cast_constness,
    clippy::ref_as_ptr,
    clippy::significant_drop_in_scrutinee,
    clippy::significant_drop_tightening,
    clippy::trailing_empty_array,
    clippy::transmute_ptr_to_ptr,
    clippy::transmute_undefined_repr,
    clippy::undocumented_unsafe_blocks,
    clippy::uninhabited_references,
    clippy::unsafe_derive_deserialize,
    clippy::volatile_composites
)]
#![cfg_attr(
    feature = "verify-exe",
    feature(proc_macro_diagnostic, proc_macro_tracked_env, proc_macro_tracked_path)
)]

//! Proc-macro implementation for the RSVZ PvZ ABI wrapper DSL.
//!
//! The procedural macro runs on the compiler host; generated calls only support
//! 32-bit x86. The injected backend owns the target-platform compile-time guard.

mod expand;
mod ir;
mod parse;
mod register;
#[cfg(feature = "verify-exe")]
mod verify;

use proc_macro::TokenStream;
use syn::parse_macro_input;

/// Expands one PvZ ABI call site into a `core::arch::asm!` block.
///
/// `pvz_abi_call!` is the low-level escape hatch. It accepts ABI facts only:
/// `addr`, `this`, `regs`, `stack`, `stack_this`, `cleanup`, `clobber`, and
/// `ret`. The call-target scratch register is chosen internally by the macro
/// and is not part of the public DSL.
///
/// Additional return rule:
///
/// - `ret:` must be written as `ret: <reg> => <place>`
/// - x87 returns must be written as `ret: st0<f32> => place` or
///   `ret: st0<f64> => place`
///
/// Because the expansion performs raw pointer reads and inline assembly,
/// `pvz_abi_call!` must be invoked from an explicit `unsafe` context.
///
/// # Examples
///
/// A plain call with no arguments:
///
/// ```rust,ignore
/// unsafe {
///     pvz_abi_call! {
///         addr: 0x415d40,
///     }
/// }
/// ```
///
/// A mixed ABI call with registers, a pushed hidden `this`, stack arguments,
/// caller cleanup, and an `al` return:
///
/// ```rust,ignore
/// let ok: u8;
/// unsafe {
///     pvz_abi_call! {
///         addr: 0x52ab10,
///         this: ecx = [0x6a9ec0, 0x768],
///         regs: {
///             eax = lane,
///             esi = scratch,
///             edi = flags,
///             al = paused,
///         },
///         stack_this: hidden_this,
///         stack: [mode, y, x],
///         cleanup: 0x10,
///         clobber: [ebx, esi, edi],
///         ret: al => ok,
///     }
/// }
/// ```
#[proc_macro]
pub fn pvz_abi_call(input: TokenStream) -> TokenStream {
    let spec = parse_macro_input!(input as ir::CallSpec);
    #[cfg(feature = "verify-exe")]
    let diagnostic = verify::verify_call(&spec).err().map(|err| err.to_compile_error());
    let expanded =
        expand::asm::expand_abi_call(&spec.abi, spec.ret, spec.span).unwrap_or_else(|err| err.to_compile_error());
    #[cfg(feature = "verify-exe")]
    {
        quote::quote!({ #diagnostic #expanded }).into()
    }
    #[cfg(not(feature = "verify-exe"))]
    expanded.into()
}

/// Generates one or more typed PvZ ABI wrappers as
/// `#[inline(always)] pub unsafe fn` items.
///
/// `pvz_abi_fn!` is the preferred backend-facing entry point. It keeps ABI
/// descriptions centralized in a declaration table instead of scattering ad-hoc
/// inline-assembly call sites across the codebase.
///
/// This DSL is meant for the odd x86 calling conventions used by the original
/// Win32 PvZ binary. Instead of pretending those calls are plain `cdecl`,
/// `stdcall`, or normal MSVC `thiscall`, the macro lets the backend describe
/// the exact ABI facts of a wrapper:
///
/// - which register receives `this`
/// - which general-purpose, low-byte, or x87 registers receive arguments
/// - which arguments are pushed on the stack, and in what order
/// - whether a hidden `this` pointer must also be pushed on the stack
/// - which general-purpose registers are actually clobbered by the callee
/// - where the return value comes back from
///
/// Supported fields inside each wrapper body:
///
/// - `addr`: absolute call target address
/// - `this`: a 32-bit register binding such as `ecx = expr` or
///   `ecx = [base, off1, off2]`
/// - `regs`: additional register bindings
/// - `stack`: ordinary stack arguments, emitted in listed order as `pushl`
/// - `stack_this`: one extra pushed argument emitted before `stack`; it accepts
///   either an expression or a pointer chain
/// - `ret`: return register; wrappers capture the return value automatically
/// - `cleanup`: explicit caller cleanup bytes, emitted as `addl $N, %esp`
/// - `clobber`: the exact general-purpose registers clobbered by the call
///
/// Register support:
///
/// - `this` and `clobber` accept only 32-bit general-purpose registers other
///   than `esp`/`ebp`
/// - `regs` accepts 32-bit registers, low-byte registers (`al/bl/cl/dl`), and
///   x87 stack registers (`st0/st1`); `ebp` is intentionally rejected
/// - `ret` accepts 32-bit registers other than `ebp`, `ax`, low-byte
///   registers, and `st0`
///
/// x87 support:
///
/// - x87 register bindings must include a size hint:
///   `st1<f64> = x, st0<f64> = y`
/// - x87 returns may be written as `ret: st0<f32>` / `ret: st0<f64>`
/// - if the wrapper return type is literally `f32` or `f64`, `ret: st0` is
///   also accepted and the width is inferred from the Rust return type
/// - the macro lowers x87 inputs and outputs through local typed temporaries
///   plus `flds/fldl` and `fstps/fstpl`; it does not try to use unsupported
///   x87 operand constraints directly
///
/// Low-byte inputs:
///
/// - `regs: { al = flag }` is supported directly
/// - the expression is truncated to `u8` before being passed to `asm!`
///
/// `ebp` is intentionally not supported as an ABI carrier. The current 1051
/// calling-convention table has no `ebp = ...` parameter/this/return/clobber
/// entries, and `ebp` is frame/base pointer related on i686.
///
/// The macro preserves callee-saved registers that it must write manually, but
/// callers are still expected to describe the real clobber set precisely.
///
/// # Examples
///
/// ```rust,ignore
/// pvz_abi_fn! {
///     game_fight_loop() {
///         addr: 0x415d40,
///     }
///
///     board_stage_has_pool(board: *mut MainObject) -> u8 {
///         addr: 0x41c0d0,
///         this: eax = board,
///         ret: al,
///     }
/// }
/// ```
#[proc_macro]
pub fn pvz_abi_fn(input: TokenStream) -> TokenStream {
    let spec = parse_macro_input!(input as ir::WrapperListSpec);
    #[cfg(feature = "verify-exe")]
    let diagnostic = verify::verify_wrappers(&spec).err().map(|err| err.to_compile_error());
    let expanded = expand::wrapper::expand_wrapper_list(spec).unwrap_or_else(|err| err.to_compile_error());
    #[cfg(feature = "verify-exe")]
    {
        quote::quote!(#diagnostic #expanded).into()
    }
    #[cfg(not(feature = "verify-exe"))]
    expanded.into()
}
