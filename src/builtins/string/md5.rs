//! Purpose:
//! Home of the PHP `md5` builtin: declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` records the elephc-crypto bridge requirement via `require_builtin_library`
//!   so the linker pulls in the MD5 implementation.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.
//! - Arity (1–2 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "md5",
    area: String,
    params: [string: Str, binary: Bool = DefaultSpec::Bool(false)],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Calculates the MD5 hash of a string.",
    php_manual: "https://www.php.net/manual/en/function.md5.php",
}

/// Returns `PhpType::Str` for an `md5` call and records the elephc-crypto bridge requirement.
///
/// `require_builtin_library` ensures the linker pulls in the MD5 implementation.
/// Argument types are inferred by the common registry dispatch path before this hook fires;
/// arity (1–2 args) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.require_builtin_library("elephc_crypto");
    Ok(PhpType::Str)
}

/// Lowers an `md5` call by dispatching to the shared `lower_md5` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_md5(ctx, inst)
}
