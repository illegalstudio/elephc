//! Purpose:
//! Declares the internal object-allocation builtin used by PDO fetch hydration.
//!
//! Called from:
//! - The generated PDO prelude when `FETCH_CLASS` uses PHP's default hydration order.
//!
//! Key details:
//! - Allocation initializes declared property defaults but deliberately does not invoke `__construct`.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_new_without_constructor",
    area: Internal,
    params: [class: Str],
    returns: Mixed,
    lower: lower,
    summary: "Allocates a dynamically named object without invoking its constructor.",
    internal: true
}

/// Lowers the allocation through the dynamic-object backend while suppressing construction.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_elephc_new_without_constructor(ctx, inst)
}
