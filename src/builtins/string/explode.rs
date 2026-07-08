//! Purpose:
//! Home of the PHP `explode` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The declared signature carries the full golden param list (`separator`, `string`,
//!   `limit`), but `max_args: 2` caps `check_arity` so a third argument is rejected,
//!   matching the legacy CHECK arm which enforced exactly two arguments.
//! - `check` returns `PhpType::Array(Box::new(PhpType::Str))`. A check hook is required
//!   because the `builtin!` macro `returns:` field cannot express an array type inline.
//!   Argument types are inferred by the common registry dispatch path before the hook
//!   fires.
//! - `lower` is a thin wrapper over the shared `lower_explode` emitter.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "explode",
    area: String,
    params: [separator: Str, string: Str, limit: Int = DefaultSpec::IntMax],
    max_args: 2,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Splits a string by a separator into an array of substrings.",
    php_manual: "https://www.php.net/manual/en/function.explode.php",
}

/// Returns `PhpType::Array(Box::new(PhpType::Str))` for an `explode` call.
///
/// A check hook is required because the `builtin!` macro cannot express array return
/// types inline. Argument types are inferred by the common registry dispatch path before
/// this hook fires; arity (capped to 2 via `max_args`) is validated by the registry.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers an `explode` call by dispatching to the shared `lower_explode` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_explode(ctx, inst)
}
