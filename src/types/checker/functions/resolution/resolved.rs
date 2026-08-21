//! Purpose:
//! Validates normalized calls against resolved function signatures.
//!
//! Called from:
//! - `Checker::check_function_call()` and function-variant resolution.
//!
//! Key details:
//! - Arity, by-reference lvalues, regular parameters, and variadic tails share one validation path.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use crate::types::checker::Checker;

impl Checker {
    /// Checks a function call where arguments are already normalized (named,
    /// spread, and order resolved) against a function already present in
    /// `self.functions`.
    ///
    /// Used by variant-group resolution and other paths that have a pre-resolved
    /// signature and do not need re-specialization or deprecation warnings.
    pub(crate) fn check_function_call_pre_normalized(
        &mut self,
        name: &str,
        normalized_args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let sig = self
            .functions
            .get(name)
            .cloned()
            .ok_or_else(|| self.unresolved_function_call_error(name, span))?;
        let effective_sig = Self::callable_sig_for_declared_params(&sig, &sig.declared_params);
        self.check_normalized_resolved_function_call(
            name,
            &sig,
            &effective_sig,
            normalized_args,
            span,
            caller_env,
        )
    }

    /// Validates a resolved, normalized function call against its effective
    /// signature: arity constraints, by-ref argument validation, and type
    /// compatibility for each argument position (regular and variadic).
    ///
    /// Does **not** re-specialize or check deprecation — those are handled by
    /// the caller before dispatching here. Returns the signature's return type
    /// on success.
    pub(super) fn check_normalized_resolved_function_call(
        &mut self,
        name: &str,
        sig: &FunctionSig,
        effective_sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let effective_arg_count = args
            .iter()
            .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
            .count();
        let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));
        let required = effective_sig.defaults.iter().filter(|d| d.is_none()).count();
        if effective_sig.ref_params.iter().any(|is_ref| *is_ref) && has_spread {
            return Err(CompileError::new(
                span,
                &format!(
                    "Function '{}' cannot be invoked with spread arguments when it has pass-by-reference parameters",
                    name
                ),
            ));
        }
        if !has_spread {
            if effective_sig.variadic.is_some() {
                if effective_arg_count < required {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "Function '{}' expects at least {} arguments, got {}",
                            name, required, effective_arg_count
                        ),
                    ));
                }
            } else if effective_arg_count < required
                || effective_arg_count > effective_sig.params.len()
            {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Function '{}' expects {} arguments, got {}",
                        name,
                        Self::format_fixed_or_range_arity(required, effective_sig.params.len()),
                        effective_arg_count
                    ),
                ));
            }
        }
        let regular_param_count = if effective_sig.variadic.is_some() {
            effective_sig.params.len().saturating_sub(1)
        } else {
            effective_sig.params.len()
        };
        let variadic_elem_ty = effective_sig.variadic.as_ref().and_then(|_| {
            effective_sig.params.last().and_then(|(_, ty)| match ty {
                PhpType::Array(elem) => Some((**elem).clone()),
                _ => None,
            })
        });
        let mut param_idx = 0usize;
        for arg in args {
            let actual_ty = self.infer_type(arg, caller_env)?;
            if matches!(arg.kind, ExprKind::Spread(_)) {
                continue;
            }
            if param_idx < regular_param_count {
                if effective_sig
                    .ref_params
                    .get(param_idx)
                    .copied()
                    .unwrap_or(false)
                    && !self.is_by_ref_argument_lvalue(arg, caller_env, true)?
                {
                    let param_name = effective_sig
                        .params
                        .get(param_idx)
                        .map(|(name, _)| name.as_str())
                        .unwrap_or("arg");
                    return Err(CompileError::new(
                        arg.span,
                        &format!(
                            "Function '{}' parameter ${} must be passed a variable",
                            name, param_name
                        ),
                    ));
                }
                if let Some((param_name, expected_ty)) = effective_sig.params.get(param_idx) {
                    if effective_sig
                        .declared_params
                        .get(param_idx)
                        .copied()
                        .unwrap_or(false)
                        && effective_sig
                            .ref_params
                            .get(param_idx)
                            .copied()
                            .unwrap_or(false)
                    {
                        self.require_boxed_by_ref_storage(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &format!("Function '{}' parameter ${}", name, param_name),
                        )?;
                    }
                    // PHP's parameter binding only applies to a *declared* parameter type.
                    // An inferred parameter's "expected" type is just what earlier call sites
                    // produced, so coercing against it would invent a conversion PHP does not
                    // perform.
                    if effective_sig
                        .declared_params
                        .get(param_idx)
                        .copied()
                        .unwrap_or(false)
                    {
                        self.require_bound_param_arg_type(
                            expected_ty,
                            &actual_ty,
                            arg,
                            caller_env,
                            &format!("Function '{}' parameter ${}", name, param_name),
                            Some((name, param_name.as_str())),
                            effective_sig.ref_params.get(param_idx).copied().unwrap_or(false),
                        )?;
                    } else {
                        self.require_compatible_arg_type(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &format!("Function '{}' parameter ${}", name, param_name),
                        )?;
                    }
                }
            } else if let (Some(vname), Some(expected_ty)) =
                (effective_sig.variadic.as_ref(), variadic_elem_ty.as_ref())
            {
                self.require_compatible_arg_type(
                    expected_ty,
                    &actual_ty,
                    arg.span,
                    &format!("Function '{}' variadic parameter ${}", name, vname),
                )?;
            }
            param_idx += 1;
        }
        Ok(sig.return_type.clone())
    }
}
