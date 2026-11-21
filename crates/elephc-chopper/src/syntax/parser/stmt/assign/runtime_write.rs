//! Purpose:
//! Retains statement-position property and dimension writes in the shared AST.
//!
//! Called from:
//! - Runtime fragment statement dispatch.
//!
//! Key details:
//! - Reuses the ordinary expression parser for receivers, computed names and dimensions.
//! - Leaves the operands intact so the consumer can lower the operation.

use crate::syntax::{
    errors::CompileError,
    lexer::{SpannedToken, Token},
    parser::{
        ast::{Expr, ExprKind, RuntimeClassMember, RuntimeExpr, RuntimeWrite, Stmt, StmtKind},
        expr::parse_expr,
    },
    span::Span,
};

use super::{
    super::expect_semicolon,
    compound::AssignmentOperator,
    postfix::{find_top_level_assignment, find_top_level_postfix_incdec},
};

/// Reads a write before compilation-specific target capture and update rewriting.
pub(in crate::syntax::parser::stmt) fn try_parse_runtime_write(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<Stmt>, CompileError> {
    let start = *pos;
    match tokens.get(start).map(|(token, _)| token) {
        Some(Token::PlusPlus | Token::MinusMinus) => {
            let Some(end) = tokens.iter().enumerate().skip(start + 1)
                .find_map(|(index, (token, _))| (*token == Token::Semicolon).then_some(index))
            else {
                return Ok(None);
            };
            let Some(target) = parse_target(&tokens[start + 1..end], false) else { return Ok(None) };
            *pos = end + 1;
            let increment = tokens[start].0 == Token::PlusPlus;
            return Ok(Some(write_stmt(target, RuntimeWrite::IncDec { increment, prefix: true }, span)));
        }
        Some(Token::Variable(_) | Token::This | Token::Identifier(_) | Token::Self_ | Token::Parent
            | Token::Static | Token::Backslash | Token::LParen) => {}
        _ => return Ok(None),
    }

    if let Some((end, op)) = find_top_level_assignment(tokens, start) {
        let lhs = &tokens[start..end];
        let append = matches!(
            lhs, [.., (Token::LBracket, _), (Token::RBracket, _)]
        );
        let target_tokens = if append { &lhs[..lhs.len() - 2] } else { lhs };
        let Some(target) = parse_target(target_tokens, append) else { return Ok(None) };
        let mut cursor = end + 1;
        let operation = match (op, append) {
            (AssignmentOperator::Assign, true) => RuntimeWrite::Append(parse_expr(tokens, &mut cursor)?),
            (AssignmentOperator::Assign, false) if matches!(tokens.get(cursor), Some((Token::Ampersand, _))) => {
                let Some((Token::Variable(source), _)) = tokens.get(cursor + 1) else {
                    return Err(CompileError::new(span, "Expected reference variable"));
                };
                cursor += 2;
                RuntimeWrite::BindReference(source.clone())
            }
            (AssignmentOperator::Assign, false) => RuntimeWrite::Assign {
                op: None,
                value: parse_expr(tokens, &mut cursor)?,
            },
            (AssignmentOperator::Compound(op), false) => {
                let value = if matches!(target.kind, ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }) {
                    let value = crate::syntax::parser::expr::parse_assignment_value_expr(tokens, &mut cursor)?;
                    if matches!(tokens.get(cursor), Some((Token::And | Token::Or | Token::Xor, _))) {
                        return Ok(None);
                    }
                    value
                } else {
                    parse_expr(tokens, &mut cursor)?
                };
                RuntimeWrite::Assign { op: Some(op), value }
            }
            _ => return Ok(None),
        };
        expect_semicolon(tokens, &mut cursor)?;
        *pos = cursor;
        return Ok(Some(write_stmt(target, operation, span)));
    }

    let Some((end, increment)) = find_top_level_postfix_incdec(tokens, start) else {
        return Ok(None);
    };
    if !matches!(tokens.get(end + 1), Some((Token::Semicolon, _))) {
        return Ok(None);
    }
    let Some(target) = parse_target(&tokens[start..end], false) else { return Ok(None) };
    *pos = end + 2;
    Ok(Some(write_stmt(target, RuntimeWrite::IncDec { increment, prefix: false }, span)))
}

/// Accepts only complete writable member and dimension expressions.
fn parse_target(tokens: &[SpannedToken], append: bool) -> Option<Expr> {
    if !tokens.iter().any(|(token, _)| matches!(token, Token::Arrow | Token::DoubleColon | Token::LBracket))
        && !matches!((tokens, append), ([(Token::Variable(_), _)], true))
    {
        return None;
    }
    let mut cursor = 0;
    let target = parse_expr(tokens, &mut cursor).ok()?;
    let writable = match &target.kind {
        ExprKind::Variable(_) => append,
        ExprKind::PropertyAccess { .. } | ExprKind::DynamicPropertyAccess { .. }
        | ExprKind::StaticPropertyAccess { .. } | ExprKind::ArrayAccess { .. } => true,
        ExprKind::Runtime(expression) => matches!(expression.as_ref(), RuntimeExpr::StaticMember {
            member: RuntimeClassMember::Property(_) | RuntimeClassMember::ComputedProperty(_),
            ..
        }),
        _ => false,
    };
    (writable && cursor == tokens.len()).then_some(target)
}

/// Packages the source operands without allocating compilation temporaries.
fn write_stmt(target: Expr, operation: RuntimeWrite, span: Span) -> Stmt {
    let write = Expr::new(ExprKind::Runtime(Box::new(RuntimeExpr::Write { target, operation })), span);
    Stmt::new(StmtKind::ExprStmt(write), span)
}
