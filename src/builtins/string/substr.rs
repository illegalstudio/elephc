//! Purpose:
//! Home of the PHP `substr` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` infers argument types (side-effecting for the type environment) and
//!   returns `PhpType::Str`, mirroring the legacy arm removed from `strings::check_builtin`.
//! - `lower` is a thin wrapper over the shared per-arch `lower_substr` emitters.
//! - Arity is validated by the registry's `check_arity` before the check hook fires;
//!   the inline arity check from the legacy arm is therefore not reproduced here.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
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

/// Infers argument types and returns `PhpType::Str` for a `substr` call.
///
/// Reproduces the return-type and type-inference logic from the legacy
/// `"substr"` arm in `strings::check_builtin`. Arity is pre-validated by the
/// registry before this hook fires, so no inline count check is needed.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    for arg in cx.args {
        cx.checker.infer_type(arg, cx.env)?;
    }
    Ok(PhpType::Str)
}

/// Lowers a `substr` call by dispatching to the shared per-arch emitters.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_substr(ctx, inst)
}
