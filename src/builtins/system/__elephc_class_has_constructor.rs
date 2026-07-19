//! Purpose:
//! Declares the internal dynamic constructor-existence predicate used by PDO hydration.
//!
//! Called from:
//! - The generated PDO prelude after allocating and hydrating a `FETCH_CLASS` object.
//!
//! Key details:
//! - The result is derived from the AOT class table and includes inherited constructors.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_class_has_constructor",
    area: Internal,
    params: [class: Str],
    returns: Bool,
    lower: lower,
    summary: "Reports whether a dynamically named AOT class has a constructor.",
    internal: true
}

/// Lowers the predicate through the dynamic class-name table in the object backend.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_elephc_class_has_constructor(ctx, inst)
}
