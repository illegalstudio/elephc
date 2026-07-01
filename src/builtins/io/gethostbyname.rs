//! Purpose:
//! Home of the PHP `gethostbyname` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the common registry path infers the hostname argument and returns `Str`.
//! - `lower` dispatches to `io::lower_gethostbyname` in the EIR backend.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "gethostbyname",
    area: Io,
    params: [hostname: Str],
    returns: Str,
    lower: lower,
    summary: "Gets the IPv4 address corresponding to the given Internet host name.",
    php_manual: "function.gethostbyname",
}

/// Lowers a `gethostbyname` call by dispatching to the shared io emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::io::lower_gethostbyname(ctx, inst)
}
