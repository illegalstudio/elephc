//! Purpose:
//! Emits PHP `hex2bin` string transformation or formatting calls.
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

/// Emits code for the PHP `hex2bin` builtin.
///
/// Consumes the string argument in `args[0]` via `emit_expr`, then calls the
/// target-aware runtime helper `__rt_hex2bin` to decode hex to raw bytes.
/// Returns `PhpType::Str` as the result is always a PHP string.
///
/// # Arguments
/// * `_name` — builtin name (unused, always `"hex2bin"`);
/// * `args` — must contain exactly one string-typed argument;
/// * `emitter` — target assembly emitter;
/// * `ctx` — codegen context (variable layout, ownership state);
/// * `data` — data section for relocations and constants.
///
/// # Returns
/// `Some(PhpType::Str)` — the decoded binary string produced by the runtime helper.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("hex2bin()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_hex2bin");                              // convert the current hexadecimal string to bytes through the target-aware runtime helper
    Some(PhpType::Str)
}
