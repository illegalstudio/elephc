//! Purpose:
//! Home of the PHP `realpath_cache_get` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `AssocArray{Str, Mixed}` to reflect the cache map structure.
//! - `arity_error` is overridden to preserve the legacy message
//!   "realpath_cache_get() takes exactly 0 arguments" (the registry default for
//!   0-arg builtins produces "takes no arguments").
//! - `lower` is a thin wrapper over `io::lower_realpath_cache_get` in the EIR backend.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "realpath_cache_get",
    area: Io,
    params: [],
    arity_error: "realpath_cache_get() takes exactly 0 arguments",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Returns realpath cache entries.",
    php_manual: "function.realpath-cache-get",
}

/// Returns `AssocArray{Str, Mixed}` reflecting the realpath cache structure.
///
/// The registry enforces 0-argument arity via `arity_error` before calling this hook.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::Str),
        value: Box::new(PhpType::Mixed),
    })
}

/// Lowers a `realpath_cache_get` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_realpath_cache_get(ctx, inst)
}
