//! Purpose:
//! Emits PHP `acos` numeric builtin calls backed by floating-point/libm-style helpers.
//! Marshals integer or float operands into the target ABI and records the numeric return type.
//!
//! Called from:
//! - `crate::codegen_support::builtins::math::emit()`.
//!
//! Key details:
//! - NaN, infinity, rounding, and division edge cases must remain PHP-compatible with type-checker signatures.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a call to the PHP `acos` builtin, backed by the host libc `acos` routine.
///
/// # Arguments
/// - `_name`: Unused; the builtin name is resolved by the dispatcher.
/// - `args`: Exactly one expression producing a float or integer value.
///
/// # Behavior
/// - Normalizes integer operands to the floating-point result register via
///   `emit_int_result_to_float_result` before the libc call.
/// - Calls `acos` through the target's native calling convention (AArch64 `bl_c`
///   or x86_64 `call acos`).
///
/// # Returns
/// `Some(PhpType::Float)` — `acos` always returns a float in PHP.
///
/// # Panics
/// Requires `args.len() == 1` and a supported target architecture (AArch64, X86_64).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("acos()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer acos() inputs into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("acos"),                                  // call libc acos() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call acos"),                       // call libc acos() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
