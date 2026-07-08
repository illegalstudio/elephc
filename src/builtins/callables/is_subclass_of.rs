//! Purpose:
//! Home of the PHP `is_subclass_of` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - No check hook: the registry common path infers all arguments and returns
//!   the declared `Bool` type.
//! - `allow_string` defaults to `true` (PHP's default for `is_subclass_of`).
//! - `lower` is a thin wrapper over `types::lower_is_a_relation` parameterized
//!   with this builtin's name.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "is_subclass_of",
    area: Callables,
    params: [object_or_class: Mixed, class: Str, allow_string: Bool = DefaultSpec::Bool(true)],
    returns: Bool,
    lower: lower,
    summary: "Checks if the object has a given class as one of its parents or implements it.",
    php_manual: "function.is-subclass-of",
}

/// Lowers an `is_subclass_of` call by dispatching to the shared is-a relation emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::types::lower_is_a_relation(
        ctx,
        inst,
        "is_subclass_of",
    )
}
