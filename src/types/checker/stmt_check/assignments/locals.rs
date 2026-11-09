//! Purpose:
//! Type-checks assignment locals forms.
//! Updates type environments and validates storage-specific rules for locals, arrays, and properties.
//!
//! Called from:
//! - `crate::types::checker::stmt_check::assignments`
//!
//! Key details:
//! - Assignment checking must distinguish value writes, by-reference mutation, nullable access, and declared property contracts.

use crate::errors::CompileError;
use crate::errors::CompileWarning;
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{
    is_compound_assignment_self_read, BinOp, CallableTarget, Expr, ExprKind, StaticReceiver,
    TypeExpr,
};
use crate::span::Span;
use crate::types::checker::builtins::array_arg_is_gradually_acceptable;
use crate::types::checker::inference::syntactic::null_coalesce_merge_type;
use crate::types::{normalized_array_key_type, PhpType, TypeEnv};

use super::super::super::Checker;

/// Returns whether a retype crosses the scalar/heap storage boundary.
///
/// Only that crossing changes the STORAGE a later read sees: a scalar frame slot and a heap or
/// Mixed slot cannot be the same slot, while any two heap-represented values (`object` to
/// `object`, `string` to `string|false`, `array` to `array<mixed>`, a union to one of its members)
/// are the same pointer and the assignment simply stores another pointer. Warning on the second
/// category named nothing a user could act on: a real Symfony build produced 199 of its 604
/// compile warnings from ordinary PHP that php never reports.
fn retype_crosses_storage_boundary(existing: &PhpType, ty: &PhpType) -> bool {
    /// Whether this representation lives in a scalar frame slot.
    fn scalar(ty: &PhpType) -> bool {
        matches!(
            ty.codegen_repr(),
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
        )
    }
    scalar(existing) != scalar(ty)
}

/// Records — or retracts — the fact that `name`'s frame slot is a boxed `Mixed` cell because this
/// assignment retyped it.
///
/// Set-or-clear, because the arm that calls this ABANDONS the old slot and creates one for `ty`:
/// `$a = 1; $a = "s"; $a = 2;` ends on a raw integer slot, and a name left in the set would type
/// every later container write from it `Mixed` for no reason. The predicate is `codegen_repr`, not
/// the type: `?int` retypes an `int` without leaving tagged-scalar storage, and only a slot that
/// really holds a cell may claim to.
fn record_retyped_boxed_storage(checker: &mut Checker, name: &str, ty: &PhpType) {
    if ty.codegen_repr() == PhpType::Mixed {
        checker
            .retyped_boxed_storage_locals
            .insert(name.to_string());
    } else {
        checker.retyped_boxed_storage_locals.remove(name);
    }
}

/// Binds an otherwise-unbound local as `Mixed` after its assignment fails.
///
/// Error recovery must preserve a valid earlier binding, while ensuring a name
/// introduced by the failed statement does not trigger misleading follow-on
/// `Undefined variable` diagnostics.
fn poison_unbound_local(env: &mut TypeEnv, name: &str) {
    env.entry(name.to_string()).or_insert(PhpType::Mixed);
}

/// Extracts the default expression from a null-coalescing assignment to a specific variable.
///
/// Returns `Some(&default)` if `value` is a `NullCoalesce` expression where the current
/// value is a variable matching `name`. Otherwise returns `None`.
///
/// This is used during null-coalescing assignment type-checking to determine whether
/// the assignment targets an existing variable and what its default expression is.
fn null_coalesce_assignment_default<'a>(name: &str, value: &'a Expr) -> Option<&'a Expr> {
    if let ExprKind::NullCoalesce {
        value: current,
        default,
    } = &value.kind
    {
        if matches!(&current.kind, ExprKind::Variable(current_name) if current_name == name) {
            return Some(default);
        }
    }
    None
}

/// Determines the resulting type of a null-coalescing assignment operation.
///
/// A definitely non-null current value wins without evaluating the default. For a nullable
/// union, removes the null arm before merging it with the default because the default replaces
/// that arm at runtime. Explicit typed-local contracts remain enforced by
/// [`merge_local_assignment_type`] after this flow-sensitive result has been computed; the
/// reachable default is also checked directly so a heterogeneous merge cannot hide an invalid
/// concrete write behind `Mixed`.
fn null_coalesce_assignment_type(
    checker: &Checker,
    name: &str,
    existing: &PhpType,
    default_ty: &PhpType,
    span: Span,
) -> Result<PhpType, CompileError> {
    let default_reachable = matches!(existing, PhpType::Void | PhpType::Mixed)
        || Checker::union_contains_void(existing);
    let declaration_key = (checker.current_loop_storage_scope.clone(), name.to_string());
    if default_reachable {
        if let Some(declared) = checker.declared_local_types.get(&declaration_key) {
            if !checker.type_accepts(declared, default_ty) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Type error: cannot reassign ${} from {} to {}",
                        name, declared, default_ty
                    ),
                ));
            }
        }
    }

    let result = if *existing == PhpType::Void {
        default_ty.clone()
    } else if *existing == PhpType::Mixed {
        PhpType::Mixed
    } else if Checker::union_contains_void(existing) {
        if *default_ty == PhpType::Void {
            existing.clone()
        } else {
            let non_null = checker.strip_void_from_union(existing);
            null_coalesce_merge_type(&non_null, default_ty)
        }
    } else {
        existing.clone()
    };
    Ok(result)
}

/// Type-checks a simple variable assignment (`$name = value`).
///
/// Handles null-coalescing assignment by extracting the default expression and combining
/// types appropriately. Preserves callable metadata when assigning closures or callable
/// expressions. Updates the flow-sensitive type environment with the assigned value type while
/// preserving any explicit local declaration contract.
///
/// On success, updates `env` with the resolved type for `name`. On error, returns a
/// type mismatch diagnostic.
///
/// `stmt_form` is `true` only when this assignment IS a statement (`StmtKind::Assign`).
/// An assignment used as an expression (`f($x = "s")`, `$y = ($x = "s")`) yields the
/// assigned value to its enclosing expression, so re-binding `$x` to a fresh slot of a
/// different type there has no single well-defined result value to hand back — only the
/// statement form is eligible for the permissive retype below.
pub(super) fn check_assign(
    checker: &mut Checker,
    name: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
    stmt_form: bool,
) -> Result<(), CompileError> {
    let suffix_key = (checker.current_loop_storage_scope.clone(), name.to_string());
    if let ExprKind::BinaryOp {
        op: crate::parser::ast::BinOp::Concat,
        right,
        ..
    } = &value.kind
    {
        if let ExprKind::StringLiteral(suffix) = &right.kind {
            checker.string_suffix_locals.insert(suffix_key, suffix.clone());
        } else {
            checker.string_suffix_locals.remove(&suffix_key);
        }
    } else {
        checker.string_suffix_locals.remove(&suffix_key);
    }

    // A direct reassignment (`$k = ...`) makes `$k` an ordinary local: it is no
    // longer the boxed `Mixed` `foreach` iteration key. Drop the foreach-key
    // marker so a subsequent `$dst[$k] = $v` is routed by `$k`'s real type (e.g.
    // a string key promotes the destination to `AssocArray`) instead of being
    // forced onto the `Array(Mixed)` / `Op::ArraySetMixedKey` path, which would
    // disagree with the lowering (it routes on the index's runtime IR type) and
    // produce a spurious `AssocArray -> Array(Mixed)` backend error.
    checker.foreach_key_locals.remove(name);

    // PHP allows compound assignment on an undefined variable (`$x += 1`),
    // treating the undefined variable as null/0 with a warning. The parser
    // lowers `$x += 1` to `Assign { name: "x", value: BinaryOp { left:
    // Variable("x"), op: Add, right: 1 } }`. When `$x` is not in `env`, the
    // inference of the BinaryOp would fail with "Undefined variable: $x".
    // Inject the variable as Void (null) so inference treats it as 0/null,
    // matching PHP semantics. `??=` is exempt: null-coalesce is designed to
    // handle undefined variables without warning.
    if !env.contains_key(name) && is_compound_assignment_self_read(value, name, span) {
        let is_null_coalesce = matches!(&value.kind, ExprKind::NullCoalesce { .. });
        env.insert(name.to_string(), PhpType::Void);
        if !is_null_coalesce {
            checker.warnings.push(CompileWarning {
                span,
                message: format!("Undefined variable: ${} (treated as null)", name),
                kind: crate::errors::WarningKind::Warning,
            });
        }
    }

    let null_coalesce_default = null_coalesce_assignment_default(name, value);
    let saved_self_ref_ty = if closure_captures_name_by_ref(value, name) {
        if let Some(previous_ty) = env.insert(name.to_string(), PhpType::Callable) {
            Some(Some(previous_ty))
        } else {
            Some(None)
        }
    } else {
        None
    };
    let callable_source = if let Some(default) = null_coalesce_default {
        if matches!(env.get(name), Some(existing) if *existing == PhpType::Void) {
            default
        } else {
            value
        }
    } else {
        value
    };
    let reflection_class_target = reflection_class_assignment_target(checker, callable_source, env);
    let ty_result: Result<PhpType, CompileError> = (|| {
        if let Some(default) = null_coalesce_default {
            if let Some(existing) = env.get(name).cloned() {
                let default_ty = if existing == PhpType::Void {
                    checker.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    checker.infer_type_with_assignment_effects(default, &mut default_env)?
                };
                null_coalesce_assignment_type(checker, name, &existing, &default_ty, span)
            } else {
                checker.infer_type_with_assignment_effects(value, env)
            }
        } else {
            checker.infer_type_with_assignment_effects(value, env)
        }
    })()
    .and_then(|ty| specialize_callable_array_assignment_type(checker, callable_source, ty, env));
    let metadata_result = match &ty_result {
        Ok(ty) => update_callable_assignment_metadata(checker, name, callable_source, ty, env),
        Err(_) => Ok(()),
    };
    if let Some(previous) = saved_self_ref_ty {
        match previous {
            Some(previous_ty) => {
                env.insert(name.to_string(), previous_ty);
            }
            None => {
                env.remove(name);
            }
        }
    }
    let ty = match ty_result {
        Ok(ty) => ty,
        Err(error) => {
            // Error recovery: the initializer failed to type, but the assignment
            // still names `$name`, so register it before propagating the real RHS
            // error. Without this, every later use of `$name` produces a
            // misleading follow-on `Undefined variable: $name` that buries the one
            // genuine diagnostic. Poison it as `Mixed` (unknown) only when it is
            // not already bound, so a valid earlier binding is preserved and no new
            // false diagnostics appear downstream.
            poison_unbound_local(env, name);
            return Err(error);
        }
    };
    if let Err(error) = metadata_result {
        poison_unbound_local(env, name);
        return Err(error);
    }
    update_reflection_class_assignment_metadata(checker, name, reflection_class_target);
    let merged = merge_local_assignment_type(checker, name, &ty, value, span, env, stmt_form);
    if merged.is_ok() {
        if checker.local_conditional_depth == 0 {
            let boxed_source = matches!(&value.kind, ExprKind::Variable(source)
                if checker.boxed_parameter_locals.contains(source));
            if boxed_source {
                checker.boxed_parameter_locals.insert(name.to_string());
            } else {
                checker.boxed_parameter_locals.remove(name);
            }
        }
        // Whatever the environment holds for `name` from here on was put there by this store,
        // so a guard that narrowed the name no longer speaks for it. `merge_local_assignment_type`
        // has already consulted the guard view, which is why this runs after it and not before.
        checker.record_store_over_flow_narrowing(name);
    }
    merged
}

