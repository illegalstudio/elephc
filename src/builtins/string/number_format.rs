//! Purpose:
//! Home of the PHP `number_format` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts a required `num` float and optional `decimals`, `decimal_separator`,
//!   and `thousands_separator` params with PHP-compatible defaults.
//! - `lower` is a thin wrapper over the shared `lower_number_format` emitter.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "number_format",
    area: String,
    params: [
        num: Float,
        decimals: Int = DefaultSpec::Int(0),
        decimal_separator: Str = DefaultSpec::Str("."),
        thousands_separator: Str = DefaultSpec::Str(",")
    ],
    returns: Str,
    lower: lower,
    summary: "Formats a number with grouped thousands.",
    php_manual: "https://www.php.net/manual/en/function.number-format.php",
}

/// Lowers a `number_format` call by dispatching to the shared number-format emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_number_format(ctx, inst)
}
