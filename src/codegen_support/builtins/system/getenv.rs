//! Purpose:
//! Emits PHP `getenv` environment/platform information builtin calls.
//! Delegates host environment lookup or platform string construction to runtime helpers.
//!
//! Called from:
//! - `crate::codegen_support::builtins::system::emit()`.
//!
//! Key details:
//! - Environment and platform state are observable and must not be folded as compile-time constants here.

use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::abi;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// Emits code for the PHP `getenv()` builtin.
///
/// Emits the environment variable name expression, then calls `__rt_getenv`
/// to perform the runtime environment lookup. The result is always a string
/// on success, or `false` if the variable is not set.
///
/// # Arguments
/// * `_name` — unused, matches the dispatcher signature
/// * `args` — must contain exactly one expression: the environment variable name
/// * `emitter` — target assembly emitter
/// * `ctx` — codegen context (variables, scope)
/// * `data` — data section for literal payloads
///
/// # Returns
/// `Some(PhpType::Str)` — the lookup result is typed as a string (may be false at runtime)
pub fn emit(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("getenv()");
    // -- evaluate the environment variable name string --
    emit_expr(&args[0], emitter, ctx, data);
    // -- convert to C string and call getenv --
    abi::emit_call_label(emitter, "__rt_getenv");                               // get env var through the target-aware runtime helper → ptr/len result regs
    Some(PhpType::Str)
}
