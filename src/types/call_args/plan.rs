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

pub(crate) struct CallArgPlan {
    pub(crate) source_args: Vec<Expr>,
    pub(crate) regular_args: Vec<PlannedRegularArg>,
    pub(crate) variadic_args: Vec<PlannedVariadicArg>,
    pub(crate) source_values: Vec<PlannedSourceValue>,
    pub(crate) spread_bounds_checks: Vec<SpreadBoundsCheck>,
    pub(crate) first_named_pos: Option<usize>,
    pub(super) passthrough_args: Option<Vec<Expr>>,
}

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
        default: Option<Expr>,
        guaranteed_present: bool,
    },
    Default(Expr),
}

#[derive(Clone)]
pub(crate) struct PlannedVariadicArg {
    pub(crate) key: Option<String>,
    pub(crate) expr: Expr,
}

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

#[derive(Clone)]
pub(crate) struct SpreadBoundsCheck {
    pub(crate) spread_expr: Expr,
    pub(crate) min_len: usize,
    pub(crate) max_len: Option<usize>,
}

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
    pub(crate) fn has_named_args(&self) -> bool {
        self.first_named_pos.is_some()
    }

    pub(crate) fn has_spread_args(&self) -> bool {
        self.source_args
            .iter()
            .any(|arg| matches!(arg.kind, ExprKind::Spread(_)))
    }

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
                    default,
                    guaranteed_present,
                    ..
                } => {
                    if let Some(default) = default {
                        if *guaranteed_present {
                            spread_element_expr(spread_expr, *element_idx, *spread_span)
                        } else {
                            spread_element_or_default_expr(
                                spread_expr,
                                *element_idx,
                                default.clone(),
                                *spread_span,
                            )
                        }
                    } else {
                        spread_element_expr(spread_expr, *element_idx, *spread_span)
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
    pub(crate) fn source_index(&self) -> usize {
        match self {
            PlannedSourceValue::Regular { source_index, .. }
            | PlannedSourceValue::Variadic { source_index, .. } => *source_index,
        }
    }

    pub(crate) fn param_idx(&self) -> Option<usize> {
        match self {
            PlannedSourceValue::Regular { param_idx, .. } => Some(*param_idx),
            PlannedSourceValue::Variadic { .. } => None,
        }
    }

    pub(crate) fn key(&self) -> Option<&str> {
        match self {
            PlannedSourceValue::Regular { .. } => None,
            PlannedSourceValue::Variadic { key, .. } => key.as_deref(),
        }
    }

    pub(crate) fn expr(&self) -> &Expr {
        match self {
            PlannedSourceValue::Regular { expr, .. }
            | PlannedSourceValue::Variadic { expr, .. } => expr,
        }
    }
}

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
