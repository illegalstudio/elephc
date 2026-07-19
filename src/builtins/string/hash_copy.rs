//! Purpose:
//! Home of the PHP `hash_copy` builtin: declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - A check hook is required to record the elephc-crypto bridge requirement AND because
//!   the returned hash-context value is `PhpType::Mixed` (the same boxed runtime resource
//!   shape produced by `hash_init`).
//! - Argument types are inferred by the common registry dispatch path before the hook fires.
//! - Arity (exactly 1 arg) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "hash_copy",
    area: String,
    params: [context: Mixed],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Copies the state of an incremental hashing context.",
    php_manual: "https://www.php.net/manual/en/function.hash-copy.php",
}

/// Returns `PhpType::Mixed` for a `hash_copy` call and records the elephc-crypto bridge requirement.
///
/// `require_builtin_library` ensures the linker pulls in the hashing context copy implementation.
/// The return type is `PhpType::Mixed` because the copied context is the same boxed runtime
/// resource shape produced by `hash_init`. Arity (exactly 1 arg) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.require_builtin_library("elephc_crypto");
    Ok(PhpType::Mixed)
}

/// Lowers a `hash_copy` call by dispatching to the shared `lower_hash_copy` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_hash_copy(ctx, inst)
}
