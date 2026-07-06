//! Purpose:
//! Emits PHP `is_nan` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen_support::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a PHP `is_nan()` type predicate call.
///
/// Compares the first argument against itself using an unordered floating-point
/// comparison. NaN is the only value that does not equal itself, so the comparison
/// result directly indicates whether the value is NaN.
///
/// # Arguments
/// * `_name` — builtin name (unused, dispatch is by caller)
/// * `args` — argument expressions; `args[0]` is the value to test
/// * `emitter` — target-specific assembly emitter
/// * `ctx` — codegen context (types, locals, etc.)
/// * `data` — data section for literals and jump tables
///
/// # Returns
/// `Some(PhpType::Bool)` — the predicate result type
///
/// # ABI / Runtime behavior
/// - Non-float inputs are first normalized into the float register via `emit_int_result_to_float_result`.
/// - AArch64: `fcmp d0, d0` sets the unordered flag for NaN; `cset x0, vs` materializes the bool.
/// - x86_64: `ucomisd xmm0, xmm0` sets the parity flag for NaN; `setp al` / `movzx` materializes the bool.
/// - Result is returned in the canonical integer register (`x0` on AArch64, `rax` on x86_64).
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_nan()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    // -- NaN is the only value that does not equal itself --
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        abi::emit_call_label(emitter, "__rt_mixed_cast_float");                  // unbox a boxed Mixed payload to a double before the NaN check (avoids treating the cell pointer as a value)
    } else if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer inputs into the active floating-point result register before the NaN check
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fcmp d0, d0");                                 // compare the floating-point value against itself so NaN sets the unordered flag
            emitter.instruction("cset x0, vs");                                 // materialize the unordered NaN comparison result as a boolean integer
        }
        Arch::X86_64 => {
            emitter.instruction("ucomisd xmm0, xmm0");                          // compare the floating-point value against itself so NaN sets the parity flag
            emitter.instruction("setp al");                                     // materialize the unordered NaN comparison result into the low boolean byte
            emitter.instruction("movzx rax, al");                               // widen the NaN boolean byte into the canonical integer result register
        }
    }
    Some(PhpType::Bool)
}
