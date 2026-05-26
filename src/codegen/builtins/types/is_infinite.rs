//! Purpose:
//! Emits PHP `is_infinite` type predicate calls.
//! Inspects static or boxed runtime value representation and returns a PHP boolean.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Predicate behavior must match PHP sentinel, Mixed tag, and object/interface layout conventions.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::{abi, platform::Arch};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for PHP's `is_infinite()` builtin.
///
/// Writes the result as a boolean integer in `x0`/`rax`. Non-float inputs are
/// normalized into the float register before the infinity check. The predicate
/// is true when the absolute value equals positive infinity (covers both `+INF`
/// and `-INF` on AArch64) or when the value equals either `+INF` or `-INF` on
/// x86_64.
///
/// # Arguments
/// * `_name` — unused, follows the builtin emitter convention
/// * `args` — the expression to test for infinity; must have at least one element
/// * `emitter` — target-specific instruction emission
/// * `ctx` — variable layout, ownership state, class/FFI metadata
/// * `data` — runtime data section for floating-point constants
///
/// # Returns
/// `Some(PhpType::Bool)` on success; never returns `None` for this builtin.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("is_infinite()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    if ty != PhpType::Float {
        abi::emit_int_result_to_float_result(emitter);                          // normalize integer inputs into the active floating-point result register before the infinity check
    }
    match emitter.target.arch {
        Arch::AArch64 => {
            // -- check if |value| equals infinity --
            emitter.instruction("fabs d0, d0");                                 // take the absolute value so both +INF and -INF compare against the same constant
            let inf_label = data.add_float(f64::INFINITY);
            emitter.adrp("x9", &inf_label);                                     // load the page that contains the infinity constant
            emitter.add_lo12("x9", "x9", &inf_label);                           // resolve the infinity constant address within that page
            emitter.instruction("ldr d1, [x9]");                                // load the infinity constant into the comparison register
            emitter.instruction("fcmp d0, d1");                                 // compare the absolute value against positive infinity
            emitter.instruction("cset x0, eq");                                 // materialize the infinity comparison result as a boolean integer
        }
        Arch::X86_64 => {
            let pos_inf_label = data.add_float(f64::INFINITY);
            let neg_inf_label = data.add_float(f64::NEG_INFINITY);
            emitter.instruction(&format!("movsd xmm1, QWORD PTR [rip + {}]", pos_inf_label)); // load the positive infinity constant into the comparison register
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the value against positive infinity
            emitter.instruction("sete al");                                     // remember whether the value equals positive infinity
            emitter.instruction(&format!("movsd xmm1, QWORD PTR [rip + {}]", neg_inf_label)); // load the negative infinity constant into the comparison register
            emitter.instruction("ucomisd xmm0, xmm1");                          // compare the value against negative infinity
            emitter.instruction("sete cl");                                     // remember whether the value equals negative infinity
            emitter.instruction("or al, cl");                                   // combine the +/- infinity comparisons into one boolean byte
            emitter.instruction("movzx rax, al");                               // widen the infinity boolean byte into the canonical integer result register
        }
    }
    Some(PhpType::Bool)
}
