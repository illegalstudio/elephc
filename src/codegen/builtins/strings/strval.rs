//! Purpose:
//! Emits PHP `strval` conversion calls for legacy callable-wrapper emission.
//! Reuses the shared string-cast helper so wrapper calls match `(string)` semantics.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - This supports synthetic first-class builtin wrappers; PHP-visible lowering
//!   still belongs to the active EIR backend.

use crate::codegen::context::{Context, HeapOwnership};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::{
    coerce_to_string_releasing_owned, emit_expr, expr_result_heap_ownership,
};
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `strval()` builtin by delegating to shared string-cast coercion.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strval()");
    let ty = emit_expr(&args[0], emitter, ctx, data);
    coerce_to_string_releasing_owned(
        emitter,
        ctx,
        data,
        &ty,
        expr_result_heap_ownership(&args[0]) == HeapOwnership::Owned,
    );
    Some(PhpType::Str)
}
