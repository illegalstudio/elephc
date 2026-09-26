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
            self.check_assignment_prelude_stmt(stmt, env)?;
        }

        if let ExprKind::Variable(name) = &target.kind {
            return self.check_local_assignment_expression(name, value, span, env);
        }

        if let ExprKind::DynamicPropertyAccess { object, property } = &target.kind {
            self.check_dynamic_property_assignment_expression(
                object,
                property,
                value,
                result_target,
                span,
                env,
            )?;
            let result_expr = match result_target {
                Some(result_target) if result_target != target => result_target,
                _ => value,
            };
            return self.infer_type(result_expr, env);
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

    /// Type-checks one parser-generated prelude statement of an assignment expression.
    ///
    /// Preludes hold the stabilizing temporaries of a complex target and, for an append
    /// expression (`($a['k'][]['x'] = $v)`), the append writes themselves, which the parser
    /// groups in `Synthetic` statements (nested appends included). Those groups are walked here
    /// in order; every other statement is an assignment-like write.
    fn check_assignment_prelude_stmt(
        &mut self,
        stmt: &Stmt,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        match &stmt.kind {
            StmtKind::Synthetic(stmts) => {
                if self.check_empty_indexed_nested_append(stmts, env)? {
                    return Ok(());
                }
                for stmt in stmts {
                    self.check_assignment_prelude_stmt(stmt, env)?;
                }
                Ok(())
            }
            _ => self.check_assignment_like_stmt(stmt, env),
        }
    }

    /// Type-checks `$object->{$property} = $value` assignment expressions.
    ///
    /// Dynamic property writes use runtime dispatch, so the checker validates the
    /// receiver and property-name expression shapes and leaves value coercion to
    /// the existing property-store lowerers for the matched runtime target.
    fn check_dynamic_property_assignment_expression(
        &mut self,
        object: &Expr,
        property: &Expr,
        value: &Expr,
        result_target: Option<&Expr>,
        span: Span,
        env: &mut TypeEnv,
    ) -> Result<(), CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        if !matches!(
            obj_ty,
            PhpType::Object(_) | PhpType::Union(_) | PhpType::Mixed
        ) {
            return Err(CompileError::new(
                span,
                "Property assignment requires an object",
            ));
        }

        let property_ty = self.infer_type(property, env)?;
        if !matches!(property_ty, PhpType::Str | PhpType::Int | PhpType::Mixed) {
            return Err(CompileError::new(
                property.span,
                "Dynamic property name must be string or integer",
            ));
        }

        // A runtime name can miss every declared property, or resolve a strict ancestor's private
        // name to a distinct dynamic property. The backend dispatches both answers per runtime
        // class, so every admitted receiver shape records the reachable class subtree for hash
        // reservation. See `crate::types::checker::scope_dynamic_storage`.
        let receiver_ty = obj_ty.clone();
        crate::types::checker::scope_dynamic_storage::record_scope_dynamic_runtime_name_receiver_mutation(
            self,
            &receiver_ty,
        );

        self.infer_type(value, env)?;
        if let Some(result_target) = result_target {
            self.infer_type(result_target, env)?;
        }
        Ok(())
    }
}
