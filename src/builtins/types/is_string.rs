//! Purpose:
//! Home of the PHP `is_string` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), via `crate::builtins::registry`.
//!
//! Key details:
//! - Pure-data builtin with no check hook; arity and arg inference are handled by the registry common path.
//! - `lower` dispatches to the shared static-type-predicate emitter with `PhpType::Str`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "is_string",
    area: Types,
    params: [value: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Checks whether a variable is a string.",
    php_manual: "function.is-string",
}

/// Lowers an `is_string` call by dispatching to the shared static-type-predicate emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_static_type_predicate(
        ctx,
        inst,
        "is_string",
        PhpType::Str,
    )
}
