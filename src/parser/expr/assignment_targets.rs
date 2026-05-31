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

/// Returns true if the expression is a non-local assignment target (array-access,
/// property-access, or static-property-access). These require stabilization because
/// the base expression may be evaluated multiple times during assignment.
pub(super) fn is_non_local_assignment_target(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayAccess { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
    )
}

/// Returns true if the expression can serve as the target of an assignment expression.
/// Valid targets are variables, property accesses, static property accesses, and
/// nested array accesses whose base is a variable/property.
pub(super) fn is_assignment_expression_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::PropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => is_array_assignment_base(array),
        _ => false,
    }
}

/// Returns true if the expression can be used as the base of an array assignment target.
/// Recursively walks into array accesses to find the underlying base.
fn is_array_assignment_base(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::PropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, .. } => is_array_assignment_base(array),
        _ => false,
    }
}

/// Stateful lowerer that classifies assignment-expression targets and generates prelude
/// statements to preserve PHP evaluation order.
///
/// Tracks variable dependencies so that when the RHS may mutate the target's container
/// (e.g., `$arr[$i] = $arr` where RHS reads `$arr`), temporaries are inserted to prevent
/// multiple evaluation of the same expression.
pub(super) struct AssignmentExpressionLowerer {
    span: Span,
    next_temp: usize,
    prelude: Vec<Stmt>,
}

impl AssignmentExpressionLowerer {
    /// Creates a new lowerer with the given source span for generated temporary names.
    pub(super) fn new(span: Span) -> Self {
        Self {
            span,
            next_temp: 0,
            prelude: Vec::new(),
        }
    }

    /// Stabilizes a non-local target expression (array-access, property-access,
    /// static-property-access) by binding any sub-expressions that the RHS may
    /// mutate. Returns a replacement expression with temporaries substituted.
    pub(super) fn stabilize_non_local_target(&mut self, target: Expr, rhs: &Expr) -> Expr {
        self.stabilize_assignment_target(target, rhs)
    }

    /// Binds a value expression for use in an assignment context. If the value
    /// can be replayed safely (no dependency conflict), returns it unchanged;
    /// otherwise emits a temporary assignment and returns a variable reference.
    pub(super) fn bind_value(&mut self, target: &Expr, value: Expr) -> Expr {
        if can_replay_assignment_target(&value)
            && !assignment_value_reads_target_container(target, &value)
        {
            return value;
        }
        self.bind_temp(value)
    }

    /// Binds a result value expression, always using a temporary to ensure
    /// the result can be referenced multiple times in the result expression.
    pub(super) fn bind_result_value(&mut self, value: Expr) -> Expr {
        self.bind_temp(value)
    }

    /// Reserves a temporary variable name for a value that will be bound later.
    /// Returns the variable name string without emitting an assignment yet.
    pub(super) fn reserve_value_temp(&mut self) -> String {
        self.next_temp_name()
    }

    /// Finishes lowering and returns all prelude statements that were accumulated
    /// during the lowering process.
    pub(super) fn finish(self) -> Vec<Stmt> {
        self.prelude
    }

    /// Transforms an assignment target by stabilizing any sub-expressions that
    /// the RHS may mutate. For array accesses, recursively stabilizes the array
    /// base and dimension index. For property accesses, stabilizes the receiver object.
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
                    other @ Expr {
                        kind: ExprKind::ArrayAccess { .. },
                        ..
                    } => self.stabilize_receiver(other, rhs),
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

    /// Stabilizes a receiver expression (the object base of a property or array access)
    /// by binding any part that the RHS may mutate, preserving PHP evaluation order.
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

    /// Stabilizes a dimension index expression for an array access. If the index
    /// may read a variable that the RHS mutates, binds it to a temporary.
    fn stabilize_dimension_index(&mut self, expr: Expr, rhs: &Expr) -> Expr {
        if self.must_bind_dimension_index(&expr, rhs) {
            self.bind_temp(expr)
        } else {
            expr
        }
    }

    /// Returns true if a target sub-expression must be bound to a temporary
    /// before the assignment proceeds. This is the case when the expression
    /// cannot be replayed or when it reads a variable that the RHS may mutate.
    fn must_bind_target_part(&self, expr: &Expr, rhs: &Expr) -> bool {
        !can_replay_assignment_target(expr)
            || target_part_reads_mutated_dependency(expr, rhs)
    }

