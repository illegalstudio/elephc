//! Purpose:
//! Emits PHP `cosh` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits a `cosh()` call to the target's libc.
///
/// Arguments:
/// - `_name`: unused, matches the builtin dispatcher signature
/// - `args[0]`: the operand, evaluated and left in the FP argument register
///
/// Behavior:
/// - Emits the argument expression and normalizes non-Float types to the FP result register.
/// - Calls the platform's `cosh` libc function (AArch64: `bl cosh`, x86_64: `call cosh`).
/// - Returns `PhpType::Float`. NaN/infinity behavior follows libc's `cosh`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("cosh()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer cosh() inputs into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("cosh"),                                  // call libc cosh() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call cosh"),                       // call libc cosh() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
