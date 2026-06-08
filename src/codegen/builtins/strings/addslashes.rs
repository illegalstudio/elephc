//! Purpose:
//! Emits PHP `addslashes` string transformation or formatting calls.
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

/// Emits code to escape single quotes, double quotes, backslashes, and NUL bytes
/// in the first argument using the `__rt_addslashes` runtime helper.
///
/// # Arguments
/// - `args[0]` is evaluated and passed as the string to escape.
/// - Calls `__rt_addslashes` through the active target ABI.
///
/// # Returns
/// `PhpType::Str` — the escaped string as an owned runtime value.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("addslashes()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_addslashes");                           // escape quotes and backslashes through the active target ABI
    Some(PhpType::Str)
}
