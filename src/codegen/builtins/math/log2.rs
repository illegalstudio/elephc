//! Purpose:
//! Emits PHP `log2` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits the `log2` builtin call.
///
/// Evaluates `args[0]` and converts integer operands to float before the libc call.
/// Dispatches to the target C library's `log2` function and returns `PhpType::Float`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("log2()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &ty); // normalize int/Mixed inputs to a float in d0/xmm0
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.bl_c("log2");                                               // call libc log2() with the scalar argument in the native AArch64 floating-point argument register
        }
        Arch::X86_64 => {
            emitter.instruction("call log2");                                   // call libc log2() with the scalar argument in the native SysV floating-point argument register
        }
    }
    Some(PhpType::Float)
}
