//! Purpose:
//! Home of the PHP `realpath_cache_size` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `realpath_cache_size` is a pure-data builtin whose
//!   return type (`Int`) is fully determined by its declaration.
//! - `arity_error` is overridden to preserve the legacy message
//!   "realpath_cache_size() takes exactly 0 arguments" (the registry default for
//!   0-arg builtins produces "takes no arguments").
//! - `lower` is a thin wrapper over `io::lower_realpath_cache_size` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "realpath_cache_size",
    area: Io,
    params: [],
    arity_error: "realpath_cache_size() takes exactly 0 arguments",
    returns: Int,
    lower: lower,
    summary: "Returns the amount of memory used by the realpath cache.",
    php_manual: "function.realpath-cache-size",
}

/// Lowers a `realpath_cache_size` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_realpath_cache_size(ctx, inst)
}
