//! Purpose:
//! Home of the PHP `serialize` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `serialize` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The registry common path
//!   infers the argument and enforces arity before falling back to `returns`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "serialize",
    area: System,
    params: [value: Mixed],
    returns: Str,
    lower: lower,
    summary: "Generates a storable representation of a value.",
}

/// Lowers a `serialize` call by dispatching to the shared serialize emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::serialize::lower_serialize(ctx, inst)
}
