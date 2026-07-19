//! Purpose:
//! Home of the PHP `base64_encode` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `base64_encode` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the shared `lower_unary_string_runtime` emitter,
//!   passing the `__rt_base64_encode` runtime helper.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "base64_encode",
    area: String,
    params: [string: Str],
    returns: Str,
    lower: lower,
    summary: "Encodes binary data into a Base64 string.",
    php_manual: "https://www.php.net/manual/en/function.base64-encode.php",
}

/// Lowers a `base64_encode` call by dispatching to the shared per-arch unary string runtime.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_unary_string_runtime(
        ctx,
        inst,
        "base64_encode",
        "__rt_base64_encode",
    )
}
