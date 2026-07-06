//! Purpose:
//! Emits PHP `rad2deg` numeric builtin calls backed by floating-point/libm-style helpers.
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
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize the radian input into the active floating-point result register before applying the conversion factor
    }
    // -- multiply by 180.0 / M_PI to convert radians to degrees --
    let label = data.add_float(180.0 / std::f64::consts::PI);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg_via_page(emitter, "d1", "x9", &label); // load the radian-to-degree conversion constant into the secondary AArch64 floating-point register
            emitter.instruction("fmul d0, d0, d1");                             // multiply the radian input by the conversion constant in the standard AArch64 floating-point result register
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(emitter, "xmm1", &label, 0);           // load the radian-to-degree conversion constant into the secondary x86_64 floating-point register
            emitter.instruction("mulsd xmm0, xmm1");                            // multiply the radian input by the conversion constant in the standard x86_64 floating-point result register
        }
    }
    Some(PhpType::Float)
}
