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
        let string_offset_write = string_offset_write_target(target, env);
        if let Some((array, index)) = string_offset_write {
            refuse_string_offset_read_modify_write(array, index, prelude, span)?;
        }
        for stmt in prelude {
            self.check_assignment_like_stmt(stmt, env)?;
        }
        if let Some((array, index)) = string_offset_write {
            // PHP's `($s[$i] = $v)` evaluates to the one-byte string it stored, never to `$v`
            // itself, so the expression is a `string` whatever the value's type.
            let stmt = Stmt::new(
                StmtKind::ArrayAssign {
                    array: array.to_string(),
                    index: index.clone(),
                    value: value.clone(),
                },
                span,
            );
            self.check_assignment_like_stmt(&stmt, env)?;
            return Ok(PhpType::Str);
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

/// Returns the local name and index when `target` is `$name[$index]` on a local typed `string`.
fn string_offset_write_target<'e>(target: &'e Expr, env: &TypeEnv) -> Option<(&'e str, &'e Expr)> {
    let ExprKind::ArrayAccess { array, index } = &target.kind else {
        return None;
    };
    let ExprKind::Variable(name) = &array.kind else {
        return None;
    };
    matches!(env.get(name), Some(PhpType::Str)).then_some((name.as_str(), index.as_ref()))
}

/// Refuses the read-modify-write forms PHP rejects on a string offset.
///
/// The parser desugars them into a prelude that reads the target first: `$s[0]++` / `--$s[0]`
/// capture the old byte with `$old = $s[0]`, and the expression form of `$s[0] .= "x"` binds
/// `$tmp = $s[0] . "x"`. PHP throws `Error` for both before touching the string, so they can
/// never succeed and are refused here with PHP's wording.
fn refuse_string_offset_read_modify_write(
    array: &str,
    index: &Expr,
    prelude: &[Stmt],
    span: Span,
) -> Result<(), CompileError> {
    let is_target = |expr: &Expr| {
        crate::types::checker::stmt_check::expr_is_string_offset_target(expr, array, index)
    };
    for stmt in prelude {
        let StmtKind::Assign { value, .. } = &stmt.kind else {
            continue;
        };
        if is_target(value) {
            return Err(CompileError::new(span, "Cannot increment/decrement string offsets"));
        }
        if matches!(&value.kind, ExprKind::BinaryOp { left, .. } if is_target(left)) {
            return Err(CompileError::new(
                span,
                "Cannot use assign-op operators with string offsets",
            ));
        }
    }
    Ok(())
}
