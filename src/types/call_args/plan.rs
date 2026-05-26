//! Purpose:
//! Defines the normalized call-argument plan shared by checker and codegen.
//! Records source values, regular slots, variadic entries, spread bounds checks, and planner errors.
//!
//! Called from:
//! - `crate::types::call_args::planner`
//!
//! Key details:
//! - Plan data must retain enough span and source-index information to produce PHP-compatible diagnostics and evaluation order.

use crate::names::Name;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::span::Span;

/// Normalized call-argument plan used by both the type checker and codegen.
///
/// Retains source-order args, resolved regular/variadic slots, spread bounds checks,
/// and any planner errors. Enough span and source-index data is kept to produce
/// PHP-compatible diagnostics while preserving evaluation order.
pub(crate) struct CallArgPlan {
    pub(crate) source_args: Vec<Expr>,
    pub(crate) regular_args: Vec<PlannedRegularArg>,
    pub(crate) variadic_args: Vec<PlannedVariadicArg>,
    pub(crate) source_values: Vec<PlannedSourceValue>,
    pub(crate) spread_bounds_checks: Vec<SpreadBoundsCheck>,
    pub(crate) first_named_pos: Option<usize>,
    pub(crate) prefix_has_dynamic_named_spread: bool,
    pub(super) passthrough_args: Option<Vec<Expr>>,
}

/// A resolved regular (non-variadic) parameter slot in the plan.
#[derive(Clone)]
pub(crate) enum PlannedRegularArg {
    Source {
        source_index: usize,
        expr: Expr,
    },
    SpreadElement {
        spread_expr: Expr,
        spread_span: Span,
        element_idx: usize,
        prefix_element_idx: usize,
        param_name: Option<String>,
        prefer_named_key: bool,
        default: Option<Expr>,
        guaranteed_present: bool,
    },
    Default(Expr),
}

/// A resolved variadic argument entry collected from positional or named call-site values.
#[derive(Clone)]
pub(crate) struct PlannedVariadicArg {
    pub(crate) key: Option<String>,
    pub(crate) expr: Expr,
}

/// A call-site argument projected into the parameter space, tracking which
/// source argument it came from and which parameter slot it fills.
#[derive(Clone)]
pub(crate) enum PlannedSourceValue {
    Regular {
        source_index: usize,
        param_idx: usize,
        expr: Expr,
    },
    Variadic {
        source_index: usize,
        key: Option<String>,
        expr: Expr,
    },
}

/// Records the minimum and optional maximum number of elements a spread argument
/// must contain to satisfy the parameter slots it feeds.
#[derive(Clone)]
pub(crate) struct SpreadBoundsCheck {
    pub(crate) spread_expr: Expr,
    pub(crate) min_len: usize,
    pub(crate) max_len: Option<usize>,
}

/// Errors from `plan_call_args` when call-site arguments cannot be mapped to the signature.
#[derive(Debug)]
pub(crate) enum CallArgPlanError {
    UnknownNamed {
        span: Span,
        name: String,
    },
    Duplicate {
        span: Span,
        param_idx: usize,
        name: String,
    },
    PositionalAfterNamed {
        span: Span,
    },
    PositionalAfterSpread {
        span: Span,
    },
    SpreadAfterNamed {
        span: Span,
    },
    MissingRequired {
        span: Span,
        param_idx: usize,
    },
}

impl CallArgPlan {
    /// Returns `true` if any source argument used named-parameter syntax.
    pub(crate) fn has_named_args(&self) -> bool {
        self.first_named_pos.is_some()
    }

    /// Returns `true` if any source argument used the spread (`...`) operator.
    pub(crate) fn has_spread_args(&self) -> bool {
        self.source_args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
    }

