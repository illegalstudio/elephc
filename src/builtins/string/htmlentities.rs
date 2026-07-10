//! Purpose:
//! Home of the PHP `htmlentities` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `htmlentities` is a pure-data builtin whose return
//!   type (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the shared `lower_html_escape` emitter,
//!   passing the builtin name for diagnostics. It reuses the
//!   `__rt_htmlspecialchars` runtime helper.

use crate::builtins::spec::DefaultSpec;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "htmlentities",
    area: String,
    params: [string: Str, flags: Int = DefaultSpec::Int(11), encoding: Str = DefaultSpec::Str("UTF-8")],
    returns: Str,
    lower: lower,
    summary: "Converts all applicable characters in a string into their HTML entities.",
    php_manual: "https://www.php.net/manual/en/function.htmlentities.php",
}

/// Lowers a `htmlentities` call. The optional flags/encoding arguments are accepted but not yet
/// applied (reuses the ENT_QUOTES-behaviour `__rt_htmlspecialchars` runtime).
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_html_escape(ctx, inst, "htmlentities")
}
