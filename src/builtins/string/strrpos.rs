//! Purpose:
//! Home of the PHP `strrpos` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature carries the full golden param list (`haystack`, `needle`,
//!   `offset`), but `max_args: 2` caps `check_arity` so a third argument is rejected,
//!   matching the legacy CHECK arm which enforced exactly two arguments.
//! - `check` returns `PhpType::Union([Int, Bool])` (position, or `false` on no match).
//!   A check hook is required because the `builtin!` macro `returns:` field only accepts
//!   a simple type identifier and cannot express a union inline. Argument types are
//!   inferred by the common registry dispatch path before the hook fires.
//! - `lower` is a thin wrapper over the shared `lower_string_position` emitter, passing
//!   the `__rt_strrpos` runtime helper.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "strrpos",
    area: String,
    params: [haystack: Str, needle: Str, offset: Int = DefaultSpec::Int(0)],
    max_args: 2,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Finds the numeric position of the last occurrence of a substring.",
    php_manual: "https://www.php.net/manual/en/function.strrpos.php",
}

/// Returns `PhpType::Union([Int, Bool])` for a `strrpos` call (position, or `false`).
///
/// A check hook is required because the `builtin!` macro cannot express a union return
/// type inline. Argument types are inferred by the common registry dispatch path before
/// this hook fires; arity (capped to 2 via `max_args`) is validated by the registry.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::Bool]))
}

/// Lowers a `strrpos` call by dispatching to the shared string-position emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_string_position(
        ctx,
        inst,
        "strrpos",
        "__rt_strrpos",
    )
}
