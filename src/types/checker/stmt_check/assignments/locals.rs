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
use crate::names::{php_symbol_key, Name};
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver, TypeExpr};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

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
/// Combines the existing type of `existing` with the inferred type of `default_ty`,
/// returning the merged type or an error if the types are incompatible.
/// Handles special cases: `Void` existing types, `Mixed`, and union types.
///
/// Returns `Ok(PhpType)` with the resolved type, or `Err` if the null-coalescing
/// assignment would violate type constraints.
fn null_coalesce_assignment_type(
    checker: &Checker,
    name: &str,
    existing: &PhpType,
    default_ty: &PhpType,
    default: &Expr,
    span: Span,
) -> Result<PhpType, CompileError> {
    if *existing == PhpType::Void {
        return Ok(default_ty.clone());
    }
    if *existing == PhpType::Mixed {
        return Ok(PhpType::Mixed);
    }
    if matches!(existing, PhpType::Union(_)) {
        if *default_ty == PhpType::Void || checker.type_accepts(existing, default_ty) {
            return Ok(existing.clone());
        }
        return Err(CompileError::new(
            span,
            &format!(
                "Type error: null coalescing assignment for ${} must keep {}, got {}",
                name, existing, default_ty
            ),
        ));
    }
    if existing == default_ty || matches!(default.kind, ExprKind::Null) {
        return Ok(existing.clone());
    }
    Err(CompileError::new(
        span,
        &format!(
            "Type error: null coalescing assignment for ${} must keep {}, got {}",
            name, existing, default_ty
        ),
    ))
}

/// Type-checks a simple variable assignment (`$name = value`).
///
/// Handles null-coalescing assignment by extracting the default expression and combining
/// types appropriately. Preserves callable metadata when assigning closures or callable
/// expressions. Updates the type environment with the merged assignment type.
///
/// On success, updates `env` with the resolved type for `name`. On error, returns a
/// type mismatch diagnostic.
pub(super) fn check_assign(
    checker: &mut Checker,
    name: &str,
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let null_coalesce_default = null_coalesce_assignment_default(name, value);
    let saved_self_ref_ty = if env.contains_key(name) && closure_captures_name_by_ref(value, name) {
        Some(env.insert(name.to_string(), PhpType::Callable))
    } else {
        None
    };
    let ty_result: Result<PhpType, CompileError> = (|| {
        if let Some(default) = null_coalesce_default {
            if let Some(existing) = env.get(name).cloned() {
                let default_ty = if existing == PhpType::Void {
                    checker.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    checker.infer_type_with_assignment_effects(default, &mut default_env)?
                };
                null_coalesce_assignment_type(checker, name, &existing, &default_ty, default, span)
            } else {
                checker.infer_type_with_assignment_effects(value, env)
            }
        } else {
            checker.infer_type_with_assignment_effects(value, env)
        }
    })();
    let callable_source = if let Some(default) = null_coalesce_default {
        if matches!(env.get(name), Some(existing) if *existing == PhpType::Void) {
            default
        } else {
            value
        }
    } else {
        value
    };
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
    let ty = ty_result?;
    metadata_result?;
    merge_local_assignment_type(checker, name, &ty, span, env)
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
    pub(crate) fn check_local_assignment_expression(
        &mut self,
        name: &str,
        value: &Expr,
        span: Span,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        check_assign(self, name, value, span, env)?;
        env.get(name).cloned().ok_or_else(|| {
            CompileError::new(span, &format!("Undefined variable: ${}", name))
        })
    }
}

/// Updates callability metadata when assigning a callable expression to a variable.
///
/// When `ty` is `Callable`, extracts and stores the callable signature, closure return
/// type, capture list, and first-class callable target on the checker. When `ty` is not
/// callable, clears any previously stored metadata for `name`.
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
    update_callable_array_assignment_metadata(checker, name, callable_source, env)?;

    if *ty == PhpType::Callable {
        if let Some(sig) = checker.resolve_expr_callable_sig(callable_source, env)? {
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
                            capture_refs.iter().any(|ref_capture| ref_capture == capture),
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
    } else {
        checker.closure_return_types.remove(name);
        checker.callable_sigs.remove(name);
        checker.callable_captures.remove(name);
        checker.first_class_callable_targets.remove(name);
    }
    Ok(())
}