    /// Returns true if a dimension index must be bound to a temporary.
    /// Literal indices (variables, ints, strings, bools, null) are always safe
    /// to reuse without binding; complex expressions require the check.
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

    /// Emits an assignment statement to a temporary variable and returns a variable
    /// reference to that temporary. Used when an expression's value may be needed
    /// multiple times and must not be re-evaluated.
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

    /// Generates a unique temporary variable name using the source span and a
    /// monotonically increasing counter. Names are formatted as
    /// `__elephc_assign_expr_{line}_{col}_{counter}`.
    fn next_temp_name(&mut self) -> String {
        let name = format!(
            "__elephc_assign_expr_{}_{}_{}",
            self.span.line, self.span.col, self.next_temp
        );
        self.next_temp += 1;
        name
    }
}

/// Returns true if the assignment value may mutate a variable that the target
/// depends on. Used to detect when stabilization is required to preserve
/// evaluation order (e.g., `$arr[$i] = $arr[$j]` where RHS reads `$arr`).
pub(super) fn assignment_value_may_mutate_target_dependency(
    target: &Expr,
    value: &Expr,
) -> bool {
    let mut dependencies = HashSet::new();
    collect_assignment_target_dependencies(target, &mut dependencies);
    !dependencies.is_empty() && expr_may_write_dependency(value, &dependencies)
}

/// Returns true if the assignment value expression reads the same array or object
/// that the target writes to. This occurs in cases like `$arr[$i] = $arr` where the
/// RHS reads the container being modified by the assignment.
pub(super) fn assignment_value_reads_target_container(target: &Expr, value: &Expr) -> bool {
    match &target.kind {
        ExprKind::ArrayAccess { array, .. } => expr_contains_equivalent(value, array),
        _ => false,
    }
}

/// Returns true if the target part may read a dependency that the assignment value
/// could mutate. Delegates to `assignment_value_may_mutate_target_dependency`.
fn target_part_reads_mutated_dependency(target: &Expr, value: &Expr) -> bool {
    assignment_value_may_mutate_target_dependency(target, value)
}

