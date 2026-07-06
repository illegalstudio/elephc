//! Purpose:
//! Home of the PHP `str_ends_with` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `str_ends_with` is a pure-data builtin whose
//!   return type (`Bool`) is fully determined by its declaration. The registry
//!   derives the return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over `lower_binary_string_runtime` which dispatches
//!   to the shared `__rt_str_ends_with` runtime helper.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "str_ends_with",
    area: String,
    params: [haystack: Str, needle: Str],
    returns: Bool,
    lower: lower,
    summary: "Checks if a string ends with a given substring.",
    php_manual: "https://www.php.net/manual/en/function.str-ends-with.php",
}

/// Lowers a `str_ends_with` call by dispatching to the shared binary-string runtime helper.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_binary_string_runtime(
        ctx,
        inst,
        "str_ends_with",
        "__rt_str_ends_with",
    )
}