/// Provides the Update callable array assignment metadata helper used by the locals module.
fn update_callable_array_assignment_metadata(
    checker: &mut Checker,
    name: &str,
    callable_source: &Expr,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    if let Some(target) = resolve_callable_array_target(checker, callable_source, env)? {
        checker
            .callable_array_targets
            .insert(name.to_string(), target);
    } else if let ExprKind::Variable(src_name) = &callable_source.kind {
        if let Some(target) = checker.callable_array_targets.get(src_name).cloned() {
            checker
                .callable_array_targets
                .insert(name.to_string(), target);
        } else {
            checker.callable_array_targets.remove(name);
        }
    } else {
        checker.callable_array_targets.remove(name);
    }
    Ok(())
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
    if checker.invokable_class_for_type(&receiver_ty).is_some() {
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
    match receiver {
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
    let class_key = php_symbol_key(class_name.trim_start_matches('\\'));
    checker
        .classes
        .keys()
        .find(|existing| php_symbol_key(existing) == class_key)
        .map(String::as_str)
}

/// Merges the assigned type into the type environment for the given variable.
///
/// If `name` already exists in `env`, attempts to merge the new type with the existing
/// type using `checker.merged_assignment_type()`. If merging is not possible, returns
/// a type incompatibility error. If `name` does not exist, inserts the type directly.
///
/// The merge operation supports widening (e.g., `Int | Float` from separate assignments)
/// and preserves type specificity where possible.
fn merge_local_assignment_type(
    checker: &Checker,
    name: &str,
    ty: &PhpType,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    if let Some(existing) = env.get(name) {
        let merged_ty = checker.merged_assignment_type(existing, ty);
        if merged_ty.is_none() {
            return Err(CompileError::new(
                span,
                &format!(
                    "Type error: cannot reassign ${} from {} to {}",
                    name, existing, ty
                ),
            ));
        }
        if let Some(merged_ty) = merged_ty {
            if &merged_ty != existing {
                env.insert(name.to_string(), merged_ty);
            }
        }
    } else {
        env.insert(name.to_string(), ty.clone());
    }
    Ok(())
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
    let value_ty = checker.infer_type(value, env)?;
    if !checker.type_accepts(&declared_ty, &value_ty) {
        return Err(CompileError::new(
            span,
            &format!(
                "Type error: cannot initialize ${} as {} with {}",
                name, declared_ty, value_ty
            ),
        ));
    }
    env.insert(name.to_string(), declared_ty);
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
/// Infers the type of the right-hand side expression and validates it is an array.
/// For array types, extracts the element type and assigns it to each variable in `vars`.
/// Returns an error if the right-hand side is not an array type.
pub(super) fn check_list_unpack(
    checker: &mut Checker,
    vars: &[String],
    value: &Expr,
    span: Span,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let arr_ty = checker.infer_type(value, env)?;
    match &arr_ty {
        PhpType::Array(elem_ty) => {
            for var in vars {
                env.insert(var.clone(), *elem_ty.clone());
            }
        }
        _ => {
            return Err(CompileError::new(
                span,
                "List unpacking requires an array on the right-hand side",
            ));
        }
    }
    Ok(())
}

/// Type-checks a `global` declaration and registers the variables as globals.
///
/// For each variable name, marks it as a global in `checker.active_globals` and
/// populates the local type environment with the variable's type from
/// `checker.top_level_env` if available, otherwise defaults to `Int`.
pub(super) fn check_global(
    checker: &mut Checker,
    vars: &[String],
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    for var in vars {
        checker.active_globals.insert(var.clone());
        if !env.contains_key(var) {
            if let Some(global_ty) = checker.top_level_env.get(var) {
                env.insert(var.clone(), global_ty.clone());
            } else {
                env.insert(var.clone(), PhpType::Int);
            }
        }
    }
    Ok(())
}

/// Type-checks a `static` variable declaration and registers it in the checker.
///
/// Infers the type of the initializer expression, marks the variable as static in
/// `checker.active_statics`, and inserts the inferred type into the local environment.
/// Static variables retain their values across function calls.
pub(super) fn check_static_var(
    checker: &mut Checker,
    name: &str,
    init: &Expr,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
    let ty = checker.infer_type(init, env)?;
    checker.active_statics.insert(name.to_string());
    env.insert(name.to_string(), ty);
    Ok(())
}
