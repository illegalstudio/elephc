//! Purpose:
//! Emits PHP `htmlspecialchars` string transformation or formatting calls.
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
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for a `htmlspecialchars` builtin call.
///
/// Marshals the string/scalar argument in `args[0]` and calls `__rt_htmlspecialchars`,
/// the target-aware runtime helper that converts special characters to HTML entities.
/// Returns `PhpType::Str` as the result type.
///
/// Arguments:
/// - `args[0]` – the expression to encode
///
/// Output:
/// - `PhpType::Str` indicating the returned PHP string
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("htmlspecialchars()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_htmlspecialchars");                    // call the target-aware runtime helper that converts special characters to HTML entities
    Some(PhpType::Str)
}
