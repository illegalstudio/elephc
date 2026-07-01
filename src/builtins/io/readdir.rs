//! Purpose:
//! Home of the PHP `readdir` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `dir_handle` argument is a stream resource and returns
//!   `Union(Str, Bool)` to reflect PHP's false-on-failure pattern.
//! - `returns: Mixed` is used because the union cannot be expressed through the scalar
//!   `returns:` field. Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` is a thin wrapper over `io::lower_readdir` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "readdir",
    area: Io,
    params: [dir_handle: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Read entry from directory handle.",
    php_manual: "function.readdir",
}

/// Validates the directory handle is a stream resource and returns `Union(Str, Bool)`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::Str,
        PhpType::Bool,
    ]))
}

/// Lowers a `readdir` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_readdir(ctx, inst)
}
