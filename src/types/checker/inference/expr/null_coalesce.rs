//! Purpose:
//! Types the result of `value ?? default`: whether the default can be reached at all, and how an
//! arm that already mixes an object with a scalar joins the other arm.
//!
//! Called from:
//! - `crate::types::checker::inference::expr::calls_objects` (`Checker::infer_type` for `??`)
//! - `crate::types::checker::inference::expr::effects` (the assignment-effects walk for `??`)
//!
//! Key details:
//! - The default is dropped only when the left operand always evaluates to a non-null value of
//!   its checker type: a fresh value (literal, `new`, `clone`, closure, `$this`) or a call whose
//!   target DECLARES a return type that excludes null. Storage reads keep the default whatever
//!   their type says: an unassigned or `unset()` variable, an absent array key and an
//!   uninitialized property all read as null inside `??`, and the checker has no
//!   definite-assignment analysis to rule that out. Inferred returns are not trusted either,
//!   because they omit the implicit `null` of a body that can fall off its end, and builtin and
//!   extern results are left out because their checker types can omit a null they return.
//! - A join where one arm is already an object/scalar union keeps a union instead of `mixed`,
//!   with the object members meeting the way two object arms do.

use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::super::Checker;
use super::merge_null_coalesce_result_type;

/// Types `value ?? default` from the left operand's type, the default's type and whether the
/// left operand can ever be null (see [`operand_is_never_null`]).
///
/// A left operand that is never null makes the default dead code, so the result is the operand's
/// own type: `find() ?? new Other()` over `function find(): Implementation|false` stays
/// `Implementation|false` instead of gaining an unreachable `Other` that the declared type then
/// refuses (issue #1462). Otherwise the null member of the left type is removed and the two arms
/// are joined.
pub(super) fn null_coalesce_result_type(
    checker: &Checker,
    value_is_never_null: bool,
    value_ty: PhpType,
    default_ty: PhpType,
) -> PhpType {
    if value_is_never_null {
        return value_ty;
    }
    let non_null_value = if Checker::union_contains_void(&value_ty) {
        checker.strip_void_from_union(&value_ty)
    } else {
        value_ty
    };
    merge_null_coalesce_result_type(checker, non_null_value, default_ty)
}

