//! Purpose:
//! Emits PHP `urlencode` string transformation or formatting calls.
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

/// Emits a `urlencode(...)` call: evaluates the first argument as a PHP string, calls the
/// runtime helper `__rt_urlencode` to produce a percent-encoded query-string result, and
/// returns `PhpType::Str` as an owned heap-allocated PHP string.
///
/// - Argument 0 is evaluated in source order and consumed by value.
/// - Result pointer/length is returned via the target ABI (ARM64: x1, x2; x86_64: rsi, rdx).
/// - The returned string is an owned runtime value; the caller owns the allocation.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("urlencode()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_urlencode");                            // call the target-aware runtime helper that percent-encodes the current string for query-style URLs
    Some(PhpType::Str)
}
