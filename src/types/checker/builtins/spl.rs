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

/// Result type for SPL builtin type-checking: `Ok(None)` means the builtin is not
/// handled by this module (caller should try the next handler), `Ok(Some(t))` means
/// the builtin was handled and returns type `t`, and `Err(e)` is a type-error.
type BuiltinResult = Result<Option<PhpType>, CompileError>;

const ITERATOR_APPLY_UNKNOWN_STATIC_CALLBACK_SIG: &str =
    "iterator_apply() callback must have a statically known callable signature";

/// Type-checks a call to an SPL autoload or object-helper builtin.
///
/// Returns `Ok(None)` for unknown SPL builtins (caller falls through); `Ok(Some(t))`
/// for handled builtins with inferred return type `t`; `Err` if argument count, types,
/// or literal constraints are violated.
///
/// # Arguments
/// * `checker` – mutable checker state used to infer argument types
/// * `name` – lowercase SPL builtin name (e.g. `"spl_autoload_register"`);
/// * `args` – call arguments to validate
/// * `span` – source location for error reporting
/// * `env` – current type environment
pub(super) fn check_builtin(
    checker: &mut Checker,
    name: &str,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    match name {
        "spl_autoload_register" => {
            if args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_register() takes at most 3 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Bool))
        }
        "spl_autoload_unregister" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_unregister() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Bool))
        }
        "spl_autoload_functions" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_functions() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Mixed))))
        }
        "spl_autoload_extensions" => {
            if args.len() > 1 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_extensions() takes at most 1 argument",
                ));
            }
            if let Some(arg) = args.first() {
                checker.infer_type(arg, env)?;
                if !matches!(
                    arg.kind,
                    ExprKind::StringLiteral(_) | ExprKind::Null
                ) {
                    return Err(CompileError::new(
                        span,
                        "spl_autoload_extensions() argument must be a string literal or null",
                    ));
                }
            }
            Ok(Some(PhpType::Str))
        }
        "spl_autoload_call" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload_call() takes exactly 1 argument",
                ));
            }
            checker.infer_type(&args[0], env)?;
            Ok(Some(PhpType::Void))
        }
        "spl_autoload" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    "spl_autoload() takes 1 or 2 arguments",
                ));
            }
            for arg in args {
                checker.infer_type(arg, env)?;
            }
            Ok(Some(PhpType::Void))
        }
        "spl_object_id" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "spl_object_id() takes exactly 1 argument",
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Object(_)) {
                return Err(CompileError::new(
                    span,
                    "spl_object_id() argument must be an object",
                ));
            }
            Ok(Some(PhpType::Int))
        }
        "spl_object_hash" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "spl_object_hash() takes exactly 1 argument",
                ));
            }
            let ty = checker.infer_type(&args[0], env)?;
            if !matches!(ty, PhpType::Object(_)) {
                return Err(CompileError::new(
                    span,
                    "spl_object_hash() argument must be an object",
                ));
            }
            Ok(Some(PhpType::Str))
        }
        "spl_classes" => {
            if !args.is_empty() {
                return Err(CompileError::new(
                    span,
                    "spl_classes() takes no arguments",
                ));
            }
            Ok(Some(PhpType::Array(Box::new(PhpType::Str))))
        }
        "iterator_count" => {
            if args.len() != 1 {
                return Err(CompileError::new(
                    span,
                    "iterator_count() takes exactly 1 argument",
                ));
            }
            check_iterator_source(checker, &args[0], span, env, "iterator_count()")?;
            Ok(Some(PhpType::Int))
        }
        "iterator_to_array" => {
            if args.is_empty() || args.len() > 2 {
                return Err(CompileError::new(
                    span,
                    "iterator_to_array() takes 1 or 2 arguments",
                ));
            }
            let source_ty =
                check_iterator_source(checker, &args[0], span, env, "iterator_to_array()")?;
            let preserve_keys = if let Some(arg) = args.get(1) {
                check_iterator_to_array_preserve_keys(checker, arg, env)?
            } else {
                Some(true)
            };
            Ok(Some(iterator_to_array_return_type(
                checker,
                &source_ty,
                preserve_keys,
            )))
        }
        "iterator_apply" => {
            if args.len() < 2 || args.len() > 3 {
                return Err(CompileError::new(
                    span,
                    "iterator_apply() takes 2 or 3 arguments",
                ));
            }
            check_iterator_apply_source(checker, &args[0], span, env)?;
            match iterator_apply_callback_args(checker, args.get(2), span, env)? {
                IteratorApplyArgs::Static(callback_args) => {
                    check_iterator_apply_static_callback(
                        checker,
                        &args[1],
                        callback_args,
                        span,
                        env,
                    )?;
                }
                IteratorApplyArgs::Dynamic { associative } => {
                    check_iterator_apply_dynamic_callback(
                        checker,
                        &args[1],
                        associative,
                        span,
                        env,
                    )?;
                }
            }
            Ok(Some(PhpType::Int))
        }
        _ => Ok(None),
    }
}

/// Checks iterator source and reports a compile error when it is invalid.
fn check_iterator_source(
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
fn check_iterator_apply_source(
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
fn check_iterator_to_array_preserve_keys(
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
fn iterator_to_array_return_type(
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

enum IteratorApplyArgs<'a> {
    Static(&'a [Expr]),
    Dynamic { associative: bool },
}

/// Builds the argument list for iterator apply callback.
fn iterator_apply_callback_args<'a>(
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
fn check_iterator_apply_dynamic_callback(
    checker: &mut Checker,
    callback: &Expr,
    associative_args: bool,
    span: crate::span::Span,
    env: &TypeEnv,
) -> Result<(), CompileError> {
    if checker.expr_call_complex_callee_needs_runtime_capture(callback) {
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

    Err(CompileError::new(
        callback.span,
        "iterator_apply() callback must be callable",
    ))
}

/// Checks iterator apply static callback and reports a compile error when it is invalid.
fn check_iterator_apply_static_callback(
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
