//! Purpose:
//! Home of the PHP `opendir` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `Union(stream_resource, Bool)` to reflect PHP's false-on-failure
//!   pattern. The `directory` argument is a path string, not a resource — it is
//!   pre-inferred by the registry and no resource validation is performed.
//! - `returns: Mixed` is used because the union involves a resource type that the
//!   scalar `returns:` field cannot express.
//! - `lower` is a thin wrapper over `io::lower_opendir` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "opendir",
    area: Io,
    params: [directory: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Open directory handle.",
    php_manual: "function.opendir",
}

/// Returns `Union(stream_resource, Bool)` for the directory open result.
///
/// The `directory` argument is a path string, not a stream resource; no resource
/// validation is performed here. The common registry path pre-infers the argument.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(cx.checker.normalize_union_type(vec![
        PhpType::stream_resource(),
        PhpType::Bool,
    ]))
}

/// Lowers an `opendir` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_opendir(ctx, inst)
}
