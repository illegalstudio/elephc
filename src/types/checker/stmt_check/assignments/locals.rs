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
use crate::parser::ast::{Expr, ExprKind, TypeExpr};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

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

fn update_callable_assignment_metadata(
    checker: &mut Checker,
    name: &str,
    callable_source: &Expr,
    ty: &PhpType,
    env: &mut TypeEnv,
) -> Result<(), CompileError> {
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