/// Type-checks a reference alias assignment (`$target =& <source>`).
///
/// The source may be a plain variable (`$b`), an object property (`$obj->prop`), or a
/// call to a by-reference-returning callee (`f()`, `$o->m()`). For a property source the
/// property is recorded for reference-property promotion so codegen routes every access
/// through its ref-cell (write-through). The target is rebound to the source value type
/// and marked as by-reference storage.
pub(super) fn check_ref_assign(
    checker: &mut Checker,
    target: &str,
    source: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    if crate::names::append_update_alias_kind(target).is_some() {
        if let ExprKind::ArrayAccess { array, .. } = &source.kind {
            if let ExprKind::Variable(name) = &array.kind {
                if let Some(ty) = env.get(name).cloned() {
                    let widened = match ty {
                        PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
                        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
                            key, value: Box::new(PhpType::Mixed),
                        },
                        other => other,
                    };
                    env.insert(name.clone(), widened);
                }
            }
        }
    }
    let mut target_has_boxed_ref_provenance = match &source.kind {
        ExprKind::Variable(source_name) => {
            checker.boxed_ref_aliased_locals.contains(source_name)
        }
        _ => false,
    };
    // `=&` always replaces the target binding. Preserve provenance only when the new source is
    // itself the same managed Mixed cell, never from a prior binding of this name.
    checker.invalidate_boxed_ref_alias_binding(target);
    // Both sides of `$target =& <source>` share one cell from here on, so neither binding can
    // be killed or re-bound independently. Recorded before the per-shape checks so an aliasing
    // that fails a later validation is still treated as an alias.
    // Both names are REF-BOUND, not merely lent: the cell they share outlives this statement, so
    // lowering binds them to it and `store_local` keeps the cell's representation. Widening
    // either slot in the checker would therefore degrade to a store through the cell at a type
    // it cannot hold.
    // The hidden local of `$array[$key] = &$object->property` binds the property's cell for the
    // element marker; the property must store a boxed `Mixed` for the marker to write through it.
    let shares_property_with_element = match &source.kind {
        ExprKind::PropertyAccess { object, property }
            if target.starts_with(crate::parser::ast::ARRAY_PROPERTY_REF_TEMP_PREFIX) =>
        {
            super::super::control_flow::widen_property_for_element_reference(
                checker, object, property, env,
            )?
        }
        // A runtime-named property of a receiver known only at run time: every candidate is
        // widened to a boxed cell by `check_ref_assign_dynamic_property` below.
        ExprKind::DynamicPropertyAccess { object, .. }
            if target.starts_with(crate::parser::ast::ARRAY_PROPERTY_REF_TEMP_PREFIX) =>
        {
            let object_ty = checker.infer_type(object, env)?;
            crate::types::checker::single_object_class_name(&object_ty).is_none()
                || receiver_is_untyped_parameter(checker, object)
        }
        _ => false,
    };
    if target.starts_with(crate::parser::ast::ARRAY_PROPERTY_REF_TEMP_PREFIX)
        && !shares_property_with_element
    {
        return Err(CompileError::new(
            span,
            "An array element that references a property (`$array[$key] = &$object->property`) \
             is not supported by the native backend",
        ));
    }
    checker.ref_aliased_locals.insert(target.to_string());
    checker
        .non_builtin_ref_aliased_locals
        .insert(target.to_string());
    checker.ref_assign_aliased_locals.insert(target.to_string());
    checker.ref_bound_locals.insert(target.to_string());
    // A `=&` cell is not the boxed capture cell, so this name loses the widening exemption
    // even if a by-reference capture had granted it earlier in the body.
    checker.by_ref_capture_boxed_locals.remove(target);
    checker.record_ref_bound_alias_root(source);
    checker.record_ref_assign_alias_root(source);
    let result = match &source.kind {
        ExprKind::Variable(source_name) => {
            check_ref_assign_variable(checker, target, source_name, span, env)
        }
        ExprKind::PropertyAccess { object, property } => {
            let object_ty = checker.infer_type(object, env)?;
            // `$r = &$o->undeclared` creates the property (php 8.2+'s deprecated dynamic one), so
            // it is claimed before the read below asks whether the name exists.
            if let Some(class) = crate::types::checker::single_object_class_name(&object_ty) {
                crate::types::checker::scope_dynamic_storage::claim_undeclared_property_creation(
                    checker, &class, property, span,
                )?;
            }
            let target_ty = checker.infer_type(source, env)?;
            let known_class = crate::types::checker::single_object_class_name(&object_ty)
                .filter(|_| !receiver_is_untyped_parameter(checker, object));
            let target_ty = if let Some(class) = known_class {
                checker
                    .reference_property_promotions
                    .insert((class, property.clone()));
                target_ty
            } else if super::super::control_flow::widen_property_for_unknown_receiver_reference(
                checker, property,
            ) {
                PhpType::Mixed
            } else {
                target_ty
            };
            env.insert(target.to_string(), target_ty);
            checker.active_ref_params.insert(target.to_string());
            clear_callable_metadata(checker, target);
            Ok(())
        }
        ExprKind::DynamicPropertyAccess { object, property } => {
            check_ref_assign_dynamic_property(checker, target, object, property, span, env)
        }
        // `$n = &static::$names` (psysh's `ConsoleTput`): the local borrows the static's storage.
        ExprKind::StaticPropertyAccess { .. } => {
            let target_ty = checker.infer_type(source, env)?;
            env.insert(target.to_string(), target_ty);
            checker.active_ref_params.insert(target.to_string());
            clear_callable_metadata(checker, target);
            Ok(())
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::MethodCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. } => {
            for arg in args {
                promote_ref_call_property_argument(checker, arg, env)?;
            }
            let target_ty = checker.infer_type(source, env)?;
            env.insert(target.to_string(), target_ty);
            checker.active_ref_params.insert(target.to_string());
            clear_callable_metadata(checker, target);
            Ok(())
        }
        ExprKind::ArrayAccess { array, index } => {
            // A static WeakMap is an object with ArrayAccess places, not a PHP array: it takes the
            // generic element-reference path below (the lowering reads the entry cell).
            let static_is_weak_map = matches!(&array.kind, ExprKind::StaticPropertyAccess { .. })
                && matches!(
                    checker.infer_type(array, env)?,
                    PhpType::Object(ref name) if name.eq_ignore_ascii_case("WeakMap")
                );
            if let (false, ExprKind::StaticPropertyAccess { receiver, property }) =
                (static_is_weak_map, &array.kind)
            {
                let target_ty = super::static_properties::check_local_ref_assign_static_prop_element(
                    checker,
                    receiver,
                    property,
                    index,
                    span,
                    env,
                )?;
                env.insert(target.to_string(), target_ty);
                checker.active_ref_params.insert(target.to_string());
                clear_callable_metadata(checker, target);
                return Ok(());
            }
            // `$r = &$obj->prop[$k]` (Symfony `Translator::trans`): lowering reaches the element
            // through the property's reference cell, so the property becomes a reference property
            // with element storage that can hold a shared cell -- exactly what a by-reference
            // `foreach` over the same property needs.
            let mut root = array.as_ref();
            while let ExprKind::ArrayAccess { array: inner, .. } = &root.kind {
                root = inner;
            }
            if matches!(root.kind, ExprKind::PropertyAccess { .. }) {
                super::super::control_flow::widen_by_ref_foreach_source_storage(
                    checker, array, env,
                )?;
            }
            if matches!(root.kind, ExprKind::Variable(_)) {
                super::super::control_flow::widen_local_root_for_element_reference(
                    checker, source, env,
                )?;
            }
            let array_ty = checker.infer_type(array, env)?;
            target_has_boxed_ref_provenance = match array_ty.codegen_repr() {
                PhpType::AssocArray { .. } | PhpType::Mixed => true,
                PhpType::Array(element) => element.codegen_repr() == PhpType::Mixed,
                _ => false,
            };
            let index_ty = checker.infer_type(index, env)?;
            // A local holding a boxed value -- an untyped by-reference parameter, typically --
            // makes the element a reference inside the array the box holds
            // (`__rt_mixed_elem_ref_cell`), vivifying a null into an empty array as PHP does.
            if matches!(array.kind, ExprKind::Variable(_))
                && matches!(array_ty.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
            {
                env.insert(target.to_string(), PhpType::Mixed);
                checker.active_ref_params.insert(target.to_string());
                clear_callable_metadata(checker, target);
                return Ok(());
            }
            let normalized_index_ty = normalized_array_key_type(index, index_ty.clone());
            // `string|int $index` (Symfony's `PropertyAccessor::readIndex()`) is a key either way.
            let normalized_index_ty = match &normalized_index_ty {
                PhpType::Union(members)
                    if members.iter().all(|member| matches!(member, PhpType::Int | PhpType::Str)) =>
                {
                    PhpType::Mixed
                }
                _ => normalized_index_ty,
            };
            let valid_php_key = matches!(
                normalized_index_ty,
                PhpType::Int | PhpType::Str | PhpType::Mixed
            );
            let supported_key = match array_ty.codegen_repr() {
                PhpType::Object(ref name) if name.eq_ignore_ascii_case("WeakMap") => true,
                PhpType::Array(element) if element.codegen_repr() == PhpType::Mixed => {
                    valid_php_key
                }
                PhpType::Array(_) => normalized_index_ty == PhpType::Int,
                PhpType::AssocArray { .. } | PhpType::Mixed | PhpType::Union(_) => {
                    valid_php_key
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "Reference assignment to an array element requires an array",
                    ));
                }
            };
            if !supported_key {
                return Err(CompileError::new(
                    span,
                    "Reference assignment to an array element requires an integer or string array key",
                ));
            }
            if matches!(&array_ty, PhpType::AssocArray { .. })
                && !super::properties::is_php_array_key_type(&index_ty)
            {
                return Err(CompileError::new(span, "Invalid associative array key type"));
            }
            let target_ty = checker.infer_type(source, env)?;
            env.insert(target.to_string(), target_ty);
            checker.active_ref_params.insert(target.to_string());
            clear_callable_metadata(checker, target);
            Ok(())
        }
        _ => Err(CompileError::new(
            span,
            "Reference assignment source must be a variable, array/property element, or a by-reference call",
        )),
    };
    if let Err(error) = result {
        poison_unbound_local(env, target);
        return Err(error);
    }
    if crate::names::append_update_alias_kind(target).is_some() {
        // The parser's writable-cell lease is representation-neutral, not a value snapshot.
        env.insert(target.to_string(), PhpType::Mixed);
        target_has_boxed_ref_provenance = true;
    }
    if target_has_boxed_ref_provenance {
        checker.boxed_ref_aliased_locals.insert(target.to_string());
    }
    Ok(())
}

