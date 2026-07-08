//! Purpose:
//! Home of the PHP `rawurldecode` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `rawurldecode` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the shared `lower_unary_string_runtime` emitter.
//!   Like the legacy arm, it reuses the `__rt_urldecode` runtime helper but without
//!   treating '+' as a space (RFC 3986 raw decoding is handled inside the helper path).

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "rawurldecode",
    area: String,
    params: [string: Str],
    returns: Str,
    lower: lower,
    summary: "Decodes an RFC 3986 percent-encoded string without treating '+' as a space.",
    php_manual: "https://www.php.net/manual/en/function.rawurldecode.php",
}

/// Lowers a `rawurldecode` call by dispatching to the shared per-arch unary string runtime.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_unary_string_runtime(
        ctx,
        inst,
        "rawurldecode",
        "__rt_urldecode",
    )
}
