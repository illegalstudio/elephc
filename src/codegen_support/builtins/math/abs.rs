//! Purpose:
//! Emits PHP `abs` numeric builtin calls.
//! Handles scalar argument lowering and returns the PHP numeric type promised by signature checking.
//!
//! Called from:
//! - `crate::codegen_support::builtins::math::emit()`.
//!
//! Key details:
//! - Integer-vs-float result selection must stay aligned with PHP semantics and local type inference.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits inline assembly for PHP `abs($value)`.
///
/// # Arguments
/// - `_name`: Unused; the caller guarantees this is `"abs"`.
/// - `args`: Must contain exactly one expression — the numeric operand.
/// - `emitter`: Write instruction stream here.
/// - `ctx`: Variable/layout context; `emit_expr` may allocate temps or load values.
/// - `data`: Data section for any literal payloads.
///
/// # Returns
/// `Some(PhpType::Int)` if the operand is or promotes to integer, `Some(PhpType::Float)` otherwise.
/// Returns `None` only if `emit_expr` returns `None` (e.g. unsupported expression type).
///
/// # Codegen behavior
/// - Float: uses IEEE-754 sign-bit masking via `fabs` (AArch64) or integer register tricks (x86_64).
/// - Integer: uses branchless two's-complement conditional-negate sequence.
/// - The result type must stay consistent with the type inferrer's expectations in the caller.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("abs()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if matches!(ty, PhpType::TaggedScalar) {
        // narrow a tagged scalar (null -> 0) before the integer absolute-value sequence
        crate::codegen_support::sentinels::emit_tagged_scalar_to_int_null_as_zero(emitter);
    }
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        // The operand is a boxed Mixed cell pointer, not a raw scalar; the runtime helper
        // unboxes it, applies the integer or float absolute value per the stored tag, and
        // reboxes — preserving PHP's int→int / float→float result typing.
        crate::codegen_support::abi::emit_call_label(emitter, "__rt_abs_mixed");
        return Some(PhpType::Mixed);
    }
    if ty == PhpType::Float {
        // -- float absolute value --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("fabs d0, d0");                             // take absolute value of the floating-point result in place
            }
            Arch::X86_64 => {
                emitter.instruction("movq r10, xmm0");                          // move the floating-point payload into a scratch integer register for sign-bit masking
                emitter.instruction("mov r11, 0x7fffffffffffffff");             // materialize a mask that clears the IEEE-754 sign bit
                emitter.instruction("and r10, r11");                            // clear the sign bit so the payload becomes its absolute value
                emitter.instruction("movq xmm0, r10");                          // move the masked floating-point payload back into the result register
            }
        }
        Some(PhpType::Float)
    } else {
        // -- integer absolute value via conditional negate --
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction("cmp x0, #0");                              // compare the integer value against zero
                emitter.instruction("cneg x0, x0, lt");                         // negate the integer result only when it was negative
            }
            Arch::X86_64 => {
                emitter.instruction("mov r10, rax");                            // copy the integer value into a scratch register before branchless sign handling
                emitter.instruction("sar r10, 63");                             // expand the sign bit to an all-zero or all-one mask
                emitter.instruction("xor rax, r10");                            // flip the payload bits when the original integer was negative
                emitter.instruction("sub rax, r10");                            // subtract the sign mask to finish the two's-complement absolute value
            }
        }
        Some(PhpType::Int)
    }
}
