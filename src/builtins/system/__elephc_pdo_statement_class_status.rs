//! Purpose:
//! Declares the internal PDO statement-class validator used by the generated PDO prelude.
//!
//! Called from:
//! - `PDO::setAttribute()` and `PDO::prepare()` for `PDO::ATTR_STATEMENT_CLASS`.
//!
//! Key details:
//! - The integer result distinguishes unknown classes, wrong ancestry, public constructors,
//!   concrete valid classes, and abstract valid classes from the AOT class table.
//! - `internal: true` keeps this compiler primitive out of PHP-visible builtin catalogs.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_pdo_statement_class_status",
    area: Internal,
    params: [class: Str],
    returns: Int,
    lower: lower,
    summary: "Classifies a dynamically named class for PDO statement construction.",
    internal: true
}

/// Lowers PDO statement-class validation through the AOT class metadata table.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_elephc_pdo_statement_class_status(ctx, inst)
}
