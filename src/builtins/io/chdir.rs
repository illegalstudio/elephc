//! Purpose:
//! Home of the PHP `chdir` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `chdir` is a pure-data builtin whose `Bool` return type is
//!   fully determined by its declaration. Unlike `unlink`, `chdir` has no PHAR
//!   side effect, so no library-linking check hook is required. The registry
//!   common path infers the argument and enforces the exactly-1-argument arity
//!   before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_chdir` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "chdir",
    area: Io,
    params: [directory: Str],
    returns: Bool,
    lower: lower,
    summary: "Changes the current directory.",
    php_manual: "function.chdir",
}

/// Lowers a `chdir` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_chdir(ctx, inst)
}