/// Returns whether `object` names an untyped parameter of the body being checked.
///
/// The checker narrows such a parameter to the class its first call site passes, but the
/// signature keeps that class even when other call sites pass other classes, and lowering stores
/// the parameter as `Mixed`. A reference into one of its properties must therefore be resolved
/// for every class at run time, never promoted for the narrowed class alone.
fn receiver_is_untyped_parameter(checker: &Checker, object: &Expr) -> bool {
    let ExprKind::Variable(name) = &object.kind else {
        return false;
    };
    let sig = match (checker.current_class.as_deref(), checker.current_method.as_deref()) {
        (Some(class), Some(method)) => checker.classes.get(class).and_then(|info| {
            info.methods.get(method).or_else(|| info.static_methods.get(method))
        }),
        _ => checker
            .current_function
            .as_deref()
            .and_then(|function| checker.functions.get(function)),
    };
    sig.and_then(|sig| {
        let index = sig.params.iter().position(|(param, _)| param == name)?;
        Some(!sig.declared_params.get(index).copied().unwrap_or(true))
    })
    .unwrap_or(false)
}

/// Type-checks a local reference alias to a runtime-named declared property.
///
/// A literal name selects one slot. A local whose value has a known string suffix selects only
/// declared slots with that suffix. Every selected slot must have a compatible runtime layout;
/// those slots are then promoted to owned reference cells before code generation.
fn check_ref_assign_dynamic_property(
    checker: &mut Checker,
    target: &str,
    object: &Expr,
    property: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let object_ty = checker.infer_type(object, env)?;
    let known_class = crate::types::checker::single_object_class_name(&object_ty)
        .filter(|_| !receiver_is_untyped_parameter(checker, object));
    let property_ty = checker.infer_type(property, env)?;
    // php converts the name like any string context, and the lowering coerces it the same way
    // (`lower_ref_assign_dynamic_property`): nette/utils' `Arrays::associate()` binds
    // `$x = &$x->{$row[$parts[$i]]}` with a `mixed` array element as the name.
    if matches!(property_ty.codegen_repr(), PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Err(CompileError::new(
            property.span,
            "Dynamic property reference name must be a string",
        ));
    }

    let exact_name = match &property.kind {
        ExprKind::StringLiteral(name) => Some(name.clone()),
        _ => None,
    };
    let suffix = match &property.kind {
        ExprKind::Variable(name) => checker
            .string_suffix_locals
            .get(&(checker.current_loop_storage_scope.clone(), name.clone()))
            .cloned(),
        _ => None,
    };
    // A stdClass property is an entry of the instance's own hash, which the backend makes a
    // reference in place for any name (`__rt_stdclass_prop_ref_cell`).
    // Also for an untyped parameter narrowed to stdClass: widening every visible declared
    // property would only change layouts the receiver cannot reach.
    if crate::types::checker::single_object_class_name(&object_ty)
        .as_deref()
        .is_some_and(crate::types::checker::builtin_stdclass::is_stdclass)
    {
        checker.dynamic_ref_local_types.insert(
            (checker.current_loop_storage_scope.clone(), target.to_string()),
            PhpType::Mixed,
        );
        env.insert(target.to_string(), PhpType::Mixed);
        checker.active_ref_params.insert(target.to_string());
        clear_callable_metadata(checker, target);
        return Ok(());
    }
    let Some(class_name) = known_class else {
        // The receiver's class is only known at run time (an untyped `$object`), so every
        // property this scope can see -- narrowed by a literal or known suffix -- may be the one
        // referenced, and each becomes a boxed cell the runtime class and name ladder resolves.
        let widened = super::super::control_flow::widen_properties_for_unknown_receiver_reference(
            checker,
            |name| match (&exact_name, &suffix) {
                (Some(exact), _) => name == exact,
                (None, Some(suffix)) => name.ends_with(suffix.as_str()),
                (None, None) => true,
            },
        );
        if !widened && !matches!(object_ty.codegen_repr(), PhpType::Mixed | PhpType::Object(_)) {
            return Err(CompileError::new(
                span,
                "Dynamic property reference requires an object receiver",
            ));
        }
        checker.dynamic_ref_local_types.insert(
            (checker.current_loop_storage_scope.clone(), target.to_string()),
            PhpType::Mixed,
        );
        env.insert(target.to_string(), PhpType::Mixed);
        checker.active_ref_params.insert(target.to_string());
        clear_callable_metadata(checker, target);
        return Ok(());
    };
    let exact_name = exact_name.as_deref();
    let suffix = suffix.as_deref();
    if exact_name.is_none() && suffix.is_none() {
        // A name known only at run time (`$r = &$o->$name`): any property of the receiver's
        // runtime class may be the one referenced, so each becomes a boxed cell the runtime name
        // ladder resolves -- the unknown-receiver contract, limited to this class and its
        // subclasses. An undeclared name lands in the instance's dynamic-property hash.
        let normalized_class = class_name.trim_start_matches('\\').to_string();
        let receivers = checker
            .classes
            .keys()
            .filter(|candidate| {
                candidate.as_str() == normalized_class
                    || checker.is_subclass_of(candidate, &normalized_class)
            })
            .cloned()
            .collect::<crate::fast_hash::FastSet<_>>();
        super::super::control_flow::widen_properties_for_receiver_reference(
            checker,
            Some(&receivers),
            |_| true,
        );
        checker.dynamic_ref_local_types.insert(
            (checker.current_loop_storage_scope.clone(), target.to_string()),
            PhpType::Mixed,
        );
        env.insert(target.to_string(), PhpType::Mixed);
        checker.active_ref_params.insert(target.to_string());
        clear_callable_metadata(checker, target);
        return Ok(());
    }

    let normalized_class = class_name.trim_start_matches('\\');
    let candidates = checker
        .classes
        .get(normalized_class)
        .ok_or_else(|| CompileError::new(span, &format!("Unknown class: {}", normalized_class)))?
        .properties
        .iter()
        .filter(|(name, _)| {
            exact_name.is_some_and(|exact| name == exact)
                || suffix.is_some_and(|suffix| name.ends_with(suffix))
        })
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Err(CompileError::new(
            property.span,
            "Dynamic property reference does not match a declared property",
        ));
    }

    let mut candidate_ty = candidates[0].1.clone();
    for (_, next_ty) in &candidates[1..] {
        candidate_ty = merge_dynamic_reference_candidate_type(checker, candidate_ty, next_ty)
            .ok_or_else(|| {
                CompileError::new(
                    property.span,
                    "Dynamic property reference candidates have incompatible storage types",
                )
            })?;
    }
    for (property_name, _) in candidates {
        checker
            .reference_property_promotions
            .insert((normalized_class.to_string(), property_name));
    }
    checker.dynamic_ref_local_types.insert(
        (checker.current_loop_storage_scope.clone(), target.to_string()),
        candidate_ty.clone(),
    );
    env.insert(target.to_string(), candidate_ty);
    checker.active_ref_params.insert(target.to_string());
    clear_callable_metadata(checker, target);
    Ok(())
}

/// Merges compatible candidate types for one runtime-named property reference.
fn merge_dynamic_reference_candidate_type(
    checker: &Checker,
    current: PhpType,
    next: &PhpType,
) -> Option<PhpType> {
    if &current == next {
        return Some(current);
    }
    match (current, next) {
        (PhpType::Array(left), PhpType::Array(right)) => Some(PhpType::Array(Box::new(
            checker
                .merge_array_element_type(&left, right)
                .unwrap_or(PhpType::Mixed),
        ))),
        (
            PhpType::AssocArray {
                key: left_key,
                value: left_value,
            },
            PhpType::AssocArray {
                key: right_key,
                value: right_value,
            },
        ) => Some(PhpType::AssocArray {
            key: Box::new(
                checker
                    .merge_array_element_type(&left_key, right_key)
                    .unwrap_or(PhpType::Mixed),
            ),
            value: Box::new(
                checker
                    .merge_array_element_type(&left_value, right_value)
                    .unwrap_or(PhpType::Mixed),
            ),
        }),
        _ => None,
    }
}

/// Type-checks `$target =& $source` where the source is a plain variable.
/// Promotes a property passed to a by-reference call so `=&` can later alias its storage.
///
/// `function &grab(&$x) { return $x; } $r = &grab($obj->prop);` must leave `$r` aliasing the
/// property itself, not the hidden temporary used to call `grab`. The property needs reference
/// storage before codegen; the direct-property RefAssign branch records the same promotion.
fn promote_ref_call_property_argument(
    checker: &mut Checker,
    arg: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let mut place = arg;
    while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &place.kind {
        place = value;
    }
    let ExprKind::PropertyAccess { object, property } = &place.kind else {
        return Ok(());
    };
    let object_ty = checker.infer_type(object, env)?;
    if let Some(class) = crate::types::checker::single_object_class_name(&object_ty) {
        checker
            .reference_property_promotions
            .insert((class, property.clone()));
    } else {
        let owners: Vec<String> = checker
            .classes
            .iter()
            .filter(|(_, info)| info.properties.iter().any(|(name, _)| name == property))
            .map(|(name, _)| name.clone())
            .collect();
        for owner in owners {
            checker
                .reference_property_promotions
                .insert((owner, property.clone()));
        }
    }
    Ok(())
}

fn check_ref_assign_variable(
    checker: &mut Checker,
    target: &str,
    source: &str,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let source_ty = env.get(source).cloned().ok_or_else(|| {
        CompileError::new(span, &format!("Undefined variable: ${}", source))
    })?;
    if target == source {
        return Ok(());
    }
    env.insert(target.to_string(), source_ty.clone());
    checker.active_ref_params.insert(target.to_string());
    if source_ty == PhpType::Callable || is_callable_array_type(&source_ty) {
        copy_callable_metadata(checker, target, source);
    } else {
        clear_callable_metadata(checker, target);
    }
    copy_reflection_class_metadata(checker, target, source);
    Ok(())
}


/// Returns `true` if `value` is a closure that captures `name` both by value and by reference.
///
/// Used to detect self-referential assignments (`$x = fn() use($x) { ... }`) where the
/// variable being assigned is captured by the closure on both sides of the assignment.
/// When detected, the variable's type is temporarily promoted to `Callable` during inference.
fn closure_captures_name_by_ref(value: &Expr, name: &str) -> bool {
    matches!(
        &value.kind,
        ExprKind::Closure {
            captures,
            capture_refs,
            ..
        } if captures.iter().any(|capture| capture == name)
            && capture_refs.iter().any(|capture| capture == name)
    )
}

impl Checker {
    /// Type-checks a local variable assignment and returns the resulting type.
    ///
    /// Validates that the variable exists in `env` after assignment, returning its type.
    /// Used by the checker when processing assignment expressions to propagate the
    /// assigned type back to the caller.
    ///
    /// This is the EXPRESSION form of an assignment, so it passes `stmt_form: false`: an
    /// incompatible retype here keeps the hard error in both modes.
    pub(crate) fn check_local_assignment_expression(
        &mut self,
        name: &str,
        value: &Expr,
        span: Span,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        check_assign(self, name, value, span, env, false)?;
        env.get(name).cloned().ok_or_else(|| {
            CompileError::new(span, &format!("Undefined variable: ${}", name))
        })
    }
}

