//! Purpose:
//! Home of the PHP `realpath` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` to reflect PHP's behaviour where `realpath`
//!   returns the resolved path on success or `false` if the path cannot be resolved.
//! - The registry pre-infers arguments before calling this hook.
//! - `lower` is a thin wrapper over `io::lower_realpath` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "realpath",
    area: Io,
    params: [path: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns canonicalized absolute pathname.",
    php_manual: "function.realpath",
}

/// Returns `Union(Str, Bool)` reflecting that `realpath` can return a path or `false`.
///
/// The registry pre-infers arguments before calling this hook.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::Bool]))
}

/// Lowers a `realpath` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_realpath(ctx, inst)
}
