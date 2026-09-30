//! Purpose:
//! Home of the PHP `array_diff` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The PHP golden signature is `variadic(&["array"], "arrays")` (one regular `array`
//!   param plus a variadic `arrays`). The legacy CHECK arm required exactly 2 arguments,
//!   so `min_args: 2, max_args: 2` reproduce that enforcement in `check_arity` only;
//!   `function_sig` and the parity gate keep the variadic shape from the golden.
//! - `check` requires the first argument to be an indexed or associative array and types the
//!   result as php shapes it: the survivors KEEP their source keys, so an indexed `array<T>`
//!   becomes `AssocArray { key: Int, value: T }`. `array_diff(["a","b","c"], ["b"])` is
//!   `{0:"a", 2:"c"}` in php, which a dense indexed array cannot represent; php's
//!   `PHP_FUNCTION(array_diff)` re-adds each survivor with
//!   `zend_hash_index_add_new(…, idx, …)` (ext/standard/array.c), never
//!   `zend_hash_next_index_insert`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_diff",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayDiff,
    ),
}

/// Validates the first argument is an array and returns its key-preserving result type.
///
/// Arity (exactly 2 args) is pre-validated by `check_arity`. The first argument is
/// re-inferred here to drive the return type; the registry already inferred every
/// argument once for side effects. The survivors keep their source keys, so an indexed
/// first operand answers an `AssocArray` keyed by `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty1 = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // An `array|false` union (scandir, glob, file) reads through to its array member;
    // the argument lowering pairs the acceptance with an unbox-or-throw for the `false`.
    let ty1 = ty1.array_or_false_member().cloned().unwrap_or(ty1);
    if !matches!(ty1, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            &format!("{}() first argument must be array", cx.name),
        ));
    }
    Ok(super::array_unique::key_preserving_set_op_result(ty1))
}
