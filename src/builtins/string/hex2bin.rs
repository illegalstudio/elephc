//! Purpose:
//! Home of the PHP `hex2bin` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `hex2bin` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the shared `lower_unary_string_runtime` emitter,
//!   passing the `__rt_hex2bin` runtime helper.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "hex2bin",
    area: String,
    params: [string: Str],
    returns: Str,
    lower: lower,
    summary: "Decodes a hexadecimal string back into its binary representation.",
    php_manual: "https://www.php.net/manual/en/function.hex2bin.php",
}

/// Lowers a `hex2bin` call by dispatching to the shared per-arch unary string runtime.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_unary_string_runtime(
        ctx,
        inst,
        "hex2bin",
        "__rt_hex2bin",
    )
}
