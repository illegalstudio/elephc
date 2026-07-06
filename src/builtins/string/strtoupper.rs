//! Purpose:
//! Home of the PHP `strtoupper` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `strtoupper` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the shared `lower_unary_string_runtime` emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "strtoupper",
    area: String,
    params: [string: Str],
    returns: Str,
    lower: lower,
    summary: "Converts a string to uppercase.",
    php_manual: "https://www.php.net/manual/en/function.strtoupper.php",
}

/// Lowers a `strtoupper` call by dispatching to the shared per-arch unary string runtime.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_unary_string_runtime(
        ctx,
        inst,
        "strtoupper",
        "__rt_strtoupper",
    )
}
