//! Purpose:
//! Home of the PHP `array_merge` builtin: its declaration, type-check hook, and lowering.
//!
//! Called from:
//! - The builtin registry (declaration), the type checker (check hook), and the EIR
//!   backend (lower hook), all via `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&[], "arrays")` (min=0). `min_args: 2` enforces
//!   at least two arrays in `check_arity`; the variadic shape (unbounded max) accepts three
//!   or more, matching PHP. `function_sig` and the parity gate keep the golden shape.
//! - `check` validates that every argument is an indexed or associative array and folds the
//!   merged result type left-to-right (`f(f(a, b), c)`), mirroring the ir_lower rewrite. When
//!   an operand is an empty array (element type `Void`), the result adopts the next operand's
//!   element type if it is a scalar-merge type.
//! - Arity (>= 2 args) is pre-validated by `check_arity`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::codegen_ir::context::FunctionContext;
use crate::codegen_ir::CodegenIrError;
use crate::errors::CompileError;
use crate::ir::Instruction;
use crate::types::PhpType;

builtin! {
    name: "array_merge",
    area: Array,
    params: [],
    variadic: "arrays",
    min_args: 2,
    returns: Mixed,
    check: check,
    lower: lower,
    summary: "Merges the elements of two arrays.",
    php_manual: "https://www.php.net/manual/en/function.array-merge.php",
}

/// Validates every argument is an array and returns the merged result type.
///
/// Arity (>= 2 args) is pre-validated by `check_arity`. The hook infers every argument type and
/// folds them left-to-right through `array_merge_return_type` (mirroring the ir_lower rewrite of
/// `array_merge(a, b, c)` into `array_merge(array_merge(a, b), c)`), so a three-plus-array merge
/// gets the same result type the nested two-array lowering produces.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let mut types = Vec::with_capacity(cx.args.len());
    for arg in cx.args {
        types.push(cx.checker.infer_type(arg, cx.env)?);
    }
    for ty in &types {
        if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
            return Err(CompileError::new(
                cx.span,
                "array_merge() arguments must be arrays",
            ));
        }
    }
    let mut result = types[0].clone();
    for ty in &types[1..] {
        result = array_merge_return_type(result, ty.clone());
    }
    Ok(result)
}

/// Lowers an `array_merge` call by delegating to the shared array-merge emitter.
fn lower(ctx: &mut FunctionContext, inst: &Instruction) -> Result<(), CodegenIrError> {
    crate::codegen_ir::lower_inst::builtins::arrays::lower_array_merge(ctx, inst)
}

/// Infers the return type for `array_merge(first, second)`.
///
/// When `first` is an empty indexed array (element type `Void`), the merged result
/// adopts `second`'s element type if it is a scalar-merge-compatible type; otherwise
/// the result keeps `first`'s type. For non-empty indexed arrays, the left operand
/// type is returned unchanged (matching legacy checker behavior).
fn array_merge_return_type(first: PhpType, second: PhpType) -> PhpType {
    match first {
        PhpType::Array(elem) if is_empty_array_element_type(elem.as_ref()) => match second {
            PhpType::Array(right) if is_scalar_merge_element_type(right.as_ref()) => {
                PhpType::Array(right)
            }
            _ => PhpType::Array(elem),
        },
        other => other,
    }
}

/// Returns true for the element sentinel used by statically empty indexed arrays.
fn is_empty_array_element_type(ty: &PhpType) -> bool {
    matches!(ty.codegen_repr(), PhpType::Void)
}

/// Returns true for element types the merged array can adopt from the next operand when the
/// current accumulator is statically empty. Scalars use the 8-byte merge helper; strings use the
/// dedicated 16-byte `__rt_array_merge_str` helper, so `Str` is included here as well (kept in sync
/// with the ir_lower copy in `src/ir_lower/expr/mod.rs`).
fn is_scalar_merge_element_type(ty: &PhpType) -> bool {
    matches!(
        ty.codegen_repr(),
        PhpType::Int
            | PhpType::Bool
            | PhpType::Float
            | PhpType::Callable
            | PhpType::Void
            | PhpType::Str
    )
}
