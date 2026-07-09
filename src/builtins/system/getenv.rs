//! Purpose:
//! Home of the PHP `getenv` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` to reflect PHP's behaviour where `getenv`
//!   returns the value string on success or `false` if the variable is unset.
//! - `lower` is a thin wrapper over `system::lower_getenv` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "getenv",
    area: System,
    params: [name: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gets the value of an environment variable.",
}

/// Returns `Union(Str, Bool)` reflecting that `getenv` can return a string or `false`.
///
/// Infers the argument type to trigger type-environment side effects before returning
/// the normalized union type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::Bool]))
}

/// Lowers a `getenv` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_getenv(ctx, inst)
}
