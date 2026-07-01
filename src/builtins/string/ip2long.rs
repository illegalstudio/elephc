//! Purpose:
//! Home of the PHP `ip2long` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns the `int|false` union: `ip2long` returns `false` for invalid
//!   IPv4 address strings. A check hook is required because the `builtin!` macro cannot
//!   express a union return type inline.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "ip2long",
    area: String,
    params: [ip: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Converts a string containing an IPv4 address into a long integer.",
    php_manual: "https://www.php.net/manual/en/function.ip2long.php",
}

/// Returns `PhpType::Union([Int, Bool])` for an `ip2long` call.
///
/// The union return (integer on success, false on invalid input) cannot be expressed
/// inline in the `builtin!` macro so a check hook is required.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (exactly 1 arg) is pre-validated by the registry.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Union(vec![PhpType::Int, PhpType::Bool]))
}

/// Lowers an `ip2long` call by dispatching to the shared `lower_ip2long` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_ip2long(ctx, inst)
}
