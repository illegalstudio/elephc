//! Purpose:
//! Home of the PHP `fsockopen` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that `error_code` (arg[2]) and `error_message` (arg[3]), if provided,
//!   are plain variables (they are written by reference). Returns `Union(stream_resource, Bool)`.
//! - Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` dispatches to `io::lower_fsockopen` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "fsockopen",
    area: Io,
    params: [
        hostname: Str,
        port: Int,
        ref error_code: Mixed = DefaultSpec::Null,
        ref error_message: Mixed = DefaultSpec::Null,
        timeout: Mixed = DefaultSpec::Null
    ],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Open Internet or Unix domain socket connection.",
    php_manual: "function.fsockopen",
}

/// Validates ref output params are plain variables, then returns `Union(stream_resource, Bool)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if let Some(ec) = cx.args.get(2) {
        if !matches!(ec.kind, ExprKind::Variable(_)) {
            return Err(CompileError::new(
                ec.span,
                &format!("{}() parameter $error_code must be passed a variable", cx.name),
            ));
        }
    }
    if let Some(em) = cx.args.get(3) {
        if !matches!(em.kind, ExprKind::Variable(_)) {
            return Err(CompileError::new(
                em.span,
                &format!("{}() parameter $error_message must be passed a variable", cx.name),
            ));
        }
    }
    Ok(cx.checker.normalize_union_type(vec![PhpType::stream_resource(), PhpType::Bool]))
}

/// Lowers a `fsockopen` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_fsockopen(ctx, inst)
}
