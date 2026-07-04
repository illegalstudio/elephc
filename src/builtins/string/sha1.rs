//! Purpose:
//! Home of the PHP `sha1` builtin: declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` records the elephc-crypto bridge requirement via `require_builtin_library`
//!   so the linker pulls in the SHA-1 implementation.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.
//! - Arity (1–2 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "sha1",
    area: String,
    params: [string: Str, binary: Bool = DefaultSpec::Bool(false)],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Calculates the SHA-1 hash of a string.",
    php_manual: "https://www.php.net/manual/en/function.sha1.php",
}

/// Returns `PhpType::Str` for a `sha1` call and records the elephc-crypto bridge requirement.
///
/// `require_builtin_library` ensures the linker pulls in the SHA-1 implementation.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (1–2 args) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.require_builtin_library("elephc_crypto");
    Ok(PhpType::Str)
}

/// Lowers a `sha1` call by dispatching to the shared `lower_sha1` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_sha1(ctx, inst)
}
