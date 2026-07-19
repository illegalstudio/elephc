//! Purpose:
//! Home of the PHP `chop` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - `chop` is a PHP alias for `rtrim`. Both share the same signature, runtime
//!   helpers, and parameter defaults.
//! - No `check` hook is needed: `chop` is a pure-data builtin. The registry's arity
//!   check (1 required, 1 optional → 1 or 2 args) exactly matches the legacy check-arm
//!   constraint, so no additional validation is needed.
//! - `lower` is a thin wrapper over `lower_trim_like` routing to the `__rt_rtrim`
//!   family of runtime helpers.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "chop",
    area: String,
    params: [
        string: Str,
        characters: Str = crate::builtins::spec::DefaultSpec::Str(" \n\r\t\u{000b}\u{000c}\0"),
    ],
    returns: Str,
    lower: lower,
    summary: "Alias of rtrim: strips whitespace (or other characters) from the end of a string.",
    php_manual: "https://www.php.net/manual/en/function.chop.php",
}

/// Lowers a `chop` call by dispatching to `lower_trim_like` with the rtrim runtime labels.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_trim_like(
        ctx,
        inst,
        "chop",
        "__rt_rtrim",
        "__rt_rtrim_mask",
    )
}
