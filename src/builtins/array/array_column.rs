//! Purpose:
//! Home of the PHP `array_column` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP 8 signature: `array_column(array $array, int|string|null $column_key,
//!   int|string|null $index_key = null)`. Arity (2 or 3) is validated by the registry.
//! - An indexed array of concrete associative rows with a string column key and no index key
//!   keeps the row value type (typed fast path). Every other row shape, a `null` or integer
//!   column key, and declared PHP arrays return indexed `Mixed` cells.
//! - A non-null index key re-keys the result, so it is typed as the boxed PHP array.
//! - Statically array- or object-typed keys are rejected with PHP's TypeError wording;
//!   `mixed` keys are checked by the runtime walker.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::types::PhpType;

builtin! {
    contract: "array_column",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayColumn,
    ),
}

/// Returns the extracted-column array type for an `array_column` call.
///
/// The arguments are re-inferred here to drive the return type; the registry already inferred
/// every argument once for side effects, and arity (2 or 3) is pre-validated.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let column_ty = cx.checker.infer_type(&cx.args[1], cx.env)?;
    validate_key_type(cx, &column_ty, 2, "column_key")?;
    let index_ty = match cx.args.get(2) {
        Some(index) => Some(cx.checker.infer_type(index, cx.env)?),
        None => None,
    };
    if let Some(index_ty) = &index_ty {
        validate_key_type(cx, index_ty, 3, "index_key")?;
    }
    if !ty.is_php_array() && !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_column() first argument must be array",
        ));
    }
    let indexed = index_ty
        .as_ref()
        .is_some_and(|index_ty| !matches!(index_ty, PhpType::Void | PhpType::Never));
    if indexed {
        return Ok(PhpType::php_array());
    }
    if column_ty == PhpType::Str {
        if let PhpType::Array(inner) = &ty {
            if let PhpType::AssocArray { value, .. } = inner.as_ref() {
                return Ok(PhpType::Array(value.clone()));
            }
        }
    }
    Ok(PhpType::Array(Box::new(PhpType::Mixed)))
}

/// Rejects a key argument whose static type can never be `int|string|null`.
fn validate_key_type(
    cx: &BuiltinCheckCtx,
    ty: &PhpType,
    position: usize,
    name: &str,
) -> Result<(), CompileError> {
    let given = match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } => "array",
        _ if ty.is_php_array() => "array",
        PhpType::Object(class) => class.as_str(),
        _ => return Ok(()),
    };
    Err(CompileError::new(
        cx.span,
        &format!(
            "array_column(): Argument #{position} (${name}) must be of type string|int|null, {given} given"
        ),
    ))
}
