//! Purpose:
//! Home of the PHP `linkinfo` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `linkinfo` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.
//! - `lower` is a thin wrapper over `io::lower_linkinfo` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "linkinfo",
    area: Io,
    params: [path: Str],
    returns: Int,
    lower: lower,
    summary: "Gets information about a link.",
    php_manual: "function.linkinfo",
}

/// Lowers a `linkinfo` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_linkinfo(ctx, inst)
}
