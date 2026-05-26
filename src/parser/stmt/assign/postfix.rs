//! Purpose:
//! Detects and lowers postfix assignments for complex expression targets.
//! Replays parseable l-values and creates effect-preserving lowerings for property/static assignments.
//!
//! Called from:
//! - `crate::parser::stmt::simple::parse_expr_stmt()` and assignment statement dispatch.
//!
//! Key details:
//! - Complex target lowering must not duplicate side effects while preserving PHP source evaluation order.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, InstanceOfTarget, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;
use super::compound::{assignment_operator, assignment_value, AssignmentOperator};

/// Parses a postfix assignment where the target involves property access, array access,
/// or other complex expressions. Detects `+=` append style via `[]` in the target.
/// Returns the lowered `StmtKind` directly for simple targets, or synthesizes a
/// temporary-variable sequence for effectful (compound operator) targets that cannot
/// be replayed safely.
/// Returns `Ok(None)` if the token range does not contain a postfix assignment pattern.
pub(in crate::parser::stmt) fn try_parse_postfix_assignment(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((assign_pos, op)) = find_top_level_assignment(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    let is_append =
        lhs.len() >= 3 && lhs[lhs.len() - 2].0 == Token::LBracket && lhs[lhs.len() - 1].0 == Token::RBracket;
    if is_append && op != AssignmentOperator::Assign {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }
    let contains_postfix = lhs
        .iter()
        .skip(1)
        .any(|(token, _)| matches!(token, Token::Arrow | Token::QuestionArrow | Token::LBracket));
    if !contains_postfix {
        return Ok(None);
    }

    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append { &lhs[..lhs.len() - 2] } else { lhs };
    let lhs_expr = parse_expr(lhs_expr_tokens, &mut lhs_pos)?;
    if lhs_pos != lhs_expr_tokens.len() {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }

    *pos = assign_pos + 1;
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_postfix_assignment(lhs_expr, op, rhs, span).map(Some);
    }
    let value = assignment_value(lhs_expr.clone(), op, rhs, span);

    let stmt = match lhs_expr.kind {
        ExprKind::Variable(array) if is_append => StmtKind::ArrayPush { array, value },
        ExprKind::PropertyAccess { object, property } if is_append => StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        },
        ExprKind::ArrayAccess { array, index } => {
            match array.kind {
                ExprKind::Variable(array) => StmtKind::ArrayAssign {
                    array,
                    index: *index,
                    value,
                },
                ExprKind::PropertyAccess { object, property } => StmtKind::PropertyArrayAssign {
                    object,
                    property,
                    index: *index,
                    value,
                },
                _ => StmtKind::NestedArrayAssign {
                    target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                    value,
                },
            }
        }
        ExprKind::PropertyAccess { object, property } => StmtKind::PropertyAssign {
            object,
            property,
            value,
        },
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };

    Ok(Some(Stmt::new(stmt, span)))
}

/// Parses a scoped (static class member) postfix assignment, handling targets like
/// `$obj::prop`, `$obj::$prop`, and `$obj::prop[]`. Detects `+=` append style via `[]`.
/// For compound operators on static properties that cannot be replayed safely, lowers
/// to a temporary-variable sequence via `lower_effectful_static_assignment`.
/// Returns `Ok(None)` when no scoped assignment pattern is found.
pub(in crate::parser::stmt) fn try_parse_scoped_property_assignment(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    let Some((assign_pos, op)) = find_top_level_assignment(tokens, start) else {
        return Ok(None);
    };
    if assign_pos < start + 3 {
        return Ok(None);
    }

    let lhs = &tokens[start..assign_pos];
    let is_append =
        lhs.len() >= 3 && lhs[lhs.len() - 2].0 == Token::LBracket && lhs[lhs.len() - 1].0 == Token::RBracket;
    if is_append && op != AssignmentOperator::Assign {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }
    let mut lhs_pos = 0;
    let lhs_expr_tokens = if is_append { &lhs[..lhs.len() - 2] } else { lhs };
    let lhs_expr = parse_expr(lhs_expr_tokens, &mut lhs_pos)?;
    if lhs_pos != lhs_expr_tokens.len() {
        return Err(CompileError::new(span, "Invalid assignment target"));
    }

    *pos = assign_pos + 1;
    let rhs = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    if op != AssignmentOperator::Assign && !can_replay_assignment_target(&lhs_expr) {
        return lower_effectful_static_assignment(lhs_expr, op, rhs, span).map(Some);
    }
    let value = assignment_value(lhs_expr.clone(), op, rhs, span);

    let stmt = match lhs_expr.kind {
        ExprKind::StaticPropertyAccess { receiver, property } if is_append => {
            StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value,
            }
        }
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: *index,
                    value,
                }
            }
            _ => StmtKind::NestedArrayAssign {
                target: Expr::new(ExprKind::ArrayAccess { array, index }, span),
                value,
            },
        },
        ExprKind::StaticPropertyAccess { receiver, property } => StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        },
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };

    Ok(Some(Stmt::new(stmt, span)))
}

