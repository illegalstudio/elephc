//! Purpose:
//! Emits PHP `sin` numeric builtin calls backed by floating-point/libm-style helpers.
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
use crate::codegen::expr::{coerce_to_float, emit_expr};
use crate::codegen::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `sin` builtin call.
 ///
 /// ## Arguments
 /// - `args[0]`: the operand (integer or float) — emits the expression and normalizes
 ///   integer results to float before the libc call.
 ///
 /// ## Returns
 /// - Always `PhpType::Float`. NaN/infinity follow from libm; type-checker signatures
 ///   guarantee PHP-compatible behavior.
 ///
 /// ## Side effects
 /// - Calls `sin` from the C library on the active floating-point register.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sin()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &ty); // normalize int/Mixed inputs to a float in d0/xmm0
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("sin"),                                   // call libc sin() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call sin"),                        // call libc sin() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