/// Updates callability metadata when assigning a callable expression to a variable.
///
/// When `ty` is `Callable`, or a string-valued expression names a statically known callable,
/// extracts and stores the callable signature, closure return type, capture list, and first-class
/// callable target on the checker. Other values clear previously stored metadata for `name`.
///
/// This ensures that subsequent uses of the variable can resolve its callable signature
/// and closure metadata. Handles closures, variables, array access, and first-class callables.
pub(super) fn update_callable_assignment_metadata(
    checker: &mut Checker,
    name: &str,
    callable_source: &Expr,
    ty: &PhpType,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    if let ExprKind::StringLiteral(callback) = &callable_source.kind {
        if let Some(function_name) = checker.canonical_function_name_folded(callback) {
            // Lowering tracks a statically known function-name string as a descriptor target.
            // Promote its variadic storage at the same point so a later dynamic
            // `call_user_func_array($name, $args)` can preserve unknown string keys in the tail.
            checker.promote_descriptor_variadic_container(&function_name)?;
        }
    }
    update_callable_array_assignment_metadata(checker, name, callable_source, env)?;
    let resolved_callable_sig = checker.resolve_expr_callable_sig(callable_source, env)?;

    if *ty == PhpType::Callable || resolved_callable_sig.is_some() {
        if let Some(sig) = resolved_callable_sig {
            checker
                .closure_return_types
                .insert(name.to_string(), sig.return_type.clone());
            checker.callable_sigs.insert(name.to_string(), sig);
            if let ExprKind::Closure {
                captures,
                capture_refs,
                ..
            } = &callable_source.kind
            {
                let capture_types = captures
                    .iter()
                    .map(|capture| {
                        (
                            capture.clone(),
                            env.get(capture).cloned().unwrap_or(PhpType::Mixed),
                            capture_refs.iter().any(|ref_capture| ref_capture == capture)
                                || checker.ref_aliased_locals.contains(capture),
                        )
                    })
                    .collect();
                checker
                    .callable_captures
                    .insert(name.to_string(), capture_types);
            } else if let ExprKind::Variable(src_name) = &callable_source.kind {
                if let Some(captures) = checker.callable_captures.get(src_name).cloned() {
                    checker.callable_captures.insert(name.to_string(), captures);
                } else {
                    checker.callable_captures.remove(name);
                }
            } else if let ExprKind::ArrayAccess { array, .. } = &callable_source.kind {
                if let ExprKind::Variable(src_name) = &array.kind {
                    if let Some(captures) = checker.callable_captures.get(src_name).cloned() {
                        checker.callable_captures.insert(name.to_string(), captures);
                    } else {
                        checker.callable_captures.remove(name);
                    }
                } else {
                    checker.callable_captures.remove(name);
                }
            } else {
                checker.callable_captures.remove(name);
            }
            if let ExprKind::FirstClassCallable(target) = &callable_source.kind {
                checker
                    .first_class_callable_targets
                    .insert(name.to_string(), target.clone());
            } else if let Some(function) = declared_function_named_by_literal(checker, callable_source) {
                // `$fn = 'h'` names the same callee as `$fn = h(...)`, so `call_user_func($fn, …)`
                // specializes `h` for its arguments instead of reading the untyped parameters'
                // `Int` placeholder.
                checker.first_class_callable_targets.insert(
                    name.to_string(),
                    CallableTarget::Function(crate::names::Name::from(function)),
                );
            } else if let ExprKind::Variable(src_name) = &callable_source.kind {
                if let Some(target) = checker.first_class_callable_targets.get(src_name).cloned() {
                    checker
                        .first_class_callable_targets
                        .insert(name.to_string(), target);
                } else {
                    checker.first_class_callable_targets.remove(name);
                }
            } else if let ExprKind::ArrayAccess { array, .. } = &callable_source.kind {
                if let ExprKind::Variable(src_name) = &array.kind {
                    if let Some(target) =
                        checker.first_class_callable_targets.get(src_name).cloned()
                    {
                        checker
                            .first_class_callable_targets
                            .insert(name.to_string(), target);
                    } else {
                        checker.first_class_callable_targets.remove(name);
                    }
                } else {
                    checker.first_class_callable_targets.remove(name);
                }
            } else {
                checker.first_class_callable_targets.remove(name);
            }
        } else {
            checker.closure_return_types.remove(name);
            checker.callable_sigs.remove(name);
            checker.callable_captures.remove(name);
            checker.first_class_callable_targets.remove(name);
        }
    } else if is_callable_array_type(ty) {
        if let Some(sig) = checker.resolve_expr_callable_array_sig(callable_source, env)? {
            checker
                .closure_return_types
                .insert(name.to_string(), sig.return_type.clone());
            checker.callable_sigs.insert(name.to_string(), sig);
            if let ExprKind::Variable(src_name) = &callable_source.kind {
                if let Some(captures) = checker.callable_captures.get(src_name).cloned() {
                    checker.callable_captures.insert(name.to_string(), captures);
                } else {
                    checker.callable_captures.remove(name);
                }
                if let Some(target) = checker.callable_array_targets.get(src_name).cloned() {
                    checker
                        .callable_array_targets
                        .insert(name.to_string(), target);
                } else {
                    checker.callable_array_targets.remove(name);
                }
                if let Some(target) = checker.first_class_callable_targets.get(src_name).cloned() {
                    checker
                        .first_class_callable_targets
                        .insert(name.to_string(), target);
                } else {
                    checker.first_class_callable_targets.remove(name);
                }
            } else {
                checker.callable_captures.remove(name);
                checker.callable_array_targets.remove(name);
                checker.first_class_callable_targets.remove(name);
            }
        } else {
            clear_callable_metadata(checker, name);
        }
    } else {
        checker.closure_return_types.remove(name);
        checker.callable_sigs.remove(name);
        checker.callable_captures.remove(name);
        checker.first_class_callable_targets.remove(name);
    }
    Ok(())
}

/// Updates static `ReflectionClass` metadata for one assigned local.
fn update_reflection_class_assignment_metadata(
    checker: &mut Checker,
    name: &str,
    reflected_class: Option<String>,
) {
    if let Some(reflected_class) = reflected_class {
        checker
            .reflection_class_targets
            .insert(name.to_string(), reflected_class);
    } else {
        checker.reflection_class_targets.remove(name);
    }
}

/// Mirrors static `ReflectionClass` metadata across a reference alias assignment.
fn copy_reflection_class_metadata(checker: &mut Checker, target: &str, source: &str) {
    if let Some(reflected_class) = checker.reflection_class_targets.get(source).cloned() {
        checker
            .reflection_class_targets
            .insert(target.to_string(), reflected_class);
    } else {
        checker.reflection_class_targets.remove(target);
    }
}

/// Resolves the statically-known class reflected by a `new ReflectionClass(...)` expression.
fn reflection_class_assignment_target(
    checker: &Checker,
    source: &Expr,
    env: &TypeEnv,
) -> Option<String> {
    let ExprKind::NewObject { class_name, args } = &source.kind else {
        return None;
    };
    if php_symbol_key(class_name.as_str().trim_start_matches('\\')) != "reflectionclass" {
        return None;
    }
    let class_arg = reflection_class_constructor_arg(args)?;
    match &class_arg.kind {
        ExprKind::StringLiteral(class_name) => {
            resolve_class_name(checker, class_name).map(str::to_string)
        }
        ExprKind::ClassConstant { receiver } => {
            resolve_static_receiver_class(checker, receiver, class_arg.span).ok()
        }
        _ => match env
            .get(variable_name(class_arg).unwrap_or_default())
            .cloned()
            .unwrap_or_else(|| crate::types::checker::infer_expr_type_syntactic(class_arg))
            .codegen_repr()
        {
            PhpType::Object(class_name) if !class_name.is_empty() => Some(class_name),
            _ => None,
        },
    }
}

/// Returns the ReflectionClass constructor class argument after simple named-argument matching.
fn reflection_class_constructor_arg(args: &[Expr]) -> Option<&Expr> {
    if let Some(positional) = args.iter().find(|arg| !matches!(arg.kind, ExprKind::NamedArg { .. })) {
        return Some(positional);
    }
    args.iter().find_map(|arg| {
        let ExprKind::NamedArg { name, value } = &arg.kind else {
            return None;
        };
        match php_symbol_key(name).as_str() {
            "class" | "classname" | "class_name" | "objectorclass" | "object_or_class" => {
                Some(value.as_ref())
            }
            _ => None,
        }
    })
}

/// Returns true when a local assignment stores an array whose elements are callable descriptors.
fn is_callable_array_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(elem_ty) => elem_ty.as_ref() == &PhpType::Callable,
        PhpType::AssocArray { value, .. } => value.as_ref() == &PhpType::Callable,
        _ => false,
    }
}

/// Specializes generic array assignment types when callable-array metadata is known.
fn specialize_callable_array_assignment_type(
    checker: &mut Checker,
    callable_source: &Expr,
    ty: PhpType,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
    if is_callable_array_type(&ty) {
        return Ok(ty);
    }
    if !matches!(ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
        return Ok(ty);
    }
    if checker
        .resolve_expr_callable_array_sig(callable_source, env)?
        .is_none()
    {
        return Ok(ty);
    }
    Ok(match ty {
        PhpType::AssocArray { key, .. } => PhpType::AssocArray {
            key,
            value: Box::new(PhpType::Callable),
        },
        _ => PhpType::Array(Box::new(PhpType::Callable)),
    })
}

/// Provides the Update callable array assignment metadata helper used by the locals module.
fn update_callable_array_assignment_metadata(
    checker: &mut Checker,
    name: &str,
    callable_source: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    let copied_target_version = match &callable_source.kind {
        ExprKind::Variable(source_name) => checker
            .callable_array_target_versions
            .get(source_name)
            .copied(),
        _ => None,
    };
    checker.mark_callable_array_target_write(name);
    if let Some(target) = resolve_callable_array_target(checker, callable_source, env)? {
        // Recording the target is the MATERIALIZATION of a callable descriptor: from here the
        // pair can be invoked with named arguments, and it can also leave this frame through a
        // `callable` parameter and be invoked where no target record survives. So the callee's
        // variadic collector moves onto the descriptor container now, not at the call, which is
        // the only point that is guaranteed to be reached.
        checker.promote_descriptor_variadic_container_for_callable_target(&target, env)?;
        checker
            .callable_array_targets
            .insert(name.to_string(), target);
    } else if let ExprKind::Variable(src_name) = &callable_source.kind {
        if let Some(target) = checker.callable_array_targets.get(src_name).cloned() {
            checker
                .callable_array_targets
                .insert(name.to_string(), target);
            if let Some(version) = copied_target_version {
                checker
                    .callable_array_target_versions
                    .insert(name.to_string(), version);
            }
        } else {
            checker.callable_array_targets.remove(name);
        }
    } else {
        checker.callable_array_targets.remove(name);
    }
    Ok(())
}

/// The canonical name of the declared user function a string literal names, if any.
///
/// A builtin or an undeclared name is not a first-class target: it keeps the signature the
/// assignment already recorded.
fn declared_function_named_by_literal(checker: &Checker, expr: &Expr) -> Option<String> {
    let ExprKind::StringLiteral(name) = &expr.kind else {
        return None;
    };
    let canonical = checker.canonical_function_name_folded(name)?;
    checker.fn_decls.contains_key(&canonical).then_some(canonical)
}

