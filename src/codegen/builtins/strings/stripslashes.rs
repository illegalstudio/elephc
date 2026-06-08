//! Purpose:
//! Emits PHP `stripslashes` string transformation or formatting calls.
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

/// Emits a call to the `stripslashes` runtime helper for the builtin `stripslashes()`.
///
/// Inputs:
/// - `args[0]` is evaluated and passed as the string argument to strip backslashes from.
/// - The runtime helper `__rt_stripslashes` removes escape backslashes following PHP rules.
///
/// Returns `PhpType::Str` as the result is always a PHP string.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stripslashes()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_stripslashes");                         // remove escape backslashes through the active target ABI
    Some(PhpType::Str)
}
