//! Purpose:
//! Home of the PHP `fgets` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` calls `ensure_stream_resource` on the stream argument for validation and
//!   returns `Mixed` (reflecting PHP's `string|false` on EOF). `returns: Mixed` is used
//!   because the precise union cannot be expressed through the scalar `returns:` field.
//! - `lower` is a thin wrapper over `io::lower_fgets` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fgets",
    area: Io,
    params: [stream: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gets line from file pointer.",
    php_manual: "function.fgets",
}

/// Validates the stream argument and returns `Mixed` for the `string|false` EOF pattern.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Mixed)
}

/// Lowers an `fgets` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_fgets(ctx, inst)
}
