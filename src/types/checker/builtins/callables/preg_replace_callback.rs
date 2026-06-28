//! Purpose:
//! Type-checks the PHP `preg_replace_callback()` builtin.
//! Provides contextual typing for callback `$matches` arrays before closure body inference.
//!
//! Called from:
//! - `crate::types::checker::builtins::callables::check_builtin()`.
//!
//! Key details:
//! - Untyped callback parameters must infer as `array<string>` so `$matches[0]`
//!   and capture-group accesses are accepted before runtime callback emission.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::{check_callback_builtin_call, BuiltinResult};
use super::super::super::Checker;

/// Builds a synthetic `[""]` array literal expression representing the contextual
/// type hint for `$matches` in `preg_replace_callback` callbacks.
///
/// The empty-string element signals to the type checker that the closure's
/// first parameter should be typed as `array<string>`, enabling safe access
/// to `$matches[0]` and named capture groups before runtime.
fn matches_arg(span: crate::span::Span) -> Expr {
    Expr::new(
        ExprKind::ArrayLiteral(vec![Expr::new(ExprKind::StringLiteral(String::new()), span)]),
        span,
    )
}

/// Returns the PHP type `array<string>` representing the contextual type of the
/// `$matches` parameter in `preg_replace_callback` callbacks.
///
/// This is the type injected into the closure's first parameter when no explicit
/// type hint is given, enabling type-safe access to match groups at analysis time.
fn matches_type() -> PhpType {
    PhpType::Array(Box::new(PhpType::Str))
}

/// Builds a `FunctionSig` for a closure with contextual argument type hints injected.
///
/// For each closure parameter:
/// - If a type annotation is present, validates compatibility with the contextual type
///   and specializes generic array hints accordingly.
/// - If no annotation is present, uses the contextual type when available, otherwise
///   falls back to `Int` for the environment and `Mixed` for the signature.
///
/// Also resolves the closure's return type against its body and validates any `use()`
/// variables. Returns `Ok(None)` if `callback` is not a `Closure` expression.
fn contextual_closure_sig(
    checker: &mut Checker,
    callback: &Expr,
    contextual_arg_types: &[PhpType],
    env: &TypeEnv,
) -> Result<Option<FunctionSig>, CompileError> {
    let ExprKind::Closure {
        params,
        variadic,
        return_type,
        body,
        captures,
        ..
    } = &callback.kind
    else {
        return Ok(None);
    };

    for cap in captures {
        if !env.contains_key(cap) {
            return Err(CompileError::new(
                callback.span,
                &format!("Undefined variable in use(): ${}", cap),
            ));
        }
    }

    let mut closure_env = env.clone();
    let mut param_types = Vec::new();
    let mut defaults = Vec::new();
    let mut ref_params = Vec::new();
    let mut declared_params = Vec::new();

    for (idx, (name, type_ann, default, is_ref)) in params.iter().enumerate() {
        let contextual_ty = contextual_arg_types.get(idx);
        let (env_ty, sig_ty, declared) = match type_ann {
            Some(type_ann) => {
                let declared_ty = checker.resolve_declared_param_type_hint(
                    type_ann,
                    callback.span,
                    &format!("Closure parameter ${}", name),
                )?;
                checker.validate_declared_default_type(
                    &declared_ty,
                    default.as_ref(),
                    callback.span,
                    &format!("Closure parameter ${}", name),
                )?;
                if let Some(actual_ty) = contextual_ty {
                    checker.require_compatible_arg_type(
                        &declared_ty,
                        actual_ty,
                        callback.span,
                        &format!("Closure parameter ${}", name),
                    )?;
                    let specialized_ty =
                        Checker::specialize_generic_array_hint(&declared_ty, actual_ty);
                    (specialized_ty.clone(), specialized_ty, true)
                } else {
                    (declared_ty.clone(), declared_ty, true)
                }
            }
            None => contextual_ty
                .cloned()
                .map(|ty| (ty.clone(), ty, false))
                .unwrap_or((PhpType::Int, PhpType::Mixed, false)),
        };

        closure_env.insert(name.clone(), env_ty);
        param_types.push((name.clone(), sig_ty));
        defaults.push(default.clone());
        ref_params.push(*is_ref);
        declared_params.push(declared);
    }

    if let Some(name) = variadic {
        closure_env.insert(name.clone(), PhpType::Array(Box::new(PhpType::Int)));
        param_types.push((name.clone(), PhpType::Array(Box::new(PhpType::Mixed))));
        defaults.push(None);
        ref_params.push(false);
        declared_params.push(false);
    }

    let (return_type, declared_return) =
        checker.resolve_closure_return_type(body, return_type, callback.span, &closure_env)?;
    Ok(Some(FunctionSig {
        params: param_types,
        defaults,
        return_type,
        declared_return,
        by_ref_return: false,
        ref_params,
        declared_params,
        variadic: variadic.clone(),
        deprecation: None,
    }))
}

/// Type-checks a call to PHP `preg_replace_callback(pattern, callback, subject)`.
///
/// Validates exactly 3 arguments, infers types for the pattern and subject
/// expressions, synthesizes an `array<string>` type for the `$matches` callback
/// parameter, and delegates to `check_known_callable_call` to verify the closure
/// signature. Returns `PhpType::Str` on success.
pub(super) fn check(
    checker: &mut Checker,
    args: &[Expr],
    span: crate::span::Span,
    env: &TypeEnv,
) -> BuiltinResult {
    if args.len() != 3 {
        return Err(CompileError::new(
            span,
            "preg_replace_callback() takes exactly 3 arguments",
        ));
    }

    checker.infer_type(&args[0], env)?;
    checker.infer_type(&args[2], env)?;

    let callback_args = vec![matches_arg(span)];
    if let Some(sig) = contextual_closure_sig(checker, &args[1], &[matches_type()], env)? {
        checker.check_known_callable_call(
            &sig,
            &callback_args,
            span,
            env,
            "preg_replace_callback() callback",
        )?;
        return Ok(Some(PhpType::Str));
    }

    check_callback_builtin_call(
        checker,
        &args[1],
        &callback_args,
        span,
        env,
        "preg_replace_callback() callback",
    )?;
    Ok(Some(PhpType::Str))
}
