//! Purpose:
//! Classifies and lowers assignment-expression targets that may have observable side effects.
//! Builds prelude statements and temporary result targets for complex l-values.
//!
//! Called from:
//! - `crate::parser::expr::pratt` when parsing assignment expressions.
//!
//! Key details:
//! - Target dependencies are tracked so PHP evaluation order is preserved during lowering.

use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};
use crate::parser::stmt::can_replay_assignment_target;
use crate::span::Span;

pub(super) fn is_non_local_assignment_target(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayAccess { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
    )
}

pub(super) fn is_assignment_expression_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::PropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => matches!(
            &array.kind,
            ExprKind::Variable(_)
                | ExprKind::PropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
        ),
        _ => false,
    }
}

pub(super) struct AssignmentExpressionLowerer {
    span: Span,
    next_temp: usize,
    prelude: Vec<Stmt>,
}

impl AssignmentExpressionLowerer {
    pub(super) fn new(span: Span) -> Self {
        Self {
            span,
            next_temp: 0,
            prelude: Vec::new(),
        }
    }

    pub(super) fn stabilize_non_local_target(&mut self, target: Expr, rhs: &Expr) -> Expr {
        self.stabilize_assignment_target(target, rhs)
    }

    pub(super) fn bind_value(&mut self, value: Expr) -> Expr {
        self.bind_temp(value)
    }

    pub(super) fn reserve_value_temp(&mut self) -> String {
        self.next_temp_name()
    }

    pub(super) fn finish(self) -> Vec<Stmt> {
        self.prelude
    }

    fn stabilize_assignment_target(&mut self, expr: Expr, rhs: &Expr) -> Expr {
        let span = expr.span;
        match expr.kind {
            ExprKind::ArrayAccess { array, index } => {
                let array = match *array {
                    Expr {
                        kind: ExprKind::PropertyAccess { object, property },
                        span: array_span,
                    } => Expr::new(
                        ExprKind::PropertyAccess {
                            object: Box::new(self.stabilize_receiver(*object, rhs)),
                            property,
                        },
                        array_span,
                    ),
                    other => other,
                };
                Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(array),
                        index: Box::new(self.stabilize_dimension_index(*index, rhs)),
                    },
                    span,
                )
            }
            ExprKind::PropertyAccess { object, property } => Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(self.stabilize_receiver(*object, rhs)),
                    property,
                },
                span,
            ),
            ExprKind::StaticPropertyAccess { receiver, property } => Expr::new(
                ExprKind::StaticPropertyAccess { receiver, property },
                span,
            ),
            other => Expr::new(other, span),
        }
    }

    fn stabilize_receiver(&mut self, expr: Expr, rhs: &Expr) -> Expr {
        let span = expr.span;
        match expr.kind {
            ExprKind::PropertyAccess { object, property } => Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(self.stabilize_receiver(*object, rhs)),
                    property,
                },
                span,
            ),
            ExprKind::ArrayAccess { array, index } => Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(self.stabilize_receiver(*array, rhs)),
                    index: Box::new(self.stabilize_dimension_index(*index, rhs)),
                },
                span,
            ),
            kind @ (ExprKind::Variable(_)
                | ExprKind::This
                | ExprKind::StaticPropertyAccess { .. }) => Expr::new(kind, span),
            other => {
                let expr = Expr::new(other, span);
                if self.must_bind_target_part(&expr, rhs) {
                    self.bind_temp(expr)
                } else {
                    expr
                }
            }
        }
    }

    fn stabilize_dimension_index(&mut self, expr: Expr, rhs: &Expr) -> Expr {
        if self.must_bind_dimension_index(&expr, rhs) {
            self.bind_temp(expr)
        } else {
            expr
        }
    }

    fn must_bind_target_part(&self, expr: &Expr, rhs: &Expr) -> bool {
        !can_replay_assignment_target(expr)
            || target_part_reads_mutated_dependency(expr, rhs)
    }

    fn must_bind_dimension_index(&self, expr: &Expr, rhs: &Expr) -> bool {
        if matches!(
            expr.kind,
            ExprKind::Variable(_)
                | ExprKind::IntLiteral(_)
                | ExprKind::StringLiteral(_)
                | ExprKind::BoolLiteral(_)
                | ExprKind::Null
        ) {
            return false;
        }

        !can_replay_assignment_target(expr)
            || target_part_reads_mutated_dependency(expr, rhs)
    }

    fn bind_temp(&mut self, value: Expr) -> Expr {
        let name = self.next_temp_name();
        self.prelude.push(Stmt::new(
            StmtKind::Assign {
                name: name.clone(),
                value,
            },
            self.span,
        ));
        Expr::new(ExprKind::Variable(name), self.span)
    }

    fn next_temp_name(&mut self) -> String {
        let name = format!(
            "__elephc_assign_expr_{}_{}_{}",
            self.span.line, self.span.col, self.next_temp
        );
        self.next_temp += 1;
        name
    }
}

