//! Purpose:
//! Emits PHP `atan` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits a call to the libc `atan` function for the first argument expression.
///
/// # Arguments
/// - `args[0]` is evaluated and its value is passed to `atan()`.
/// - Integer arguments are normalized to float before the call via `emit_int_result_to_float_result`.
/// - The return type is always `PhpType::Float`.
///
/// # Behavior
/// Calls the target-native `atan` routine (AArch64: `bl_c("atan")`, X86_64: `call atan`)
/// with the scalar in the native floating-point argument register. NaN and infinity
/// propagate according to libm semantics, which matches PHP's `atan` behavior.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("atan()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer atan() inputs into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("atan"),                                  // call libc atan() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call atan"),                       // call libc atan() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
