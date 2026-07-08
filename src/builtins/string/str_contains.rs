//! Purpose:
//! Home of the PHP `str_contains` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `str_contains` is a pure-data builtin whose return
//!   type (`Bool`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over `lower_str_contains` which uses `__rt_strpos`
//!   and normalizes its signed result to a PHP boolean.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "str_contains",
    area: String,
    params: [haystack: Str, needle: Str],
    returns: Bool,
    lower: lower,
    summary: "Determines if a string contains a given substring.",
    php_manual: "https://www.php.net/manual/en/function.str-contains.php",
}

/// Lowers a `str_contains` call by dispatching to the dedicated strpos-based emitter.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_str_contains(ctx, inst)
}
