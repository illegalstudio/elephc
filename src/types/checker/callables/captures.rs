//! Purpose:
//! Type-checks callable captures behavior.
//! Infers callable signatures and validates invocation details that affect later lowering and optimizer effects.
//!
//! Called from:
//! - `crate::types::checker::callables`
//! - `crate::types::checker::inference`
//!
//! Key details:
//! - Closure captures, first-class callable syntax, and extern calls must agree with shared call argument planning.

use crate::errors::CompileError;
use crate::parser::ast::{CallableTarget, Expr, ExprKind, StaticReceiver};

use super::super::Checker;

impl Checker {
pub(crate) fn first_class_callable_target_needs_runtime_capture(
        target: &CallableTarget,
    ) -> bool {
        matches!(
            target,
            CallableTarget::Method { .. }
                | CallableTarget::StaticMethod {
                    receiver: StaticReceiver::Static,
                    ..
                }
        )
    }

    pub(crate) fn reject_captured_first_class_callable_callback(
        &self,
        callback: &Expr,
        span: crate::span::Span,
        builtin: &str,
    ) -> Result<(), CompileError> {
        let target = match &callback.kind {
            ExprKind::FirstClassCallable(target) => Some(target),
            ExprKind::Variable(var_name) => self.first_class_callable_targets.get(var_name),
            _ => None,
        };
        if target.is_some_and(Self::first_class_callable_target_needs_runtime_capture) {
            return Err(CompileError::new(
                span,
                &format!(
                    "{}() does not support captured first-class callable targets yet",
                    builtin
                ),
            ));
        }
        Ok(())
    }

    pub(crate) fn expr_call_callee_needs_runtime_capture(&self, callee: &Expr) -> bool {
        match &callee.kind {
            ExprKind::Closure { captures, .. } => !captures.is_empty(),
            ExprKind::FirstClassCallable(target) => {
                Self::first_class_callable_target_needs_runtime_capture(target)
            }
            ExprKind::Variable(var_name) => {
                self.callable_captures
                    .get(var_name)
                    .is_some_and(|captures| !captures.is_empty())
                    || self
                        .first_class_callable_targets
                        .get(var_name)
                        .is_some_and(Self::first_class_callable_target_needs_runtime_capture)
            }
            ExprKind::Assignment { value, .. } => {
                self.expr_call_callee_needs_runtime_capture(value)
            }
            ExprKind::Ternary {
                then_expr,
                else_expr,
                ..
            } => {
                self.expr_call_callee_needs_runtime_capture(then_expr)
                    || self.expr_call_callee_needs_runtime_capture(else_expr)
            }
            ExprKind::ShortTernary { value, default }
            | ExprKind::NullCoalesce { value, default } => {
                self.expr_call_callee_needs_runtime_capture(value)
                    || self.expr_call_callee_needs_runtime_capture(default)
            }
            _ => false,
        }
    }
}
