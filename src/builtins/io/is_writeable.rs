//! Purpose:
//! Home of the PHP `is_writeable` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `is_writeable` is a pure-data builtin whose return
//!   type (`Bool`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `is_writeable` is an alias for `is_writable`; both share the same lowering.
//! - `lower` is a thin wrapper over `io::lower_is_writeable` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "is_writeable",
    area: Io,
    params: [filename: Str],
    returns: Bool,
    lower: lower,
    summary: "Tells whether the filename is writable (alias of is_writable).",
    php_manual: "function.is-writable",
}

/// Lowers an `is_writeable` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_is_writeable(ctx, inst)
}
