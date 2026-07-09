//! Purpose:
//! Home of the PHP `hash` builtin: declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` records the elephc-crypto bridge requirement via `require_builtin_library`
//!   so the linker pulls in the full algorithm set (raw $binary output, catchable ValueError).
//! - Argument types are inferred by the common registry dispatch path before the hook fires.
//! - Arity (2–3 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "hash",
    area: String,
    params: [algo: Str, data: Str, binary: Bool = DefaultSpec::Bool(false)],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Generates a hash value using the given algorithm.",
    php_manual: "https://www.php.net/manual/en/function.hash.php",
}

/// Returns `PhpType::Str` for a `hash` call and records the elephc-crypto bridge requirement.
///
/// `require_builtin_library` ensures the linker pulls in the full hash algorithm set.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (2–3 args) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.require_builtin_library("elephc_crypto");
    Ok(PhpType::Str)
}

/// Lowers a `hash` call by dispatching to the shared `lower_hash` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_hash(ctx, inst)
}
