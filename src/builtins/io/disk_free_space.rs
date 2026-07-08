//! Purpose:
//! Home of the PHP `disk_free_space` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `disk_free_space` is a pure-data builtin whose return
//!   type (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_disk_free_space` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "disk_free_space",
    area: Io,
    params: [directory: Str],
    returns: Float,
    lower: lower,
    summary: "Returns available space on filesystem or disk partition.",
    php_manual: "function.disk-free-space",
}

/// Lowers a `disk_free_space` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_disk_free_space(ctx, inst)
}