pub(super) fn assignment_value_may_mutate_target_dependency(
    target: &Expr,
    value: &Expr,
) -> bool {
    let mut dependencies = HashSet::new();
    collect_assignment_target_dependencies(target, &mut dependencies);
    !dependencies.is_empty() && expr_may_write_dependency(value, &dependencies)
}

fn target_part_reads_mutated_dependency(target: &Expr, value: &Expr) -> bool {
    assignment_value_may_mutate_target_dependency(target, value)
}

fn collect_assignment_target_dependencies(expr: &Expr, dependencies: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Variable(name) => {
            dependencies.insert(name.clone());
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_assignment_target_dependencies(array, dependencies);
            collect_assignment_target_dependencies(index, dependencies);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. }
        | ExprKind::MethodCall { object, .. }
        | ExprKind::NullsafeMethodCall { object, .. } => {
            collect_assignment_target_dependencies(object, dependencies);
        }
        ExprKind::BinaryOp { left, right, .. } => {
            collect_assignment_target_dependencies(left, dependencies);
            collect_assignment_target_dependencies(right, dependencies);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_assignment_target_dependencies(value, dependencies);
            collect_instanceof_target_dependencies(target, dependencies);
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value) => collect_assignment_target_dependencies(value, dependencies),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            collect_assignment_target_dependencies(value, dependencies);
            collect_assignment_target_dependencies(default, dependencies);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_assignment_target_dependencies(condition, dependencies);
            collect_assignment_target_dependencies(then_expr, dependencies);
            collect_assignment_target_dependencies(else_expr, dependencies);
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. } => {
            for arg in args {
                collect_assignment_target_dependencies(arg, dependencies);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_assignment_target_dependencies(item, dependencies);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_assignment_target_dependencies(key, dependencies);
                collect_assignment_target_dependencies(value, dependencies);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_assignment_target_dependencies(subject, dependencies);
            for (patterns, arm_value) in arms {
                for pattern in patterns {
                    collect_assignment_target_dependencies(pattern, dependencies);
                }
                collect_assignment_target_dependencies(arm_value, dependencies);
            }
            if let Some(default) = default {
                collect_assignment_target_dependencies(default, dependencies);
            }
        }
        ExprKind::Closure { .. }
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::Assignment { .. }
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::ConstRef(_)
        | ExprKind::NewObject { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::BufferNew { .. }
        | ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::NewScopedObject { .. }
        | ExprKind::Yield { .. }
        | ExprKind::YieldFrom(_)
        | ExprKind::MagicConstant(_) => {}
    }
}

