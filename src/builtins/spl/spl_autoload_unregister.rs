//! Purpose:
//! Home of the PHP `spl_autoload_unregister` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - The AOT stub accepts exactly one callable argument and returns `true`.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "spl_autoload_unregister",
    area: Spl,
    params: [callback: Mixed],
    returns: Bool,
    lower: lower,
    summary: "Unregister given function as __autoload() implementation.",
    php_manual: "https://www.php.net/manual/en/function.spl-autoload-unregister.php",
}

/// Lowers `spl_autoload_unregister` by evaluating the argument for side effects and returning true.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::spl::lower_spl_autoload_bool(
        ctx,
        inst,
        "spl_autoload_unregister",
    )
}
