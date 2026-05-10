//! Purpose:
//! Infers expression effects forms for the checker.
//! Handles type facts and diagnostics for expression shapes that need more than scalar/operator inference.
//!
//! Called from:
//! - `crate::types::checker::inference::expr`
//!
//! Key details:
//! - Expression inference shares environments with statement checking, so variable and effect updates must stay synchronized.

use crate::errors::CompileError;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;
use super::super::syntactic::wider_type_syntactic;

impl Checker {
    pub(crate) fn infer_type_with_assignment_effects(
        &mut self,
        expr: &Expr,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::Assignment {
                target,
                value,
                result_target,
                prelude,
                ..
            } => {
                self.check_assignment_expression(
                    target,
                    value,
                    result_target.as_deref(),
                    prelude,
                    expr.span,
                    env,
                )
            }
            ExprKind::BinaryOp { left, op, right } => {
                self.infer_type_with_assignment_effects(left, env)?;
                if matches!(op, BinOp::And | BinOp::Or) {
                    let mut right_env = env.clone();
                    self.infer_type_with_assignment_effects(right, &mut right_env)?;
                    Ok(PhpType::Bool)
                } else {
                    self.infer_type_with_assignment_effects(right, env)?;
                    self.infer_type(expr, env)
                }
            }
            ExprKind::NullCoalesce { value, default } => {
                let value_ty = self.infer_type_with_assignment_effects(value, env)?;
                let default_ty = if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    self.infer_type_with_assignment_effects(default, &mut default_env)?
                };
                if Self::union_contains_void(&value_ty) {
                    Ok(wider_type_syntactic(
                        &self.strip_void_from_union(&value_ty),
                        &default_ty,
                    ))
                } else {
                    Ok(wider_type_syntactic(&value_ty, &default_ty))
                }
            }
            ExprKind::ShortTernary { value, default } => {
                let value_ty = self.infer_type_with_assignment_effects(value, env)?;
                let default_ty = if value_ty == PhpType::Void {
                    self.infer_type_with_assignment_effects(default, env)?
                } else {
                    let mut default_env = env.clone();
                    self.infer_type_with_assignment_effects(default, &mut default_env)?
                };
                Ok(wider_type_syntactic(&value_ty, &default_ty))
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.infer_type_with_assignment_effects(condition, env)?;
                let mut then_env = env.clone();
                let then_ty = self.infer_type_with_assignment_effects(then_expr, &mut then_env)?;
                let mut else_env = env.clone();
                let else_ty = self.infer_type_with_assignment_effects(else_expr, &mut else_env)?;
                Ok(wider_type_syntactic(&then_ty, &else_ty))
            }
            ExprKind::ArrayLiteral(elems) => {
                for elem in elems {
                    self.infer_type_with_assignment_effects(elem, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::ArrayLiteralAssoc(pairs) => {
                for (key, value) in pairs {
                    self.infer_type_with_assignment_effects(key, env)?;
                    self.infer_type_with_assignment_effects(value, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                self.infer_type_with_assignment_effects(subject, env)?;
                let mut result_ty = None;
                for (conditions, result) in arms {
                    let mut arm_env = env.clone();
                    for condition in conditions {
                        self.infer_type_with_assignment_effects(condition, &mut arm_env)?;
                    }
                    let arm_ty = self.infer_type_with_assignment_effects(result, &mut arm_env)?;
                    result_ty = Some(match result_ty {
                        Some(current) => wider_type_syntactic(&current, &arm_ty),
                        None => arm_ty,
                    });
                }
                if let Some(default) = default {
                    let mut default_env = env.clone();
                    let default_ty =
                        self.infer_type_with_assignment_effects(default, &mut default_env)?;
                    result_ty = Some(match result_ty {
                        Some(current) => wider_type_syntactic(&current, &default_ty),
                        None => default_ty,
                    });
                }
                Ok(result_ty.unwrap_or(PhpType::Void))
            }
            ExprKind::ArrayAccess { array, index } => {
                self.infer_type_with_assignment_effects(array, env)?;
                self.infer_type_with_assignment_effects(index, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::Negate(inner)
            | ExprKind::Not(inner)
            | ExprKind::BitNot(inner)
            | ExprKind::Throw(inner)
            | ExprKind::ErrorSuppress(inner)
            | ExprKind::Print(inner)
            | ExprKind::Spread(inner) => {
                self.infer_type_with_assignment_effects(inner, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::Cast { expr: inner, .. } | ExprKind::PtrCast { expr: inner, .. } => {
                self.infer_type_with_assignment_effects(inner, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::FunctionCall { args, .. }
            | ExprKind::NewObject { args, .. }
            | ExprKind::StaticMethodCall { args, .. } => {
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::ClosureCall { args, .. } => {
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::ExprCall { callee, args } => {
                self.infer_type_with_assignment_effects(callee, env)?;
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::NamedArg { value, .. } => {
                self.infer_type_with_assignment_effects(value, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::PropertyAccess { object, .. }
            | ExprKind::NullsafePropertyAccess { object, .. } => {
                self.infer_type_with_assignment_effects(object, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::MethodCall { object, args, .. }
            | ExprKind::NullsafeMethodCall { object, args, .. } => {
                self.infer_type_with_assignment_effects(object, env)?;
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            ExprKind::BufferNew { len, .. } => {
                self.infer_type_with_assignment_effects(len, env)?;
                self.infer_type(expr, env)
            }
            ExprKind::NewScopedObject { args, .. } => {
                let expanded_args = crate::types::call_args::expand_static_assoc_spread_args(args);
                for arg in &expanded_args {
                    self.infer_type_with_assignment_effects(arg, env)?;
                }
                self.infer_type(expr, env)
            }
            _ => self.infer_type(expr, env),
        }
    }
}
