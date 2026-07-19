//! Purpose:
//! Home of the PHP `str_repeat` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `str_repeat` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the dedicated `lower_str_repeat` emitter in the
//!   strings lowering module.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "str_repeat",
    area: String,
    params: [string: Str, times: Int],
    returns: Str,
    lower: lower,
    summary: "Repeats a string a given number of times.",
    php_manual: "https://www.php.net/manual/en/function.str-repeat.php",
}

/// Lowers a `str_repeat` call by dispatching to the dedicated per-arch emitter.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_str_repeat(ctx, inst)
}
