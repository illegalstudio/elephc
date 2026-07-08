//! Purpose:
//! Home of the internal `__elephc_strtotime_raw` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - This is an internal builtin (`internal: true`) not exposed as a PHP-visible function.
//!   It is a raw strtotime alias returning a plain integer rather than int|false.
//! - The `arity_error` override preserves the user-facing `strtotime` error message.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_strtotime_raw",
    area: System,
    params: [datetime: Str, baseTimestamp: Int = DefaultSpec::Null],
    arity_error: "strtotime() takes 1 or 2 arguments",
    returns: Int,
    lower: lower,
    summary: "Internal raw strtotime alias returning a plain integer.",
    internal: true,
}

/// Lowers an `__elephc_strtotime_raw` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_elephc_strtotime_raw(ctx, inst)
}
