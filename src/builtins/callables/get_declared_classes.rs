//! Purpose:
//! Home of the PHP `get_declared_classes` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook via support),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - Check hook returns `Array<Str>` unconditionally (zero-arg builtin).
//! - `lower` is a thin wrapper over `types::lower_get_declared_names` parameterized
//!   with this builtin's name.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "get_declared_classes",
    area: Callables,
    params: [],
    returns: Mixed,
    check: crate::builtins::callables::support::check_declared_names,
    lower: lower,
    summary: "Returns an array of the names of the defined classes.",
    php_manual: "function.get-declared-classes",
}

/// Lowers a `get_declared_classes` call by dispatching to the shared declared-names emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::types::lower_get_declared_names(
        ctx,
        inst,
        "get_declared_classes",
    )
}
