//! Purpose:
//! Home of the PHP `filemtime` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `filemtime` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_filemtime` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "filemtime",
    area: Io,
    params: [filename: Str],
    returns: Int,
    lower: lower,
    summary: "Gets file modification time.",
    php_manual: "function.filemtime",
}

/// Lowers a `filemtime` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_filemtime(ctx, inst)
}
