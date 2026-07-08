//! Purpose:
//! Home of the PHP `rmdir` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `rmdir` is a pure-data builtin whose `Bool` return type is
//!   fully determined by its declaration. Unlike `unlink`, `rmdir` has no PHAR
//!   side effect, so no library-linking check hook is required. The registry
//!   common path infers the argument and enforces the exactly-1-argument arity
//!   before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_rmdir` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "rmdir",
    area: Io,
    params: [directory: Str],
    returns: Bool,
    lower: lower,
    summary: "Removes a directory.",
    php_manual: "function.rmdir",
}

/// Lowers a `rmdir` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_rmdir(ctx, inst)
}
