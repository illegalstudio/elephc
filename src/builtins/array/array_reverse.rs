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
//! - Gradual storage is accepted for the default form and narrowed at the helper boundary;
//!   `check` still rejects statically non-array arguments and computes the result shape. A
//!   gradual source with a key-preserving (or run-time) flag answers `mixed` and
//!   lowers through `crate::array_gradual_prelude`.

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
/// A `Mixed` input or a union containing an array member is accepted: lowering routes it
/// through the runtime-validating compatibility helper, which rejects a non-array runtime
/// variant, while the checker keeps the array member as the result type. A literal flag is
/// still required for concrete storage; a run-time flag on an indexed source answers the union
/// of the two arms.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let boxed = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if boxed.is_php_array() {
        return Ok(boxed);
    }
    // `None` means the flag is only known at run time. Twig's
    // `CoreExtension::reverse($charset, $item, $preserveKeys = false)` forwards an UNTYPED
    // parameter straight into the call, which no caller makes literal.
    let preserve = match cx.args.get(1) {
        None => Some(false),
        Some(flag) => match flag.kind {
            ExprKind::BoolLiteral(value) => Some(value),
            ExprKind::IntLiteral(value) => Some(value != 0),
            _ => None,
        },
    };
    let inferred = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let (ty, gradual) = match inferred {
        ty @ (PhpType::Array(_) | PhpType::AssocArray { .. }) => (ty, false),
        PhpType::Mixed => (PhpType::Array(Box::new(PhpType::Mixed)), true),
        PhpType::Union(members) => {
            let mut arrays = members.into_iter().filter(|member| {
                matches!(member, PhpType::Array(_) | PhpType::AssocArray { .. })
            });
            let Some(first) = arrays.next() else {
                return Err(CompileError::new(
                    cx.span,
                    "array_reverse() argument must be array",
                ));
            };
            let ty = if arrays.next().is_some() {
                PhpType::Array(Box::new(PhpType::Mixed))
            } else {
                first
            };
            (ty, true)
        }
        _ => {
            return Err(CompileError::aot_literal_required(
                cx.span,
                "array_reverse() argument must be array",
            ))
        }
    };
    if preserve.unwrap_or(true) && gradual {
        // A gradual source has no statically known arms to emit, so `array_reverse($mixed, true)`
        // answers `mixed` -- the marker the lowering redirects on, because a declared `array`
        // source answers a declared array and keeps the native boxed path -- and lowers through
        // `crate::array_gradual_prelude`, whose helper reverses the keys the run-time value
        // actually has (the flag may be a run-time value too) and raises php's `TypeError` for a
        // non-array.
        return Ok(PhpType::Mixed);
    }
    let Some(preserve) = preserve else {
        // Both shapes are possible, so the call answers with their union and the backend picks the
        // branch at run time, exactly as `iterator_to_array()` already does for its own flag. An
        // indexed array reverses to an array or an integer-keyed hash; a hash reverses to a hash
        // either way, its `false` arm being the key-preserving reverse followed by
        // `__rt_hash_reindex`. A GRADUAL source has already been refused above: it has no two
        // statically known arms to emit.
        let preserved = array_reverse_preserved_type(ty.clone());
        return Ok(cx.checker.normalize_union_type(vec![ty, preserved]));
    };
    if !preserve {
        return Ok(ty);
    }
    Ok(array_reverse_preserved_type(ty))
}

/// Returns the type `array_reverse($value, true)` produces for one already-validated array type.
///
/// An indexed array keeps its integer keys in reversed insertion order, which is a hash keyed by
/// `Int`; a source that is already associative keeps its own shape, because reordering a hash
/// preserves its keys.
fn array_reverse_preserved_type(ty: PhpType) -> PhpType {
    match ty {
        PhpType::Array(elem) => PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        },
        other => other,
    }
}
