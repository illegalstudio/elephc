//! Purpose:
//! Home of the PHP `mb_ereg_match` builtin: declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook), both via
//!   `crate::builtins::registry`.
//!
//! Key details:
//! - `returns: Bool` expresses the return type inline; no check hook needed.
//! - `mb_ereg_match($pattern, $string)` is anchored at the START of the subject (verified vs
//!   PHP 8.5). The lowering dispatches to the shared `__rt_mb_ereg_match` regex helper, which
//!   reuses the PCRE2 engine and enforces the start-anchor via `rm_so == 0`. UTF-8/ASCII
//!   patterns are supported; the optional `$options` argument is not.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "mb_ereg_match",
    area: String,
    params: [pattern: Str, subject: Str],
    returns: Bool,
    lower: lower,
    summary: "Tests whether a regex pattern matches the beginning of a string (multibyte).",
    php_manual: "https://www.php.net/manual/en/function.mb-ereg-match.php",
}

/// Lowers an `mb_ereg_match` call by dispatching to the shared regex emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::regex::lower_mb_ereg_match(ctx, inst)
}
