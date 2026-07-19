//! Purpose:
//! Home of the PHP `fseek` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Union(Int, Bool)`. `returns: Mixed` is used because the union cannot be
//!   expressed through the scalar `returns:` field. Arguments are pre-inferred by the
//!   registry before the hook runs.
//! - `lower` is a thin wrapper over `io::lower_fseek` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fseek",
    area: Io,
    params: [stream: Mixed, offset: Int, whence: Int = DefaultSpec::Int(0)],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Seeks on a file pointer.",
    php_manual: "function.fseek",
}

/// Validates the stream argument and returns `Union(Int, Bool)` for the seek result.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(cx.checker.normalize_union_type(vec![PhpType::Int, PhpType::False]))
}

/// Lowers an `fseek` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_fseek(ctx, inst)
}
