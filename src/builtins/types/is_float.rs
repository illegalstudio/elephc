//! Purpose:
//! Home of the PHP `is_float` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` dispatches to the shared static-type-predicate emitter with `PhpType::Float`.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "is_float",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a variable is a floating-point number.",
    php_manual: "function.is-float",
}

/// Lowers an `is_float` call by dispatching to the shared static-type-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::lower_static_type_predicate(
        ctx,
        inst,
        "is_float",
        PhpType::Float,
    )
}
