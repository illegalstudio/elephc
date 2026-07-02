//! Purpose:
//! Home of the PHP `hash_hmac` builtin: declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` records the elephc-crypto bridge requirement via `require_builtin_library`
//!   so the linker pulls in the HMAC implementation.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.
//! - Arity (3–4 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "hash_hmac",
    area: String,
    params: [algo: Str, data: Str, key: Str, binary: Bool = DefaultSpec::Bool(false)],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Generates a keyed hash value using the HMAC method.",
    php_manual: "https://www.php.net/manual/en/function.hash-hmac.php",
}

/// Returns `PhpType::Str` for a `hash_hmac` call and records the elephc-crypto bridge requirement.
///
/// `require_builtin_library` ensures the linker pulls in the HMAC implementation.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (3–4 args) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.require_builtin_library("elephc_crypto");
    Ok(PhpType::Str)
}

/// Lowers a `hash_hmac` call by dispatching to the shared `lower_hash_hmac` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_hash_hmac(ctx, inst)
}
