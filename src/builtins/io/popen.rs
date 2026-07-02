//! Purpose:
//! Home of the PHP `popen` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` to reflect PHP's false-on-failure
//!   pattern. The arguments are a command string and mode string, not resources — they
//!   are pre-inferred by the registry and no resource validation is performed.
//! - `returns: Mixed` is used because the union involves a resource type that the
//!   scalar `returns:` field cannot express.
//! - `lower` is a thin wrapper over `io::lower_popen` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "popen",
    area: Io,
    params: [command: Str, mode: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Opens process file pointer.",
    php_manual: "function.popen",
}

/// Returns `Union(stream_resource, Bool)` for the pipe open result.
///
/// The arguments are command and mode strings, not stream resources; no resource
/// validation is performed here. The common registry path pre-infers the arguments.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::Bool,
    ]))
}

/// Lowers a `popen` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_popen(ctx, inst)
}
