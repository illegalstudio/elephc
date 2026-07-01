//! Purpose:
//! Home of the PHP `stream_context_get_default` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `PhpType::stream_resource()` which is not scalar-expressible, so
//!   `returns: Mixed` is used and the hook overrides the return type.
//! - Arguments are pre-inferred by the registry before the hook runs; the hook does NOT
//!   re-infer them.
//! - `lower` is a thin wrapper over `io::lower_stream_context_get_default` in the EIR backend.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "stream_context_get_default",
    area: Io,
    params: [options: Mixed = DefaultSpec::Null],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Retrieves the default stream context.",
    php_manual: "function.stream-context-get-default",
}

/// Returns `stream_resource()` as the precise return type for `stream_context_get_default`.
///
/// Arguments are pre-inferred by the registry; this hook only refines the return type
/// beyond what the scalar `returns: Mixed` field can express.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let _ = cx;
    Ok(PhpType::stream_resource())
}

/// Lowers a `stream_context_get_default` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_stream_context_get_default(ctx, inst)
}
