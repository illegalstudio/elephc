//! Purpose:
//! Emits PHP `rad2deg` numeric builtin calls backed by floating-point/libm-style helpers.
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

/// Converts a radian value to degrees by multiplying by `180.0 / PI`.
///
/// Loads the radian input from `args[0]` into the floating-point result register,
/// normalizing integer operands to float first. Multiplies by the `180.0 / PI`
/// constant and returns `PhpType::Float`.
///
/// # Arguments
/// * `_name` — unused; the builtin name is inferred from the call site
/// * `args` — single argument: the radian value (int or float)
/// * `emitter` — target-aware instruction emitter
/// * `ctx` — codegen context carrying variable layout and class metadata
/// * `data` — mutable data section for embedding the conversion constant
///
/// # Returns
/// Always returns `Some(PhpType::Float)` as the result is always a float.
///
/// # ABI notes
/// - AArch64: input in `d0`, constant loaded via `adrp`/`ldr_lo12` into `d1`, result in `d0`
/// - x86_64: input in `xmm0`, constant loaded via `movsd` into `xmm1`, result in `xmm0`
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("rad2deg()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_float(emitter, &ty); // normalize int/Mixed inputs to a float in d0/xmm0
    // -- multiply by 180.0 / M_PI to convert radians to degrees --
    let label = data.add_float(180.0 / std::f64::consts::PI);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.adrp("x9", &format!("{}", label));                           // load the page address that contains the radian-to-degree conversion constant
            emitter.ldr_lo12("d1", "x9", &format!("{}", label));                // load the radian-to-degree conversion constant into the secondary AArch64 floating-point register
            emitter.instruction("fmul d0, d0, d1");                             // multiply the radian input by the conversion constant in the standard AArch64 floating-point result register
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movsd xmm1, QWORD PTR [rip + {}]", label)); // load the radian-to-degree conversion constant into the secondary x86_64 floating-point register
            emitter.instruction("mulsd xmm0, xmm1");                            // multiply the radian input by the conversion constant in the standard x86_64 floating-point result register
        }
    }
    Some(PhpType::Float)
}
