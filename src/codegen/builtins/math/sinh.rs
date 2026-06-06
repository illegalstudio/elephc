//! Purpose:
//! Emits PHP `sinh` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits the `sinh()` builtin call to compute the hyperbolic sine of a float or integer.
///
/// # Inputs
/// - `_name`: Unused name parameter (matches dispatcher signature).
/// - `args`: Single argument expression to be evaluated and passed to `sinh()`.
/// - `emitter`: Target assembly emitter.
/// - `ctx`: Codegen context carrying operand type info.
/// - `data`: Data section for any embedded literals.
///
/// # Behavior
/// Evaluates the argument expression to determine its type. For integer operands,
/// inserts an integer-to-float conversion before the libc call to normalize the
/// input into the active floating-point result register. Then issues a platform-
/// specific `bl sinh` (AArch64) or `call sinh` (x86_64) instruction to invoke the
/// C library routine. Returns `PhpType::Float` regardless of input type.
///
/// # ABI notes
/// - AArch64: scalar argument is placed in `d0` per the AAPCS calling convention.
/// - x86_64: scalar argument is placed in `xmm0` per the SysV AMD64 ABI.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("sinh()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &ty); // normalize int/Mixed inputs to a float in d0/xmm0
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("sinh"),                                  // call libc sinh() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call sinh"),                       // call libc sinh() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
