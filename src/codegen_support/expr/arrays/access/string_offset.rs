//! Purpose:
//! Lowers PHP string-offset expressions whose source index may be an integer-like string literal.
//! Keeps offset coercion separate from array payload addressing.
//!
//! Called from:
//! - `crate::codegen_support::expr::arrays::access::indexed::emit_array_access_with_loaded_base()`
//!
//! Key details:
//! - The string-indexing path expects the final offset in the integer result register before bounds checks.

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits integer offset computation for string index expressions with integer-like string literal support.
pub(super) fn emit_string_offset_index(
    index: &Expr,
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> PhpType {
    if let ExprKind::StringLiteral(value) = &index.kind {
        if let Some(offset) = crate::types::parse_php_string_offset_literal(value) {
            abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), offset);
            return PhpType::Int;
        }
    }

    emit_expr(index, emitter, ctx, data)
}
