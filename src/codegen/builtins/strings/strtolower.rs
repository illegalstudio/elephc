//! Purpose:
//! Emits PHP `strtolower` string transformation or formatting calls.
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

/// Emits a `strtolower` call for a single string argument.
///
/// # Arguments
/// - `args[0]`: The string expression to convert to lowercase.
///
/// # Behavior
/// Emits code to evaluate `args[0]` as a string, then calls `__rt_strtolower` to
/// perform the case conversion and return an owned PHP string. The returned string
/// pointer/length is treated as an owned runtime value.
///
/// # Returns
/// `PhpType::Str` — the lowered string.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("strtolower()");
    // Coerce the operand to a string in x1/x2 (rdi/rsi) via emit_string_arg, so a
    // Mixed argument (a `mixed` property/return value or an assoc-array element)
    // is cast through __rt_mixed_cast_string rather than left as a boxed cell in
    // x0 with stale string registers (which produced an empty result).
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_strtolower");                           // lowercase the input string through the target-aware runtime helper and return an owned result slice

    Some(PhpType::Str)
}
