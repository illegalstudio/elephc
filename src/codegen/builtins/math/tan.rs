//! Purpose:
//! Emits PHP `tan` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Emits a PHP `tan` numeric builtin call backed by the platform libc `tan` routine.
///
/// # Arguments
/// * `_name` — Unused name parameter (保留 for dispatcher signature compatibility).
/// * `args` — Single argument: the operand to compute tangent on.
/// * `emitter` — Target assembly emitter.
/// * `ctx` — Codegen context carrying variable layout and class metadata.
/// * `data` — Mutable data section for constant pools.
///
/// # Behavior
/// - Emits the operand expression and captures its type.
/// - Normalizes the operand to a float via `coerce_to_float` (integers convert with
///   `scvtf`/`cvtsi2sd`; boxed `Mixed`/`Union` values unbox through `__rt_mixed_cast_float`)
///   so the floating-point register holds the value before the libc call.
/// - Emits a `bl tan` (AArch64) or `call tan` (x86_64) instruction to invoke the
///   platform's libm tangent function.
///
/// # Returns
/// Always returns `Some(PhpType::Float)` — `tan` produces a floating-point result.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("tan()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &ty); // normalize int/Mixed inputs to a float in d0/xmm0
    match emitter.target.arch {
        Arch::AArch64 => emitter.bl_c("tan"),                                   // call libc tan() with the scalar argument in the native AArch64 floating-point argument register
        Arch::X86_64 => emitter.instruction("call tan"),                        // call libc tan() with the scalar argument in the native SysV floating-point argument register
    }
    Some(PhpType::Float)
}
