//! Purpose:
//! Home of the PHP `base64_decode` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: the legacy CHECK arm declared a `Str` return type
//!   (matching the migration golden), fully determined by this declaration. The
//!   registry derives the return type from the `returns:` field without a check hook.
//! - `lower` is a thin wrapper over the shared `lower_unary_string_runtime` emitter,
//!   passing the `__rt_base64_decode` runtime helper.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "base64_decode",
    area: String,
    params: [string: Str],
    returns: Str,
    lower: lower,
    summary: "Decodes a Base64-encoded string back into its original data.",
    php_manual: "https://www.php.net/manual/en/function.base64-decode.php",
}

/// Lowers a `base64_decode` call by dispatching to the shared per-arch unary string runtime.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_unary_string_runtime(
        ctx,
        inst,
        "base64_decode",
        "__rt_base64_decode",
    )
}
