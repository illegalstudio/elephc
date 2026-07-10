//! Purpose:
//! Home of the PHP `touch` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` delegates to the relocated `check_touch` helper, which validates that
//!   the optional `mtime`/`atime` timestamp arguments are `int` or `null` and that
//!   `mtime` is not `null` when `atime` is provided.
//! - `arity_error` is overridden to preserve the legacy message
//!   "touch() takes 1, 2, or 3 arguments" (the registry default for a 1-required,
//!   3-max builtin produces "1 to 3 arguments").
//! - `lower` is a thin wrapper over `io::lower_touch` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::Expr;
use crate::types::checker::Checker;
use crate::types::{PhpType, TypeEnv};

builtin! {
    name: "touch",
    area: Io,
    params: [filename: Str, mtime: Int = DefaultSpec::Null, atime: Int = DefaultSpec::Null],
    arity_error: "touch() takes 1, 2, or 3 arguments",
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Sets access and modification time of a file.",
    php_manual: "function.touch",
}

/// Returns `Bool` after validating `touch()` timestamp arguments via `check_touch`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    check_touch(cx.checker, cx.args, cx.span, cx.env)
}

/// Validates `touch()` arity (1–3 args) and timestamp argument types.
/// Timestamp args must be `int` (a Unix timestamp) or `null` (omit to use current time).
///
/// # Errors
/// Returns an error if:
/// - Arity is 0 or greater than 3
/// - Any timestamp arg is neither `int` nor `null`
/// - `atime` is `null` but `mtime` is non-null (atime implies current time, so mtime cannot be set separately)
///
/// # Returns
/// `Ok(PhpType::Bool)` on success.
fn check_touch(
    checker: &mut Checker,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
    if args.is_empty() || args.len() > 3 {
        return Err(CompileError::new(span, "touch() takes 1, 2, or 3 arguments"));
    }
    checker.infer_type(&args[0], env)?;
    let mut timestamp_types = Vec::new();
    for arg in args.iter().skip(1) {
        let ty = checker.infer_type(arg, env)?;
        if !matches!(ty, PhpType::Int | PhpType::Void) {
            return Err(CompileError::new(
                arg.span,
                "touch() timestamp arguments must be int or null",
            ));
        }
        timestamp_types.push(ty);
    }
    if matches!(timestamp_types.first(), Some(PhpType::Void))
        && matches!(timestamp_types.get(1), Some(ty) if !matches!(ty, PhpType::Void))
    {
        return Err(CompileError::new(
            span,
            "touch() mtime cannot be null when atime is provided",
        ));
    }
    Ok(PhpType::Bool)
}

/// Lowers a `touch` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_touch(ctx, inst)
}
