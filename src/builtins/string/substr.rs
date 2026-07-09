//! Purpose:
//! Home of the PHP `substr` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `PhpType::Str`. Argument type inference happens in the common
//!   registry dispatch path (`check_builtin` in `src/types/checker/builtins/mod.rs`)
//!   before the hook fires, so the hook does not need to call `infer_type` again.
//! - `lower` is a thin wrapper over the shared per-arch `lower_substr` emitters.
//! - Arity is validated by the registry's `check_arity` before the check hook fires;
//!   the inline arity check from the legacy arm is therefore not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "substr",
    area: String,
    params: [string: Str, offset: Int, length: Int = crate::builtins::spec::DefaultSpec::Null],
    returns: Str,
    check: check,
    lower: lower,
    summary: "Returns a portion of a string specified by the offset and length.",
    php_manual: "https://www.php.net/manual/en/function.substr.php",
}

/// Returns `PhpType::Str` for a `substr` call.
///
/// Argument types are inferred by the common registry dispatch path in
/// `check_builtin` before this hook fires; the hook only needs to return
/// the correct return type. Arity is pre-validated by the registry before
/// the hook fires, so no inline count check is needed.
fn check(_cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    Ok(PhpType::Str)
}

/// Lowers a `substr` call by dispatching to the shared per-arch emitters.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::strings::lower_substr(ctx, inst)
}
