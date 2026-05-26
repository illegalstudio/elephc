//! Purpose:
//! Emits PHP `html_entity_decode` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits a call to the `html_entity_decode` runtime helper.
///
/// Arguments:
/// - `args[0]`: the input string to decode (already emitted)
///
/// Behavior:
/// - Emits the input expression, then calls `__rt_html_entity_decode` to decode HTML entities.
/// - Return type is `PhpType::Str` — caller receives an owned PHP string.
///
/// ABI:
/// - Caller is responsible for managing input expression lifecycle.
/// - Returned string pointer/length is an owned runtime value.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("html_entity_decode()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_html_entity_decode");                  // call the target-aware runtime helper that decodes HTML entities back into plain characters
    Some(PhpType::Str)
}
