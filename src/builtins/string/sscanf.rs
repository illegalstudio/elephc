//! Purpose:
//! Home of the PHP `sscanf` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - Accepts required `string` and `format` params plus a variadic `vars` list.
//! - `check` returns `PhpType::Array(Box::new(PhpType::Str))` because the macro
//!   `returns:` field cannot express a parameterized array type inline.
//! - `lower` is a thin wrapper over the shared `lower_sscanf` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "sscanf",
    area: String,
    params: [string: Str, format: Str],
    variadic: "vars",
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Parses a string according to a format.",
    php_manual: "https://www.php.net/manual/en/function.sscanf.php",
}

/// Returns `PhpType::Array(Box::new(PhpType::Str))` for a `sscanf` call.
///
/// A check hook is required because the `builtin!` macro cannot express a
/// parameterized array return type inline.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let _ = cx;
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers a `sscanf` call by dispatching to the shared sscanf emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_sscanf(ctx, inst)
}