/// Resolves callable array target using the available compile-time metadata.
fn resolve_callable_array_target(
    checker: &Checker,
    expr: &Expr,
    env: &TypeEnv,
) -> Result<Option<CallableTarget>, CompileError> {
    let Some((receiver, method)) = callable_array_parts(expr) else {
        return Ok(None);
    };
    if let Some(receiver) = static_callable_receiver(checker, receiver, expr.span)? {
        return Ok(Some(CallableTarget::StaticMethod {
            receiver,
            method: method.to_string(),
        }));
    }
    let receiver_ty = env
        .get(variable_name(receiver).unwrap_or_default())
        .cloned()
        .unwrap_or_else(|| crate::types::checker::infer_expr_type_syntactic(receiver));
    if let Some(class_name) = checker.invokable_class_for_type(&receiver_ty) {
        // An unreachable method is a runtime `TypeError` (or a `__call()`), not a binding.
        if checker.instance_callable_method_unreachable(&class_name, method) {
            return Ok(None);
        }
        return Ok(Some(CallableTarget::Method {
            object: Box::new(receiver.clone()),
            method: method.to_string(),
        }));
    }
    Ok(None)
}

/// Provides the Callable array parts helper used by the locals module.
fn callable_array_parts(expr: &Expr) -> Option<(&Expr, &str)> {
    let elems = match &expr.kind {
        ExprKind::ArrayLiteral(elems) => elems,
        _ => return None,
    };
    if elems.len() != 2 {
        return None;
    }
    let ExprKind::StringLiteral(method) = &elems[1].kind else {
        return None;
    };
    Some((&elems[0], method.as_str()))
}

/// Provides the Variable name helper used by the locals module.
fn variable_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name),
        _ => None,
    }
}

/// Provides the Static callable receiver helper used by the locals module.
fn static_callable_receiver(
    checker: &Checker,
    receiver: &Expr,
    span: Span,
) -> Result<Option<StaticReceiver>, CompileError> {
    let class_name = match &receiver.kind {
        ExprKind::StringLiteral(class_name) => resolve_class_name(checker, class_name)
            .map(str::to_string),
        ExprKind::ClassConstant { receiver } => {
            Some(resolve_static_receiver_class(checker, receiver, span)?)
        }
        _ => None,
    };
    Ok(class_name.map(|class_name| StaticReceiver::Named(Name::from(class_name))))
}

/// Resolves static receiver class using the available compile-time metadata.
fn resolve_static_receiver_class(
    checker: &Checker,
    receiver: &StaticReceiver,
    span: Span,
) -> Result<String, CompileError> {
    // A receiver written `Box<int>::of()` names `Box` until instantiation renames
    // it, and this pass can run on a generic function's template body — which is
    // walked and then stripped, never instantiated.
    let receiver = &receiver.written_class_receiver();
    match receiver {
        // A generic receiver is instantiated into an ordinary named one before type checking;
        // a template has no class to reach through.
        StaticReceiver::Generic(_) => unreachable!(
            "StaticReceiver::Generic must be instantiated by generics::classes"
        ),
        StaticReceiver::Named(name) => resolve_class_name(checker, name.as_str())
            .map(str::to_string)
            .ok_or_else(|| CompileError::new(span, &format!("Undefined class: {}", name))),
        StaticReceiver::Self_ | StaticReceiver::Static => checker
            .current_class
            .clone()
            .ok_or_else(|| CompileError::new(span, "Cannot use self::class outside a class context")),
        StaticReceiver::Parent => {
            let current_class = checker.current_class.as_ref().ok_or_else(|| {
                CompileError::new(span, "Cannot use parent::class outside a class context")
            })?;
            checker
                .classes
                .get(current_class)
                .and_then(|class_info| class_info.parent.clone())
                .ok_or_else(|| {
                    CompileError::new(
                        span,
                        &format!("Class '{}' has no parent class", current_class),
                    )
                })
        }
    }
}

/// Resolves class name using the available compile-time metadata.
fn resolve_class_name<'a>(checker: &'a Checker, class_name: &str) -> Option<&'a str> {
    let wanted = class_name.trim_start_matches('\\');
    checker
        .classes
        .keys()
        .find(|existing| existing.eq_ignore_ascii_case(wanted))
        .map(String::as_str)
}

