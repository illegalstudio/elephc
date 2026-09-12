//! Purpose:
//! Carries by-reference PHP array storage effects from signature validation to caller inference.
//!
//! Called from:
//! - Function/callable argument validation and expression assignment-effect inference.
//!
//! Key details:
//! - Arguments are already normalized by the shared call planner.
//! - Only the referenced local changes representation, not the root of an element reference.

use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::Checker;

impl Checker {
    /// Records a concrete array local whose reference binding permits packed or keyed writes.
    pub(crate) fn record_php_array_reference_output(
        &mut self,
        arg: &Expr,
        expected: &PhpType,
        actual: &PhpType,
        call_span: Span,
    ) {
        if !expected.is_php_array()
            || !matches!(actual, PhpType::Array(_) | PhpType::AssocArray { .. })
            || !call_span.identifies_a_node()
        {
            return;
        }
        let mut arg = arg;
        while let ExprKind::NamedArg { value, .. } | ExprKind::ErrorSuppress(value) = &arg.kind {
            arg = value;
        }
        if let ExprKind::Variable(name) = &arg.kind {
            self.php_array_reference_outputs
                .entry((self.current_loop_storage_scope.clone(), call_span))
                .or_default()
                .insert(name.clone());
        }
    }

    /// Applies one successful call's storage changes before checking subsequent expressions.
    pub(crate) fn apply_php_array_reference_outputs(&mut self, span: Span, env: &mut TypeEnv) {
        let key = (self.current_loop_storage_scope.clone(), span);
        if let Some(names) = self.php_array_reference_outputs.remove(&key) {
            for name in names {
                env.insert(name, PhpType::php_array());
            }
        }
    }
}
