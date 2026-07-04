//! Purpose:
//! Home of the PHP `chgrp` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and requires the `group` argument to be `Int` or `Str`
//!   (a numeric GID or a group name), emitting the diagnostic at that argument's span.
//! - `lower` is a thin wrapper over `io::lower_chgrp` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "chgrp",
    area: Io,
    params: [filename: Str, group: Str],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Changes file group.",
    php_manual: "function.chgrp",
}

/// Returns `Bool`, rejecting a `group` argument that is neither `Int` nor `Str`.
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

/// Lowers a `chgrp` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_chgrp(ctx, inst)
}