/// Records the new flow-sensitive type of an ordinary local assignment.
///
/// Ordinary PHP locals may change runtime representation on each assignment. An explicit
/// typed-local declaration is the only local form that keeps a persistent type contract and
/// therefore validates later writes before preserving its declared type in the environment;
/// everything below applies to the untyped, flow-sensitive ones.
///
/// If `name` already exists in `env`, attempts to merge the new type with the existing
/// type using `checker.merged_assignment_type()`. If merging is not possible, returns
/// a type incompatibility error. If `name` does not exist, inserts the type directly.
///
/// The merge operation supports widening (e.g., `Int | Float` from separate assignments)
/// and preserves type specificity where possible.
///
/// When the two types cannot merge, the assignment either RE-BINDS `name` to a fresh
/// binding of the new type (permissive default) or stays a hard error (`--strict-locals`,
/// and every shape a re-bind would be unsound for). Re-binding is what PHP does — the old
/// value is simply dropped — and it is only safe where the store definitely runs and no
/// reference can still reach the old cell, which is exactly
/// [`Checker::local_binding_is_killable`].
///
/// A COMPATIBLE merge between two different numeric types may re-bind as well, silently and
/// for storage alone: see [`splits_numeric_binding`]. `value` is the assigned expression,
/// which that decision inspects.
fn merge_local_assignment_type(
    checker: &mut Checker,
    name: &str,
    ty: &PhpType,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
    stmt_form: bool,
) -> Result<(), CompileError> {
    // An explicitly DECLARED local (`int $x = …`) carries a persistent type contract instead of
    // the flow-sensitive binding below: a later write is validated against the declaration and
    // the environment keeps the DECLARED type, so an `int` stored into a declared `float` stays
    // `float`. Recorded per function-like scope by `check_typed_assign`.
    let declaration_key = (checker.current_loop_storage_scope.clone(), name.to_string());
    if let Some(declared) = checker.declared_local_types.get(&declaration_key) {
        if !checker.type_accepts(declared, ty) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot reassign ${} from {} to {}",
                    name, declared, ty
                ),
            ));
        }
        let declared = declared.clone();
        env.insert(name.to_string(), declared);
        return Ok(());
    }
    // A SUPERGLOBAL is a declaration too, and the one place a "reassignment" is not a rebinding at
    // all: `$_GET = […]` writes the program's request storage, a hash with `Str` keys and `Mixed`
    // values that every scope reaches by name (`superglobals::superglobal_type`). Nothing about
    // the assignment can change that type, so — exactly like the declared-local arm above — the
    // environment keeps it and the value is validated against it instead of merged with it.
    //
    // `Request::overrideGlobals()` is the shape: `$_GET = $this->query->all();` and its four
    // siblings, plus `$_REQUEST = [[]];`. Each was `cannot reassign $_GET from
    // array<string, mixed> to array<mixed>` because the merge treated the superglobal as an
    // ordinary local binding, and because the widening arm below refuses it for the RIGHT reason —
    // storage this frame does not own is not this frame's to re-represent
    // (`Checker::name_is_seeded_program_storage`). Neither is what a superglobal assignment does:
    // it stores INTO that storage, and `ir_lower::stmt::local_assignments` already converts the
    // value to the superglobal's contract on the way in.
    //
    // Only an ARRAY is accepted here. PHP lets a superglobal hold any value, but the storage this
    // compiler gives it is a hash: publishing a scalar through the symbol every reader
    // dereferences as one would be a silent wrong answer, so `$_GET = 5;` keeps its loud error
    // until that storage is gradual.
    if crate::superglobals::is_superglobal(name) {
        let superglobal = crate::superglobals::superglobal_type();
        if matches!(
            ty.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Mixed
        ) {
            env.insert(name.to_string(), superglobal);
            return Ok(());
        }
        if let Some(existing) = env.get(name) {
            if checker.merged_assignment_type(existing, ty).is_none() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Type error: cannot reassign ${} from {} to {}",
                        name, existing, ty
                    ),
                ));
            }
        }
        env.insert(name.to_string(), superglobal);
        return Ok(());
    }
    // This visit RE-DECIDES the site, so any decision a superseded walk recorded for it is
    // dropped first. The checker walks a body more than once (top level twice, method bodies
    // to stability, a function body once per call-site re-specialization) and only the LAST
    // walk's decisions may reach EIR lowering — a retype recorded against a type the final
    // walk no longer infers would otherwise make lowering re-bind a compatible assignment.
    // Mirrors the `unset` kill sites in `inference::expr::effects`, and the per-scope clear
    // `loop_storage_types` does before each re-walk.
    //
    // Qualified by NAME, because a `Span` has no file identity: the same (line, column) in an
    // included file is an EQUAL span, and this visit must not drop the decision recorded for
    // that unrelated assignment — a `$x = …` in the main file would otherwise silently disarm
    // the retype of a `$w = …` at the same position in a library. Exactly ONE name leaves the
    // span's set of re-bound locals; the key itself goes only once the set is empty.
    checker.numeric_region_retypes.remove(&(span, name.to_owned()));
    if checker
        .local_retype_sites
        .get(&span)
        .is_some_and(|retyped| retyped.contains(name))
    {
        if let Some(retyped) = checker.local_retype_sites.get_mut(&span) {
            retyped.remove(name);
            if retyped.is_empty() {
                checker.local_retype_sites.remove(&span);
            }
        }
        // The WARNING is part of the decision and goes with it. A superseded walk's warning used
        // to survive its own retraction, which made the diagnostic depend on the order of a
        // function's call sites — see `Checker::binding_decision_warnings`.
        checker
            .binding_decision_warnings
            .remove(&(span, name.to_string()));
    }
    // The syntactic pre-scan's mark is AUTHORITATIVE, on every assignment path and not just at the
    // first store: the name's frame slot is boxed `Mixed` for the whole body (see
    // `checker::mixed_storage_scan`), so `Mixed` is the only type the environment may hold for it.
    //
    // Consulting the mark on the fresh-insert branch alone left the invariant falsifiable by
    // FLOW NARROWING. `control_flow` inserts a guard's narrowed type into the shared environment
    // for the guarded body, so `if (…) { $a = 1; } else { $a = "s"; } if (is_int($a)) { $a = "z"; }`
    // reached this function with `$a` bound `int` — an existing binding, so the mark was never
    // looked at — and the merge failed with the hard `cannot reassign $a from int to string` in
    // BOTH modes. Re-asserting `Mixed` here is what makes the boxed slot and the checker's view
    // agree again; the guard's narrowing is restored by `control_flow` after the branch either way.
    //
    // Everything above still runs (the retype-site re-decision, and the callable/reflection
    // metadata updates `check_assign` performed before calling in), so only the type merge is
    // short-circuited.
    // All stores of a region-split name must remain numeric. A later nonnumeric store
    // retracts earlier region decisions as well, before any lowering can consume them.
    let numeric_region_value = super::super::super::numeric_regions::numeric_assignment(
        checker, value, ty, env);
    if !numeric_region_value {
        let mut pending = vec![name.to_owned()];
        let mut rejected = crate::fast_hash::FastSet::default();
        while let Some(name) = pending.pop() {
            if !rejected.insert(name.clone()) { continue; }
            pending.extend(checker.numeric_region_dependents.get(&name).into_iter().flatten().cloned());
            for (site, names) in &mut checker.numeric_region_sites {
                if names.remove(&name) && checker.numeric_region_retypes.remove(&(*site, name.clone())) {
                    if let Some(names) = checker.local_retype_sites.get_mut(site) { names.remove(&name); }
                }
            }
        }
        checker.local_retype_sites.retain(|_, names| !names.is_empty());
        checker.numeric_region_sites.retain(|_, names| !names.is_empty());
    }
    // A by-reference PARAMETER the program-wide pre-pass widened holds the same contract for the
    // same reason, and needs the same re-assertion for a second one. PHP's reference cell is
    // untyped, `Checker::scan_widened_ref_params` has already decided this one is boxed on both
    // sides of every call, and the body's environment is SEEDED `Mixed` — but a call that lends
    // the parameter on to another by-reference parameter re-binds it to THAT parameter's declared
    // type (`prepare_by_ref_variable_storage` ends by inserting the callee's `expected`), which
    // narrowed the name back to a representation its slot no longer has.
    // `Symfony\Component\Yaml\Inline::parseMapping` is the shape: `self::parseScalar(…, $i, false)`
    // re-bound `$i` to `parseScalar`'s declared `int`, and the `$i = \strpos(…)` eleven lines
    // below it was `cannot reassign $i from int to int|false` again.
    //
    // Re-asserting rather than widening is what keeps `--strict-locals` honest: nothing is being
    // discarded here and no decision is being taken, so this must not warn and must not become an
    // error in strict mode. The cell really is `mixed`.
    // An UNTYPED by-reference parameter is the third holder of the same contract, with no
    // pre-pass needed: its cell is `mixed` by declaration. What reaches this function with a
    // narrower type is FLOW NARROWING of that cell, and the merge against it failed for the
    // ordinary way of writing such a callback -- the widening arm below rightly refuses a by-ref
    // parameter, whose storage is not this frame's:
    //
    //     array_walk_recursive($extra, static function (&$v) {
    //         if (\is_object($v)) { $v = get_object_vars($v); }   // cannot reassign $v from
    //     });                                                      // (object) to array<string, mixed>
    //
    // That is `Symfony\Component\Routing\Generator\UrlGenerator::doGenerate()`, and php 8.5.10
    // runs it. Re-asserting `mixed` is not a widening -- the cell already is one.
    // A `static $uid = null;` cache is the fourth: ramsey/uuid's `SystemDceSecurityProvider`
    // stores the system uid string, then `$uid = new IntegerObject($uid)`, into the same cell.
    if checker.mixed_storage_locals.contains(name)
        || checker.mixed_static_locals.contains(name)
        || checker.ref_param_is_widened(&checker.current_loop_storage_scope, name)
        || checker.untyped_ref_params.contains(name)
        || checker.boxed_ref_aliased_locals.contains(name)
    {
        // A marked name still needs its binding depth on its FIRST store, for the same reason the
        // fresh-insert branch below records one: it is the name's single authority on whether a
        // later decision is judged against a binding this body definitely created.
        checker
            .local_binding_depth
            .entry(name.to_string())
            .or_insert(checker.local_conditional_depth);
        env.insert(name.to_string(), PhpType::Mixed);
        return Ok(());
    }
    // The widening arm below warns WITHOUT recording a decision site, so its warning has no site
    // key a later walk could retract it through. This visit re-decides the site, so retract it
    // here — below the mark's early return, because the MARK files its own warning against these
    // same (span, name) keys (`mixed_storage_scan`) and retracting it there would silently delete
    // the diagnostic of a decision that is still standing.
    checker
        .binding_decision_warnings
        .remove(&(span, name.to_string()));
    if let Some(existing) = env.get(name) {
        let merged_ty = checker.merged_assignment_type(existing, ty).or_else(|| {
            checker.guarded_union_types.get(name)
                .and_then(|original| checker.merged_assignment_type(original, ty))
        });
        // The same flow-fact re-measure as below, for a merge that SUCCEEDS by keeping the guard's
        // view: `instanceof \Closure` narrows the view to `Callable`, and `Callable` absorbs an
        // array (an array can be a callable), so `if ($c instanceof \Closure) { $c = [$a, $b]; }`
        // left `$c` typed `Callable` and `$c[0]` was "Cannot index non-array" (symfony/cache's
        // `EarlyExpirationMessage::create()`), while `is_array($c)` after the `if` answered false.
        // The store is the newest fact about the value; the binding it lands in is the origin.
        if merged_ty.as_ref().is_some_and(|merged| merged != ty) {
            if let Some(narrowing) = checker.narrowed_local_origins.get(name) {
                if narrowing.view.as_ref() == Some(existing)
                    && narrowing.origin == PhpType::Mixed
                {
                    env.insert(name.to_string(), ty.clone());
                    return Ok(());
                }
            }
        }
        if merged_ty.is_none() {
            // `existing` may be a FLOW FACT rather than the binding: inside a guarded region the
            // environment holds the guard's view, and `$g = glob(…); if ($g === false) { $g =
            // []; }` reached here with `false` on the left and `array<never>` on the right. That
            // store is not a retype at all — the binding is `array<string>|false` and an empty
            // array is one of the things it holds — so the failed merge is re-measured against
            // the type the binding had before the guard narrowed it (issue #509).
            //
            // Only while the environment still holds the GUARD'S OWN view, which is what makes
            // this a fact about the narrowing rather than about the binding. A region that has
            // already stored something of its own put that entry there itself, and the shapes the
            // branch-divergent `Mixed`-storage marking exists for are exactly those —
            // `$a = 1; if (is_string($a)) { $a = "x"; $a = 2; }` must keep reaching
            // `mixed_storage_scan` and its `--strict-locals` error, not be waved through here on
            // the strength of `$a`'s original `int`.
            //
            // The ENVIRONMENT takes the assigned type, not the origin: the store is the newest
            // fact about the value and the rest of the guarded region should read it as such
            // (`if (is_int($x)) { $x = "s"; strlen($x); }`). The enclosing construct restores the
            // pre-guard view when the region ends, as it always did.
            //
            // Nothing is recorded in `local_retype_sites`: no slot is abandoned, because the one
            // the binding already owns is wide enough. That is also why this arm does not need
            // `local_binding_is_killable` — the depth-0 requirement exists to make abandoning a
            // slot safe, and there is no abandonment here.
            if let Some(narrowing) = checker.narrowed_local_origins.get(name) {
                if narrowing.view.as_ref() == Some(existing)
                    && checker
                        .merged_assignment_type(&narrowing.origin, ty)
                        .is_some()
                {
                    env.insert(name.to_string(), ty.clone());
                    return Ok(());
                }
            }
            // A DECLARED local's contract is its declaration, not whatever a guard narrowed it
            // to. `?Node $c = $head; while ($c !== null) { $c = $c->next; }` arrives here with
            // `$c` bound `Node` — the guard's narrowing, inserted into the shared environment
            // for the loop body — and the merge rejects the `?Node` the declaration always
            // allowed. That rejected the cursor idiom every linked structure is written with.
            //
            // Same shape as the `mixed_storage_locals` re-assertion above, and for the same
            // reason: flow narrowing falsified an invariant the declaration owns. Consulted only
            // once the merge has FAILED, so no program that compiles today changes type here —
            // this branch can only accept what was previously an error.
            if let Some(declared) = checker.typed_local_names.get(name).cloned() {
                if checker.type_accepts(&declared, ty) {
                    env.insert(name.to_string(), ty.clone());
                    return Ok(());
                }
            }
            // Two UNIONS are the same boxed cell whatever members they name, so storing one into
            // the other abandons nothing and re-represents nothing: the binding simply holds the
            // wider union. Doctrine's DQL parser stores `Token::$value` (`string|int`) into its
            // `string|null &$fieldAlias` parameter -- php checks a parameter's type only when it is
            // bound -- and the refusal interpreted the whole parser.
            if matches!(existing, PhpType::Union(_))
                && matches!(ty, PhpType::Union(_))
                && existing.codegen_repr() == PhpType::Mixed
                && ty.codegen_repr() == PhpType::Mixed
            {
                let widened = checker.normalize_union_type(vec![existing.clone(), ty.clone()]);
                env.insert(name.to_string(), widened);
                return Ok(());
            }
            if !checker.strict_locals && stmt_form && checker.local_binding_is_killable(name) {
                if checker.lint_warnings && retype_crosses_storage_boundary(&existing, &ty) {
    let message = format!(
                        "${} changes type from {} to {}; the previous value is discarded (compile with --strict-locals to make this an error)",
                        name, existing, ty
                    );
                    // Filed against the DECISION's key, not pushed straight into `warnings`, so a
                    // later walk that re-decides this site and drops the retype drops the warning
                    // too. A span that names no node files no decision either, so its warning has
                    // nothing that could retract it and goes out directly, as before.
                    if span.identifies_a_node() {
                        checker.binding_decision_warnings.insert(
                            (span, name.to_string()),
                            CompileWarning::new(span, &message),
                        );
                    } else {
                        checker.warnings.push(CompileWarning::new(span, &message));
                    }
                }
                // The span EIR lowering consults to abandon the old frame slot instead of
                // storing the new value through it.
                //
                // A `Span::dummy()` is NOT such a span: it names no node
                // (`Span::identifies_a_node`), it is what every compiler-generated AST node
                // carries, and lowering consults this set at EVERY `StmtKind::Assign`. One
                // prelude assignment recorded under it would therefore re-bind the local at
                // every OTHER dummy-span assignment in the program — synthetic class bodies
                // and the PDO/mysqli/curl preludes are made of those. The re-bind still
                // happens in the type ENVIRONMENT (that is what keeps such a body compiling
                // at all); withholding only the span leaves the assignment on the storage
                // widening path it used before retypes were lowered.
                //
                // Inserted into the span's SET of re-bound names rather than over it: two
                // DIFFERENT locals re-bound at one (line, column) in two files are two decisions,
                // and one used to evict the other.
                if span.identifies_a_node() {
                    checker
                        .local_retype_sites
                        .entry(span)
                        .or_default()
                        .insert(name.to_string());
                }
                // The fresh binding is created here, at depth 0 — pin that explicitly so a
                // later kill or retype of the same name is judged against THIS binding.
                checker.local_binding_depth.insert(name.to_string(), 0);
                // This arm ABANDONS the old slot, so the name's storage is exactly the new type's
                // representation — set-or-clear rather than accumulate, or a name re-bound back to
                // a concrete type would keep answering "boxed" for the rest of the body.
                record_retyped_boxed_storage(checker, name, ty);
                checker.guarded_union_types.remove(name);
                // Unlike the `unset` kill, the per-name callable/reflection tables are NOT
                // cleared here. The old binding's metadata is already gone: `check_assign`
                // ran `update_callable_assignment_metadata`,
                // `update_callable_array_assignment_metadata` and
                // `update_reflection_class_assignment_metadata` for THIS assignment before
                // calling in, and between them they set-or-remove every entry
                // `Checker::clear_local_binding_metadata` touches (`foreach_key_locals` is
                // dropped at the top of `check_assign`). Clearing again would discard the
                // NEW binding's metadata instead: `$f = 1; $f = function (int $a) {…};
                // $f("s")` would lose the signature that reports the bad argument.
                env.insert(name.to_string(), ty.clone());
                return Ok(());
            }
            // The precise re-bind was refused, but PHP still ALLOWS the write: a local may hold a
            // value of any type at any point, and `function f(int $a) { $a = "s"; … }` — a write to
            // a type-hinted parameter — is ordinary code that the hard error rejected outright.
            // Fall back to the storage-widening path this compiler used before precise re-binds
            // existed: bind the new type in the environment and record NO retype span, so lowering
            // keeps the slot and `store_local` joins its storage type instead of abandoning it.
            // Correct, just less precise — the same degradation `rebind_local_for_retype` already
            // takes when the name's slot is not the value's home, and the same one this function
            // takes for a `Span::dummy()` assignment.
            //
            // Eligibility is `Checker::local_binding_is_widenable`: this frame's own storage and
            // no other name reaching the same cell. The store's conditional DEPTH is deliberately
            // not part of it — a kill must prove the store runs before abandoning a slot, while a
            // widening keeps the slot and lets `store_local` widen it, which is sound on both
            // paths. What that costs is paid in lowering, where `join_arm_types` joins the merge
            // through the widened frame slot instead of through the untaken arm's type.
            // `--strict-locals` keeps the hard error, which is the whole point of the flag.
            if !checker.strict_locals && checker.local_binding_is_widenable(name) {
                if checker.lint_warnings && retype_crosses_storage_boundary(&existing, &ty) {
    let message = format!(
                        "${} changes type from {} to {}; the previous value is discarded (compile with --strict-locals to make this an error)",
                        name, existing, ty
                    );
                    if span.identifies_a_node() {
                        checker.binding_decision_warnings.insert(
                            (span, name.to_string()),
                            CompileWarning::new(span, &message),
                        );
                    } else {
                        checker.warnings.push(CompileWarning::new(span, &message));
                    }
                }
                // Deliberately NOT recorded in `local_retype_sites`, and the binding depth is
                // deliberately left alone: no binding ended here, so nothing downstream may
                // abandon the slot or judge a later decision against a binding this arm did not
                // create.
                //
                // The slot IS boxed, though, and unconditionally: this arm is only reached
                // because `merged_assignment_type` refused, so the kept slot has to hold two
                // representations at once and `store_local` joins it to a boxed cell.
                checker
                    .retyped_boxed_storage_locals
                    .insert(name.to_string());
                env.insert(name.to_string(), ty.clone());
                return Ok(());
            }
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot reassign ${} from {} to {}",
                    name, existing, ty
                ),
            ));
        }
        // A store the value's type FITS keeps the binding's type, which is right for a merge
        // across control flow and wrong for a straight-line assignment: PHP replaces the value,
        // so the name holds exactly what was stored.
        //
        // It only shows when the binding is a UNION, because that is the only shape a fitting
        // value can be strictly narrower than. `ContainerBuilderDebugDumpPass::process` is the
        // one in the fixture: `$file = $container->getParameter(…)` binds
        // `array|bool|string|int|float|UnitEnum|null`, `$file = substr_replace($file, '.ser', -4)`
        // stores a `string` into it — and the call three lines later reported
        // `Filesystem::chmod parameter $files expects Union([Str, Iterable]), got
        // Union([Array(Mixed), Bool, Str, Int, Float, Object("UnitEnum"), Void])`, the type
        // `$file` had BEFORE the store.
        //
        // Restricted to conditional depth 0, which is the same proof the `unset` kill demands:
        // the store dominates everything after it, so nothing downstream can be reached with the
        // wider value still in the name. Inside a branch or a loop body the merge has to stand —
        // a `while` back edge and an untaken arm both deliver the old type to the code below.
        //
        // And restricted to storage this FRAME owns, for the same reason the widening is: a
        // by-reference parameter, a `=&` alias, a `global` and a `static` are all reachable by
        // another name whose view of the cell this store does not get to narrow.
        let narrows_a_union_in_straight_line = matches!(existing, PhpType::Union(_))
            && !matches!(ty, PhpType::Union(_) | PhpType::Mixed)
            && merged_ty.as_ref() == Some(existing)
            && checker.local_conditional_depth == 0
            && !checker.name_is_seeded_program_storage(name)
            && !checker.top_level_binding_is_program_global(name)
            && !checker.active_ref_params.contains(name)
            && !checker.ref_bound_locals.contains(name)
            && !checker.active_globals.contains(name)
            && !checker.static_local_names.contains(name);
        // The same replacement inside a guarded region, for a declared `iterable`. The guard
        // `if (!$variables instanceof VariableBag) { $variables = new VariableBag($variables); }`
        // leaves the then-arm holding `iterable` (the value fits, so the merge kept the binding's
        // type) while the implicit else-arm holds `VariableBag`; their join stayed `iterable` and
        // the following `$this->expandAll($variables)` was refused (league/uri `Template::expand`,
        // and `Create::iterFor` of guzzle/promises in the same shape). `iterable` is the one
        // contract here that is not a storage shape of its own -- it is already a boxed
        // array-or-Traversable -- so recording the stored value's type as the flow fact is
        // exact, never a re-representation.
        let narrows_an_iterable_in_a_guard = *existing == PhpType::Iterable
            && matches!(ty, PhpType::Object(class) if !class.is_empty())
            && checker
                .narrowed_local_origins
                .get(name)
                .is_some_and(|narrowing| narrowing.origin == PhpType::Iterable)
            && !checker.name_is_seeded_program_storage(name)
            && !checker.top_level_binding_is_program_global(name)
            && !checker.active_ref_params.contains(name)
            && !checker.ref_bound_locals.contains(name)
            && !checker.active_globals.contains(name)
            && !checker.static_local_names.contains(name);
        if narrows_a_union_in_straight_line || narrows_an_iterable_in_a_guard {
            env.insert(name.to_string(), ty.clone());
        } else if let Some(merged_ty) = merged_ty {
            // A storage-only re-bind (see `splits_numeric_binding`), at the eligibility the
            // retype above uses. Nothing typing-visible changes: the environment still takes the
            // merged type and no warning is filed, so the decision is only which frame slot the
            // store writes. `--strict-locals` does not veto it for the same reason. A parameter
            // keeps its one slot: backtrace frames read its current argument value by name from
            // the first slot of that name (`debug_backtrace()[0]["args"]`).
            let independent_split = splits_numeric_binding(existing, ty, value)
                && checker.local_binding_is_killable(name);
            let region_split = checker.numeric_region_sites.get(&span).is_some_and(|names| names.contains(name))
                && (matches!(existing, PhpType::Int | PhpType::Float | PhpType::Mixed) || existing.is_int_float_union())
                && numeric_region_value;
            if stmt_form
                && span.identifies_a_node()
                && (independent_split || region_split)
                && !checker.local_parameter_names.contains(name)
                && !checker.top_level_binding_is_program_global(name)
            {
                checker
                    .local_retype_sites
                    .entry(span)
                    .or_default()
                    .insert(name.to_string());
                if !independent_split {
                    checker.numeric_region_retypes.insert((span, name.to_owned()));
                }
            }
            if &merged_ty != existing {
                env.insert(name.to_string(), merged_ty);
            }
        }
    } else {
        // A fresh binding: remember the conditional depth it was created at, so a later
        // depth-0 `unset`/retype knows whether the store that created it definitely ran.
        checker
            .local_binding_depth
            .insert(name.to_string(), checker.local_conditional_depth);
        // A MARKED name never reaches here: the authoritative check at the top of this function
        // binds it `Mixed` and returns, on this store and on every later one alike.
        env.insert(name.to_string(), ty.clone());
    }
    Ok(())
}

