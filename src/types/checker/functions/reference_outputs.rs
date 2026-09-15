//! Purpose:
//! Carries boxed by-reference storage effects from signature validation to caller inference.
//!
//! Called from:
//! - Function/callable argument validation and expression assignment-effect inference.
//!
//! Key details:
//! - Arguments are already normalized by the shared call planner.
//! - A referenced array element widens its local root to boxed Mixed element storage.

use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Records a local whose reference binding requires canonical boxed storage after the call.
    pub(crate) fn record_boxed_reference_output(
        &mut self,
        arg: &Expr,
        expected: &PhpType,
        actual: &PhpType,
        call_span: Span,
        env: &TypeEnv,
    ) {
        let output_ty = if expected.is_php_array()
            && matches!(actual, PhpType::Array(_) | PhpType::AssocArray { .. })
        {
            PhpType::php_array()
        } else if expected.codegen_repr() == PhpType::Mixed
            && actual.codegen_repr() != PhpType::Mixed
        {
            PhpType::Mixed
        } else {
            return;
        };
        if !call_span.identifies_a_node() {
            return;
        }
        let mut arg = arg;
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
            arg = value;
        }
        let (name, output_ty) = match &arg.kind {
            ExprKind::Variable(name) => {
                if output_ty == PhpType::Mixed && arg.span.identifies_a_node() {
                    self.boxed_reference_promotion_sites
                        .entry((self.current_loop_storage_scope.clone(), arg.span))
                        .or_default()
                        .insert(name.clone());
                }
                (name, output_ty)
            }
            ExprKind::ArrayAccess { array, .. } if output_ty == PhpType::Mixed => {
                let mut root = array.as_ref();
                while let ExprKind::ArrayAccess { array, .. } = &root.kind {
                    root = array;
                }
                let ExprKind::Variable(name) = &root.kind else {
                    return;
                };
                let Some(root_ty) = env.get(name) else {
                    return;
                };
                let widened = match root_ty.codegen_repr() {
                    PhpType::Array(_) => PhpType::Array(Box::new(PhpType::Mixed)),
                    PhpType::AssocArray { key, .. } => PhpType::AssocArray {
                        key,
                        value: Box::new(PhpType::Mixed),
                    },
                    _ => return,
                };
                (name, widened)
            }
            _ => return,
        };
        self.boxed_reference_outputs
            .entry((self.current_loop_storage_scope.clone(), call_span))
            .or_default()
            .insert(name.clone(), output_ty);
    }

    /// Returns whether this call has already approved boxing the same local for an earlier slot.
    pub(crate) fn boxed_reference_promotion_pending(
        &self,
        arg: &Expr,
        call_span: Span,
    ) -> bool {
        let mut arg = arg;
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
            arg = value;
        }
        let ExprKind::Variable(name) = &arg.kind else {
            return false;
        };
        self.boxed_reference_outputs
            .get(&(self.current_loop_storage_scope.clone(), call_span))
            .and_then(|outputs| outputs.get(name))
            .is_some_and(|ty| ty.codegen_repr() == PhpType::Mixed)
    }

    /// Applies one successful call's storage changes before checking subsequent expressions.
    pub(crate) fn apply_boxed_reference_outputs(&mut self, span: Span, env: &mut TypeEnv) {
        let key = (self.current_loop_storage_scope.clone(), span);
        if let Some(outputs) = self.boxed_reference_outputs.remove(&key) {
            for (name, output_ty) in outputs {
                env.insert(name, output_ty);
            }
        }
    }
}
