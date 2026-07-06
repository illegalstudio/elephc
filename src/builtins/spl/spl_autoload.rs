//! Purpose:
//! Home of the PHP `spl_autoload` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts 1 required argument (`class`) and 1 optional argument (`file_extensions`).
//! - The AOT stub evaluates arguments for side effects and returns void.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "spl_autoload",
    area: Spl,
    params: [class: Mixed, file_extensions: Mixed = DefaultSpec::Null],
    returns: Void,
    lower: lower,
    summary: "Default implementation for __autoload().",
    php_manual: "https://www.php.net/manual/en/function.spl-autoload.php",
}

/// Lowers `spl_autoload` by evaluating arguments for side effects and returning null.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_void(
        ctx,
        inst,
        "spl_autoload",
    )
}
