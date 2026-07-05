//! Purpose:
//! Home of the PHP `fgetcsv` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the `stream` argument is a stream resource and returns `Array<Str>`.
//! - `returns: Mixed` is used because the array type cannot be expressed through the
//!   scalar `returns:` field. Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` is a thin wrapper over `io::lower_fgetcsv` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "fgetcsv",
    area: Io,
    params: [stream: Mixed, length: Int = DefaultSpec::Null, separator: Str = DefaultSpec::Str(",")],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Gets line from file pointer and parse for CSV fields.",
    php_manual: "function.fgetcsv",
}

/// Validates the stream argument is a stream resource and returns `Array<Str>`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers a `fgetcsv` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_fgetcsv(ctx, inst)
}