/// Returns whether the `??` left operand `value`, inferred as `value_ty` in `env`, always
/// evaluates to a non-null value.
///
/// Only operands whose value cannot be absent qualify: fresh values, and calls to functions and
/// methods that declare a return type excluding null. Variables, array elements and properties
/// never do, because `??` reads them as null when they are unassigned, unset, missing or
/// uninitialized whatever their checker type says.
pub(super) fn operand_is_never_null(
    checker: &Checker,
    value: &Expr,
    value_ty: &PhpType,
    env: &TypeEnv,
) -> bool {
    if type_admits_null(value_ty) {
        return false;
    }
    match &value.kind {
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::ArrayLiteral(_)
        | ExprKind::ArrayLiteralAssoc(_)
        | ExprKind::ArrayLiteralMixed(_)
        | ExprKind::NewObject { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::Clone(_)
        | ExprKind::Closure { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This => true,
        ExprKind::FunctionCall { name, .. } => {
            function_call_declares_non_null_return(checker, value, name.as_str())
        }
        ExprKind::MethodCall { object, method, .. } => {
            method_call_declares_non_null_return(checker, object, method, env)
        }
        ExprKind::StaticMethodCall {
            receiver, method, ..
        } => static_call_declares_non_null_return(checker, receiver, method),
        _ => false,
    }
}

/// Returns whether a checker type admits PHP null (`void` is the checker's null type).
fn type_admits_null(ty: &PhpType) -> bool {
    match ty {
        PhpType::Void | PhpType::Mixed | PhpType::Never | PhpType::TaggedScalar => true,
        PhpType::Union(members) => members.iter().any(type_admits_null),
        _ => false,
    }
}

/// Returns whether a signature declares a return type that excludes null.
fn declares_non_null_return(sig: &FunctionSig) -> bool {
    sig.declared_return && !type_admits_null(&sig.return_type)
}

/// Returns whether the call `call` to `name` resolves to a user function whose declared return
/// type excludes null.
///
/// Builtins and externs are resolved ahead of user functions and are left out, so a call the
/// checker typed as a builtin (recorded in `builtin_call_types` under its span) never qualifies.
fn function_call_declares_non_null_return(checker: &Checker, call: &Expr, name: &str) -> bool {
    if checker.builtin_call_types.contains_key(&call.span)
        || checker.canonical_extern_function_name_folded(name).is_some()
    {
        return false;
    }
    checker
        .canonical_function_name_folded(name)
        .and_then(|canonical| checker.functions.get(&canonical))
        .is_some_and(declares_non_null_return)
}

/// Returns whether `$object->method()` resolves, on every object its receiver can hold, to a
/// method whose declared return type excludes null.
///
/// Only `$this` and plain variables are resolved as receivers. A non-object member of the
/// receiver type (`false` in `Repo|false`) raises an `Error` instead of answering null, and an
/// override must keep a covariant declared return, so the static receiver class decides.
fn method_call_declares_non_null_return(
    checker: &Checker,
    object: &Expr,
    method: &str,
    env: &TypeEnv,
) -> bool {
    let receiver = match &object.kind {
        ExprKind::This => checker.current_class.clone().map(PhpType::Object),
        ExprKind::Variable(name) => env.get(name).cloned(),
        _ => None,
    };
    let Some(receiver) = receiver else {
        return false;
    };
    let method_key = php_symbol_key(method);
    let mut classes = union_members(&receiver)
        .filter_map(|member| match member {
            PhpType::Object(class_name) => Some(class_name),
            _ => None,
        })
        .peekable();
    classes.peek().is_some()
        && classes.all(|class_name| {
            let sig = match checker.interfaces.get(class_name) {
                Some(interface) => interface.methods.get(&method_key),
                None => checker
                    .classes
                    .get(class_name)
                    .and_then(|class_info| class_info.methods.get(&method_key)),
            };
            sig.is_some_and(declares_non_null_return)
        })
}

/// Returns whether `Class::method()` (or a `self::`, `static::` or `parent::` call) resolves to
/// a method whose declared return type excludes null.
///
/// Enum static calls are left out: their `tryFrom()` answers null by design.
fn static_call_declares_non_null_return(
    checker: &Checker,
    receiver: &StaticReceiver,
    method: &str,
) -> bool {
    let class_name = match receiver {
        StaticReceiver::Named(name) => Some(name.as_str().to_string()),
        StaticReceiver::Self_ | StaticReceiver::Static => checker.current_class.clone(),
        StaticReceiver::Parent => checker
            .current_class
            .as_ref()
            .and_then(|current| checker.classes.get(current))
            .and_then(|class_info| class_info.parent.clone()),
        // `generics::classes` rewrites every generic receiver before the checker runs; should one
        // survive, "may return null" is the answer that cannot fold anything away wrongly.
        StaticReceiver::Generic(_) => None,
    };
    let Some(class_name) = class_name else {
        return false;
    };
    if checker.enums.contains_key(&class_name) {
        return false;
    }
    let method_key = php_symbol_key(method);
    checker
        .classes
        .get(&class_name)
        .and_then(|class_info| {
            class_info
                .static_methods
                .get(&method_key)
                .or_else(|| class_info.methods.get(&method_key))
        })
        .is_some_and(declares_non_null_return)
}

/// Joins `??` arms when one of them already mixes an object with a non-false scalar.
///
/// `?(Contract|int) ?? new Implementation()` is `Contract|int`: the object members of both arms
/// meet at the supertype one side already accepts (or stay a union of both), and every other
/// member is kept beside them. The `int` member used to send the pair to the scalar join, which
/// answered `mixed` and lost the object type that a later `is_int()` guard narrows back to
/// (issue #1463). Returns `None` for every other pair: an arm holding anything beyond objects,
/// `false`, null and scalars, a bare null or `never` default, and a pure object arm against a
/// pure scalar arm, which keeps joining to `mixed` exactly like a ternary's branches.
pub(super) fn merge_object_scalar_union_types(
    checker: &Checker,
    value: &PhpType,
    default: &PhpType,
) -> Option<PhpType> {
    if [value, default]
        .iter()
        .any(|arm| matches!(arm, PhpType::Void | PhpType::Never))
        || !(mixes_object_with_scalar(value) || mixes_object_with_scalar(default))
        || !union_members(value)
            .chain(union_members(default))
            .all(is_joinable_member)
    {
        return None;
    }
    let (value_objects, value_rest) = split_object_members(checker, value);
    let (default_objects, default_rest) = split_object_members(checker, default);
    let objects = match (value_objects, default_objects) {
        (Some(value_objects), Some(default_objects)) => {
            if checker.type_accepts(&value_objects, &default_objects) {
                value_objects
            } else if checker.type_accepts(&default_objects, &value_objects) {
                default_objects
            } else {
                checker.normalize_union_type(vec![value_objects, default_objects])
            }
        }
        (Some(objects), None) | (None, Some(objects)) => objects,
        (None, None) => return None,
    };
    let mut members = vec![objects];
    members.extend(value_rest);
    members.extend(default_rest);
    Some(checker.normalize_union_type(members))
}

/// Returns whether a type is a union holding both an object and a non-false scalar member.
fn mixes_object_with_scalar(ty: &PhpType) -> bool {
    let PhpType::Union(members) = ty else {
        return false;
    };
    members
        .iter()
        .any(|member| matches!(member, PhpType::Object(_)))
        && members.iter().any(is_non_false_scalar)
}

/// Returns whether a type is `int`, `float`, `string` or `bool`.
fn is_non_false_scalar(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Bool
    )
}

/// Returns whether a union member can take part in an object/scalar join.
fn is_joinable_member(ty: &PhpType) -> bool {
    matches!(ty, PhpType::Object(_) | PhpType::False | PhpType::Void) || is_non_false_scalar(ty)
}

/// Splits a type into its object members, normalized into one type (`None` when there are
/// none), and its other members in their original order.
fn split_object_members(checker: &Checker, ty: &PhpType) -> (Option<PhpType>, Vec<PhpType>) {
    let (objects, rest): (Vec<PhpType>, Vec<PhpType>) = union_members(ty)
        .cloned()
        .partition(|member| matches!(member, PhpType::Object(_)));
    let objects = (!objects.is_empty()).then(|| checker.normalize_union_type(objects));
    (objects, rest)
}

/// Yields a type's union members, or the type itself when it is not a union.
fn union_members(ty: &PhpType) -> std::slice::Iter<'_, PhpType> {
    match ty {
        PhpType::Union(members) => members.iter(),
        other => std::slice::from_ref(other).iter(),
    }
}
