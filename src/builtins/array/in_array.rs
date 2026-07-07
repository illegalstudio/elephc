//! Purpose:
//! Home of the PHP `in_array` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the second argument is an array and returns `Bool`.
//! - The optional `strict` (3rd) argument selects PHP `===` membership; omitted or
//!   false strictness uses PHP `==` semantics for the supported scalar/string paths.
//! - `lower` is a thin wrapper over the shared `arrays::lower_in_array` emitter.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "in_array",
    area: Array,
    params: [needle: Mixed, haystack: Mixed, strict: Bool = DefaultSpec::Bool(false)],
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Checks if a value exists in an array.",
    php_manual: "https://www.php.net/manual/en/function.in-array.php",
}

/// Validates that the second argument is an array and returns `Bool`.
///
/// The registry's `check_arity` handles the 2-to-3 argument range. This hook validates
/// that `haystack` is an array and returns the `Bool` return type.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "in_array() second argument must be array",
        ));
    }
    Ok(PhpType::Bool)
}

/// Lowers an `in_array` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_in_array(ctx, inst)
}
