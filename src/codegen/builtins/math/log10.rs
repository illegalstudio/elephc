//! Purpose:
//! Emits PHP `log10` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits a PHP `log10($arg)` builtin call, computing the base-10 logarithm of the argument.
///
/// Inputs:
/// - `args[0]` is the operand, which may be an integer or float. Integer operands are
///   normalized to floating-point before the libc call.
/// - `emitter` is used to emit the conversion, call, and any target-specific instructions.
/// - `ctx` carries variable layout and metadata through the call.
/// - `data` provides access to the data section for any constant materialization.
///
/// Outputs:
/// - Always returns `Some(PhpType::Float)` since `log10` produces a float result.
///
/// Side effects:
/// - Normalizes the operand to the float argument register via `coerce_to_float`: integers
///   convert from the integer result register, and boxed `Mixed`/`Union` values are unboxed.
/// - Calls the platform's libc `log10` function via `bl_c` (AArch64) or `call` (x86_64).
/// - On AArch64 the scalar argument is in `d0`; on x86_64 it follows the SysV float ABI.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("log10()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &ty); // normalize int/Mixed inputs to a float in d0/xmm0
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.bl_c("log10");                                              // call libc log10() with the scalar argument in the native AArch64 floating-point argument register
        }
        Arch::X86_64 => {
            emitter.instruction("call log10");                                  // call libc log10() with the scalar argument in the native SysV floating-point argument register
        }
    }
    Some(PhpType::Float)
}
