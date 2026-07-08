//! Purpose:
//! Home of the PHP `get_parent_class` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the registry common path infers the optional argument and
//!   returns the declared `Str` type.
//! - `lower` is a thin wrapper over `types::lower_class_name_lookup` parameterized
//!   with this builtin's name.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "get_parent_class",
    area: Callables,
    params: [object_or_class: Mixed = DefaultSpec::Null],
    returns: Str,
    lower: lower,
    summary: "Returns the name of the parent class of an object or class.",
    php_manual: "function.get-parent-class",
}

/// Lowers a `get_parent_class` call by dispatching to the shared class-name lookup emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::types::lower_class_name_lookup(
        ctx,
        inst,
        "get_parent_class",
    )
}
