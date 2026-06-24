//! Purpose:
//! Parses direct variable compound assignment statements.
//! Maps compound assignment tokens into binary operations plus assignment statement values.
//!
//! Called from:
//! - `crate::parser::stmt::assign::simple::parse_variable_stmt()`.
//!
//! Key details:
//! - Compound lowering must preserve PHP's read-modify-write semantics for the target variable.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{BinOp, Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::super::expect_semicolon;

/// Compound assignment operators: plain assignment (`=`), compound binary operators
/// (`+=`, `-=`, `*=`, etc.), and null coalesce assignment (`??=`).
#[derive(Debug, Clone, PartialEq)]
pub(super) enum AssignmentOperator {
    Assign,
    Compound(BinOp),
    NullCoalesce,
}

/// Parses a direct variable compound assignment statement (`$x += 1`, `$x ??= 2`, etc.).
///
/// Consumes the variable name token, then the assignment operator, then the RHS expression.
/// If the RHS ends with `and`/`or`/`xor` (bitwise assignment with expression chain), falls back
/// to a full expression parse and emits an `ExprStmt` instead.
///
/// Returns the parsed `Assign` statement wrapping the target variable and the computed value
/// expression, or an `ExprStmt` for the bitwise-chain fallback case.
pub(super) fn parse_assign(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let start = *pos;
    let name = match &tokens[*pos].0 {
        Token::Variable(n) => n.clone(),
        _ => unreachable!(),
    };
    *pos += 1;

    if *pos >= tokens.len() {
        return Err(CompileError::new(span, "Expected '=' after variable name"));
    }

    let op = assignment_operator(&tokens[*pos].0)
        .ok_or_else(|| CompileError::new(span, "Expected '=' after variable name"))?;
    *pos += 1;

    if op == AssignmentOperator::Assign
        && matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Ampersand))
    {
        return parse_ref_assign(tokens, pos, name, span);
    }

    // `$x = require X;` assigns the included file's value (its top-level `return`, or `1`).
    if op == AssignmentOperator::Assign {
        if let Some(include_value) = super::super::simple::try_parse_value_include(tokens, pos)? {
            expect_semicolon(tokens, pos)?;
            return Ok(Stmt::new(
                StmtKind::Assign {
                    name,
                    value: include_value,
                },
                span,
            ));
        }
    }

    let rhs = parse_assignment_value_expr(tokens, pos)?;
    if matches!(
        tokens.get(*pos).map(|(token, _)| token),
        Some(Token::And | Token::Or | Token::Xor)
    ) {
        *pos = start;
        let expr = parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
    }
    expect_semicolon(tokens, pos)?;

    let target = Expr::new(ExprKind::Variable(name.clone()), span);
    let value = assignment_value(target, op, rhs, span);

    Ok(Stmt::new(StmtKind::Assign { name, value }, span))
}

/// Parses direct variable reference assignment after the leading `$target =` tokens.
///
/// Returns whether an lvalue reference source is free of evaluation side effects.
///
/// `$var =& $a[0]` is lowered by re-evaluating the source lvalue twice (once to copy the value into
/// the target variable, once as the write target of the reverse reference assignment). That is only
/// sound when re-evaluating the lvalue has no observable side effects, so this allows variables,
/// scalar literal subscripts, and chains of array/property access over those, and rejects anything
/// that could run user code (calls, increments, nested assignments) on the second evaluation.
fn ref_source_lvalue_is_side_effect_free(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Variable(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::Null => true,
        ExprKind::ArrayAccess { array, index } => {
            ref_source_lvalue_is_side_effect_free(array)
                && ref_source_lvalue_is_side_effect_free(index)
        }
        ExprKind::PropertyAccess { object, .. } => ref_source_lvalue_is_side_effect_free(object),
        _ => false,
    }
}

