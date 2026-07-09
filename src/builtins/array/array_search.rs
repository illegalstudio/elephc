//! Purpose:
//! Home of the PHP `array_search` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - `check` validates the second argument is an array and returns a union of the
//!   key type and Bool (false on not-found), or Int|Bool for indexed arrays.
//! - The golden signature carries the optional `strict` param (min=2, max=3), but the
//!   legacy CHECK arm enforced exactly 2 arguments and the `lower_array_search` emitter
//!   only supports 2 args. `max_args: 2` reproduces that exact-2 enforcement in
//!   `check_arity` only; `function_sig` and the parity gate keep the full param-derived
//!   bounds from the golden. This keeps the clean "takes exactly 2 arguments" checker
//!   diagnostic for a 3-arg call instead of an EIR backend error.
//! - `lower` is a thin wrapper over the shared `arrays::lower_array_search` emitter.

use crate::builtins::spec::{BuiltinCheckCtx, DefaultSpec};
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_search",
    area: Array,
    params: [needle: Mixed, haystack: Mixed, strict: Bool = DefaultSpec::Bool(false)],
    max_args: 2,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Searches the array for a given value and returns the first corresponding key if successful.",
    php_manual: "https://www.php.net/manual/en/function.array-search.php",
}

/// Validates haystack is an array and returns the key-or-false union type.
///
/// The registry's `check_arity` handles arity enforcement (capped at 2 by `max_args`
/// to match the legacy CHECK arm). For assoc arrays the return is `key_type | bool`;
/// for indexed arrays it is `int | bool`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    cx.checker.infer_type(&cx.args[0], cx.env)?;
    let arr_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    if !matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_search() second argument must be array",
        ));
    }
    match arr_ty {
        PhpType::AssocArray { key, .. } => {
            Ok(cx.checker.normalize_union_type(vec![*key, PhpType::Bool]))
        }
        _ => Ok(PhpType::Union(vec![PhpType::Int, PhpType::Bool])),
    }
}

/// Lowers an `array_search` call by dispatching to the shared array emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_search(ctx, inst)
}
