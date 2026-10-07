//! Purpose:
//! Parses empty dimensions only after their write or update context is known.
//! Lowers append expressions to existing assignment preludes and runtime array pushes.
//!
//! Called from:
//! - `super::pratt` at an empty postfix dimension.
//!
//! Key details:
//! - Empty dimensions never become null or guessed numeric indices in the AST.
//! - Receiver/key effects precede RHS effects, while appends occur after RHS evaluation.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use super::assignment_targets::{AssignmentExpressionLowerer, is_assignment_expression_target};
use super::pratt::{AssignmentOperator, assignment_bp, parse_expr_bp};

/// Consumes an empty dimension and its assignment, postfix update, or enclosing prefix update.
pub(super) fn parse_append_write(
    base: Expr, tokens: &[SpannedToken], pos: &mut usize, span: Span,
    prefix_increment: bool,
) -> Result<Expr, CompileError> {
    *pos += 1; // The opening bracket was consumed by Pratt; consume its empty close.
    let mut dimensions = Vec::new();
    while matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::LBracket)) {
        let dimension_span = tokens[*pos].1.span;
        *pos += 1;
        let index = if matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::RBracket)) {
            None
        } else { Some(super::parse_expr(tokens, pos)?) };
        if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::RBracket)) {
            return Err(CompileError::new(dimension_span, "Expected ']'"));
        }
        *pos += 1;
        dimensions.push(index);
    }
    let property_suffix = matches!(
        tokens.get(*pos).map(|(token, _)| token), Some(Token::Arrow),
    );
    let (update, rhs) = if property_suffix {
        (AppendUpdate::Assign, Expr::new(ExprKind::Null, span))
    } else if prefix_increment {
        (AppendUpdate::Compound(BinOp::Add), Expr::new(ExprKind::IntLiteral(1), span))
    } else { match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::PlusPlus) => {
            *pos += 1;
            (AppendUpdate::PostIncrement, Expr::new(ExprKind::Null, span))
        }
        Some(Token::MinusMinus) => return Err(CompileError::new(
            span, "Post-decrement on an append dimension is not supported",
        )),
        Some(token) => {
            let Some((operator, _, right_bp)) = assignment_bp(token) else {
                return Err(CompileError::new(span, "Cannot use [] for reading"));
            };
            let update = match operator {
                AssignmentOperator::Assign => AppendUpdate::Assign,
                AssignmentOperator::Compound(operator) => AppendUpdate::Compound(operator),
                AssignmentOperator::NullCoalesce => {
                    return Err(CompileError::new(span, "Cannot use [] for reading"));
                }
            };
            *pos += 1;
            (update, parse_expr_bp(tokens, pos, right_bp)?)
        }
        None => return Err(CompileError::new(span, "Cannot use [] for reading")),
    } };
    if !is_assignment_expression_target(&base) {
        return Err(CompileError::new(base.span, "Invalid assignment target"));
    }
    let span = span.merge(rhs.span);
    let mut stabilizer = AssignmentExpressionLowerer::new(span);
    let base = stabilizer.stabilize_non_local_target(base, &rhs);
    for index in dimensions.iter_mut().flatten() {
        let literal = matches!(index.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)
            | ExprKind::BoolLiteral(_) | ExprKind::StringLiteral(_) | ExprKind::Null)
            || matches!(&index.kind, ExprKind::Negate(inner)
                if matches!(inner.kind, ExprKind::IntLiteral(_) | ExprKind::FloatLiteral(_)));
        if !literal && !matches!(index.kind, ExprKind::Variable(_)) {
            *index = stabilizer.bind_result_value(index.clone());
        }
    }
    let marker = property_suffix.then(|| Expr::new(ExprKind::Variable(
        crate::names::generated_local_name(&format!("__elephc_append_property_null_{}_{}", span.line, span.col)),
    ), span));
    let rhs = if property_suffix { rhs } else { stabilizer.bind_result_value(rhs) };
    let mut prelude = stabilizer.finish();
    if let Some(Expr { kind: ExprKind::Variable(name), .. }) = &marker {
        // This boundary separates receiver/key capture from the delayed null append.
        prelude.push(Stmt::new(StmtKind::Assign {
            name: name.clone(), value: Expr::new(ExprKind::Null, span),
        }, span));
    }
    let mut lowerer = AppendLowerer { span, next_temp: 0, prelude, result: None };
    let value = lowerer.nested_value(&dimensions, &rhs, &update);
    lowerer.push(base, value)?;
    let result = marker.unwrap_or_else(|| lowerer.result.expect("append leaf binds its result"));
    Ok(Expr::new(ExprKind::Assignment {
        target: Box::new(result.clone()), value: Box::new(result), result_target: None,
        prelude: lowerer.prelude, conditional_value_temp: None,
    }, span))
}

