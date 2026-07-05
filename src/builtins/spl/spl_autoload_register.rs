//! Purpose:
//! Home of the PHP `spl_autoload_register` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - The autoload registration is an AOT stub: all three parameters are optional
//!   and any combination of 0–3 arguments is accepted. Returns `true` always.

use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "spl_autoload_register",
    area: Spl,
    params: [
        callback: Mixed = DefaultSpec::Null,
        throw: Bool = DefaultSpec::Bool(true),
        prepend: Bool = DefaultSpec::Bool(false),
    ],
    returns: Bool,
    lower: lower,
    summary: "Register given function as __autoload() implementation.",
    php_manual: "https://www.php.net/manual/en/function.spl-autoload-register.php",
}

/// Lowers `spl_autoload_register` by evaluating arguments for side effects and returning true.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::spl::lower_spl_autoload_bool(
        ctx,
        inst,
        "spl_autoload_register",
    )
}
