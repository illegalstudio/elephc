//! Purpose:
//! Home of the PHP `chown` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and requires the `user` argument to be `Int` or `Str`
//!   (a numeric UID or a user name), emitting the diagnostic at that argument's span.
//! - `lower` is a thin wrapper over `io::lower_chown` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "chown",
    area: Io,
    params: [filename: Str, user: Str],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Changes file owner.",
    php_manual: "function.chown",
}

/// Returns `Bool`, rejecting a `user` argument that is neither `Int` nor `Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let principal_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(principal_ty, PhpType::Int | PhpType::Str) {
        return Err(CompileError::new(
            cx.args[1].span,
            &format!("{}() owner/group must be int or string", cx.name),
        ));
    }
    Ok(PhpType::Bool)
}

/// Lowers a `chown` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_chown(ctx, inst)
}
