//! Purpose:
//! Home of the PHP `is_int` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` dispatches to the shared static-type-predicate emitter with `PhpType::Int`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "is_int",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a variable is an integer.",
    php_manual: "function.is-int",
}

/// Lowers an `is_int` call by dispatching to the shared static-type-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_static_type_predicate(
        ctx,
        inst,
        "is_int",
        PhpType::Int,
    )
}
