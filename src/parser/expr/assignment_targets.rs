//! Purpose:
//! Classifies and lowers assignment-expression targets that may have observable side effects.
//! Builds prelude statements and temporary result targets for complex l-values.
//!
//! Called from:
//! - `crate::parser::expr::pratt` when parsing assignment expressions.
//! - `crate::synthetic_class::e_assign()` when building equivalent internal expressions.
//!
//! Key details:
//! - Target dependencies are tracked so PHP evaluation order is preserved during lowering.

use std::collections::HashSet;

use crate::parser::ast::{BinOp, Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};
use crate::parser::stmt::{
    can_replay_assignment_target, lower_postfix_incdec_assignment,
    update_dimension_base_is_snapshotted, update_index_needs_snapshot,
};
use crate::span::Span;

/// Recognizes property and indexed traversal rooted in a delayed static-property load.
pub(super) fn static_property_receiver_chain(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::DynamicPropertyAccess { object, .. } => static_property_receiver_chain(object),
        ExprKind::ArrayAccess { array, .. } => static_property_receiver_chain(array),
        _ => false,
    }
}

/// Recognizes writes whose static owner must be fetched after the RHS settles.
pub(super) fn delayed_static_assignment_target(target: &Expr) -> bool {
    match &target.kind {
        ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::DynamicPropertyAccess { object, .. } => static_property_receiver_chain(object),
        ExprKind::ArrayAccess { array, .. } => delayed_static_assignment_target(array),
        _ => false,
    }
}

/// Builds a plain assignment with the same capture and result rules for parsed and built ASTs.
pub(crate) fn plain_assignment_expression(target: Expr, value: Expr, span: Span) -> Expr {
    if !is_non_local_assignment_target(&target) {
        return Expr::new(
            ExprKind::Assignment {
                target: Box::new(target),
                value: Box::new(value),
                result_target: None,
                prelude: Vec::new(),
                conditional_value_temp: None,
            },
            span,
        );
    }
    let mut lowerer = AssignmentExpressionLowerer::new(span);
    let target = lowerer.stabilize_non_local_target(target, &value);
    // A syntactically replayable property read can still call a hook that replaces the owner.
    // Capture it after computed dimensions, before either the write or result uses it.
    let value = if delayed_static_assignment_target(&target) {
        lowerer.bind_result_value(value)
    } else {
        lowerer.bind_value(&target, value)
    };
    Expr::new(
        ExprKind::Assignment {
            target: Box::new(target),
            value: Box::new(value.clone()),
            result_target: Some(Box::new(value)),
            prelude: lowerer.finish(),
            conditional_value_temp: None,
        },
        span,
    )
}

/// Builds a compound expression whose captured dimensions are shared by its read and write.
pub(crate) fn compound_assignment_expression(target: Expr, op: BinOp, rhs: Expr, span: Span) -> Expr {
    let mut lowerer = AssignmentExpressionLowerer::new(span);
    let target = lowerer.stabilize_non_local_target(target, &rhs);
    let rhs = if delayed_static_assignment_target(&target) {
        lowerer.bind_result_value(rhs)
    } else {
        lowerer.bind_value(&target, rhs)
    };
    let target = lowerer.snapshot_update_dimension(target);
    let value = Expr::new(ExprKind::BinaryOp {
        left: Box::new(target.clone()), op, right: Box::new(rhs),
    }, span);
    let result = lowerer.bind_result_value(value);
    Expr::new(ExprKind::Assignment {
        target: Box::new(target), value: Box::new(result.clone()),
        result_target: Some(Box::new(result)), prelude: lowerer.finish(),
        conditional_value_temp: None,
    }, span)
}