/// PHP spells reference aliasing as `$target =& $source;`. This parser accepts a bare variable
/// source (`$a =& $b`, lowered to `RefAssign`) and a side-effect-free array/property element source
/// (`$r =& $a[0]`, the reverse direction of `$a[0] =& $r`).
///
/// The element-source form is lowered to a synthetic pair `$target = $source; $source =& $target;`,
/// which copies the element value into the target and then aliases the element to the target's
/// reference cell, so both observe subsequent writes. The lowering re-evaluates the source lvalue,
/// so a source with side effects is rejected rather than miscompiled.
fn parse_ref_assign(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    target: String,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    // A bare variable source (no trailing `[`/`->`) keeps the local-to-local reference form.
    let bare_variable = matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Variable(_)))
        && !matches!(
            tokens.get(*pos + 1).map(|(t, _)| t),
            Some(Token::LBracket | Token::Arrow)
        );
    if bare_variable {
        let Some((Token::Variable(source), _)) = tokens.get(*pos) else {
            unreachable!("bare variable source was just confirmed");
        };
        let source = source.clone();
        *pos += 1;
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::RefAssign { target, source }, span));
    }

    if !matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Variable(_))) {
        return Err(CompileError::new(
            span,
            "Reference assignment source must be a variable or array/property element",
        ));
    }
    let source_expr = parse_expr(tokens, pos)?;
    if !matches!(
        source_expr.kind,
        ExprKind::ArrayAccess { .. } | ExprKind::PropertyAccess { .. }
    ) {
        return Err(CompileError::new(
            span,
            "Reference assignment source must be a variable or array/property element",
        ));
    }
    if !ref_source_lvalue_is_side_effect_free(&source_expr) {
        return Err(CompileError::new(
            span,
            "Reference assignment from an array or property element with side effects is not yet supported",
        ));
    }
    expect_semicolon(tokens, pos)?;
    let copy = Stmt::new(
        StmtKind::Assign {
            name: target.clone(),
            value: source_expr.clone(),
        },
        span,
    );
    let alias = Stmt::new(
        StmtKind::RefAssignTarget {
            target: source_expr,
            source: target,
        },
        span,
    );
    Ok(Stmt::new(StmtKind::Synthetic(vec![copy, alias]), span))
}

/// Converts a lexer `Token` into an `AssignmentOperator` variant.
///
/// Returns `None` for tokens that are not assignment operators.
pub(super) fn assignment_operator(token: &Token) -> Option<AssignmentOperator> {
    match token {
        Token::Assign => Some(AssignmentOperator::Assign),
        Token::PlusAssign => Some(AssignmentOperator::Compound(BinOp::Add)),
        Token::MinusAssign => Some(AssignmentOperator::Compound(BinOp::Sub)),
        Token::StarAssign => Some(AssignmentOperator::Compound(BinOp::Mul)),
        Token::StarStarAssign => Some(AssignmentOperator::Compound(BinOp::Pow)),
        Token::SlashAssign => Some(AssignmentOperator::Compound(BinOp::Div)),
        Token::PercentAssign => Some(AssignmentOperator::Compound(BinOp::Mod)),
        Token::DotAssign => Some(AssignmentOperator::Compound(BinOp::Concat)),
        Token::AmpAssign => Some(AssignmentOperator::Compound(BinOp::BitAnd)),
        Token::PipeAssign => Some(AssignmentOperator::Compound(BinOp::BitOr)),
        Token::CaretAssign => Some(AssignmentOperator::Compound(BinOp::BitXor)),
        Token::LessLessAssign => Some(AssignmentOperator::Compound(BinOp::ShiftLeft)),
        Token::GreaterGreaterAssign => Some(AssignmentOperator::Compound(BinOp::ShiftRight)),
        Token::QuestionQuestionAssign => Some(AssignmentOperator::NullCoalesce),
        _ => None,
    }
}

/// Builds the value expression for an assignment.
///
/// - Plain `Assign`: returns the RHS unchanged.
/// - `Compound(BinOp)`: wraps `target op= rhs` as a `BinaryOp` node (`target` on the left,
///   `rhs` on the right) so the codegen emits read-modify-write for the target variable.
/// - `NullCoalesce`: wraps as a `NullCoalesce` node with `target` as the value and `rhs` as
///   the default, preserving the short-circuit semantics of `??`.
pub(super) fn assignment_value(
    target: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Expr {
    match op {
        AssignmentOperator::Assign => rhs,
        AssignmentOperator::Compound(op) => Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(target),
                op,
                right: Box::new(rhs),
            },
            span,
        ),
        AssignmentOperator::NullCoalesce => Expr::new(
            ExprKind::NullCoalesce {
                value: Box::new(target),
                default: Box::new(rhs),
            },
            span,
        ),
    }
}
