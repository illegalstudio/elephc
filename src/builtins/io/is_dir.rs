//! Purpose:
//! Home of the PHP `is_dir` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `is_dir` is a pure-data builtin whose return type
//!   (`Bool`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_is_dir` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "is_dir",
    area: Io,
    params: [filename: Str],
    returns: Bool,
    lower: lower,
    summary: "Tells whether the filename is a directory.",
    php_manual: "function.is-dir",
}

/// Lowers an `is_dir` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_is_dir(ctx, inst)
}
