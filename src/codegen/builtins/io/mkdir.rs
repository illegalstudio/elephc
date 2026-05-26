//! Purpose:
//! Emits PHP `mkdir` filesystem mutation builtin calls.
//! Passes path and mode/owner arguments to runtime helpers that perform observable OS operations.
//!
//! Called from:
//! - `crate::codegen::builtins::io::emit()`.
//!
//! Key details:
//! - These calls are effectful and must preserve PHP-visible ordering and boolean failure results.

use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::codegen::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits x86_64 ARM64 call to the `__rt_mkdir` runtime helper.
///
/// Arguments:
///   - `args[0]`: path expression, emitted via `emit_expr` before the call.
///   - `_name`: unused; preserved for dispatcher signature parity.
///   - `ctx`, `data`: carried through to `emit_expr` for path materialization.
///
/// Returns: `Some(PhpType::Bool)` — PHP `mkdir` returns `bool` on success/failure.
///
/// Runtime contract: `__rt_mkdir` receives the path pointer/length via ABI registers,
/// performs the observable OS mkdir call, and sets errno-derived boolean return in `x0`.
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("mkdir()");
    emit_expr(&args[0], emitter, ctx, data);
    abi::emit_call_label(emitter, "__rt_mkdir");                                // call the target-aware runtime helper that creates a directory path
    Some(PhpType::Bool)
}