/// Scans tokens starting from `start` (skipping nested parentheses, brackets, and braces)
/// and returns the position and operator of the first top-level assignment at nesting depth 0.
/// Returns `None` if no assignment operator is found before a semicolon at depth 0.
fn find_top_level_assignment(
    tokens: &[(Token, Span)],
    start: usize,
) -> Option<(usize, AssignmentOperator)> {
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut pos = start;

    while pos < tokens.len() {
        match tokens[pos].0 {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                return None;
            }
            _ if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                if let Some(op) = assignment_operator(&tokens[pos].0) {
                    return Some((pos, op));
                }
            }
            _ => {}
        }
        pos += 1;
    }

    None
}

/// Returns `true` if the expression is safe to replay as an l-value in a compound assignment,
/// meaning its value can be read multiple times without observable side effects.
/// Replayable expressions include variables, literals, property/static-property access on
/// replayable bases, and recursively their sub-expressions.
/// Function calls, new[], and most other `ExprKind` variants return `false`.
pub(crate) fn can_replay_assignment_target(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => true,
        ExprKind::ArrayAccess { array, index } => {
            can_replay_assignment_target(array) && can_replay_assignment_target(index)
        }
        ExprKind::PropertyAccess { object, .. } => can_replay_assignment_target(object),
        ExprKind::BinaryOp { left, right, .. } => {
            can_replay_assignment_target(left) && can_replay_assignment_target(right)
        }
        ExprKind::InstanceOf { value, target } => {
            can_replay_assignment_target(value) && can_replay_instanceof_target(target)
        }
        ExprKind::Negate(value)
        | ExprKind::Not(value)
        | ExprKind::BitNot(value)
        | ExprKind::Cast { expr: value, .. }
        | ExprKind::PtrCast { expr: value, .. }
        | ExprKind::NamedArg { value, .. }
        | ExprKind::Spread(value) => can_replay_assignment_target(value),
        ExprKind::NullCoalesce { value, default }
        | ExprKind::ShortTernary { value, default } => {
            can_replay_assignment_target(value) && can_replay_assignment_target(default)
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            can_replay_assignment_target(condition)
                && can_replay_assignment_target(then_expr)
                && can_replay_assignment_target(else_expr)
        }
        ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::ClassConstant { .. } | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => true,
        _ => false,
    }
}

/// Recursively checks whether `target` can be used as an r-value in a replay-safe assignment.
fn can_replay_instanceof_target(target: &InstanceOfTarget) -> bool {
    match target {
        InstanceOfTarget::Name(_) => true,
        InstanceOfTarget::Expr(expr) => can_replay_assignment_target(expr),
    }
}

/// Lowers a compound postfix assignment (e.g., `+=`, `-=`) on a non-replayable l-value
/// by extracting sub-expressions into temporary variables so each is evaluated exactly once.
/// Builds a `Synthetic` statement containing the temporaries followed by the final assignment.
fn lower_effectful_postfix_assignment(
    lhs_expr: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lowered = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::Variable(array) => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(ExprKind::Variable(array.clone()), span)),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::ArrayAssign { array, index, value }
            }
            ExprKind::PropertyAccess { object, property } => {
                let object = Box::new(lowerer.stabilize(*object));
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::PropertyAccess {
                                object: object.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::PropertyArrayAssign {
                    object,
                    property,
                    index,
                    value,
                }
            }
            _ => {
                let target = lowerer.stabilize_array_target(Expr::new(
                    ExprKind::ArrayAccess { array, index },
                    span,
                ));
                let value = assignment_value(target.clone(), op, rhs, span);
                StmtKind::NestedArrayAssign { target, value }
            }
        },
        ExprKind::PropertyAccess { object, property } => {
            let object = Box::new(lowerer.stabilize(*object));
            let target = Expr::new(
                ExprKind::PropertyAccess {
                    object: object.clone(),
                    property: property.clone(),
                },
                span,
            );
            let value = assignment_value(target, op, rhs, span);
            StmtKind::PropertyAssign {
                object,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(lowerer.finish(lowered))
}

/// Lowers a compound static property assignment where the target cannot be replayed safely.
/// Temporaries are created for any sub-expression that could produce observable side effects
/// (e.g., method calls on the object or array accesses). Returns a `Synthetic` statement.
fn lower_effectful_static_assignment(
    lhs_expr: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut lowerer = EffectfulTargetLowerer::new(span);
    let lowered = match lhs_expr.kind {
        ExprKind::ArrayAccess { array, index } => match array.kind {
            ExprKind::StaticPropertyAccess { receiver, property } => {
                let index = lowerer.stabilize(*index);
                let target = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(Expr::new(
                            ExprKind::StaticPropertyAccess {
                                receiver: receiver.clone(),
                                property: property.clone(),
                            },
                            span,
                        )),
                        index: Box::new(index.clone()),
                    },
                    span,
                );
                let value = assignment_value(target, op, rhs, span);
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index,
                    value,
                }
            }
            _ => {
                let target = lowerer.stabilize_array_target(Expr::new(
                    ExprKind::ArrayAccess { array, index },
                    span,
                ));
                let value = assignment_value(target.clone(), op, rhs, span);
                StmtKind::NestedArrayAssign { target, value }
            }
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            let target = Expr::new(
                ExprKind::StaticPropertyAccess {
                    receiver: receiver.clone(),
                    property: property.clone(),
                },
                span,
            );
            let value = assignment_value(target, op, rhs, span);
            StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value,
            }
        }
        _ => return Err(CompileError::new(span, "Invalid assignment target")),
    };
    Ok(lowerer.finish(lowered))
}

