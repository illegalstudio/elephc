//! Purpose:
//! Home of the PHP `array_count_values` builtin: its single-source registry declaration and
//! semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - `check` is required because the return type depends on the argument: the source VALUES
//!   become the result KEYS, so the result is `AssocArray<key-from-value, Int>`. The `Int`
//!   value type is fixed — every entry is an occurrence tally.
//! - php-src warns (`E_WARNING`) and SKIPS any element that is neither int nor string, so a
//!   heterogeneous source is still accepted at compile time; the runtime helper emits the
//!   warning.
//! - Arity (exactly 1 argument) is validated by the registry's `check_arity` before the hook
//!   fires.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::{array_key_type_from_value_type, PhpType};

builtin! {
    contract: "array_count_values",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayCountValues,
    ),
}

/// Returns the tally associative-array type for an `array_count_values` call.
///
/// Source values become result keys, so the key type is derived from the source element/value
/// type via `array_key_type_from_value_type`; the value type is always `Int`. The argument is
/// re-inferred here to drive the return type, and arity is pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    // An `array|false` union (scandir, glob, file) reads through to its array member;
    // the argument lowering pairs the acceptance with an unbox-or-throw for the `false`.
    let ty = ty.array_or_false_member().cloned().unwrap_or(ty);
    match ty {
        PhpType::Array(elem) => Ok(PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(*elem)),
            value: Box::new(PhpType::Int),
        }),
        PhpType::AssocArray { value, .. } => Ok(PhpType::AssocArray {
            key: Box::new(array_key_type_from_value_type(*value)),
            value: Box::new(PhpType::Int),
        }),
        _ => Err(CompileError::new(
            cx.span,
            "array_count_values() argument must be array",
        )),
    }
}
