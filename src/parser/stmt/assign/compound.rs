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
use crate::lexer::{SpannedToken, Token};
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
    /// An internal incdec update, distinct from numeric compound arithmetic.
    IncDec(bool),
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
    tokens: &[SpannedToken],
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
/// PHP spells reference aliasing as `$target =& $source;`. This parser accepts
/// direct variable sources and leaves broader lvalue reference targets for
/// future storage-specific lowering.
fn parse_ref_assign(
    tokens: &[SpannedToken],
    pos: &mut usize,
    target: String,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    if let Some(stmt) = try_parse_reference_append_source(tokens, pos, &target, span)? {
        return Ok(stmt);
    }
    let source = parse_expr(tokens, pos)?;
    if !is_valid_reference_source(&source.kind) {
        return Err(CompileError::new(
            span,
            "Reference assignment source must be a variable, array/property element, or a by-reference call",
        ));
    }
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::RefAssign { target, source }, span))
}

/// Parses `$target =& $array[]` by sharing a fresh temporary cell with the appended slot.
///
/// The temporary avoids evaluating the append target twice and avoids mutating any cell that
/// `target` might have referenced before it is rebound. Existing append-by-reference lowering
/// then owns container growth, nested write-back, copy-on-write, and target-specific storage.
fn try_parse_reference_append_source(
    tokens: &[SpannedToken],
    pos: &mut usize,
    target: &str,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let source_start = *pos;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 0usize;
    let mut brace_depth = 0usize;
    let mut cursor = source_start;
    let semicolon = loop {
        let Some((token, _)) = tokens.get(cursor) else {
            return Ok(None);
        };
        match token {
            Token::LParen => paren_depth += 1,
            Token::RParen => paren_depth = paren_depth.saturating_sub(1),
            Token::LBracket => bracket_depth += 1,
            Token::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            Token::Semicolon if paren_depth == 0 && bracket_depth == 0 && brace_depth == 0 => {
                break cursor;
            }
            _ => {}
        }
        cursor += 1;
    };
    if semicolon < source_start + 2
        || tokens[semicolon - 2].0 != Token::LBracket
        || tokens[semicolon - 1].0 != Token::RBracket
    {
        return Ok(None);
    }

    let source_tokens = &tokens[source_start..semicolon - 2];
    let mut source_pos = 0usize;
    let append_target = parse_expr(source_tokens, &mut source_pos)?;
    if source_pos != source_tokens.len() {
        return Err(CompileError::new(span, "Invalid reference append target"));
    }

    let temp = format!(
        "__elephc_ref_append_{}_{}_{}",
        span.line, span.col, source_start
    );
    let temp_expr = Expr::new(ExprKind::Variable(temp.clone()), span);
    // EVERY EXECUTION MUST GET A FRESH REFERENCE, NOT A FRESH NAME. The temp is named after its
    // SOURCE POSITION, so a `$x = &$arr[];` inside a loop reuses one local — and `$temp = null`
    // on an already-reference-bound local writes THROUGH the existing cell instead of rebinding.
    // Every element appended by that loop then aliased ONE cell, so the array ended up holding
    // the last iteration's value in every slot:
    //
    //     foreach ([1, 2, 3] as $i) { $c = &$opt[]; $c = $i; }   // [3, 3, 3], not [1, 2, 3]
    //
    // `unset()` is what breaks a reference binding in PHP and in this compiler alike: the IR's
    // `UnsetLocal` drops the slot from the ref-cell dataflow state, so the NEXT execution's
    // promotion allocates a new cell.
    //
    // IT HAS TO COME LAST, after the bind, not first. Lowering walks this block ONCE, and
    // `unset_local` only emits `Op::UnsetLocal` for a name that is ref-bound AT LOWERING TIME —
    // before the append, the temp is not bound yet, so a leading `unset` lowered to nothing and
    // the loop still shared one cell. After the bind it is bound, the op is emitted, and the
    // target keeps the cell because the alias took its own reference on it.
    //
    // Symfony's `EventDispatcher::optimizeListeners` is why this matters: it builds
    // `$closure = &$this->optimized[$eventName][]` once per listener, so a shared cell left every
    // event dispatching its LAST listener over and over — which 404s every route in a Symfony app,
    // because `RouterListener` is never the last one.
    let unbind = Stmt::new(
        StmtKind::ExprStmt(Expr::new(
            ExprKind::FunctionCall {
                name: crate::names::Name::unqualified("unset"),
                args: vec![temp_expr.clone()],
            },
            span,
        )),
        span,
    );
    let init = Stmt::new(
        StmtKind::Assign {
            name: temp.clone(),
            value: Expr::new(ExprKind::Null, span),
        },
        span,
    );
    let append = super::postfix::assignment_target_append_stmt(
        append_target,
        Expr::new(ExprKind::ArrayReference(Box::new(temp_expr.clone())), span),
        span,
    )?;
    let bind = Stmt::new(
        StmtKind::RefAssign {
            target: target.to_string(),
            source: temp_expr,
        },
        span,
    );
    *pos = semicolon + 1;
    Ok(Some(Stmt::new(
        StmtKind::Synthetic(vec![init, append, bind, unbind]),
        span,
    )))
}

/// Returns true when an expression is a legal source for `$x = &<source>`.
///
/// PHP allows aliasing variables, array elements, object properties, and the
/// results of calls that return by reference. Other expressions are rejected.
pub(crate) fn is_valid_reference_source(kind: &ExprKind) -> bool {
    matches!(
        kind,
        ExprKind::Variable(_)
            | ExprKind::ArrayAccess { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::FunctionCall { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::StaticMethodCall { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. }
    )
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
/// - `IncDec`: captures the element once and applies ordinary pre-incdec to that local.
pub(super) fn assignment_value(
    target: Expr,
    op: AssignmentOperator,
    rhs: Expr,
    span: Span,
) -> Expr {
    match op {
        AssignmentOperator::Assign => rhs,
        AssignmentOperator::IncDec(increment) => {
            let old = crate::names::generated_local_name(&format!(
                "__elephc_element_incdec_old_{}_{}", span.line, span.col));
            let new = crate::names::generated_local_name(&format!(
                "__elephc_element_incdec_new_{}_{}", span.line, span.col));
            let operation = if increment { ExprKind::PreIncrement(old.clone()) }
                else { ExprKind::PreDecrement(old.clone()) };
            Expr::new(ExprKind::Assignment {
                target: Box::new(Expr::new(ExprKind::Variable(new), span)),
                value: Box::new(Expr::new(operation, span)),
                result_target: None,
                prelude: vec![Stmt::new(StmtKind::Assign { name: old, value: target }, span)],
                conditional_value_temp: None,
            }, span)
        }
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