/// Helper that rewrites complex l-value targets into sequences of temporary-variable
/// assignments so that source evaluation order is preserved and side effects are not duplicated.
struct EffectfulTargetLowerer {
    span: Span,
    next_temp: usize,
    stmts: Vec<Stmt>,
}

impl EffectfulTargetLowerer {
    /// Initializes the lowerer with the source span used for all synthesized statements
    /// and temporary variable names.
    fn new(span: Span) -> Self {
        Self {
            span,
            next_temp: 0,
            stmts: Vec::new(),
        }
    }

    /// If `expr` is replay-safe, returns it unchanged. Otherwise, emits an `Assign`
    /// statement to a uniquely-named temporary and returns a `Variable` reference to it.
    /// Increments `next_temp` to keep temporary names unique across the same statement.
    fn stabilize(&mut self, expr: Expr) -> Expr {
        if can_replay_assignment_target(&expr) {
            return expr;
        }
        let name = format!(
            "__elephc_compound_{}_{}_{}",
            self.span.line, self.span.col, self.next_temp
        );
        self.next_temp += 1;
        self.stmts.push(Stmt::new(
            StmtKind::Assign {
                name: name.clone(),
                value: expr,
            },
            self.span,
        ));
        Expr::new(ExprKind::Variable(name), self.span)
    }

    /// Stabilizes an array-access target, recursively stabilizing both the array base
    /// and the index. For simple array bases (Variable, This, StaticPropertyAccess),
    /// the array base is kept as-is; deeper bases are stabilized via `stabilize_array_base`.
    fn stabilize_array_target(&mut self, expr: Expr) -> Expr {
        let span = expr.span;
        match expr.kind {
            ExprKind::ArrayAccess { array, index } => Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(self.stabilize_array_base(*array)),
                    index: Box::new(self.stabilize(*index)),
                },
                span,
            ),
            _ => self.stabilize(expr),
        }
    }

    /// Stabilizes the base of a nested array access chain. Recursively processes
    /// `ArrayAccess` and `PropertyAccess` chains; returns `Variable`, `This`,
    /// and `StaticPropertyAccess` directly; calls `stabilize` for all other expressions.
    fn stabilize_array_base(&mut self, expr: Expr) -> Expr {
        let span = expr.span;
        match expr.kind {
            ExprKind::ArrayAccess { array, index } => Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(self.stabilize_array_base(*array)),
                    index: Box::new(self.stabilize(*index)),
                },
                span,
            ),
            ExprKind::PropertyAccess { object, property } => Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(self.stabilize_array_base(*object)),
                    property,
                },
                span,
            ),
            ExprKind::Variable(_) | ExprKind::This | ExprKind::StaticPropertyAccess { .. } => expr,
            _ => self.stabilize(expr),
        }
    }

    /// Appends `final_stmt` as the last statement and wraps the entire sequence
    /// in a `Synthetic` statement node returned as a single `Stmt`.
    fn finish(mut self, final_stmt: StmtKind) -> Stmt {
        self.stmts.push(Stmt::new(final_stmt, self.span));
        Stmt::new(StmtKind::Synthetic(self.stmts), self.span)
    }
}
