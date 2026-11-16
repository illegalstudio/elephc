//! Purpose:
//! Home of the PHP `array_push` builtin: its single-source registry declaration and semantic target.
//!
//! Called from:
//! - Checker, EIR, optimizer, ownership, and callable consumers through `crate::builtins::registry`.
//!
//! Key details:
//! - The golden signature is `first_param_ref(variadic(["array"], "values"))`: `array`
//!   by-ref plus a variadic `values` param, so `min_args: 1` is the only `check_arity`
//!   override and the maximum is unbounded. It used to be pinned to `2, 2`, which rejected
//!   ordinary PHP (`array_push($a, 3, 4)`) with an arity error (issue #677).
//! - The `ref` marker on `array` is mandatory — it is what makes by-reference mutation
//!   lower correctly (ir_lower reads `ref_params` from the registry sig).
//! - `lazy_check: true` lets this builtin admit generator and `Traversable` spreads through
//!   its runtime iterator coordinator before generic call inference rejects non-array unpacking.
//! - Returns `Int` — the new number of elements, as PHP does and as the `array_unshift`
//!   sibling already did. It used to return `Void`, so `$n = array_push($a, 1)` read `NULL`.

use crate::builtins::spec::BuiltinCheckCtx;
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

builtin! {
    contract: "array_push",
    check: check,
    lazy_check: true,
    semantics: crate::builtins::semantics::runtime_fn_semantics(
        crate::ir::RuntimeFnId::ArrayPush,
    ),
}

/// Validates indexed, associative and boxed PHP array receivers for an `array_push` call.
///
/// Arity (at least 1 arg) is pre-validated by `check_arity`. Every argument is inferred so the
/// appended values still produce their side effects; spreads may be arrays or Traversables because
/// the associative coordinator owns their runtime iterator walk. The receiver must be indexed,
/// associative or a declared PHP array with boxed storage. Returns the new element count as `Int`.
fn check(cx: &mut BuiltinCheckCtx) -> Result<PhpType, CompileError> {
    if cx.argument_plan.is_some_and(|plan| plan.regular_argument_is_unpacked(0)
        || plan.source_args.first().is_some_and(|arg| matches!(arg.kind, ExprKind::Spread(_)))) {
        return Err(CompileError::new(cx.args[0].span,
            "array_push() receiver must be supplied directly before unpacked arguments"));
    }
    if crate::types::checker::Checker::reference_receiver_uses_nullsafe(&cx.args[0]) {
        return Err(CompileError::new(cx.args[0].span,
            "array_push(): Argument #1 ($array) could not be passed by reference"));
    }
    let indexed_root = indexed_receiver_root(&cx.args[0]);
    if matches!(indexed_root.unwrap_or(&cx.args[0]).kind, ExprKind::DynamicPropertyAccess { .. }) {
        return Err(CompileError::new(cx.args[0].span,
            "array_push() dynamic property receivers are not supported; use a direct property name"));
    }
    if indexed_root.is_some_and(|root| matches!(&root.kind, ExprKind::Variable(name)
        if cx.checker.local_parameter_names.contains(name)
            || cx.checker.boxed_parameter_locals.contains(name)))
    {
        return Err(CompileError::new(cx.args[0].span,
            "array_push() nested parameter storage is not supported"));
    }
    let arr_ty = cx.checker.infer_type(&cx.args[0], cx.env)?;
    if (matches!(arr_ty.codegen_repr(), PhpType::AssocArray { .. }) || arr_ty.is_php_array())
        && indexed_root.is_some_and(|root| !matches!(root.kind, ExprKind::Variable(_)
            | ExprKind::This | ExprKind::PropertyAccess { .. } | ExprKind::StaticPropertyAccess { .. }))
    {
        return Err(CompileError::new(cx.args[0].span,
            "array_push() temporary associative or declared-array receivers require a local parent variable"));
    }
    if matches!(&cx.args[0].kind, ExprKind::Variable(name)
        if cx.checker.active_statics.contains(name))
        && matches!(arr_ty.codegen_repr(), PhpType::AssocArray { .. })
    {
        return Err(CompileError::new(cx.args[0].span,
            "array_push() does not support associative static-local receivers"));
    }
    let denied_place = cx
        .checker
        .record_array_push_reference_place(&cx.args[0], cx.env)?;
    let aliased_indexed_receiver = cx
        .checker
        .local_has_ref_assign_alias(&cx.args[0])
        || cx.checker.local_has_prior_array_capture(&cx.args[0])
        || (indexed_root.is_some()
            && cx.checker.local_has_non_builtin_reference_alias(&cx.args[0]));
    let runtime_spread_supported = cx
        .checker
        .array_push_runtime_spread_supported(&cx.args[0], &arr_ty, cx.env)?;
    if !denied_place && !runtime_spread_supported && !literal_or_local_indexes(&cx.args[0]) {
        return Err(CompileError::new(cx.args[0].span,
            "array_push() native receiver places require literal or local indexes"));
    }
    let value_constraint = match arr_ty.codegen_repr() {
        PhpType::AssocArray { value, .. }
            if !matches!(value.codegen_repr(), PhpType::Mixed | PhpType::Iterable) => Some((
                value.as_ref().clone(),
                "array_push() values must match the associative receiver entry type",
            )),
        PhpType::Array(element)
            if aliased_indexed_receiver
                && !matches!(element.codegen_repr(), PhpType::Mixed | PhpType::Iterable) =>
        {
            Some((
                element.as_ref().clone(),
                "array_push() cannot widen a reference-aliased indexed receiver",
            ))
        }
        _ => None,
    };
    for index in 1..cx.args.len() {
        let value_ty = if matches!(cx.args[index].kind, ExprKind::Spread(_)) {
            let (_, element) = cx
                .checker
                .infer_runtime_iterated_call_arg_types(&cx.args[index], cx.env)?;
            if is_boxed_argument_source(cx, &cx.args[index]) {
                PhpType::Mixed
            } else {
                element
            }
        } else {
            let inferred = cx.checker.infer_type(&cx.args[index], cx.env)?;
            if is_boxed_argument_source(cx, &cx.args[index]) {
                PhpType::Mixed
            } else {
                inferred
            }
        };
        if is_runtime_spread_argument(&cx.args[index]) && !runtime_spread_supported {
            return Err(CompileError::new(
                cx.args[index].span,
                "array_push() runtime spreads require stable non-aliased receiver storage",
            ));
        }
        if !denied_place && !runtime_spread_supported
            && !matches!(cx.args[0].kind, ExprKind::Variable(_))
            && !literal_or_local_value(&cx.args[index])
            && !cx.checker.array_push_value_is_local(&cx.args[index], cx.env)
        {
            return Err(CompileError::new(cx.args[index].span,
                "array_push() native receiver places require literal or local values, numeric computation without PHP callbacks, or buffer operations that cannot change the receiver"));
        }
        if let Some((expected, message)) = &value_constraint {
            if value_ty.codegen_repr() != expected.codegen_repr() {
                return Err(CompileError::new(
                    cx.args[index].span,
                    message,
                ));
            }
        }
    }
    if denied_place || matches!(arr_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) || arr_ty.is_php_array() {
        cx.checker.record_array_push_parent_storage(&cx.args[0], cx.span, cx.env);
        Ok(PhpType::Int)
    } else {
        Err(CompileError::new(cx.span, "array_push() first argument must be array"))
    }
}

