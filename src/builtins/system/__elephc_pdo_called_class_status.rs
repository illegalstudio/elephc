//! Purpose:
//! Declares the internal late-static PDO factory class classifier.
//!
//! Called from:
//! - `PDO::connect()` in the generated PHP 8.4+ PDO prelude.
//!
//! Key details:
//! - The result distinguishes base PDO, each driver hierarchy, and generic PDO subclasses.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_pdo_called_class_status",
    area: Internal,
    params: [class: Str],
    returns: Int,
    lower: lower,
    summary: "Classifies PDO::connect's late-static called class by driver hierarchy.",
    internal: true
}

/// Lowers the late-static PDO factory classifier through AOT class metadata.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_elephc_pdo_called_class_status(ctx, inst)
}