/// Desugars `++$place` / `$place++` into a read-modify-write an expression can carry.
///
/// The `Pre*`/`Post*` nodes hold a variable NAME, so a property, an array element, a static
/// property or any nesting of those has nowhere to live in them. This rewrites such a place into
/// an `Assignment` whose prelude does the work and whose value is what PHP yields: the NEW value
/// for the prefix form, the value the place held BEFORE the write for the postfix form.
///
/// The place is STABILIZED first, exactly as a compound assignment's target is. PHP evaluates the
/// receiver and every index once, so `$b[ix()]++` calls `ix()` once and the temporary it binds is
/// what both the read and the write address. Without it the rewrite had to replay the place
/// verbatim, so `can_replay_assignment_target` turned away every computed index and the operator
/// was left unconsumed — `echo $b[ix()]++;` reported `Expected ';'` (issue #682).
///
/// A target that is still not replayable after stabilization is declined rather than guessed at,
/// which leaves the caller's original error in place.
pub(super) fn desugar_lvalue_incdec(
    target: Expr,
    increment: bool,
    prefix: bool,
    span: Span,
) -> Option<Expr> {
    if matches!(target.kind, ExprKind::Variable(_)) {
        // A bare variable already has a dedicated node; only the shapes it cannot name reach here.
        return None;
    }
    if !is_non_local_assignment_target(&target) {
        return None;
    }
    let mut lowerer = AssignmentExpressionLowerer::new(span);
    // The increment reads and writes the same place, so the place itself is what the stabilizer
    // measures the target's parts against.
    let probe = target.clone();
    let target = lowerer.stabilize_non_local_target(target, &probe);
    let target = lowerer.snapshot_update_dimension(target);
    if !can_replay_assignment_target(&target) {
        return None;
    }
    let mut prelude = lowerer.finish();
    // Capture one-dimensional elements once, including the expression result, so neither
    // prefix nor postfix needs another key diagnosis. Static elements use the PHP local
    // incdec kernel; local and instance-property numeric lowering stays unchanged below.
    if let ExprKind::ArrayAccess { array, .. } = &target.kind {
        if matches!(
            &array.kind,
            ExprKind::Variable(_)
                | ExprKind::PropertyAccess { .. }
                | ExprKind::DynamicPropertyAccess { .. }
                | ExprKind::StaticPropertyAccess { .. }
        ) {
            let old_name = crate::names::generated_local_name(&format!(
                "__elephc_incdec_old_{}_{}", span.line, span.col
            ));
            let new_name = crate::names::generated_local_name(&format!(
                "__elephc_incdec_new_{}_{}", span.line, span.col
            ));
            prelude.push(Stmt::new(StmtKind::Assign {
                name: old_name.clone(),
                value: target.clone(),
            }, span));
            if matches!(array.kind, ExprKind::StaticPropertyAccess { .. }) {
                let operation = match (increment, prefix) {
                    (true, true) => ExprKind::PreIncrement(old_name.clone()),
                    (true, false) => ExprKind::PostIncrement(old_name.clone()),
                    (false, true) => ExprKind::PreDecrement(old_name.clone()),
                    (false, false) => ExprKind::PostDecrement(old_name.clone()),
                };
                prelude.push(Stmt::new(StmtKind::Assign {
                    name: new_name.clone(), value: Expr::new(operation, span),
                }, span));
                return Some(Expr::new(ExprKind::Assignment {
                    target: Box::new(target),
                    value: Box::new(Expr::new(ExprKind::Variable(old_name), span)),
                    result_target: Some(Box::new(Expr::new(ExprKind::Variable(new_name), span))),
                    prelude, conditional_value_temp: None,
                }, span));
            }
            let old_value = Expr::new(ExprKind::Variable(old_name.clone()), span);
            let next_value = Expr::new(ExprKind::BinaryOp {
                left: Box::new(old_value),
                op: if increment { BinOp::Add } else { BinOp::Sub },
                right: Box::new(Expr::new(ExprKind::IntLiteral(1), span)),
            }, span);
            prelude.push(Stmt::new(StmtKind::Assign {
                name: new_name.clone(),
                value: next_value,
            }, span));
            let result_name = if prefix { new_name.clone() } else { old_name };
            return Some(Expr::new(ExprKind::Assignment {
                target: Box::new(target),
                value: Box::new(Expr::new(ExprKind::Variable(new_name), span)),
                result_target: Some(Box::new(Expr::new(ExprKind::Variable(result_name), span))),
                prelude,
                conditional_value_temp: None,
            }, span));
        }
    }
    let mut write = lower_postfix_incdec_assignment(target.clone(), increment, span).ok()?;
    let value = if prefix {
        // Nested static incdec already captures its new value. Re-reading the place
        // would diagnose converted keys twice and could observe a handler's replacement.
        let captured = nested_incdec_result(&mut write, true);
        prelude.push(write);
        captured.unwrap_or(target)
    } else if let Some(captured) = nested_incdec_result(&mut write, false) {
        // The write-context lookup creates missing parents and diagnoses each key once.
        prelude.push(write);
        captured
    } else {
        // `$p++` evaluates to the OLD value, so it has to be captured before the write.
        let temp = crate::names::generated_local_name(&format!(
            "__elephc_incdec_{}_{}",
            span.line, span.col
        ));
        prelude.push(Stmt::new(
            StmtKind::Assign {
                name: temp.clone(),
                value: target,
            },
            span,
        ));
        prelude.push(write);
        Expr::new(ExprKind::Variable(temp), span)
    };
    let result = crate::names::generated_local_name(&format!(
        "__elephc_incdec_result_{}_{}",
        span.line, span.col
    ));
    Some(Expr::new(
        ExprKind::Assignment {
            target: Box::new(Expr::new(ExprKind::Variable(result), span)),
            value: Box::new(value),
            result_target: None,
            prelude,
            conditional_value_temp: None,
        },
        span,
    ))
}

