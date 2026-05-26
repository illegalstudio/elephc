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
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `floatval()` builtin, which converts a value to a float.
///
/// Converts the first argument to a double-precision floating-point value.
/// If the argument is already a `Float`, no conversion occurs; otherwise an integer
/// result is converted to the target float register via the ABI conversion routine.
///
/// - `args[0]`: the expression to convert
/// - `emitter`: used to emit instructions and comments
/// - `ctx`: carries variable layout and compilation context
/// - `data`: data section for literals and runtime symbols
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
    if ty != PhpType::Float {
        // -- convert integer to double-precision float --
        abi::emit_int_result_to_float_result(emitter);                          // convert signed int result to the target float result register
    }
    Some(PhpType::Float)
}
