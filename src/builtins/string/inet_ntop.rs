//! Purpose:
//! Home of the PHP `inet_ntop` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns the `string|false` union: `inet_ntop` returns `false` for invalid
//!   packed IP addresses. A check hook is required because the `builtin!` macro cannot
//!   express a union return type inline.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "inet_ntop",
    area: String,
    params: [ip: Str],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Converts a packed internet address to a human-readable representation.",
    php_manual: "https://www.php.net/manual/en/function.inet-ntop.php",
}

/// Returns `PhpType::Union([Str, Bool])` for an `inet_ntop` call.
///
/// The union return (string on success, false on invalid input) cannot be expressed
/// inline in the `builtin!` macro so a check hook is required.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (exactly 1 arg) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let _ = cx;
    Ok(PhpType::Union(vec![PhpType::Str, PhpType::Bool]))
}

/// Lowers an `inet_ntop` call by dispatching to the shared `lower_inet` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_inet(
        ctx,
        inst,
        "inet_ntop",
        "__rt_inet_ntop",
    )
}