/// Collects all variable dependencies of an assignment target into the provided
/// set. Walks into array accesses, property accesses, method calls, binary operations,
/// and other expressions that may reference variables.
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
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_assignment_target_dependencies(object, dependencies);
            collect_assignment_target_dependencies(property, dependencies);
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
        ExprKind::Pipe { value, callable } => {
            collect_assignment_target_dependencies(value, dependencies);
            collect_assignment_target_dependencies(callable, dependencies);
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
        | ExprKind::NewDynamicObject { .. }
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

/// Returns true if the expression may write to any variable in the dependency set.
/// Checks assignment statements, increment/decrement operators, function calls
/// with arguments that could mutate dependencies, and recursively checks nested expressions.
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
        ExprKind::Pipe { value, callable } => {
            expr_may_write_dependency(value, dependencies)
                || expr_may_write_dependency(callable, dependencies)
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
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_may_write_dependency(object, dependencies)
                || expr_may_write_dependency(property, dependencies)
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
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            expr_may_write_dependency(class_name, dependencies)
                || args.iter().any(|arg| {
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

/// Returns true if the statement may write to any variable in the dependency set.
/// Only Assign and Synthetic (multi-statement) kinds are checked; other statement
/// types are assumed not to write to tracked dependencies.
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

/// Returns true if the assignment target may write to any variable in the
/// dependency set. Variables and array accesses check the container; property
/// accesses check the object receiver.
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

/// Collects variable dependencies from an instanceof target expression.
/// If the target is a name (class constant), no dependencies are collected.
fn collect_instanceof_target_dependencies(
    target: &InstanceOfTarget,
    dependencies: &mut HashSet<String>,
) {
    if let InstanceOfTarget::Expr(expr) = target {
        collect_assignment_target_dependencies(expr, dependencies);
    }
}

/// Returns true if an instanceof target expression may write to any variable
/// in the dependency set. Name targets (class constants) cannot write; expression
/// targets delegate to `expr_may_write_dependency`.
fn instanceof_target_may_write_dependency(
    target: &InstanceOfTarget,
    dependencies: &HashSet<String>,
) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_may_write_dependency(expr, dependencies),
    }
}

/// Returns true if the expression contains any variable that appears in the
/// dependency set. Collects all dependencies from the expression and checks
/// for overlap with the provided set.
fn expr_contains_dependency(expr: &Expr, dependencies: &HashSet<String>) -> bool {
    let mut found = HashSet::new();
    collect_assignment_target_dependencies(expr, &mut found);
    found.iter().any(|name| dependencies.contains(name))
}

/// Returns true if the expression structurally contains an equivalent node to
/// the needle expression. Uses deep equality comparison on the expression tree,
/// recursing into binary operations, ternaries, assignments, calls, and other
/// composite expressions. Used to detect cases like `$arr[$i] = $arr` where
/// the RHS contains the same array variable as the target's container.
fn expr_contains_equivalent(expr: &Expr, needle: &Expr) -> bool {
    if expr == needle {
        return true;
    }

    match &expr.kind {
        ExprKind::BinaryOp { left, right, .. } => {
            expr_contains_equivalent(left, needle) || expr_contains_equivalent(right, needle)
        }
        ExprKind::InstanceOf { value, target } => {
            expr_contains_equivalent(value, needle)
                || instanceof_target_contains_equivalent(target, needle)
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
        | ExprKind::Spread(value)
        | ExprKind::YieldFrom(value) => expr_contains_equivalent(value, needle),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            expr_contains_equivalent(value, needle)
                || expr_contains_equivalent(default, needle)
        }
        ExprKind::Pipe { value, callable } => {
            expr_contains_equivalent(value, needle)
                || expr_contains_equivalent(callable, needle)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            expr_contains_equivalent(condition, needle)
                || expr_contains_equivalent(then_expr, needle)
                || expr_contains_equivalent(else_expr, needle)
        }
        ExprKind::Assignment {
            target,
            value,
            result_target,
            ..
        } => {
            expr_contains_equivalent(target, needle)
                || expr_contains_equivalent(value, needle)
                || result_target
                    .as_deref()
                    .is_some_and(|target| expr_contains_equivalent(target, needle))
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::StaticMethodCall { args, .. }
        | ExprKind::ClosureCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            args.iter().any(|arg| expr_contains_equivalent(arg, needle))
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            expr_contains_equivalent(class_name, needle)
                || args.iter().any(|arg| expr_contains_equivalent(arg, needle))
        }
        ExprKind::ExprCall { callee, args } => {
            expr_contains_equivalent(callee, needle)
                || args.iter().any(|arg| expr_contains_equivalent(arg, needle))
        }
        ExprKind::ArrayLiteral(items) => {
            items.iter().any(|item| expr_contains_equivalent(item, needle))
        }
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_contains_equivalent(key, needle)
                || expr_contains_equivalent(value, needle)
        }),
        ExprKind::ArrayAccess { array, index } => {
            expr_contains_equivalent(array, needle)
                || expr_contains_equivalent(index, needle)
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            expr_contains_equivalent(subject, needle)
                || arms.iter().any(|(patterns, arm_value)| {
                    patterns
                        .iter()
                        .any(|pattern| expr_contains_equivalent(pattern, needle))
                        || expr_contains_equivalent(arm_value, needle)
                })
                || default
                    .as_deref()
                    .is_some_and(|default| expr_contains_equivalent(default, needle))
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            expr_contains_equivalent(object, needle)
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            expr_contains_equivalent(object, needle) || expr_contains_equivalent(property, needle)
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            expr_contains_equivalent(object, needle)
                || args.iter().any(|arg| expr_contains_equivalent(arg, needle))
        }
        ExprKind::BufferNew { len, .. } => expr_contains_equivalent(len, needle),
        ExprKind::Yield { key, value } => {
            key.as_deref()
                .is_some_and(|key| expr_contains_equivalent(key, needle))
                || value
                    .as_deref()
                    .is_some_and(|value| expr_contains_equivalent(value, needle))
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::Variable(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::PreIncrement(_)
        | ExprKind::PostIncrement(_)
        | ExprKind::PreDecrement(_)
        | ExprKind::PostDecrement(_)
        | ExprKind::Closure { .. }
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => false,
    }
}

/// Returns true if an instanceof target contains an expression equivalent to
/// the needle. Name targets cannot contain equivalent expressions; expression
/// targets delegate to `expr_contains_equivalent`.
fn instanceof_target_contains_equivalent(target: &InstanceOfTarget, needle: &Expr) -> bool {
    match target {
        InstanceOfTarget::Name(_) => false,
        InstanceOfTarget::Expr(expr) => expr_contains_equivalent(expr, needle),
    }
}
