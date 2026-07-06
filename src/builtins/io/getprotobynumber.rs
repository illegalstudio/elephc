//! Purpose:
//! Home of the PHP `getprotobynumber` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(Str, Bool)` reflecting PHP's false-on-failure return.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar field.
//! - `lower` dispatches to `io::lower_getprotobynumber` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "getprotobynumber",
    area: Io,
    params: [protocol: Int],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gets the protocol name associated with the given protocol number.",
    php_manual: "function.getprotobynumber",
}

/// Returns `Union(Str, Bool)` reflecting PHP's false-on-failure return.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::Bool]))
}

/// Lowers a `getprotobynumber` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_getprotobynumber(ctx, inst)
}
