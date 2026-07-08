//! Purpose:
//! Home of the PHP `long2ip` builtin: its declaration and lowering.
//!
//! Called from:
//! - The builtin registry (declaration) and the EIR backend (lower hook),
//!   both via `crate::builtins::registry`.
//!
//! Key details:
//! - No `check` hook is needed: `long2ip` is a pure-data builtin whose return type
//!   (`Str`) is fully determined by its declaration. The registry derives the
//!   return type from the `returns:` field without calling a check hook.
//! - `lower` is a thin wrapper over the dedicated `lower_long2ip` emitter in the
//!   strings lowering module.

use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::ir::Instruction;

builtin! {
    name: "long2ip",
    area: String,
    params: [ip: Int],
    returns: Str,
    lower: lower,
    summary: "Converts an IPv4 address from long integer to dotted string notation.",
    php_manual: "https://www.php.net/manual/en/function.long2ip.php",
}

/// Lowers a `long2ip` call by dispatching to the shared `lower_long2ip` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_long2ip(ctx, inst)
}