/// Selects a nested update's capture without re-reading the place or mutating its old result.
fn nested_incdec_result(write: &mut Stmt, prefix: bool) -> Option<Expr> {
    let value = match &mut write.kind {
        StmtKind::NestedArrayAssign { value, .. } => value,
        StmtKind::Synthetic(statements) => return nested_incdec_result(statements.last_mut()?, prefix),
        _ => return None,
    };
    let ExprKind::Assignment { target, value: operation, prelude, .. } = &mut value.kind else { return None; };
    if prefix { return Some(target.as_ref().clone()); }
    let StmtKind::Assign { name, .. } = &prelude.first()?.kind else { return None; };
    let old = name.clone();
    let working = crate::names::generated_local_name(&format!(
        "__elephc_incdec_work_{}_{}", value.span.line, value.span.col));
    let local = match &mut operation.kind {
        ExprKind::PreIncrement(local) | ExprKind::PreDecrement(local) if *local == old => local,
        _ => return None,
    };
    // Generic property roots execute this AST prelude, unlike specialized array roots.
    // Mutating a separate working local preserves the postfix result on both paths.
    *local = working.clone();
    prelude.push(Stmt::new(StmtKind::Assign {
        name: working, value: Expr::new(ExprKind::Variable(old.clone()), value.span),
    }, value.span));
    Some(Expr::new(ExprKind::Variable(old), value.span))
}

