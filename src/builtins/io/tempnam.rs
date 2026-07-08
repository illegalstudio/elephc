//! Purpose:
//! Home of the PHP `tempnam` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook: `tempnam` is a pure-data builtin whose `Str` return type is
//!   fully determined by its declaration. The registry common path infers the
//!   arguments and enforces the exactly-2-argument arity before falling back to
//!   `returns`.
//! - `lower` is a thin wrapper over `io::lower_tempnam` in the EIR backend.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "tempnam",
    area: Io,
    params: [directory: Str, prefix: Str],
    returns: Str,
    lower: lower,
    summary: "Creates a file with a unique filename.",
    php_manual: "function.tempnam",
}

/// Lowers a `tempnam` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::io::lower_tempnam(ctx, inst)
}
