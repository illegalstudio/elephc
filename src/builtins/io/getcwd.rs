//! Purpose:
//! Home of the PHP `getcwd` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `getcwd` is a pure-data builtin whose `Str` return type is
//!   fully determined by its declaration. The registry common path enforces its
//!   0-argument arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_getcwd` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "getcwd",
    area: Io,
    params: [],
    returns: Str,
    lower: lower,
    summary: "Gets the current working directory.",
    php_manual: "function.getcwd",
}

/// Lowers a `getcwd` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_getcwd(ctx, inst)
}
