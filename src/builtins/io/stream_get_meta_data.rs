//! Purpose:
//! Home of the PHP `stream_get_meta_data` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the stream resource and returns `AssocArray{Str, Mixed}`, which is not
//!   scalar-expressible, so `returns: Mixed` is used and the hook overrides the return type.
//! - Arguments are pre-inferred by the registry before the hook runs.
//! - `lower` is a thin wrapper over `io::lower_stream_get_meta_data` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stream_get_meta_data",
    area: Io,
    params: [stream: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Retrieves metadata from streams/file pointers.",
    php_manual: "function.stream-get-meta-data",
}

/// Validates the stream resource and returns `AssocArray{Str, Mixed}`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    crate::types::checker::builtins::io::common::ensure_stream_resource(
        cx.checker,
        cx.name,
        &cx.args[0],
        cx.env,
    )?;
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}

/// Lowers a `stream_get_meta_data` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_stream_get_meta_data(ctx, inst)
}
