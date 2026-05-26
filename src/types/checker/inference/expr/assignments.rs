//! Purpose:
//! Infers expression assignments forms for the checker.
//! Handles type facts and diagnostics for expression shapes that need more than scalar/operator inference.
//!
//! Called from:
//! - `crate::types::checker::inference::expr`
//!
//! Key details:
//! - Expression inference shares environments with statement checking, so variable and effect updates must stay synchronized.

use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    /// Infers the type of an assignment expression and updates the type environment.
    ///
    /// Handles all assignment forms: simple variable (`$a = 1`), array access
    /// (`$a[0] = 1`), property access (`$obj->prop = 1`), and static property access.
    /// Compound assignments (e.g., `+=`) use `result_target` to distinguish the value
    /// expression from the target expression.
    ///
    /// # Arguments
    /// * `target` - Left-hand side of the assignment (Variable, ArrayAccess, PropertyAccess, StaticPropertyAccess)
    /// * `value` - Right-hand side expression providing the assigned value
    /// * `result_target` - For compound assignments, the expression whose type becomes the result; if None or same as target, `value`'s type is used
    /// * `prelude` - Statements to execute before the assignment (e.g., from null coalescing `??=` initializer)
    /// * `span` - Source span for error reporting
    /// * `env` - Mutable type environment; updated with the target's new type
    ///
    /// # Returns
    /// The `PhpType` of the result expression (type of `value` or `result_target`).
    ///
    /// # Errors
    /// Returns `CompileError` for invalid assignment targets (e.g., literals, expressions).
    pub(super) fn check_assignment_expression(
        &mut self,
        target: &Expr,
        value: &Expr,
        result_target: Option<&Expr>,
        prelude: &[Stmt],
        span: Span,
        env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        for stmt in prelude {
            self.check_assignment_like_stmt(stmt, env)?;
        }

        if let ExprKind::Variable(name) = &target.kind {
            return self.check_local_assignment_expression(name, value, span, env);
        }

        let stmt_kind = match &target.kind {
            ExprKind::ArrayAccess { array, index } => match &array.kind {
                ExprKind::Variable(array) => StmtKind::ArrayAssign {
                    array: array.clone(),
                    index: *index.clone(),
                    value: value.clone(),
                },
                ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                    object: object.clone(),
                    property: property.clone(),
                    index: *index.clone(),
                    value: value.clone(),
                },
                ExprKind::StaticPropertyAccess { receiver, property } => {
                    StmtKind::StaticPropertyArrayAssign {
                        receiver: receiver.clone(),
                        property: property.clone(),
                        index: *index.clone(),
                        value: value.clone(),
                    }
                }
                _ => StmtKind::NestedArrayAssign {
                    target: target.clone(),
                    value: value.clone(),
                },
            },
            ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
                object: object.clone(),
                property: property.clone(),
                value: value.clone(),
            },
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyAssign {
                    receiver: receiver.clone(),
                    property: property.clone(),
                    value: value.clone(),
                }
            }
            _ => return Err(CompileError::new(span, "Invalid assignment target")),
        };

        let stmt = Stmt::new(stmt_kind, span);
        self.check_assignment_like_stmt(&stmt, env)?;
        let result_expr = match result_target {
            Some(result_target) if result_target != target => result_target,
            _ => value,
        };
        self.infer_type(result_expr, env)
    }
}
