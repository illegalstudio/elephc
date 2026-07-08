//! Purpose:
//! Home of the PHP `str_pad` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   all via `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts required `string` and `length` params, plus optional `pad_string`
//!   and `pad_type` params with PHP-compatible defaults.
//! - `lower` is a thin wrapper over the shared `lower_str_pad` emitter.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "str_pad",
    area: String,
    params: [
        string: Str,
        length: Int,
        pad_string: Str = DefaultSpec::Str(" "),
        pad_type: Int = DefaultSpec::Int(1)
    ],
    returns: Str,
    lower: lower,
    summary: "Pads a string to a certain length with another string.",
    php_manual: "https://www.php.net/manual/en/function.str-pad.php",
}

/// Lowers a `str_pad` call by dispatching to the shared str-pad emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_str_pad(ctx, inst)
}
