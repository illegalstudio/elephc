//! Purpose:
//! Home of the PHP `array_reverse` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP's signature is `array_reverse(array $array, bool $preserve_keys = false)`; both the
//!   positional and the `preserve_keys:` named form are accepted.
//! - For concrete indexed storage, `preserve_keys` changes the result shape and must be literal.
//!   Boxed PHP array declarations preserve either shape and accept runtime flags.
//!   With `false` the concrete result is the input array type; with
//!   `true` an indexed `array<T>` becomes `AssocArray { key: Int, value: T }`, because PHP keeps
//!   the original integer keys while reversing the iteration order, something elephc's dense
//!   indexed representation cannot express.
//! - `check` is required both to reject non-array arguments and to compute that shape.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::ExprKind;
use crate::types::PhpType;

builtin! {
    contract: "array_reverse",
    check: check,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayReverse,
    ),
}

/// Returns the reversed array's type, which depends on the literal `preserve_keys` flag.
///
/// Without `preserve_keys` (or with a literal `false`) reversing keeps the array shape, so the
/// input array/assoc type is returned unchanged. With a literal `true` an indexed array keeps its
/// integer keys in reversed insertion order, which is an `AssocArray` keyed by `Int`; a source
/// that is already associative keeps its own shape because reordering a hash preserves its keys.
/// Boxed PHP arrays keep their declared type and accept a runtime flag. Concrete storage
/// requires a literal flag. Arity and argument inference run through the registry's common path.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if ty.is_php_array() {
        return Ok(ty);
    }
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_reverse() argument must be array",
        ));
    }
    let Some(flag) = cx.args.get(1) else {
        return Ok(ty);
    };
    let preserve = match flag.kind {
        ExprKind::BoolLiteral(value) => value,
        ExprKind::IntLiteral(value) => value != 0,
        _ => {
            return Err(CompileError::new(
                cx.span,
                "array_reverse() preserve_keys argument must be a literal bool in AOT mode",
            ))
        }
    };
    if !preserve {
        return Ok(ty);
    }
    match ty {
        PhpType::Array(elem) => Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        }),
        other => Ok(other),
    }
}