/// Returns whether a type-compatible reassignment should still start a FRESH binding, so two
/// numeric bindings of one name get two frame slots instead of sharing one.
///
/// A slot is typed by everything ever stored into it. `for ($i = $n - 1; …; $i--)` stores a
/// checked `int - int`, which is `mixed` until proven, so it used to widen the one slot an
/// earlier `for ($i = 0; $i < $n; $i++)` also used, and the earlier loop, whose own updates are
/// provably integer, ran boxed too. With a binding per assignment each loop keeps the storage
/// its own stores need. PHP cannot observe the difference: the assignment drops the old value
/// either way.
///
/// Only numeric bindings qualify (`int`, `float`, the `int|float` an increment leaves behind,
/// and `mixed`), and only when the assigned type differs from the binding's (two `mixed`
/// bindings would share a representation anyway). The re-bind releases the old occupant
/// BEFORE the store retains the new value, so the value must not be able to borrow the old
/// cell: it is either a raw `int`/`float` or a fresh arithmetic result. `$i = $c ? $i : 0`
/// keeps the shared slot.
fn splits_numeric_binding(existing: &PhpType, incoming: &PhpType, value: &Expr) -> bool {
    let raw_scalar = |ty: &PhpType| matches!(ty, PhpType::Int | PhpType::Float);
    let numeric = |ty: &PhpType| raw_scalar(ty) || ty.is_int_float_union() || *ty == PhpType::Mixed;
    existing != incoming
        && numeric(existing)
        && numeric(incoming)
        && (raw_scalar(incoming) || is_fresh_arithmetic_result(value))
}

/// Returns whether an expression evaluates to a freshly computed arithmetic result.
///
/// The arithmetic operators always produce a new value (an integer, a float, a boxed numeric
/// result or an operator-overloaded object), never a view of one of their operands.
fn is_fresh_arithmetic_result(value: &Expr) -> bool {
    matches!(
        &value.kind,
        ExprKind::BinaryOp {
            op: BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Mod | BinOp::Pow,
            ..
        }
    )
}

/// Type-checks a typed local variable declaration with a type hint (`Type $name = value`).
///
/// Resolves the declared type from `type_expr`, infers the value's type, validates
/// that the value is assignable to the declared type, and inserts the declared type
/// (not the inferred type) into the environment.
///
/// Unlike `check_assign`, this uses the declared type as the final type rather than
/// inferring from the value, enforcing the programmer's type hint.
pub(super) fn check_typed_assign(
    checker: &mut Checker,
    type_expr: &TypeExpr,
    name: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let declared_ty = checker.resolve_declared_local_type_hint(
        type_expr,
        span,
        &format!("Typed local ${}", name),
    )?;
    let value_ty = match checker.infer_type(value, env) {
        Ok(value_ty) => value_ty,
        Err(error) => {
            // Error recovery: the initializer failed to type. Bind `$name` before
            // propagating the real RHS error so later uses do not cascade into
            // misleading `Undefined variable: $name` diagnostics. Poison it as
            // `Mixed` (unknown) rather than the declared type: a poisoned target
            // must not spawn new follow-on type errors downstream (e.g. a declared
            // `int` would make a later `count($name)` a spurious second error).
            // Only bind when unbound so a valid earlier binding is preserved.
            poison_unbound_local(env, name);
            return Err(error);
        }
    };
    if !checker.type_accepts(&declared_ty, &value_ty) {
        // The initializer typed successfully but violates the declared type. Poison
        // `$name` as `Mixed` for recovery so later uses do not cascade, then report
        // the initialization mismatch as the one real diagnostic.
        poison_unbound_local(env, name);
        return Err(CompileError::new(
            span,
            &format!(
                "Type error: cannot initialize ${} as {} with {}",
                name, declared_ty, value_ty
            ),
        ));
    }
    // A declared type is a programmer contract: the local is never kill/retype eligible, in
    // either mode. The binding depth is still recorded so the name has one authority, and the
    // declaration itself is remembered so `merge_local_assignment_type` validates every later
    // write against it.
    checker.declared_local_types.insert(
        (checker.current_loop_storage_scope.clone(), name.to_string()),
        declared_ty.clone(),
    );
    checker
        .typed_local_names
        .insert(name.to_string(), declared_ty.clone());
    checker
        .local_binding_depth
        .entry(name.to_string())
        .or_insert(checker.local_conditional_depth);
    env.insert(name.to_string(), declared_ty);
    let reflected_class = reflection_class_assignment_target(checker, value, env);
    update_reflection_class_assignment_metadata(checker, name, reflected_class);
    Ok(())
}

