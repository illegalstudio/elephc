//! Purpose:
//! Home of the PHP `fgetc` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Union(Str, Bool)` reflecting PHP behaviour where `fgetc` returns a
//!   single character or `false` on EOF. `returns: Mixed` is used because the union
//!   cannot be expressed through the scalar `returns:` field.
//! - `lower` is a thin wrapper over `io::lower_fgetc` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fgetc",
    area: Io,
    params: [stream: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gets a character from the given file pointer.",
    php_manual: "function.fgetc",
}

/// Validates the stream argument and returns `Union(Str, Bool)` for the EOF pattern.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Str, PhpType::Bool]))
}

/// Lowers an `fgetc` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_fgetc(ctx, inst)
}
