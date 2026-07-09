//! Purpose:
//! Home of the PHP `spl_autoload_call` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - The AOT stub accepts exactly one class-name argument and returns void.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "spl_autoload_call",
    area: Spl,
    params: [class: Mixed],
    returns: Void,
    lower: lower,
    summary: "Try all registered __autoload() functions to load the requested class.",
    php_manual: "https://www.php.net/manual/en/function.spl-autoload-call.php",
}

/// Lowers `spl_autoload_call` by evaluating the argument for side effects and returning null.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_void(
        ctx,
        inst,
        "spl_autoload_call",
    )
}
