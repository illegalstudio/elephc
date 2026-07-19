//! Purpose:
//! Home of the PHP `dirname` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the optional `levels` argument, when a static integer literal,
//!   is greater than or equal to 1 (PHP requirement).
//! - The registry pre-infers arguments before calling the hook; the hook does not
//!   call `infer_type` again.
//! - `lower` is a thin wrapper over `io::lower_dirname` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    name: "dirname",
    area: Io,
    params: [path: Str, levels: Int = DefaultSpec::Int(1)],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Returns a parent directory's path.",
    php_manual: "function.dirname",
}

/// Returns `Str`, rejecting static integer `levels` arguments less than 1.
///
/// The registry pre-infers arguments before calling this hook. The hook checks
/// whether the optional `levels` argument is a compile-time integer literal less
/// than 1 and emits a diagnostic if so; otherwise returns `PhpType::Str`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if matches!(
        cx.args.get(1).map(|arg| &arg.kind),
        Some(ExprKind::IntLiteral(levels)) if *levels < 1
    ) {
        return Err(CompileError::new(
            cx.span,
            "dirname() levels must be greater than or equal to 1",
        ));
    }
    Ok(PhpType::Str)
}

/// Lowers a `dirname` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_dirname(ctx, inst)
}
