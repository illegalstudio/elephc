//! Purpose:
//! Emits PHP `boolval` type conversion or type-name builtin calls.
//! Applies PHP scalar conversion rules or materializes runtime type names for values.
//!
//! Called from:
//! - `crate::codegen_support::builtins::types::emit()`.
//!
//! Key details:
//! - Conversion results must stay aligned with type-checker signatures and boxed Mixed handling.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::{coerce_to_truthiness, emit_expr};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `boolval()` builtin, converting a value to boolean.
///
/// Converts `args[0]` to PHP truthiness/falsiness using the shared
/// `coerce_to_truthiness` helper. The result is always `PhpType::Bool`.
///
/// # Arguments
/// - `_name`: Unused; dispatch is already resolved.
/// - `args`: Single expression to convert.
/// - `emitter`: Target assembly emitter.
/// - `ctx`: Codegen context (variable layout, class metadata).
/// - `data`: Data section for literals and runtime symbols.
///
/// # Returns
/// Always `Some(PhpType::Bool)`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("boolval()");
    // -- convert any value to boolean (truthy/falsy) --
    let src_ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_truthiness(emitter, ctx, &src_ty);                                // normalize the value to PHP truthiness through the shared target-aware coercion helper
    Some(PhpType::Bool)
}
