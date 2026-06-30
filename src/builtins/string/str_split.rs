//! Purpose:
//! Home of the PHP `str_split` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` returns `PhpType::Array(Box::new(PhpType::Str))`. A check hook is
//!   required because the `builtin!` macro `returns:` field only accepts a simple
//!   type identifier and cannot express `ArrayOf(Str)` inline. Argument types are
//!   inferred by the common registry dispatch path before the hook fires.
//! - Arity is validated by the registry's `check_arity` before the check hook fires;
//!   the inline arity check from the legacy arm is therefore not reproduced here.
//! - `lower` is a thin wrapper over the shared `lower_str_split` emitter.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::builtins::spec::DefaultSpec;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "str_split",
    area: String,
    params: [
        string: Str,
        length: Int = DefaultSpec::Int(1),
    ],
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Converts a string into an array of chunks of the given length.",
    php_manual: "https://www.php.net/manual/en/function.str-split.php",
}

/// Returns `PhpType::Array(Box::new(PhpType::Str))` for a `str_split` call.
///
/// A check hook is required because the `builtin!` macro cannot express array
/// return types inline. Argument types are inferred by the common registry
/// dispatch path before this hook fires; arity is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let _ = cx; // arguments already inferred; arity already checked by registry
    Ok(PhpType::Array(Box::new(PhpType::Str)))
}

/// Lowers a `str_split` call by dispatching to the shared per-arch emitter.
fn lower(
    ctx: &mut FunctionContext,
    inst: &Instruction,
) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::strings::lower_str_split(ctx, inst)
}
