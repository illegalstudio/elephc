//! Purpose:
//! Emits PHP `tanh` numeric builtin calls backed by floating-point/libm-style helpers.
//! Marshals integer or float operands into the target ABI and records the numeric return type.
//!
//! Called from:
//! - `crate::codegen::builtins::math::emit()`.
//!
//! Key details:
//! - NaN, infinity, rounding, and division edge cases must remain PHP-compatible with type-checker signatures.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP `tanh($arg)` call.
///
/// Normalizes integer operands to float (via `emit_int_result_to_float_result`) before
/// the libc call, ensuring the floating-point argument register holds the correct value.
/// Dispatches to the target-specific libc `tanh` symbol (AArch64 `bl_c`, x86_64 `call`).
///
/// # Arguments
/// * `name` — builtin name (unused, only for signature compatibility)
/// * `args` — single argument expression
/// * `emitter` — target assembly emitter
/// * `ctx` — codegen context (variable layout, ownership)
/// * `data` — data section for literals/constants
///
/// # Returns
/// Always returns `Some(PhpType::Float)`. NaN and infinity are produced by the underlying
/// libc implementation and are PHP-compatible by construction.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("tanh()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer tanh() inputs into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("tanh"),                                  // call libc tanh() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call tanh"),                       // call libc tanh() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
