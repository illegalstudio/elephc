//! Purpose:
//! Home of the PHP `class_parents` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook via support),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `lazy_check: true` so the hook infers each argument exactly once in source order,
//!   matching the legacy arm.
//! - The check hook validates that the first argument is an object or string literal
//!   and that the optional autoload arg is a literal bool or int.
//! - `lower` is a thin wrapper over `class_relations::lower_class_relation` parameterized
//!   with this builtin's name.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "class_parents",
    area: Callables,
    params: [object_or_class: Mixed, autoload: Bool = DefaultSpec::Bool(true)],
    returns: Mixed,
    check: crate::builtins::callables::support::check_class_relation,
    lazy_check: true,
    lower: lower,
    summary: "Returns the parent classes of the given class.",
    php_manual: "function.class-parents",
}

/// Lowers a `class_parents` call by dispatching to the shared class-relation emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::class_relations::lower_class_relation(
        ctx,
        inst,
        "class_parents",
    )
}
