//! Purpose:
//! Emits PHP `cos` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits a `cos` builtin call for a single numeric argument.
///
/// Loads the argument into the native floating-point argument register, calls
/// the platform's libc `cos` function, and returns `PhpType::Float`. Integer
/// operands are normalized into the floating-point result register before the call.
/// On AArch64 the scalar argument is in `d0`; on x86_64 it is in the SysV FP register.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("cos()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &ty); // normalize int/Mixed inputs to a float in d0/xmm0
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("cos"),                                   // call libc cos() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call cos"),                        // call libc cos() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
