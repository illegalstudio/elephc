use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    pub(crate) fn has_named_args(args: &[Expr]) -> bool {
        args.iter()
            .any(|arg| matches!(arg.kind, ExprKind::NamedArg { .. }))
    }

    pub(crate) fn normalize_named_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
    ) -> Result<Vec<Expr>, CompileError> {
        if !Self::has_named_args(args) {
            return Ok(args.to_vec());
        }

        if args.iter().any(|arg| matches!(arg.kind, ExprKind::Spread(_))) {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} does not support mixing named arguments with spread arguments yet",
                    callee_desc
                ),
            ));
        }

        let regular_param_count = if sig.variadic.is_some() {
            sig.params.len().saturating_sub(1)
        } else {
            sig.params.len()
        };
        let mut resolved: Vec<Option<Expr>> = vec![None; regular_param_count];
        let mut variadic_args = Vec::new();
        let mut positional_idx = 0usize;
        let mut seen_named = false;

        for arg in args {
            match &arg.kind {
                ExprKind::NamedArg { name, value } => {
                    seen_named = true;
                    let Some(param_idx) = sig
                        .params
                        .iter()
                        .take(regular_param_count)
                        .position(|(param_name, _)| param_name == name)
                    else {
                        return Err(CompileError::new(
                            arg.span,
                            &format!("{} has no parameter ${}", callee_desc, name),
                        ));
                    };
                    if resolved[param_idx].is_some() {
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "{} parameter ${} is already assigned",
                                callee_desc, name
                            ),
                        ));
                    }
                    resolved[param_idx] = Some((**value).clone());
                }
                _ => {
                    if seen_named {
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "{} cannot use positional arguments after named arguments",
                                callee_desc
                            ),
                        ));
                    }
                    if positional_idx < regular_param_count {
                        resolved[positional_idx] = Some(arg.clone());
                    } else {
                        variadic_args.push(arg.clone());
                    }
                    positional_idx += 1;
                }
            }
        }

        let mut normalized = Vec::new();
        for (idx, slot) in resolved.into_iter().enumerate() {
            if let Some(arg) = slot {
                normalized.push(arg);
            } else if let Some(Some(default_expr)) = sig.defaults.get(idx) {
                normalized.push(default_expr.clone());
            } else {
                let param_name = sig
                    .params
                    .get(idx)
                    .map(|(name, _)| name.as_str())
                    .unwrap_or("arg");
                return Err(CompileError::new(
                    span,
                    &format!("{} missing required parameter ${}", callee_desc, param_name),
                ));
            }
        }
        normalized.extend(variadic_args);
        Ok(normalized)
    }

    pub(crate) fn types_compatible(expected: &PhpType, actual: &PhpType) -> bool {
        if expected == actual {
            return true;
        }

        match (expected, actual) {
            (PhpType::Mixed, _) => true,
            (PhpType::Union(members), _) => {
                members.iter().any(|m| Self::types_compatible(m, actual))
            }
            (
                PhpType::AssocArray { key, value },
                PhpType::Array(_) | PhpType::AssocArray { .. },
            ) if **key == PhpType::Mixed && **value == PhpType::Mixed => true,
            (PhpType::Float, PhpType::Int | PhpType::Bool | PhpType::Void) => true,
            (PhpType::Int, PhpType::Bool | PhpType::Void) => true,
            (PhpType::Bool, PhpType::Int | PhpType::Void) => true,
            (PhpType::Pointer(_), PhpType::Pointer(_) | PhpType::Void) => true,
            (PhpType::Callable, PhpType::Callable) => true,
            _ => false,
        }
    }

    pub(crate) fn require_compatible_arg_type(
        &self,
        expected: &PhpType,
        actual: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if Self::types_compatible(expected, actual) || self.type_accepts(expected, actual) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                &format!("{} expects {:?}, got {:?}", context, expected, actual),
            ))
        }
    }

    pub(crate) fn format_fixed_or_range_arity(min_args: usize, max_args: usize) -> String {
        if min_args == max_args {
            format!("{}", min_args)
        } else {
            format!("{} to {}", min_args, max_args)
        }
    }

    pub(crate) fn check_known_callable_call(
        &mut self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        caller_env: &TypeEnv,
        callee_desc: &str,
    ) -> Result<PhpType, CompileError> {
        let normalized_args = self.normalize_named_call_args(sig, args, span, callee_desc)?;
        let args = normalized_args.as_slice();
        let effective_arg_count = args
            .iter()
            .filter(|a| !matches!(a.kind, ExprKind::Spread(_)))
            .count();
        let has_spread = args.iter().any(|a| matches!(a.kind, ExprKind::Spread(_)));
        let required = sig.defaults.iter().filter(|d| d.is_none()).count();

        if sig.ref_params.iter().any(|is_ref| *is_ref) && has_spread {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} cannot be invoked with spread arguments when it has pass-by-reference parameters",
                    callee_desc
                ),
            ));
        }

        if !has_spread {
            if sig.variadic.is_some() {
                if effective_arg_count < required {
                    return Err(CompileError::new(
                        span,
                        &format!(
                            "{} expects at least {} arguments, got {}",
                            callee_desc, required, effective_arg_count
                        ),
                    ));
                }
            } else if effective_arg_count < required || effective_arg_count > sig.params.len() {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "{} expects {} arguments, got {}",
                        callee_desc,
                        Self::format_fixed_or_range_arity(required, sig.params.len()),
                        effective_arg_count
                    ),
                ));
            }
        }

        let regular_param_count = if sig.variadic.is_some() {
            sig.params.len().saturating_sub(1)
        } else {
            sig.params.len()
        };
        let variadic_elem_ty = sig.variadic.as_ref().and_then(|_| {
            sig.params.last().and_then(|(_, ty)| match ty {
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
                if sig.ref_params.get(param_idx).copied().unwrap_or(false)
                    && !matches!(arg.kind, ExprKind::Variable(_))
                {
                    let param_name = sig
                        .params
                        .get(param_idx)
                        .map(|(name, _)| name.as_str())
                        .unwrap_or("arg");
                    return Err(CompileError::new(
                        arg.span,
                        &format!(
                            "{} parameter ${} must be passed a variable",
                            callee_desc, param_name
                        ),
                    ));
                }
                if let Some((param_name, expected_ty)) = sig.params.get(param_idx) {
                    if sig.declared_params.get(param_idx).copied().unwrap_or(false)
                        && sig.ref_params.get(param_idx).copied().unwrap_or(false)
                    {
                        self.require_boxed_by_ref_storage(
                            expected_ty,
                            &actual_ty,
                            arg.span,
                            &format!("{} parameter ${}", callee_desc, param_name),
                        )?;
                    }
                    self.require_compatible_arg_type(
                        expected_ty,
                        &actual_ty,
                        arg.span,
                        &format!("{} parameter ${}", callee_desc, param_name),
                    )?;
                }
            } else if let (Some(vname), Some(expected_ty)) =
                (sig.variadic.as_ref(), variadic_elem_ty.as_ref())
            {
                self.require_compatible_arg_type(
                    expected_ty,
                    &actual_ty,
                    arg.span,
                    &format!("{} variadic parameter ${}", callee_desc, vname),
                )?;
            }
            param_idx += 1;
        }

        Ok(sig.return_type.clone())
    }
}
