//! Purpose:
//! Emits PHP `filemtime` filesystem metadata builtin calls.
//! Delegates platform stat work to runtime helpers and boxes PHP false-or-value results.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - Filesystem state is observable, so emitters must preserve call order and failure sentinels.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits the `filemtime` builtin call.
///
/// Emits the path argument, calls the target-aware runtime helper `__rt_filemtime`,
/// and returns `Some(PhpType::Int)`. The runtime handles the false sentinel on failure
/// (missing file, permission error, etc.) and converts it to a Unix timestamp representation
/// that remains distinguishable from valid timestamps.
///
/// # Arguments
/// * `_name` - Unused, always "filemtime" (kept for dispatcher signature parity)
/// * `args` - Exactly one argument: the path expression
/// * `emitter` - Target assembly emitter
/// * `ctx` - Codegen context (variable layout, class metadata)
/// * `data` - Data section for relocations and string constants
///
/// # Returns
/// `Some(PhpType::Int)` — the modification timestamp is always typed as Int,
/// even when the underlying filesystem stat fails; the runtime sentinel preserves
/// this distinction.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("filemtime()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_filemtime");                            // call the target-aware runtime helper that returns the Unix modification timestamp
    Some(PhpType::Int)
}