/// Bounds native copy/write-back to operand trees that cannot change the selected storage.
/// Calls, casts, member reads and other computed values may enter PHP handlers or rebind a root.
fn literal_or_local_value(value: &Expr) -> bool {
    match &value.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_) | ExprKind::StringLiteral(_) | ExprKind::BoolLiteral(_)
        | ExprKind::Null | ExprKind::ConstRef(_) => true,
        ExprKind::ArrayLiteral(items) => items.iter().all(literal_or_local_value),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().all(|(key, value)|
            matches!(key.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_))
                && literal_or_local_value(value)),
        ExprKind::Spread(inner) | ExprKind::NamedArg { value: inner, .. }
        | ExprKind::ErrorSuppress(inner) => literal_or_local_value(inner),
        _ => false,
    }
}

/// A native parent is not leased before evaluating a computed index, so its selection must be inert.
fn literal_or_local_indexes(place: &Expr) -> bool {
    match &place.kind {
        ExprKind::ArrayAccess { array, index } => literal_or_local_indexes(array)
            && matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::StringLiteral(_)
                | ExprKind::Variable(_)),
        ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) =>
            literal_or_local_indexes(value),
        _ => true,
    }
}

/// Returns whether an unpack source needs the runtime iterator coordinator.
fn is_runtime_spread_argument(argument: &Expr) -> bool {
    matches!(&argument.kind, ExprKind::Spread(source)
        if !matches!(source.kind, ExprKind::ArrayLiteral(_) | ExprKind::ArrayLiteralAssoc(_)))
}

/// Returns whether a pushed value or spread source is backed by boxed global or parameter storage.
fn is_boxed_argument_source(cx: &BuiltinCheckCtx<'_>, argument: &Expr) -> bool {
    let mut value = argument;
    while let ExprKind::Spread(inner)
    | ExprKind::NamedArg { value: inner, .. }
    | ExprKind::ErrorSuppress(inner) = &value.kind
    {
        value = inner;
    }
    matches!(&value.kind, ExprKind::Variable(name)
        if cx.checker.active_globals.contains(name)
            || cx.checker.boxed_parameter_locals.contains(name))
}

/// Finds the storage root when the receiver selects an element beneath an indexed container.
fn indexed_receiver_root(receiver: &Expr) -> Option<&Expr> {
    let mut current = receiver;
    while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &current.kind {
        current = value;
    }
    if !matches!(current.kind, ExprKind::ArrayAccess { .. }) { return None; }
    while let ExprKind::ArrayAccess { array, .. }
    | ExprKind::NamedArg { value: array, .. }
    | ExprKind::ErrorSuppress(array) = &current.kind { current = array; }
    Some(current)
}
