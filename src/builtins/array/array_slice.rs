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
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArraySlice,
    ),
}

/// Reads a literal `preserve_keys` flag, returning `None` when the argument is not a literal.
///
/// An absent argument reads as a literal `false` so callers can treat "omitted" and "explicit
/// false" identically. Integer literals follow PHP truthiness, matching `array_reverse()`.
fn literal_preserve_keys(flag: Option<&Expr>) -> Option<bool> {
    match flag {
        None => Some(false),
        Some(flag) => match flag.kind {
            ExprKind::BoolLiteral(value) => Some(value),
            ExprKind::IntLiteral(value) => Some(value != 0),
            _ => None,
        },
    }
}

/// Reports whether a literal integer offset is exactly zero.
fn literal_zero_offset(offset: &Expr) -> bool {
    matches!(offset.kind, ExprKind::IntLiteral(0))
}

/// Reports whether a variable source may load through boxed or aliased storage.
///
/// A call-site-specialized untyped parameter is checked as `array<never>` even though its
/// boxed ABI can hold a hash at run time. The same applies to globals, `static` locals and
/// reference aliases. Their key-preserving slice must keep the hash result type so the
/// enclosing return conversion cannot mistake a hash payload for a packed list.
fn source_may_be_boxed(cx: &BuiltinCheckCtx, expr: &Expr) -> bool {
    let ExprKind::Variable(name) = &expr.kind else {
        return false;
    };
    let name = name.as_str();
    cx.checker.local_parameter_names.contains(name)
        || cx.checker.active_globals.contains(name)
        || cx.checker.static_local_names.contains(name)
        || cx.checker.ref_aliased_locals.contains(name)
        || cx.checker.by_ref_captured_locals.contains(name)
        || cx.checker.boxed_ref_aliased_locals.contains(name)
        || cx.checker.top_level_binding_is_program_global(name)
}

/// Returns the slice's array type for an `array_slice` call.
///
/// Without `preserve_keys` a slice preserves the input array shape, so the (array-or-assoc)
/// first-argument type is returned unchanged and a boxed `Mixed`/`Union` first argument yields
/// the boxed PHP array type, since its payload may be a list or a hash. With a literal
/// `preserve_keys: true` an indexed source keeps the integer keys of
/// the selected window, which is an `AssocArray` keyed by `Int`; a source that is already associative
/// keeps its own shape because narrowing a hash preserves its keys. Non-array first arguments and
/// a non-literal flag are rejected, and so is a key-preserving slice of a boxed `Mixed` array,
/// whose element layout is not statically known. The first argument is re-inferred here; the
/// registry already inferred every argument once for side effects, and arity (2 to 4) is
/// pre-validated by the registry.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    let ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    let preserve = literal_preserve_keys(cx.args.get(3)).ok_or_else(|| {
        CompileError::aot_literal_required(
            cx.span,
            "array_slice() preserve_keys argument must be a literal bool in AOT mode",
        )
    })?;
    if matches!(ty, PhpType::Mixed | PhpType::Union(_)) {
        if preserve {
            return Err(CompileError::new(
                cx.span,
                "array_slice() preserve_keys requires a statically known array type",
            ));
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
        // A generic untyped parameter is an `array<never>` here even though its call-site
        // specialization may hold a list: its boxed lowering builds a hash under
        // `preserve_keys`, so it must keep the hash result type. Only a concrete indexed
        // element type can stay a list at offset 0.
        PhpType::Array(elem)
            if literal_zero_offset(&cx.args[1])
                && !matches!(elem.as_ref(), PhpType::Never)
                && !source_may_be_boxed(cx, &cx.args[0]) =>
        {
            Ok(PhpType::Array(elem))
        }
        PhpType::Array(elem) => Ok(PhpType::AssocArray {
            key: Box::new(PhpType::Int),
            value: elem,
        }),
        other => Ok(other),
    }
}