    /// Returns a flat list of expressions in parameter order suitable for codegen
    /// materialization. Preserves spread expressions for static-array spreads that
    /// were resolved in place; applies default guards for optional slots when the
    /// spread may not have provided those elements.
    pub(crate) fn normalized_args(&self) -> Vec<Expr> {
        if let Some(args) = &self.passthrough_args {
            return args.clone();
        }

        let mut args = Vec::new();
        for arg in &self.regular_args {
            args.push(match arg {
                PlannedRegularArg::Source { expr, .. } => expr.clone(),
                PlannedRegularArg::SpreadElement {
                    spread_expr,
                    spread_span,
                    element_idx,
                    param_name,
                    prefer_named_key,
                    default,
                    guaranteed_present,
                    ..
                } => {
                    if let Some(default) = default {
                        if *guaranteed_present {
                            spread_element_expr(
                                spread_expr,
                                *element_idx,
                                param_name.as_deref(),
                                *prefer_named_key,
                                *spread_span,
                            )
                        } else {
                            spread_element_or_default_expr(
                                spread_expr,
                                *element_idx,
                                param_name.as_deref(),
                                *prefer_named_key,
                                default.clone(),
                                *spread_span,
                            )
                        }
                    } else {
                        spread_element_expr(
                            spread_expr,
                            *element_idx,
                            param_name.as_deref(),
                            *prefer_named_key,
                            *spread_span,
                        )
                    }
                }
                PlannedRegularArg::Default(default) => default.clone(),
            });
        }

        for arg in &self.variadic_args {
            if let Some(key) = &arg.key {
                args.push(Expr::new(
                    ExprKind::NamedArg {
                        name: key.clone(),
                        value: Box::new(arg.expr.clone()),
                    },
                    arg.expr.span,
                ));
            } else {
                args.push(arg.expr.clone());
            }
        }
        args
    }

    /// Returns the leading positional-only prefix as either a bare spread expression
    /// (when there is exactly one prefix element that is a spread) or an array literal
    /// of all prefix elements. Used to reconstruct the leading positional arguments
    /// when forwarding calls with named parameters.
    pub(crate) fn positional_prefix_expr(&self, call_span: Span) -> Option<Expr> {
        let first_named_pos = self.first_named_pos?;
        let prefix_args = self.source_args[..first_named_pos].to_vec();
        let prefix_span = prefix_args
            .first()
            .map(|arg| arg.span)
            .unwrap_or(call_span);
        if let [arg] = prefix_args.as_slice() {
            if let ExprKind::Spread(inner) = &arg.kind {
                return Some((**inner).clone());
            }
        }
        Some(Expr::new(ExprKind::ArrayLiteral(prefix_args), prefix_span))
    }
}

impl PlannedSourceValue {
    /// The position of this value's source argument in the original call-site argument list.
    pub(crate) fn source_index(&self) -> usize {
        match self {
            PlannedSourceValue::Regular { source_index, .. }
            | PlannedSourceValue::Variadic { source_index, .. } => *source_index,
        }
    }

    /// The parameter index this value fills, or `None` for variadic entries.
    pub(crate) fn param_idx(&self) -> Option<usize> {
        match self {
            PlannedSourceValue::Regular { param_idx, .. } => Some(*param_idx),
            PlannedSourceValue::Variadic { .. } => None,
        }
    }

    /// The string key for variadic named entries, or `None` for positional variadic entries.
    pub(crate) fn key(&self) -> Option<&str> {
        match self {
            PlannedSourceValue::Regular { .. } => None,
            PlannedSourceValue::Variadic { key, .. } => key.as_deref(),
        }
    }

    /// The expression from the source argument for this planned value.
    pub(crate) fn expr(&self) -> &Expr {
        match self {
            PlannedSourceValue::Regular { expr, .. }
            | PlannedSourceValue::Variadic { expr, .. } => expr,
        }
    }
}

/// Emits an `ArrayAccess` expression to extract `spread_expr[element_idx]` using
/// a numeric index. When `prefer_named_key` is `true` and `param_name` is present,
/// uses the parameter name as the string key instead of the numeric index.
/// Used to materialize a single element from a spread array.
fn spread_element_expr(
    spread_expr: &Expr,
    element_idx: usize,
    param_name: Option<&str>,
    prefer_named_key: bool,
    span: Span,
) -> Expr {
    let index = if prefer_named_key {
        if let Some(param_name) = param_name {
            Expr::new(ExprKind::StringLiteral(param_name.to_string()), span)
        } else {
            Expr::new(ExprKind::IntLiteral(element_idx as i64), span)
        }
    } else {
        Expr::new(ExprKind::IntLiteral(element_idx as i64), span)
    };
    Expr::new(
        ExprKind::ArrayAccess {
            array: Box::new(spread_expr.clone()),
            index: Box::new(index),
        },
        span,
    )
}

