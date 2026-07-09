//! Purpose:
//! Home of the internal `__elephc_mktime_raw` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - This is an internal builtin (`internal: true`) not exposed as a PHP-visible function.
//!   It is used by the synthetic DateTime body as a raw mktime alias.
//! - The lower hook delegates to the same emitter as `mktime`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_mktime_raw",
    area: System,
    params: [hour: Int, minute: Int, second: Int, month: Int, day: Int, year: Int],
    returns: Int,
    lower: lower,
    summary: "Internal raw mktime alias used by the synthetic DateTime body.",
    internal: true,
}

/// Lowers an `__elephc_mktime_raw` call by delegating to the shared mktime emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_mktime(ctx, inst)
}
