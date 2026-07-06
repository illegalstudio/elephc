//! Purpose:
//! Emits PHP `nl2br` string transformation or formatting calls.
//! Marshals string/scalar arguments into runtime helpers that allocate returned PHP strings.
//!
//! Called from:
//! - `crate::codegen_support::builtins::strings::emit()`.
//!
//! Key details:
//! - Returned string pointer/length pairs must be treated as owned runtime values when the helper allocates.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits codegen for the PHP `nl2br(string)` builtin.
///
/// Materializes `args[0]` (the input string) in registers per ABI, then calls the
/// target-aware runtime helper `__rt_nl2br` which allocates and returns a new PHP string
/// with every newline replaced by `<br />\n`. The returned string pointer/length is an
/// owned runtime value; the caller is responsible for releasing it.
///
/// # Arguments
/// - `args[0]`: the input string expression (other parameters are currently unsupported).
/// - `ctx`: carries variable layout, ownership state, and class metadata through codegen.
/// - `data`: receives any data-section allocations required by the call sequence.
///
/// # Output
/// Always returns `Some(PhpType::Str)` — the runtime helper produces an owned PHP string.
///
/// # ABI
/// `emit_string_arg` materializes the input string in `x1`/`x2` (pointer/length) or equivalent
/// registers per target ABI; `__rt_nl2br` returns the result string pointer in `x1`,
/// length in `x2`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("nl2br()");
    // Coerce the operand to a string in the string ABI registers via emit_string_arg, so a
    // Mixed argument is cast through __rt_mixed_cast_string instead of leaving a boxed cell in
    // the result register with stale string registers.
    super::args::emit_string_arg(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_nl2br");                                // call the target-aware runtime helper that expands newlines into HTML break tags
    Some(PhpType::Str)
}