/// Emits a ternary that selects `spread_expr[element_idx]` if the spread is
/// long enough, otherwise falls back to `default_expr`. The bounds check
/// uses `array_key_exists` when a named key is available, or `count() > element_idx`
/// for numeric keys. This handles optional spread elements that may or may not
/// be present at a given position.
fn spread_element_or_default_expr(
    spread_expr: &Expr,
    element_idx: usize,
    param_name: Option<&str>,
    prefer_named_key: bool,
    default_expr: Expr,
    span: Span,
) -> Expr {
    let condition = if prefer_named_key {
        if let Some(param_name) = param_name {
            Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("array_key_exists"),
                    args: vec![
                        Expr::new(ExprKind::StringLiteral(param_name.to_string()), span),
                        spread_expr.clone(),
                    ],
                },
                span,
            )
        } else {
            spread_len_gt_expr(spread_expr, element_idx, span)
        }
    } else {
        spread_len_gt_expr(spread_expr, element_idx, span)
    };
    Expr::new(
        ExprKind::Ternary {
            condition: Box::new(condition),
            then_expr: Box::new(spread_element_expr(
                spread_expr,
                element_idx,
                param_name,
                prefer_named_key,
                span,
            )),
            else_expr: Box::new(default_expr),
        },
        span,
    )
}

/// Emits `count($spread_expr) > element_idx` as a `BinaryOp` expression.
fn spread_len_gt_expr(spread_expr: &Expr, element_idx: usize, span: Span) -> Expr {
    Expr::new(
        ExprKind::BinaryOp {
            left: Box::new(spread_len_expr(spread_expr, span)),
            op: BinOp::Gt,
            right: Box::new(Expr::new(ExprKind::IntLiteral(element_idx as i64), span)),
        },
        span,
    )
}

/// Emits `count($spread_expr)` as a function call expression.
fn spread_len_expr(spread_expr: &Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("count"),
            args: vec![spread_expr.clone()],
        },
        span,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FunctionSig, PhpType};

    fn sig_with_defaults(defaults: Vec<Option<Expr>>) -> FunctionSig {
        let params = ["a", "b", "c"]
            .iter()
            .map(|name| ((*name).to_string(), PhpType::Int))
            .collect();
        FunctionSig {
            params,
            defaults,
            return_type: PhpType::Int,
            declared_return: true,
            ref_params: vec![false; 3],
            declared_params: vec![true; 3],
            variadic: None,
            deprecation: None,
        }
    }

    fn spread_then_named_c() -> Vec<Expr> {
        vec![
            Expr::new(ExprKind::Spread(Box::new(Expr::var("args"))), Span::dummy()),
            Expr::new(
                ExprKind::NamedArg {
                    name: "c".to_string(),
                    value: Box::new(Expr::int_lit(30)),
                },
                Span::dummy(),
            ),
        ]
    }

    #[test]
    fn normalized_args_skip_default_guard_when_spread_check_guarantees_slot() {
        let sig = sig_with_defaults(vec![Some(Expr::int_lit(1)), None, None]);
        let plan = super::super::planner::plan_call_args(
            &sig,
            &spread_then_named_c(),
            Span::dummy(),
            false,
            true,
        )
        .expect("planner should accept spread before named argument");

        assert_eq!(plan.spread_bounds_checks[0].min_len, 2);
        let normalized = plan.normalized_args();
        assert!(matches!(
            &normalized[0].kind,
            ExprKind::ArrayAccess { .. }
        ));
    }

    #[test]
    fn normalized_args_keep_default_guard_when_spread_may_skip_optional_slot() {
        let sig = sig_with_defaults(vec![None, Some(Expr::int_lit(2)), None]);
        let plan = super::super::planner::plan_call_args(
            &sig,
            &spread_then_named_c(),
            Span::dummy(),
            false,
            true,
        )
        .expect("planner should accept spread before named argument");

        assert_eq!(plan.spread_bounds_checks[0].min_len, 1);
        let normalized = plan.normalized_args();
        assert!(matches!(&normalized[1].kind, ExprKind::Ternary { .. }));
    }
}