/// Type-checks a constant declaration and records it in the checker's constant table.
///
/// Infers the type of the constant value expression and inserts it into
/// `checker.constants` under `name`. Unlike variable assignments, constants are
/// stored on the checker itself and not in the local type environment.
pub(super) fn check_const_decl(
    checker: &mut Checker,
    name: &str,
    value: &Expr,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let ty = checker.infer_type(value, env)?;
    checker.constants.entry(name.to_string()).or_insert(ty);
    Ok(())
}

/// Type-checks a list unpacking assignment (`[$a, $b, ...] = $arr`).
///
/// Infers the right-hand side and accepts homogeneous indexed arrays, associative arrays, or a
/// gradual value that may contain an array and is guarded by the runtime reader. Indexed arrays
/// propagate their element type, while associative and gradual values bind adaptively as `Mixed`.
/// Returns an error for types that cannot contain an array.
pub(super) fn check_list_unpack(
    checker: &mut Checker,
    vars: &[String],
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let arr_ty = match checker.infer_type_with_assignment_effects(value, env) {
        Ok(arr_ty) => arr_ty,
        Err(error) => {
            for var in vars {
                poison_unbound_local(env, var);
            }
            return Err(error);
        }
    };
    let unpack_ty = match &arr_ty {
        ty if ty.is_php_array() => PhpType::Mixed,
        PhpType::Array(elem_ty) => *elem_ty.clone(),
        // Associative arrays can contain integer keys used by positional destructuring. Their
        // element type stays adaptive because hash values may be heterogeneous or absent.
        PhpType::AssocArray { .. } => PhpType::Mixed,
        // Dynamic method/eval boundaries cannot expose a static return shape. EIR list-unpack
        // lowering already routes boxed Mixed sources through `__rt_mixed_array_get`, which
        // validates the runtime tag before reading the positional keys.
        PhpType::Mixed => PhpType::Mixed,
        // `never` is NOT accepted here, deliberately. It arises from a loop-carried accumulator
        // that is still `[]` where the loop is typed and only gains its shape from an append
        // further down the same body — Symfony's `CompiledUrlMatcherDumper` iterates
        // `foreach ($dynamicRegex as [$hostRx, $rx, $prefix])` over exactly that. Treating it as
        // gradual makes the CHECKER accept the program, but the foreach still lowers against
        // `array<never>` storage and reads every element as null: measured, the loop body printed
        // nothing and warned "Trying to access array offset on null" where PHP prints the rows.
        // The fix belongs in loop-carried storage widening, not here; until then the compile-time
        // refusal is the truthful answer.
        // A union containing an array remains representation-boxed. The runtime reader applies
        // PHP's missing/non-array element semantics to whichever member arrives at execution.
        PhpType::Union(_) if array_arg_is_gradually_acceptable(&arr_ty) => PhpType::Mixed,
        // php destructures an `ArrayAccess` object through `offsetGet()`, one call per position:
        // Laravel's `[$text, $author] = (new Stringable($quote))->explode('-')` (a Collection).
        // Lowering boxes the object, and the boxed reader dispatches the interface methods.
        PhpType::Object(class_name)
            if checker.object_type_implements_interface(class_name, "ArrayAccess") =>
        {
            PhpType::Mixed
        }
        // An element of a list only something else fills (flex's `$retries`, appended to from a
        // closure), and php destructures null into nulls.
        PhpType::Never | PhpType::Void => PhpType::Mixed,
        _ => {
            for var in vars {
                poison_unbound_local(env, var);
            }
            return Err(CompileError::new(
                span,
                "List unpacking requires an array on the right-hand side",
            ));
        }
    };
    for var in vars {
        env.insert(var.clone(), unpack_ty.clone());
        update_list_unpack_callable_metadata(checker, var, value, &unpack_ty);
        checker.reflection_class_targets.remove(var);
    }
    Ok(())
}

/// Updates callable metadata for a variable assigned by list unpacking.
///
/// List unpacking copies an array element into a local without constructing an
/// explicit `ArrayAccess` assignment node. When the homogeneous element type is
/// callable and the source array has known callable metadata, propagate that
/// signature/capture/target information to the destination variable. Non-callable
/// unpack targets clear stale callable metadata.
fn update_list_unpack_callable_metadata(
    checker: &mut Checker,
    dest: &str,
    source_array: &Expr,
    elem_ty: &PhpType,
) {
    if elem_ty != &PhpType::Callable {
        clear_callable_metadata(checker, dest);
        return;
    }
    if let ExprKind::Variable(src_name) = &source_array.kind {
        copy_callable_metadata(checker, dest, src_name);
    } else {
        clear_callable_metadata(checker, dest);
    }
}

/// Copies callable signature, capture, first-class target, and callable-array metadata.
///
/// The checker stores homogeneous callable-array metadata under the source array
/// variable name so later element reads can be treated as callable variables with
/// a known signature. This helper mirrors that metadata onto a list-unpack target.
fn copy_callable_metadata(checker: &mut Checker, dest: &str, src: &str) {
    let copied_target_version = checker.callable_array_target_versions.get(src).copied();
    checker.mark_callable_array_target_write(dest);
    if let Some(return_ty) = checker.closure_return_types.get(src).cloned() {
        checker
            .closure_return_types
            .insert(dest.to_string(), return_ty);
    } else {
        checker.closure_return_types.remove(dest);
    }
    if let Some(sig) = checker.callable_sigs.get(src).cloned() {
        checker.callable_sigs.insert(dest.to_string(), sig);
    } else {
        checker.callable_sigs.remove(dest);
    }
    if let Some(captures) = checker.callable_captures.get(src).cloned() {
        checker.callable_captures.insert(dest.to_string(), captures);
    } else {
        checker.callable_captures.remove(dest);
    }
    if let Some(target) = checker.callable_array_targets.get(src).cloned() {
        checker
            .callable_array_targets
            .insert(dest.to_string(), target);
        if let Some(version) = copied_target_version {
            checker
                .callable_array_target_versions
                .insert(dest.to_string(), version);
        }
    } else {
        checker.callable_array_targets.remove(dest);
    }
    if let Some(target) = checker.first_class_callable_targets.get(src).cloned() {
        checker
            .first_class_callable_targets
            .insert(dest.to_string(), target);
    } else {
        checker.first_class_callable_targets.remove(dest);
    }
}

/// Clears all callable metadata for a list-unpack destination.
fn clear_callable_metadata(checker: &mut Checker, dest: &str) {
    checker.mark_callable_array_target_write(dest);
    checker.closure_return_types.remove(dest);
    checker.callable_sigs.remove(dest);
    checker.callable_captures.remove(dest);
    checker.callable_array_targets.remove(dest);
    checker.first_class_callable_targets.remove(dest);
}

/// Type-checks a `global` declaration and registers the variables as globals.
///
/// A global declaration replaces a same-named local binding. Ordinary PHP
/// globals use boxed Mixed storage and may change type at runtime; neither a
/// local's old type nor the top-level initial value constrains that storage.
pub(super) fn check_global(
    checker: &mut Checker,
    vars: &[String],
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    for var in vars {
        checker.active_globals.insert(var.clone());
        let ty = checker.extern_globals.get(var).cloned().unwrap_or_else(|| {
            if crate::superglobals::is_superglobal(var) {
                crate::superglobals::superglobal_type()
            } else {
                PhpType::Mixed
            }
        });
        clear_callable_metadata(checker, var);
        env.insert(var.clone(), ty);
    }
    Ok(())
}

/// Type-checks a `static` variable declaration and registers it in the checker.
///
/// Infers the type of the initializer expression, marks the variable as static in
/// `checker.active_statics`, and inserts the inferred type into the local environment.
/// Static variables retain their values across function calls.
///
/// The type this inserts is the CHECKER's view, used for acceptance and diagnostics. The STORAGE
/// a `static` gets is decided separately, at the declaration's lowering, by
/// `LoweringContext::required_static_local_storage_type` — and for an EMPTY ARRAY initializer the
/// two deliberately differ. `static $q = [];` runs its initializer on the first call only while
/// the declaration re-enters the environment on every call, so `array<never>` asserts, on call
/// two, that nothing call one stored is there. `array<never>` element slots are ZERO WIDTH and
/// codegen believes it: `array_shift($q)` lowered the removed payload as `mov x11, #0` and
/// returned `NULL` while the array shrank correctly. The storage rule widens that to
/// `array<mixed>`; the checker keeps the literal's own type, because every narrowing and
/// acceptance rule the body relies on is already written against it.
/// Whether `ty` has an element type nothing filled yet (`array<never>`), at any depth.
fn type_mentions_never(ty: &PhpType) -> bool {
    match ty {
        PhpType::Never => true,
        PhpType::Array(elem) => type_mentions_never(elem),
        PhpType::AssocArray { key, value } => type_mentions_never(key) || type_mentions_never(value),
        PhpType::Union(members) => members.iter().any(type_mentions_never),
        _ => false,
    }
}

pub(super) fn check_static_var(
    checker: &mut Checker,
    name: &str,
    init: &Expr,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    // A `static` local's storage outlives the call, so its binding is never killable — record
    // it before inference so an erroring initializer still marks the name.
    checker.static_local_names.insert(name.to_string());
    let ty = match checker.infer_type(init, env) {
        Ok(ty) => ty,
        Err(error) => {
            poison_unbound_local(env, name);
            checker.active_statics.insert(name.to_string());
            return Err(error);
        }
    };
    checker.active_statics.insert(name.to_string());
    // `static $x;` / `static $x = null;` is a lazily filled cache: on every call but the first it
    // holds whatever an earlier call stored. Typing it as the null initializer made
    // `if (null !== $x) { return $x; }` narrow to nothing, and `bool` methods built on that idiom
    // (Symfony `Process::isPtySupported()`, `FileBinaryMimeTypeGuesser::isGuesserSupported()`)
    // were refused and left to the interpreter.
    // An initializer with an empty array somewhere (`static $c = ['orig' => [], 'replace' => []];`,
    // voku's `ASCII::normalize_msword()`) is just as unsettled: the first fill stores the element
    // type the declaration could not name, into the same cell.
    let ty = if matches!(ty, PhpType::Void) || type_mentions_never(&ty) { PhpType::Mixed } else { ty };
    if ty == PhpType::Mixed {
        checker.mixed_static_locals.insert(name.to_string());
    } else {
        checker.mixed_static_locals.remove(name);
    }
    env.insert(name.to_string(), ty);
    Ok(())
}
