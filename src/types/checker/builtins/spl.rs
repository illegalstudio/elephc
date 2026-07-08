//! Purpose:
//! Type-checks SPL helper builtins implemented by the current SPL foundation.
//! Enforces conservative argument contracts that the AOT codegen can lower safely.
//!
//! Called from:
//! - `crate::types::checker::builtins::check_builtin()`
//!
//! Key details:
//! - Autoload helpers are static/AOT approximations rather than runtime code loaders.
//! - `spl_autoload_extensions()` only accepts literal setters until the runtime owns copied strings.

use crate::errors::CompileError;
use crate::parser::ast::{CallableTarget, Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

const ITERATOR_APPLY_UNKNOWN_STATIC_CALLBACK_SIG: &str =
    "iterator_apply() callback must have a statically known callable signature";

/// Checks iterator source and reports a compile error when it is invalid.
pub(crate) fn check_iterator_source(
    checker: &mut Checker,
    arg: &Expr,
    span: crate::span::Span,
    env: &TypeEnv,
    label: &str,
) -> Result<PhpType, CompileError> {
    let ty = checker.infer_type(arg, env)?;
    if iterator_source_supported(checker, &ty) {
        return Ok(ty);
    }
    Err(CompileError::new(
        span,
        &format!(
            "{} first argument must be a statically known array or Traversable",
            label
        ),
    ))
}

/// Provides the Iterator source supported helper used by the SPL module.
fn iterator_source_supported(checker: &Checker, ty: &PhpType) -> bool {
    match ty {
        PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable => true,
        PhpType::Object(name) => traversable_object_supported(checker, name),
        _ => false,
    }
}

/// Checks iterator apply source and reports a compile error when it is invalid.
pub(crate) fn check_iterator_apply_source(
    checker: &mut Checker,
    arg: &Expr,
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<PhpType, CompileError> {
    let ty = checker.infer_type(arg, env)?;
    if matches!(&ty, PhpType::Iterable)
        || matches!(&ty, PhpType::Object(name) if traversable_object_supported(checker, name))
    {
        return Ok(ty);
    }
    Err(CompileError::new(
        span,
        "iterator_apply() first argument must be Traversable",
    ))
}

/// Provides the Traversable object supported helper used by the SPL module.
fn traversable_object_supported(checker: &Checker, name: &str) -> bool {
    if name == "Traversable" {
        return true;
    }
    checker.object_type_implements_interface(name, "Iterator")
        || checker.object_type_implements_interface(name, "IteratorAggregate")
}

/// Checks iterator to array preserve keys and reports a compile error when it is invalid.
pub(crate) fn check_iterator_to_array_preserve_keys(
    checker: &mut Checker,
    arg: &Expr,
    env: &TypeEnv,
) -> Result<Option<bool>, CompileError> {
    if let Some(value) = static_preserve_keys(arg) {
        return Ok(Some(value));
    }
    let ty = checker.infer_type(arg, env)?;
    if preserve_keys_type_supported(&ty) {
        return Ok(None);
    }
    Err(CompileError::new(
        arg.span,
        "iterator_to_array() preserve_keys must be bool-compatible scalar",
    ))
}

/// Provides the Preserve keys type supported helper used by the SPL module.
fn preserve_keys_type_supported(ty: &PhpType) -> bool {
    match ty {
        PhpType::Bool | PhpType::Int | PhpType::Float | PhpType::Str | PhpType::Void => true,
        PhpType::Union(members) => members.iter().all(preserve_keys_type_supported),
        _ => false,
    }
}

/// Provides the Static preserve keys helper used by the SPL module.
fn static_preserve_keys(expr: &Expr) -> Option<bool> {
    match &expr.kind {
        ExprKind::BoolLiteral(value) => Some(*value),
        ExprKind::IntLiteral(value) => Some(*value != 0),
        ExprKind::FloatLiteral(value) => Some(*value != 0.0),
        ExprKind::StringLiteral(value) => Some(!value.is_empty() && value != "0"),
        ExprKind::Null => Some(false),
        ExprKind::Negate(inner) => match &inner.kind {
            ExprKind::IntLiteral(value) => Some(*value != 0),
            ExprKind::FloatLiteral(value) => Some(*value != 0.0),
            _ => None,
        },
        _ => None,
    }
}

/// Computes the type metadata for iterator to array return.
pub(crate) fn iterator_to_array_return_type(
    checker: &Checker,
    source_ty: &PhpType,
    preserve_keys: Option<bool>,
) -> PhpType {
    match preserve_keys {
        Some(value) => iterator_to_array_static_return_type(source_ty, value),
        None => checker.normalize_union_type(vec![
            iterator_to_array_static_return_type(source_ty, true),
            iterator_to_array_static_return_type(source_ty, false),
        ]),
    }
}

/// Computes the type metadata for iterator to array static return.
fn iterator_to_array_static_return_type(source_ty: &PhpType, preserve_keys: bool) -> PhpType {
    match source_ty {
        PhpType::Array(elem_ty) => PhpType::Array(elem_ty.clone()),
        PhpType::AssocArray { key, value } if preserve_keys => PhpType::AssocArray {
            key: key.clone(),
            value: value.clone(),
        },
        PhpType::AssocArray { value, .. } => PhpType::Array(value.clone()),
        _ if preserve_keys => PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
        _ => PhpType::Array(Box::new(PhpType::Mixed)),
    }
}

pub(crate) enum IteratorApplyArgs<'a> {
    Static(&'a [Expr]),
    Dynamic { associative: bool },
}

/// Builds the argument list for iterator apply callback.
pub(crate) fn iterator_apply_callback_args<'a>(
    checker: &mut Checker,
    args_expr: Option<&'a Expr>,
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<IteratorApplyArgs<'a>, CompileError> {
    let Some(args_expr) = args_expr else {
        return Ok(IteratorApplyArgs::Static(&[]));
    };
    match &args_expr.kind {
        ExprKind::Null => Ok(IteratorApplyArgs::Static(&[])),
        ExprKind::ArrayLiteral(elems) => {
            if elems.iter().all(is_static_callback_arg_literal) {
                Ok(IteratorApplyArgs::Static(elems.as_slice()))
            } else {
                let args_ty = checker.infer_type(args_expr, env)?;
                Ok(IteratorApplyArgs::Dynamic {
                    associative: matches!(args_ty, PhpType::AssocArray { .. }),
                })
            }
        }
        ExprKind::ArrayLiteralAssoc(_) => {
            checker.infer_type(args_expr, env)?;
            Ok(IteratorApplyArgs::Dynamic { associative: true })
        }
        _ => {
            let args_ty = checker.infer_type(args_expr, env)?;
            if matches!(args_ty, PhpType::Array(_) | PhpType::AssocArray { .. }) {
                Ok(IteratorApplyArgs::Dynamic {
                    associative: matches!(args_ty, PhpType::AssocArray { .. }),
                })
            } else {
                Err(CompileError::new(
                    span,
                    "iterator_apply() args must be null, a literal array, or an array value",
                ))
            }
        }
    }
}

/// Checks iterator apply dynamic callback and reports a compile error when it is invalid.
pub(crate) fn check_iterator_apply_dynamic_callback(
    checker: &mut Checker,
    callback: &Expr,
    associative_args: bool,
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    if checker.expr_call_complex_callee_needs_runtime_capture(callback)
        && !super::callables::callback_supports_complex_descriptor_env(callback)
    {
        return Err(CompileError::new(
            callback.span,
            "iterator_apply() callback does not support complex expressions that select captured callables at runtime",
        ));
    }

    if let ExprKind::FirstClassCallable(target) = &callback.kind {
        let sig = checker.resolve_first_class_callable_sig(target, span, env)?;
        reject_dynamic_ref_args(&sig, span)?;
        specialize_iterator_apply_dynamic_assoc_variadic_target(
            checker,
            target,
            &sig,
            associative_args,
        )?;
        return Ok(());
    }

    if let ExprKind::Variable(var_name) = &callback.kind {
        if let Some(target) = checker.first_class_callable_targets.get(var_name).cloned() {
            let sig = checker.resolve_first_class_callable_sig(&target, span, env)?;
            checker.callable_sigs.insert(var_name.clone(), sig.clone());
            checker
                .closure_return_types
                .insert(var_name.clone(), sig.return_type.clone());
            reject_dynamic_ref_args(&sig, span)?;
            specialize_iterator_apply_dynamic_assoc_variadic_target(
                checker,
                &target,
                &sig,
                associative_args,
            )?;
            return Ok(());
        }
        if let Some(target) = checker.callable_array_targets.get(var_name).cloned() {
            let sig = checker.resolve_first_class_callable_sig(&target, span, env)?;
            reject_dynamic_ref_args(&sig, span)?;
            specialize_iterator_apply_dynamic_assoc_variadic_target(
                checker,
                &target,
                &sig,
                associative_args,
            )?;
            return Ok(());
        }
    }

    if let ExprKind::StringLiteral(cb_name) = &callback.kind {
        if let Some(extern_name) = checker.canonical_extern_function_name_folded(cb_name) {
            if let Some(sig) = checker.functions.get(extern_name.as_str()).cloned() {
                reject_dynamic_ref_args(&sig, span)?;
                return Ok(());
            }
        }
        if let Some(builtin_name) = super::canonical_builtin_function_name(cb_name) {
            if let Some(sig) = crate::types::first_class_callable_builtin_sig(&builtin_name) {
                reject_dynamic_ref_args(&sig, span)?;
                return Ok(());
            }
        }
        let cb_name = checker
            .canonical_function_name_folded(cb_name)
            .unwrap_or_else(|| cb_name.clone());
        if let Some(sig) = checker.functions.get(cb_name.as_str()).cloned() {
            reject_dynamic_ref_args(&sig, span)?;
            if associative_args && sig.variadic.is_some() {
                super::callables::specialize_dynamic_assoc_variadic_user_callback(
                    checker,
                    &cb_name,
                    &sig,
                )?;
            }
            return Ok(());
        }
        if checker.fn_decls.contains_key(cb_name.as_str()) {
            return Ok(());
        }
    }

    if let Some(sig) = checker.resolve_expr_callable_sig(callback, env)? {
        reject_dynamic_ref_args(&sig, span)?;
        return Ok(());
    }

    let callback_ty = checker.infer_type(callback, env)?;
    if callback_ty == PhpType::Str {
        return Ok(());
    }
    if callback_ty == PhpType::Callable {
        return Ok(());
    }
    if super::callables::runtime_callable_array_type(&callback_ty) {
        return Ok(());
    }

    Err(CompileError::new(
        callback.span,
        "iterator_apply() callback must be callable",
    ))
}

/// Checks iterator apply static callback and reports a compile error when it is invalid.
pub(crate) fn check_iterator_apply_static_callback(
    checker: &mut Checker,
    callback: &Expr,
    callback_args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    match super::callables::check_callback_builtin_call(
        checker,
        callback,
        callback_args,
        span,
        env,
        "iterator_apply() callback",
    ) {
        Ok(_) => Ok(()),
        Err(error) if error.message == ITERATOR_APPLY_UNKNOWN_STATIC_CALLBACK_SIG => {
            let callback_ty = checker.infer_type(callback, env)?;
            if callback_ty != PhpType::Callable && callback_ty != PhpType::Str {
                return Err(error);
            }
            for arg in callback_args {
                checker.infer_type(arg, env)?;
            }
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// Builds the argument list for reject dynamic ref.
fn reject_dynamic_ref_args(
    _sig: &crate::types::FunctionSig,
    _span: crate::span::Span,
) -> Result<(), CompileError> {
    Ok(())
}

/// Provides the Specialize iterator apply dynamic assoc variadic target helper used by the SPL module.
fn specialize_iterator_apply_dynamic_assoc_variadic_target(
    checker: &mut Checker,
    target: &CallableTarget,
    sig: &crate::types::FunctionSig,
    associative_args: bool,
) -> Result<(), CompileError> {
    if !associative_args || sig.variadic.is_none() {
        return Ok(());
    }
    if let CallableTarget::Function(name) = target {
        super::callables::specialize_dynamic_assoc_variadic_user_callback(
            checker,
            name.as_str(),
            sig,
        )?;
    }
    Ok(())
}

/// Returns true when static callback arg literal.
fn is_static_callback_arg_literal(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        ExprKind::Negate(inner) => matches!(
            inner.kind,
            ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
        ),
        _ => false,
    }
}
