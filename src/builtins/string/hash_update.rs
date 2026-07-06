//! Purpose:
//! Home of the PHP `hash_update` builtin: declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` records the elephc-crypto bridge requirement via `require_builtin_library`
//!   so the linker pulls in the incremental hashing context implementation.
//! - Argument types are inferred by the common registry dispatch path before the hook fires.
//! - Arity (exactly 2 args) is validated by the registry's `check_arity` before the hook fires.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "hash_update",
    area: String,
    params: [context: Mixed, data: Str],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Pumps data into an active incremental hashing context.",
    php_manual: "https://www.php.net/manual/en/function.hash-update.php",
}

/// Returns `PhpType::Bool` for a `hash_update` call and records the elephc-crypto bridge requirement.
///
/// `require_builtin_library` ensures the linker pulls in the incremental hashing context
/// implementation. Argument types are inferred by the common registry dispatch path before
/// this hook fires; arity (exactly 2 args) is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.require_builtin_library("elephc_crypto");
    Ok(PhpType::Bool)
}

/// Lowers a `hash_update` call by dispatching to the shared `lower_hash_update` emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_hash_update(ctx, inst)
}
