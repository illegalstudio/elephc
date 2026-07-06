//! Purpose:
//! Emits PHP `cos` numeric builtin calls backed by floating-point/libm-style helpers.
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
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer cos() inputs into the active floating-point result register before the libc call
    }
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("cos"),                                   // call libc cos() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call cos"),                        // call libc cos() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
