//! Purpose:
//! Home of the PHP `copy` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `copy` is a pure-data builtin whose `Bool` return type is
//!   fully determined by its declaration. The registry common path infers the
//!   arguments and enforces the exactly-2-argument arity before falling back to
//!   `returns`.
//! - `lower` is a thin wrapper over `io::lower_copy` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "copy",
    area: Io,
    params: [from: Str, to: Str],
    returns: Bool,
    lower: lower,
    summary: "Copies a file.",
    php_manual: "function.copy",
}

/// Lowers a `copy` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_copy(ctx, inst)
}
