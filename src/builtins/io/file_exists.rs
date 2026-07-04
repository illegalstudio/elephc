//! Purpose:
//! Home of the PHP `file_exists` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `file_exists` is a pure-data builtin whose return
//!   type (`Bool`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_file_exists` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "file_exists",
    area: Io,
    params: [filename: Str],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a file or directory exists.",
    php_manual: "function.file-exists",
}

/// Lowers a `file_exists` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_file_exists(ctx, inst)
}
