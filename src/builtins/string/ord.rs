//! Purpose:
//! Home of the PHP `ord` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `ord` is a pure-data builtin whose return type
//!   (`Int`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the dedicated `lower_ord` emitter in the
//!   strings lowering module.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "ord",
    area: String,
    params: [character: Str],
    returns: Int,
    lower: lower,
    summary: "Returns the ASCII value of the first character of a string.",
    php_manual: "https://www.php.net/manual/en/function.ord.php",
}

/// Lowers an `ord` call by dispatching to the shared per-arch emitter.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_ord(ctx, inst)
}