/// The operation that writes a newly appended element, never an existing array value.
enum AppendUpdate { Assign, Compound(BinOp), PostIncrement }

/// Finds a property chain rooted in an implicit null append, never an ordinary assignment result.
fn null_append_property_chain(target: &Expr) -> Option<(&Expr, Vec<Expr>)> {
    let mut receiver = target;
    let mut properties = Vec::new();
    loop {
        match &receiver.kind {
            ExprKind::PropertyAccess { object, property } => {
                properties.push(Expr::new(ExprKind::StringLiteral(property.clone()), receiver.span));
                receiver = object;
            }
            ExprKind::DynamicPropertyAccess { object, property } => {
                properties.push(*property.clone());
                receiver = object;
            }
            ExprKind::Assignment { target, .. } if matches!(&target.kind,
                ExprKind::Variable(name) if name.starts_with("__elephc_append_property_null_")) => {
                properties.reverse();
                return (!properties.is_empty()).then_some((receiver, properties));
            }
            _ => return None,
        }
    }
}

/// Identifies a dangling append-property read so only genuine write contexts accept it.
pub(super) fn is_null_append_property(target: &Expr) -> bool {
    null_append_property_chain(target).is_some()
}

/// Recognizes the implicit null receiver before a property suffix has been attached.
pub(super) fn is_null_append_receiver(target: &Expr) -> bool {
    matches!(&target.kind, ExprKind::Assignment { target, .. } if matches!(&target.kind,
        ExprKind::Variable(name) if name.starts_with("__elephc_append_property_null_")))
}

/// Preserves receiver, selector and RHS effects before appending null and throwing PHP's Error.
/// A deeper property chain fails while modifying its first property, not at its final member.
pub(super) fn lower_null_property_write(target: &Expr, rhs: Option<&Expr>, span: Span) -> Option<Expr> {
    let (root, properties) = null_append_property_chain(target)?;
    let ExprKind::Assignment { target: marker, prelude: append, .. } = &root.kind else { return None; };
    let ExprKind::Variable(marker_name) = &marker.kind else { return None; };
    let boundary = append.iter().position(|statement| matches!(&statement.kind,
        StmtKind::Assign { name, .. } if name == marker_name))?;
    let mut prelude = append[..boundary].to_vec();
    let nested = properties.len() > 1;
    // Use a distinct namespace from the append's own reserved locals.
    let mut first_property = None;
    for (index, property) in properties.into_iter().enumerate() {
        let name = crate::names::generated_local_name(&format!(
            "__elephc_append_property_selector_{}_{}_{}", span.line, span.col, index,
        ));
        prelude.push(Stmt::new(StmtKind::Assign { name: name.clone(), value: property }, span));
        if index == 0 { first_property = Some(Expr::new(ExprKind::Variable(name), span)); }
    }
    let rhs_temp = rhs.map(|rhs| {
        let name = crate::names::generated_local_name(&format!(
            "__elephc_append_property_rhs_{}_{}", span.line, span.col,
        ));
        prelude.push(Stmt::new(StmtKind::Assign { name: name.clone(), value: rhs.clone() }, span));
        Expr::new(ExprKind::Variable(name), span)
    });
    prelude.extend_from_slice(&append[boundary..]);
    let action = if nested { "modify" } else if rhs.is_some() { "assign" } else { "increment/decrement" };
    let message = Expr::new(ExprKind::BinaryOp {
        left: Box::new(Expr::new(ExprKind::BinaryOp {
            left: Box::new(Expr::new(ExprKind::StringLiteral(
                format!("Attempt to {action} property \""),
            ), span)),
            op: BinOp::Concat,
            right: Box::new(first_property?),
        }, span)),
        op: BinOp::Concat,
        right: Box::new(Expr::new(ExprKind::StringLiteral("\" on null".to_string()), span)),
    }, span);
    let error = Expr::new(ExprKind::NewObject {
        class_name: crate::names::Name::from_parts(
            crate::names::NameKind::FullyQualified, vec!["Error".to_string()],
        ), args: vec![message],
    }, span);
    let error_name = crate::names::generated_local_name(&format!(
        "__elephc_append_property_error_{}_{}", span.line, span.col,
    ));
    prelude.push(Stmt::new(StmtKind::Assign { name: error_name.clone(), value: error }, span));
    if let Some(rhs_temp) = rhs_temp {
        prelude.push(Stmt::new(StmtKind::ExprStmt(Expr::new(ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("unset"), args: vec![rhs_temp],
        }, span)), span));
    }
    let error = Expr::new(ExprKind::Variable(error_name), span);
    let error = Expr::new(ExprKind::Assignment {
        target: Box::new(error.clone()), value: Box::new(error), result_target: None,
        prelude, conditional_value_temp: None,
    }, span);
    Some(Expr::new(ExprKind::Throw(Box::new(error)), span))
}

