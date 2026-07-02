//! Purpose:
//! Home of the PHP `chmod` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Bool` and requires the `permissions` argument to be `Int`,
//!   emitting the diagnostic at the mode argument's span.
//! - `lower` is a thin wrapper over `io::lower_chmod` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "chmod",
    area: Io,
    params: [filename: Str, permissions: Int],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Changes file mode.",
    php_manual: "function.chmod",
}

/// Returns `Bool`, rejecting a non-`Int` `permissions` argument at its own span.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let mode_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if mode_ty != PhpType::Int {
        return Err(CompileError::new(cx.args[1].span, "chmod() mode must be int"));
    }
    Ok(PhpType::Bool)
}

/// Lowers a `chmod` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_chmod(ctx, inst)
}