fn expr_may_write_dependency(expr: &Expr, dependencies: &HashSet<String>) -> bool {
    match &expr.kind {
        ExprKind::Assignment {
            target,
            value,
            prelude,
            result_target,
            ..
        } => {
            assignment_target_may_write_dependency(target, dependencies)
                || expr_may_write_dependency(value, dependencies)
                || prelude
                    .iter()
                    .any(|stmt| stmt_may_write_dependency(stmt, dependencies))
                || result_target
                    .as_deref()
                    .is_some_and(|target| expr_may_write_dependency(target, dependencies))
        }
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => dependencies.contains(name),
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::ExprCall { args, .. } => args.iter().any(|arg| {
            expr_contains_dependency(arg, dependencies)
                || expr_may_write_dependency(arg, dependencies)
        }),
        ExprKind::BinaryOp { left, right, .. } => {
            expr_may_write_dependency(left, dependencies)
                || expr_may_write_dependency(right, dependencies)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_may_write_dependency(value, dependencies)
                || instanceof_target_may_write_dependency(target, dependencies)
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Throw(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value) => expr_may_write_dependency(value, dependencies),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_may_write_dependency(value, dependencies)
                || expr_may_write_dependency(default, dependencies)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_may_write_dependency(condition, dependencies)
                || expr_may_write_dependency(then_expr, dependencies)
                || expr_may_write_dependency(else_expr, dependencies)
        }
        ExprKind::ArrayLiteral(items) => {
            items.iter().any(|item| expr_may_write_dependency(item, dependencies))
        }
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_may_write_dependency(key, dependencies)
                || expr_may_write_dependency(value, dependencies)
        }),
        ExprKind::ArrayAccess { array, index } => {
            expr_may_write_dependency(array, dependencies)
                || expr_may_write_dependency(index, dependencies)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_may_write_dependency(subject, dependencies)
                || arms.iter().any(|(patterns, arm_value)| {
                    patterns
                        .iter()
                        .any(|pattern| expr_may_write_dependency(pattern, dependencies))
                        || expr_may_write_dependency(arm_value, dependencies)
                })
                || default
                    .as_deref()
                    .is_some_and(|default| expr_may_write_dependency(default, dependencies))
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_may_write_dependency(object, dependencies)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_may_write_dependency(object, dependencies)
                || args.iter().any(|arg| {
                    expr_contains_dependency(arg, dependencies)
                        || expr_may_write_dependency(arg, dependencies)
                })
        }
        ExprKind::NewObject { args, .. } | ExprKind::NewScopedObject { args, .. } => {
            args.iter().any(|arg| {
                expr_contains_dependency(arg, dependencies)
                    || expr_may_write_dependency(arg, dependencies)
            })
        }
        ExprKind::BufferNew { len, .. } => expr_may_write_dependency(len, dependencies),
        ExprKind::Closure { .. }
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::Yield { .. }
        | ExprKind::YieldFrom(_)
        | ExprKind::MagicConstant(_) => false,
    }
}

fn stmt_may_write_dependency(stmt: &Stmt, dependencies: &HashSet<String>) -> bool {
    match &stmt.kind {
        StmtKind::Assign { name, value } => {
            dependencies.contains(name) || expr_may_write_dependency(value, dependencies)
        }
        StmtKind::Synthetic(stmts) => stmts
            .iter()
            .any(|stmt| stmt_may_write_dependency(stmt, dependencies)),
        _ => false,
    }
}

fn assignment_target_may_write_dependency(target: &Expr, dependencies: &HashSet<String>) -> bool {
    match &target.kind {
        ExprKind::Variable(name) => dependencies.contains(name),
        ExprKind::ArrayAccess { array, .. } => expr_contains_dependency(array, dependencies),
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_contains_dependency(object, dependencies)
        }
        ExprKind::StaticPropertyAccess { .. } => false,
        _ => expr_contains_dependency(target, dependencies),
    }
}

fn collect_instanceof_target_dependencies(
    target: &InstanceOfTarget,
    dependencies: &mut HashSet<String>,
) {
    if let InstanceOfTarget::Expr(expr) = target {
        collect_assignment_target_dependencies(expr, dependencies);
    }
}

fn instanceof_target_may_write_dependency(
    target: &InstanceOfTarget,
    dependencies: &HashSet<String>,
) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_may_write_dependency(expr, dependencies),
    }
}

fn expr_contains_dependency(expr: &Expr, dependencies: &HashSet<String>) -> bool {
    let mut found = HashSet::new();
    collect_assignment_target_dependencies(expr, &mut found);
    found.iter().any(|name| dependencies.contains(name))
}
