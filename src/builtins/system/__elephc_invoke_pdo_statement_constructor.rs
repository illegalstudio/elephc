//! Purpose:
//! Declares the internal constructor invoker used for PDO custom statement classes.
//!
//! Called from:
//! - `PDO::prepare()` after native PDOStatement fields have been initialized.
//!
//! Key details:
//! - Dispatch uses AOT class metadata and deliberately bypasses userland visibility, matching
//!   php-src's internal call of protected/private PDOStatement subclass constructors.
//! - Constructor arguments remain a boxed runtime container so named arguments are preserved.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "__elephc_invoke_pdo_statement_constructor",
    area: Internal,
    params: [class: Str, statement: Mixed, arguments: Mixed],
    returns: Void,
    lower: lower,
    summary: "Invokes a PDO statement subclass constructor after native initialization.",
    internal: true
}

/// Lowers the internal PDO constructor invocation through dynamic class dispatch.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::system::lower_elephc_invoke_pdo_statement_constructor(ctx, inst)
}
