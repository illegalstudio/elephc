//! Purpose:
//! Home of the PHP `localtime` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `localtime` is a pure-data builtin whose return type
//!   (`Mixed`) is fully determined by its declaration. Both parameters are optional:
//!   `timestamp` defaults to -1 (current time) and `associative` defaults to `false`.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "localtime",
    area: System,
    params: [timestamp: Int = DefaultSpec::Int(-1), associative: Bool = DefaultSpec::Bool(false)],
    returns: Mixed,
    lower: lower,
    summary: "Returns the local time.",
}

/// Lowers a `localtime` call by dispatching to the shared system emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_localtime(ctx, inst)
}
