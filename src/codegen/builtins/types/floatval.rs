//! Purpose:
//! Emits PHP `floatval` type conversion or type-name builtin calls.
//! Applies PHP scalar conversion rules or materializes runtime type names for values.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`.
//!
//! Key details:
//! - Conversion results must stay aligned with type-checker signatures and boxed Mixed handling.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{coerce_to_float, emit_expr};
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `floatval()` builtin, which converts a value to a float.
///
/// Dispatches on the argument type:
/// - `Str`: parses the string to a double via `__rt_str_to_number` (libc `strtod`), matching
///   PHP's lenient leading-numeric semantics (`"3.14abc"` → 3.14, `"abc"` → 0.0). The numeric
///   flag the helper also returns is ignored.
/// - `Float`: already in the float result register — no conversion.
/// - `Mixed`/`Union`: unboxes the cell to a double via `__rt_mixed_cast_float`.
/// - integer/bool/null: converted to a double via `scvtf`/`cvtsi2sd`.
///
/// - `args[0]`: the expression to convert
/// - Returns `Some(PhpType::Float)` unconditionally; callers can ignore the result.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("floatval()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    match ty {
        PhpType::Str => {
            // -- parse the string to a double (PHP floatval leniency via strtod) --
            abi::emit_call_label(emitter, "__rt_str_to_number");                // parse the string to a double in d0/xmm0 (the numeric flag in x0/rax is ignored)
        }
        // Float is a no-op; Mixed/Union unbox via __rt_mixed_cast_float; int/bool/null convert.
        _ => coerce_to_float(emitter, &ty),
    }
    Some(PhpType::Float)
}
