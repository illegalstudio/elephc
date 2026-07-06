//! Purpose:
//! Home of the PHP `in_array` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates that the second argument is an array and returns `Bool`.
//! - The golden signature carries the optional `strict` param (min=2, max=3), but the
//!   legacy CHECK arm enforced exactly 2 arguments and the `lower_in_array` emitter
//!   only supports 2 args. `max_args: 2` reproduces that exact-2 enforcement in
//!   `check_arity` only; `function_sig` and the parity gate keep the full param-derived
//!   bounds from the golden. This keeps the clean "takes exactly 2 arguments" checker
//!   diagnostic for a 3-arg call instead of an EIR backend error.
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
    max_args: 2,
    returns: Bool,
    check: check,
    lower: lower,
    summary: "Checks if a value exists in an array.",
    php_manual: "https://www.php.net/manual/en/function.in-array.php",
}

/// Validates that the second argument is an array and returns `Bool`.
///
/// The registry's `check_arity` handles arity enforcement (capped at 2 by `max_args`
/// to match the legacy CHECK arm). This hook validates that `haystack` is an array
/// and returns the `Bool` return type.
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
