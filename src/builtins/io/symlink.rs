//! Purpose:
//! Home of the PHP `symlink` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `symlink` is a pure-data builtin whose `Bool` return type is
//!   fully determined by its declaration. The registry common path infers the
//!   arguments and enforces the exactly-2-argument arity before falling back to
//!   `returns`.
//! - `lower` is a thin wrapper over `io::lower_symlink` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "symlink",
    area: Io,
    params: [target: Str, link: Str],
    returns: Bool,
    lower: lower,
    summary: "Creates a symbolic link.",
    php_manual: "function.symlink",
}

/// Lowers a `symlink` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_symlink(ctx, inst)
}
