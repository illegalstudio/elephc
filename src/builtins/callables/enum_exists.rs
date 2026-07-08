//! Purpose:
//! Home of the PHP `enum_exists` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook via support),
//!   and the EIR backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The check hook validates that the first argument is a string literal and the
//!   optional autoload argument is a literal bool or int (AOT constraint).
//! - Arguments are pre-inferred by the registry common path before the hook runs.
//! - `lower` is a thin wrapper over `lower_class_like_exists` parameterized with
//!   this builtin's name.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "enum_exists",
    area: Callables,
    params: [enum: Str, autoload: Bool = DefaultSpec::Bool(true)],
    returns: Bool,
    check: crate::builtins::callables::support::check_class_like_exists,
    lower: lower,
    summary: "Checks if the enum has been defined.",
    php_manual: "function.enum-exists",
}

/// Lowers an `enum_exists` call by dispatching to the shared class-like existence emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::lower_class_like_exists(ctx, inst, "enum_exists")
}