/// Builds nested containers and the expression result using reserved compiler locals.
struct AppendLowerer { span: Span, next_temp: usize, prelude: Vec<Stmt>, result: Option<Expr> }

impl AppendLowerer {
    /// Reserves a unique local for one append value or expression result.
    fn next_temp(&mut self) -> Expr {
        let name = crate::names::generated_local_name(&format!(
            "__elephc_append_{}_{}_{}", self.span.line, self.span.col, self.next_temp,
        ));
        self.next_temp += 1;
        Expr::new(ExprKind::Variable(name), self.span)
    }

    /// Captures one expression so the write and result never re-evaluate it.
    fn bind(&mut self, value: Expr) -> Expr {
        let target = self.next_temp();
        let ExprKind::Variable(name) = &target.kind else { unreachable!() };
        self.prelude.push(Stmt::new(StmtKind::Assign { name: name.clone(), value }, self.span));
        target
    }

    /// Constructs one fresh nested array without converting an append to an indexed read.
    fn nested_value(&mut self, dimensions: &[Option<Expr>], rhs: &Expr, update: &AppendUpdate) -> Expr {
        let Some((dimension, tail)) = dimensions.split_first() else {
            return self.leaf(Expr::new(ExprKind::Null, self.span), rhs, update);
        };
        let container = self.bind(Expr::new(ExprKind::ArrayLiteral(Vec::new()), self.span));
        if let Some(index) = dimension {
            // The eager RHS is already bound. Capture the update key now, before a missing-key
            // warning can call user code that changes the variable used to select this entry.
            let index = if matches!(update, AppendUpdate::Assign)
                || matches!(index.kind, ExprKind::IntLiteral(_)) { index.clone() }
                else { self.bind(index.clone()) };
            let target = Expr::new(ExprKind::ArrayAccess {
                array: Box::new(container.clone()), index: Box::new(index.clone()),
            }, self.span);
            let value = if tail.is_empty() { self.leaf(target.clone(), rhs, update) }
                else { self.nested_value(tail, rhs, update) };
            let ExprKind::Variable(array) = &container.kind else { unreachable!() };
            self.prelude.push(Stmt::new(StmtKind::ArrayAssign {
                array: array.clone(), index: index.clone(), value,
            }, self.span));
        } else {
            let value = self.nested_value(tail, rhs, update);
            self.push(container.clone(), value).expect("fresh container is an append target");
        }
        container
    }

    /// Applies the leaf operation and records its PHP expression result.
    fn leaf(&mut self, current: Expr, rhs: &Expr, update: &AppendUpdate) -> Expr {
        let value = match update {
            AppendUpdate::Assign => rhs.clone(),
            AppendUpdate::Compound(operator) => {
                let value = Expr::new(ExprKind::BinaryOp {
                    left: Box::new(current), op: operator.clone(), right: Box::new(rhs.clone()),
                }, self.span);
                let result = self.next_temp();
                self.result = Some(result.clone());
                // Keep the read-modify-write expression on the store. EIR lowering can then
                // reuse its diagnosed key, while this inline assignment captures the result.
                return Expr::new(ExprKind::Assignment {
                    target: Box::new(result), value: Box::new(value), result_target: None,
                    prelude: Vec::new(), conditional_value_temp: None,
                }, self.span);
            }
            AppendUpdate::PostIncrement => {
                let current = self.bind(current);
                let result = self.bind(current.clone());
                let ExprKind::Variable(name) = &current.kind else { unreachable!() };
                self.prelude.push(Stmt::new(StmtKind::Assign {
                    name: name.clone(), value: Expr::new(ExprKind::PreIncrement(name.clone()), self.span),
                }, self.span));
                self.result = Some(result);
                return current;
            }
        };
        let value = self.bind(value);
        self.result = Some(value.clone());
        value
    }

    /// Uses the existing target-aware append lowering, including nested COW write-back.
    fn push(&mut self, base: Expr, value: Expr) -> Result<(), CompileError> {
        let statement = match base.kind {
            ExprKind::Variable(array) => Stmt::new(StmtKind::ArrayPush { array, value }, self.span),
            ExprKind::PropertyAccess { object, property } => {
                Stmt::new(StmtKind::PropertyArrayPush { object, property, value }, self.span)
            }
            ExprKind::StaticPropertyAccess { receiver, property } => {
                Stmt::new(StmtKind::StaticPropertyArrayPush { receiver, property, value }, self.span)
            }
            ExprKind::ArrayAccess { .. } => {
                crate::parser::stmt::lower_nested_append_assignment(base, value, self.span)?
            }
            _ => return Err(CompileError::new(base.span, "Invalid assignment target")),
        };
        self.prelude.push(statement);
        Ok(())
    }
}
