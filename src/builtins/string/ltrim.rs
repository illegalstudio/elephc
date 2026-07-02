//! Purpose:
//! Home of the PHP `ltrim` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `ltrim` is a pure-data builtin. The registry's arity
//!   check (1 required, 1 optional → 1 or 2 args) exactly matches the legacy check-arm
//!   constraint, so no additional validation is needed.
//! - `lower` is a thin wrapper over `lower_trim_like` which dispatches to the appropriate
//!   runtime helper depending on whether a mask argument is provided.

use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "ltrim",
    area: String,
    params: [
        string: Str,
        characters: Str = crate::builtins::spec::DefaultSpec::Str(" \n\r\t\u{000b}\u{000c}\0"),
    ],
    returns: Str,
    lower: lower,
    summary: "Strips whitespace (or other characters) from the beginning of a string.",
    php_manual: "https://www.php.net/manual/en/function.ltrim.php",
}

/// Lowers an `ltrim` call by dispatching to `lower_trim_like` with the default and mask runtime labels.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_trim_like(
        ctx,
        inst,
        "ltrim",
        "__rt_ltrim",
        "__rt_ltrim_mask",
    )
}
