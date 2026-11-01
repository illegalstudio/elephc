//! Purpose:
//! Home of the PHP `array_slice` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - PHP's signature is
//!   `array_slice(array $array, int $offset, ?int $length = null, bool $preserve_keys = false)`;
//!   every parameter is accepted positionally and by name.
//! - `preserve_keys` CHANGES THE RESULT SHAPE, so it must be a literal in AOT mode (same rule as
//!   `array_reverse()`'s flag). With `false` a slice preserves the array shape, so the return type
//!   is the (array-or-assoc) input type unchanged. With `true` an indexed `array<T>` becomes
//!   `AssocArray { key: Int, value: T }`, because PHP keeps the source integer keys of the
//!   selected window — something elephc's dense indexed representation cannot express.
//! - The result shape depends on an argument VALUE, not on argument types, so it must travel on
//!   the `Checked` contract: a `Shared` resolver is re-run by `semantics::lower_registry_call`
//!   with `args: &[]` and could not see the flag there. A boxed `Mixed`/`Union` source reports the
//!   boxed PHP array type, because its payload may be a list or a hash: `lower_mixed_array_slice`
//!   renumbers a list, keeps a hash's string keys, and boxes whichever storage it built.
//!   `RuntimeFnId::ArraySlice::fallback_result_type` covers call sites with no checked type: an
//!   associative source keeps its layout, a boxed source gets the boxed PHP array type (the one
//!   the callable wrapper pins), and an indexed source keeps `array<mixed>`.
//!   A key-preserving gradual source uses a mixed-key/mixed-value associative result,
//!   materialized by the PHP compatibility prelude without guessing an indexed slot layout.
//! - The checked type below is a CHECKER type: call-site specialization narrows an untyped
//!   parameter (`function top($scores)`) that EIR still lowers under the boxed-`Mixed` ABI
//!   contract, so the type recorded here can be narrower than the operand the slice helper
//!   actually copies. `RuntimeFnId::ArraySlice::checked_result_type_fits_operands` rejects such a
//!   type during EIR lowering so the boxed-`Mixed` layout above is used instead.
//! - `check` is required both to reject non-array arguments and to compute that shape.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "array_slice",
    check: check,
    semantics: array_slice_semantics(),
}

/// The pure `array_slice` summary, widened when an argument still needs php's conversion.
///
/// A float offset or length is converted at the call: `2.5` raises the "loses precision"
/// deprecation and `NAN` throws a `TypeError`. Without `MAY_THROW` the call's operand owners
/// were retired before it, so the array literal handed to `array_slice([1, 2], 0, NAN)` leaked
/// when the TypeError unwound. Integer arguments over a typed array keep the pure summary.
fn array_slice_effects(
    input: &crate::builtins::semantics::BuiltinSemanticInput<'_>,
) -> crate::ir::Effects {
    let base = crate::ir::RuntimeFnId::ArraySlice.effects();
    let converts = |ty: &PhpType| {
        matches!(
            ty.codegen_repr(),
            PhpType::Float | PhpType::Str | PhpType::Mixed | PhpType::Union(_)
        )
    };
    let source_unchecked = input
        .arg_types
        .first()
        .is_some_and(|ty| matches!(ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_)));
    if source_unchecked || input.arg_types.iter().skip(1).take(2).any(converts) {
        base | crate::ir::Effects::MAY_THROW | crate::ir::Effects::MAY_WARN
    } else {
        base
    }
}

/// `array_slice`'s descriptor with the argument-dependent throw summary.
const fn array_slice_semantics() -> crate::builtins::semantics::BuiltinSemantics {
    let mut semantics =
        crate::builtins::semantics::runtime_fn_semantics(crate::ir::RuntimeFnId::ArraySlice);
    semantics.effects = crate::builtins::semantics::BuiltinEffects::Shared(array_slice_effects);
    semantics
}

/// Reads a literal `preserve_keys` flag, returning `None` when the argument is not a literal.
///
/// An absent argument reads as a literal `false` so callers can treat "omitted" and "explicit
/// false" identically. Integer literals follow PHP truthiness, matching `array_reverse()`.
fn literal_preserve_keys(flag: Option<&Expr>) -> Option<bool> {
    match flag {
        None => Some(false),
        Some(flag) => match &flag.kind {
            ExprKind::BoolLiteral(value) => Some(*value),
            ExprKind::IntLiteral(value) => Some(*value != 0),
            // `$preserveKeys = true` passes the assigned literal (nunomaduro/collision's
            // `Highlighter::getCodeSnippet()` names its flag this way).
            ExprKind::Assignment { value, .. } => literal_preserve_keys(Some(value)),
            _ => None,
        },
    }
}

/// Returns the slice's array type for an `array_slice` call.
///
/// Without `preserve_keys` a slice preserves the input array shape, so the (array-or-assoc)
/// first-argument type is returned unchanged and a boxed `Mixed`/`Union` first argument yields
/// the boxed PHP array type, since its payload may be a list or a hash. With a literal
/// `preserve_keys: true` an indexed source keeps the integer keys of
/// the selected window, which is an `AssocArray` keyed by `Int`; a source that is already associative
/// keeps its own shape because narrowing a hash preserves its keys. Non-array first arguments and
/// a non-literal flag are rejected. A key-preserving boxed `Mixed` source yields a gradual
/// associative result and is routed through the compatibility prelude. The first argument is re-inferred here; the
/// registry already inferred every argument once for side effects, and arity (2 to 4) is
/// pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let Some(preserve) = literal_preserve_keys(cx.args.get(3)) else {
        // A flag known only at run time: an INDEXED source has two arms the backend can both
        // emit, so the call answers with their union and the branch picks one, the same shape
        // `array_reverse()` and `iterator_to_array()` use. Twig's `CoreExtension::slice()`
        // forwards an untyped parameter here, which no caller can make literal.
        //
        // Every other source keeps the diagnostic: a gradual one routes its key-preserving arm
        // through the compatibility prelude rather than a native lowering, so the two arms are not
        // both available at the branch, and a hash has no `array_slice` lowering at all.
        let PhpType::Array(elem) = ty.clone() else {
            return Err(CompileError::new(
                cx.span,
                "array_slice() preserve_keys argument must be a literal bool in AOT mode",
            ));
        };
        let preserved = PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        };
        return Ok(cx.checker.normalize_union_type(vec![ty, preserved]));
    };
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        if preserve {
            return Ok(PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            });
        }
        return Ok(PhpType::php_array());
    }
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            cx.span,
            "array_slice() first argument must be array",
        ));
    }
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
