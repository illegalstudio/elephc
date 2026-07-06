//! Purpose:
//! Home of the PHP `chr` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `chr` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The registry derives the return
//!   type from the `returns:` field without calling a check hook.
//! - The parameter is named `codepoint` (matching the parity golden) and typed `Int`,
//!   reflecting PHP's `chr(int $codepoint): string`. The dedicated `lower_chr` emitter
//!   coerces the operand to an integer via `load_as_int`, so the declared `Int` type
//!   is consistent with the existing lowering.
//! - `lower` is a thin wrapper over the dedicated `lower_chr` emitter.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "chr",
    area: String,
    params: [codepoint: Int],
    returns: Str,
    lower: lower,
    summary: "Returns a one-character string from the given byte code point.",
    php_manual: "https://www.php.net/manual/en/function.chr.php",
}

/// Lowers a `chr` call by dispatching to the dedicated per-arch `lower_chr` emitter.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_chr(ctx, inst)
}