/// Returns true if the expression is a non-local assignment target (array-access,
/// property-access, dynamic-property-access, or static-property-access). These
/// require stabilization because the base expression may be evaluated multiple
/// times during assignment.
pub(super) fn is_non_local_assignment_target(expr: &Expr) -> bool {
    matches!(
        &expr.kind,
        ExprKind::ArrayAccess { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::DynamicPropertyAccess { .. }
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
        | ExprKind::DynamicPropertyAccess { .. }
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
        | ExprKind::DynamicPropertyAccess { .. }
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

    /// Captures a mutable index for both halves of an update.
    ///
    /// Covers one dimension of a local, property, or static-property array, so the write lands
    /// on the key its read converted even when a diagnostic handler reassigns the index variable.
    pub(super) fn snapshot_update_dimension(&mut self, target: Expr) -> Expr {
        let span = target.span;
        match target.kind {
            ExprKind::ArrayAccess { array, index }
                if update_dimension_base_is_snapshotted(&array) =>
            {
                let index = if update_index_needs_snapshot(&index) {
                    self.bind_temp(*index)
                } else {
                    *index
                };
                Expr::new(ExprKind::ArrayAccess {
                    array,
                    index: Box::new(index),
                }, span)
            }
            kind => Expr::new(kind, span),
        }
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
                    Expr {
                        kind: ExprKind::DynamicPropertyAccess { object, property },
                        span: array_span,
                    } => Expr::new(
                        ExprKind::DynamicPropertyAccess {
                            object: Box::new(self.stabilize_receiver(*object, rhs)),
                            property: Box::new(self.stabilize_dimension_index(*property, rhs)),
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
            ExprKind::DynamicPropertyAccess { object, property } => Expr::new(
                ExprKind::DynamicPropertyAccess {
                    object: Box::new(self.stabilize_receiver(*object, rhs)),
                    property: Box::new(self.stabilize_dimension_index(*property, rhs)),
                },
                span,
            ),
            ExprKind::StaticPropertyAccess { receiver, property } => {
                Expr::new(ExprKind::StaticPropertyAccess { receiver, property }, span)
            }
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
            ExprKind::DynamicPropertyAccess { object, property } => Expr::new(
                ExprKind::DynamicPropertyAccess {
                    object: Box::new(self.stabilize_receiver(*object, rhs)),
                    property: Box::new(self.stabilize_dimension_index(*property, rhs)),
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
        !can_replay_assignment_target(expr) || target_part_reads_mutated_dependency(expr, rhs)
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

        !can_replay_assignment_target(expr) || target_part_reads_mutated_dependency(expr, rhs)
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
        let name = crate::names::generated_local_name(&format!(
            "__elephc_assign_expr_{}_{}_{}",
            self.span.line, self.span.col, self.next_temp
        ));
        self.next_temp += 1;
        name
    }
}

/// Returns true if the assignment value may mutate a variable that the target
/// depends on. Used to detect when stabilization is required to preserve
/// evaluation order (e.g., `$arr[$i] = $arr[$j]` where RHS reads `$arr`).
pub(super) fn assignment_value_may_mutate_target_dependency(target: &Expr, value: &Expr) -> bool {
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
        // A value-position include is never an assignment target, so it contributes no
        // target dependencies.
        ExprKind::IncludeValue { .. } => {}
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
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            collect_assignment_target_dependencies(object, dependencies);
            collect_assignment_target_dependencies(method, dependencies);
            for arg in args {
                collect_assignment_target_dependencies(arg, dependencies);
            }
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
        | ExprKind::Clone(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value)
        | ExprKind::ObjectClassName { object: value } => {
            collect_assignment_target_dependencies(value, dependencies)
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
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
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries.iter() {
                for expr in entry.exprs() {
                    collect_assignment_target_dependencies(expr, dependencies);
                }
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
        | ExprKind::NewGeneric { .. }
        | ExprKind::NewDynamic { .. }
        | ExprKind::NewDynamicObject { .. }
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::BufferNew { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
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
        // A value-position include runs in the caller's scope and may write any variable.
        ExprKind::IncludeValue { .. } => true,
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
        | ExprKind::Clone(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value)
        | ExprKind::ObjectClassName { object: value } => {
            expr_may_write_dependency(value, dependencies)
        }
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
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
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .any(|item| expr_may_write_dependency(item, dependencies)),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_may_write_dependency(key, dependencies)
                || expr_may_write_dependency(value, dependencies)
        }),
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(|expr| expr_may_write_dependency(expr, dependencies)),
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
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            expr_may_write_dependency(object, dependencies)
                || expr_may_write_dependency(method, dependencies)
                || args.iter().any(|arg| {
                    expr_contains_dependency(arg, dependencies)
                        || expr_may_write_dependency(arg, dependencies)
                })
        }
        ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            args.iter().any(|arg| {
                expr_contains_dependency(arg, dependencies)
                    || expr_may_write_dependency(arg, dependencies)
            })
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_contains_dependency(name_expr, dependencies)
                || expr_may_write_dependency(name_expr, dependencies)
                || args.iter().any(|arg| {
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
        | ExprKind::DynamicPropertyAccess { object, .. }
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
        InstanceOfTarget::Generic(_) => false,
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
        ExprKind::IncludeValue { path, .. } => expr_contains_equivalent(path, needle),
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
        | ExprKind::Clone(value)
        | ExprKind::ErrorSuppress(value)
        | ExprKind::Print(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value)
        | ExprKind::ObjectClassName { object: value }
        | ExprKind::YieldFrom(value) => expr_contains_equivalent(value, needle),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            expr_contains_equivalent(value, needle) || expr_contains_equivalent(default, needle)
        }
        ExprKind::Pipe { value, callable } => {
            expr_contains_equivalent(value, needle) || expr_contains_equivalent(callable, needle)
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
        | ExprKind::NewGeneric { args, .. }
        | ExprKind::NewScopedObject { args, .. } => {
            args.iter().any(|arg| expr_contains_equivalent(arg, needle))
        }
        ExprKind::NewDynamic { name_expr, args } => {
            expr_contains_equivalent(name_expr, needle)
                || args.iter().any(|arg| expr_contains_equivalent(arg, needle))
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
        ExprKind::ArrayLiteral(items) => items
            .iter()
            .any(|item| expr_contains_equivalent(item, needle)),
        ExprKind::ArrayLiteralAssoc(items) => items.iter().any(|(key, value)| {
            expr_contains_equivalent(key, needle) || expr_contains_equivalent(value, needle)
        }),
        ExprKind::ArrayLiteralMixed(entries) => entries
            .iter()
            .flat_map(|entry| entry.exprs())
            .any(|expr| expr_contains_equivalent(expr, needle)),
        ExprKind::ArrayAccess { array, index } => {
            expr_contains_equivalent(array, needle) || expr_contains_equivalent(index, needle)
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
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            expr_contains_equivalent(object, needle)
                || expr_contains_equivalent(method, needle)
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
        InstanceOfTarget::Generic(_) => false,
        InstanceOfTarget::Expr(expr) => expr_contains_equivalent(expr, needle),
    }
}

/// Diagnostic for a write through `$class::$prop`, shared by every write shape.
pub(crate) const DYNAMIC_STATIC_PROPERTY_WRITE_ERROR: &str =
    "Writing a static property through a dynamic class (`$class::$prop = ...`) is not supported; use the class name";

/// Returns true for the parser's `$class::X` / `$class::$prop` read desugars, which are values,
/// not places: a write through them would silently land in a temporary.
pub(crate) fn is_dynamic_class_member_read(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::FunctionCall { name, .. } => matches!(
            name.as_str().trim_start_matches('\\'),
            "__elephc_dynamic_class_constant" | "__elephc_dynamic_static_property"
        ),
        ExprKind::ArrayAccess { array, .. } => is_dynamic_class_member_read(array),
        _ => false,
    }
}

/// Rejects a write context (`++`, `--`, `=&`, `foreach (... as &$v)`, `unset`, destructuring)
/// whose place is rooted in a `$class::$prop` read desugar: that desugar is a value, so the
/// write would land in a temporary and silently do nothing.
pub(crate) fn reject_dynamic_member_write(expr: &Expr, span: Span) -> Result<(), crate::errors::CompileError> {
    if is_dynamic_class_member_place(expr) {
        return Err(crate::errors::CompileError::new(span, DYNAMIC_STATIC_PROPERTY_WRITE_ERROR));
    }
    Ok(())
}

/// Like `is_dynamic_class_member_read`, also looking through property links
/// (`$c::$o->x`), which reach the same temporary.
fn is_dynamic_class_member_place(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::PropertyAccess { object, .. } | ExprKind::NullsafePropertyAccess { object, .. } => {
            is_dynamic_class_member_place(object)
        }
        ExprKind::ArrayAccess { array, .. } => is_dynamic_class_member_place(array),
        _ => is_dynamic_class_member_read(expr),
    }
}
