//! Purpose:
//! Home of the PHP `array_merge_recursive` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&[], "arrays")` (min=0). The legacy CHECK
//!   arm requires exactly 2 arguments. `min_args: 2, max_args: 2` reproduce that
//!   enforcement in `check_arity` only; `function_sig` and the parity gate keep the
//!   variadic shape from the golden.
//! - `check` validates that both arguments are associative or scalar-indexed arrays and
//!   returns an `AssocArray` whose value type is always `Mixed` (scalar collisions
//!   combine into lists). The key type widens to `Mixed` when the two input key types
//!   disagree.
//! - Arity is pre-validated by `check_arity`; the hook can assume exactly 2 args.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen::context::FunctionContext;
use crate::codegen::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_merge_recursive",
    area: Array,
    params: [],
    variadic: "arrays",
    min_args: 2,
    max_args: 2,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Recursively merges two arrays, combining scalar collisions into lists.",
    php_manual: "https://www.php.net/manual/en/function.array-merge-recursive.php",
}

/// Validates both arguments are compatible arrays and returns the recursively-merged type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. The hook re-infers both
/// argument types. Both must be associative arrays or indexed arrays of scalars. Scalar
/// collisions combine into lists, so the value type of the result is always `Mixed`; the
/// key type widens to `Mixed` when the two input key types disagree.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let ty2 = cx.checker.infer_type(&cx.args[1], cx.env)?;
    let accepted = |t: &PhpType| {
        matches!(t, PhpType::AssocArray { .. }) || t.is_scalar_indexed_array()
    };
    if !accepted(&ty1) || !accepted(&ty2) {
        return Err(CompileError::new(
            cx.span,
            "array_merge_recursive() arguments must be associative arrays or indexed arrays of scalars",
        ));
    }
    Ok(PhpType::AssocArray {
        key: Box::new(PhpType::widen(ty1.hash_key_type(), ty2.hash_key_type())),
        value: Box::new(PhpType::Mixed),
    })
}

/// Lowers an `array_merge_recursive` call by delegating to the shared emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen::lower_inst::builtins::arrays::lower_array_merge_recursive(ctx, inst)
}
