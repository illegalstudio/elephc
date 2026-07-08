//! Purpose:
//! Home of the PHP `basename` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `basename` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The registry common path
//!   infers arguments and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_basename` in the EIR backend.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "basename",
    area: Io,
    params: [path: Str, suffix: Str = DefaultSpec::Str("")],
    returns: Str,
    lower: lower,
    summary: "Returns the trailing name component of a path.",
    php_manual: "function.basename",
}

/// Lowers a `basename` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_basename(ctx, inst)
}
