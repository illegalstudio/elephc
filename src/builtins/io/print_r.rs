//! Purpose:
//! Home of the PHP `print_r` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `print_r` is a pure-data builtin whose return type
//!   (`Void`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `debug::lower_print_r` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "print_r",
    area: Io,
    params: [value: Mixed],
    returns: Void,
    lower: lower,
    summary: "Prints human-readable information about a variable.",
    php_manual: "function.print-r",
}

/// Lowers a `print_r` call by dispatching to the shared debug emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::debug::lower_print_r(ctx, inst)
}
