//! Purpose:
//! Home of the PHP `strcasecmp` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `strcasecmp` is a pure-data builtin whose return
//!   type (`Int`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over `lower_binary_string_runtime` which dispatches
//!   to the shared `__rt_strcasecmp` runtime helper.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "strcasecmp",
    area: String,
    params: [string1: Str, string2: Str],
    returns: Int,
    lower: lower,
    summary: "Binary safe case-insensitive string comparison. Returns negative, zero, or positive.",
    php_manual: "https://www.php.net/manual/en/function.strcasecmp.php",
}

/// Lowers a `strcasecmp` call by dispatching to the shared binary-string runtime helper.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_binary_string_runtime(
        ctx,
        inst,
        "strcasecmp",
        "__rt_strcasecmp",
    )
}
