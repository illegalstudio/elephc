use crate::errors::CompileError;
use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::span::Span;
use crate::types::call_args::{self, NamedParamMatch, NamedParamTracker, PrefixArg};
use crate::types::{FunctionSig, PhpType, TypeEnv};

use super::super::Checker;

fn spread_element_expr(spread_expr: &Expr, element_idx: usize, span: Span) -> Expr {
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(Expr::new(ExprKind::IntLiteral(element_idx as i64), span)),
        },
        span,
    )
}

fn spread_element_or_default_expr(
    spread_expr: &Expr,
    element_idx: usize,
    default_expr: Expr,
    span: Span,
) -> Expr {
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(spread_len_expr(spread_expr, span)),
                    op: BinOp::Gt,
                    right: Box::new(Expr::new(ExprKind::IntLiteral(element_idx as i64), span)),
                },
                span,
            )),
            then_expr: Box::new(spread_element_expr(spread_expr, element_idx, span)),
            else_expr: Box::new(default_expr),
        },
        span,
    )
}

fn spread_len_expr(spread_expr: &Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("count"),
            args: vec![spread_expr.clone()],
        },
        span,
    )
}

impl Checker {
    pub(crate) fn has_named_args(args: &[Expr]) -> bool {
        call_args::has_named_args(args)
    }

    pub(crate) fn normalize_named_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
    ) -> Result<Vec<Expr>, CompileError> {
        self.normalize_call_args(sig, args, span, callee_desc, false, true)
    }

    pub(crate) fn normalize_builtin_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
    ) -> Result<Vec<Expr>, CompileError> {
        self.normalize_call_args(sig, args, span, callee_desc, true, false)
    }

    fn normalize_call_args(
        &self,
        sig: &FunctionSig,
        args: &[Expr],
        span: crate::span::Span,
        callee_desc: &str,
        trim_trailing_defaults: bool,
        allow_unknown_named_variadic: bool,
    ) -> Result<Vec<Expr>, CompileError> {
        let expanded_args = call_args::expand_static_assoc_spread_args(args);
        let args = expanded_args.as_slice();

        if !call_args::has_named_args(args) {
            let mut seen_spread = false;
            for arg in args {
                if matches!(arg.kind, ExprKind::Spread(_)) {
                    seen_spread = true;
                } else if seen_spread {
                    return Err(CompileError::new(
                        arg.span,
                        &format!(
                            "{} cannot use positional arguments after spread arguments",
                            callee_desc
                        ),
                    ));
                }
            }
            return Ok(args.to_vec());
        }

        let regular_param_count = call_args::regular_param_count(sig);
        let mut named_values: Vec<Option<(Expr, Span)>> = vec![None; regular_param_count];
        let mut named_tracker = NamedParamTracker::new(regular_param_count);
        let mut prefix_args = Vec::new();
        let mut variadic_args = Vec::new();
        let mut seen_named = false;
        let mut seen_spread = false;

        for arg in args {
            match &arg.kind {
                ExprKind::NamedArg { name, value } => {
                    seen_named = true;
                    match named_tracker.assign(
                        sig,
                        regular_param_count,
                        name,
                        allow_unknown_named_variadic,
                    ) {
                        Ok(NamedParamMatch::Regular(param_idx)) => {
                            named_values[param_idx] = Some(((**value).clone(), arg.span));
                        }
                        Ok(NamedParamMatch::Variadic) => {
                            variadic_args.push(Expr::new(
                                ExprKind::NamedArg {
                                    name: name.clone(),
                                    value: value.clone(),
                                },
                                arg.span,
                            ));
                            continue;
                        }
                        Ok(NamedParamMatch::Unknown) => {
                            return Err(CompileError::new(
                                arg.span,
                                &format!("{} has no parameter ${}", callee_desc, name),
                            ));
                        }
                        Err(duplicate) => {
                            let param_name = sig
                                .params
                                .get(duplicate.param_idx)
                                .map(|(name, _)| name.as_str())
                                .unwrap_or(name);
                            return Err(CompileError::new(
                                arg.span,
                                &format!(
                                    "{} parameter ${} is already assigned",
                                    callee_desc, param_name
                                ),
                            ));
                        }
                    }
                }
                ExprKind::Spread(inner) => {
                    if seen_named {
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "{} cannot use argument unpacking after named arguments",
                                callee_desc
                            ),
                        ));
                    }
                    seen_spread = true;
                    prefix_args.push(PrefixArg::Spread((**inner).clone(), arg.span));
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
                    if seen_spread {
                        return Err(CompileError::new(
                            arg.span,
                            &format!(
                                "{} cannot use positional arguments after spread arguments",
                                callee_desc
                            ),
                        ));
                    }
                    prefix_args.push(PrefixArg::Positional(arg.clone()));
                }
            }
        }

        let mut resolved: Vec<Option<Expr>> = vec![None; regular_param_count];
        let mut positional_idx = 0usize;

        for prefix_arg in prefix_args {
            match prefix_arg {
                PrefixArg::Positional(arg) => {
                    if positional_idx < regular_param_count {
                        if let Some((_, named_span)) = &named_values[positional_idx] {
                            let param_name = sig
                                .params
                                .get(positional_idx)
                                .map(|(name, _)| name.as_str())
                                .unwrap_or("arg");
                            return Err(CompileError::new(
                                *named_span,
                                &format!(
                                    "{} parameter ${} is already assigned",
                                    callee_desc, param_name
                                ),
                            ));
                        }
                        resolved[positional_idx] = Some(arg);
                    } else {
                        variadic_args.push(arg);
                    }
                    positional_idx += 1;
                }
                PrefixArg::Spread(inner, spread_span) => {
                    let next_named_idx = (positional_idx..regular_param_count)
                        .find(|idx| named_values[*idx].is_some())
                        .unwrap_or(regular_param_count);
                    for element_idx in 0..next_named_idx.saturating_sub(positional_idx) {
                        let default = sig
                            .defaults
                            .get(positional_idx)
                            .and_then(|default| default.as_ref());
                        resolved[positional_idx] = Some(if let Some(default) = default {
                            spread_element_or_default_expr(
                                &inner,
                                element_idx,
                                default.clone(),
                                spread_span,
                            )
                        } else {
                            spread_element_expr(&inner, element_idx, spread_span)
                        });
                        positional_idx += 1;
                    }
                }
            }
        }

        for (idx, named_value) in named_values.into_iter().enumerate() {
            if let Some((value, named_span)) = named_value {
                if resolved[idx].is_some() {
                    let param_name = sig
                        .params
                        .get(idx)
                        .map(|(name, _)| name.as_str())
                        .unwrap_or("arg");
                    return Err(CompileError::new(
                        named_span,
                        &format!(
                            "{} parameter ${} is already assigned",
                            callee_desc, param_name
                        ),
                    ));
                }
                resolved[idx] = Some(value);
            }
        }

        let mut normalized = Vec::new();
        let output_len = if trim_trailing_defaults {
            resolved
                .iter()
                .rposition(|slot| slot.is_some())
                .map(|idx| idx + 1)
                .unwrap_or(0)
        } else {
            regular_param_count
        };
        for (idx, slot) in resolved.into_iter().take(output_len).enumerate() {
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
            (_, PhpType::Never) => true, // never is the bottom type — compatible with any expected type
            (PhpType::Iterable, PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable) => true,
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
            (PhpType::Resource(_), PhpType::Resource(_)) => {
                PhpType::resource_types_compatible(expected, actual)
            }
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
