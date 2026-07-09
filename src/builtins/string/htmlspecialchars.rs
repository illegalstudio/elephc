//! Purpose:
//! Home of the PHP `htmlspecialchars` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `htmlspecialchars` is a pure-data builtin whose
//!   return type (`Str`) is fully determined by its declaration. The registry derives
//!   the return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the shared `lower_html_escape` emitter,
//!   passing the builtin name for diagnostics; the runtime helper is
//!   `__rt_htmlspecialchars`.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "htmlspecialchars",
    area: String,
    params: [string: Str, flags: Int = DefaultSpec::Int(11), encoding: Str = DefaultSpec::Str("UTF-8")],
    returns: Str,
    lower: lower,
    summary: "Converts the HTML special characters in a string into their entities.",
    php_manual: "https://www.php.net/manual/en/function.htmlspecialchars.php",
}

/// Lowers a `htmlspecialchars` call. The optional flags/encoding arguments are accepted but not
/// yet applied (the runtime uses ENT_QUOTES behaviour, matching PHP's default and ENT_QUOTES).
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_html_escape(ctx, inst, "htmlspecialchars")
}
