//! Purpose:
//! Home of the PHP `disk_total_space` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `disk_total_space` is a pure-data builtin whose return
//!   type (`Float`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_disk_total_space` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "disk_total_space",
    area: Io,
    params: [directory: Str],
    returns: Float,
    lower: lower,
    summary: "Returns the total size of a filesystem or disk partition.",
    php_manual: "function.disk-total-space",
}

/// Lowers a `disk_total_space` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_disk_total_space(ctx, inst)
}
