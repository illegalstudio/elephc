//! Purpose:
//! Home of the PHP `gethostname` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers no arguments and returns `Str`.
//! - `lower` dispatches to `io::lower_gethostname` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "gethostname",
    area: Io,
    params: [],
    returns: Str,
    lower: lower,
    summary: "Gets the standard host name for the local machine.",
    php_manual: "function.gethostname",
}

/// Lowers a `gethostname` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_gethostname(ctx, inst)
}
