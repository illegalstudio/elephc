//! Purpose:
//! Home of the PHP `is_bool` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` dispatches to the shared static-type-predicate emitter with `PhpType::Bool`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "is_bool",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a variable is a boolean.",
    php_manual: "function.is-bool",
}

/// Lowers an `is_bool` call by dispatching to the shared static-type-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::lower_static_type_predicate(
        ctx,
        inst,
        "is_bool",
        PhpType::Bool,
    )
}
