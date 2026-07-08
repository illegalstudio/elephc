//! Purpose:
//! Home of the PHP `hrtime` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `hrtime` is a pure-data builtin whose return type
//!   (`Mixed`) is fully determined by its declaration. The `as_number` parameter
//!   is optional and defaults to `false`.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "hrtime",
    area: System,
    params: [as_number: Bool = DefaultSpec::Bool(false)],
    returns: Mixed,
    lower: lower,
    summary: "Returns the current high-resolution time.",
}

/// Lowers an `hrtime` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_hrtime(ctx, inst)
}
