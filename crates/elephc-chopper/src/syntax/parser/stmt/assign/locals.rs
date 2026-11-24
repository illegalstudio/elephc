//! Purpose:
//! Parses local variable statement forms beyond ordinary assignment.
//! Handles increment/decrement statements, global declarations, static locals, and typed assignments.
//!
//! Called from:
//! - `crate::syntax::parser::stmt::assign::simple::parse_variable_stmt()` and statement dispatch.
//!
//! Key details:
//! - Typed local syntax is a parser-level distinction that later passes use for declaration semantics.

use crate::syntax::parser::validation_only;
use crate::syntax::errors::CompileError;
use crate::syntax::lexer::{SpannedToken, Token};
use crate::syntax::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::syntax::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::syntax::span::Span;

use super::super::params::parse_type_expr;
use super::super::{expect_semicolon, expect_token};

/// Handles discarded prefix increments/decrements on locals, scoped properties and array places.
/// Complex storage shares the postfix read-modify-write lowering and index snapshots.
#[allow(unreachable_code)]
pub(in crate::syntax::parser::stmt) fn parse_incdec_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    // Validation parses the whole prefix-inc/dec expression from the operator so an
    // assignment to its result (`++$a = 1`) is rejected by the Pratt target check.
    if validation_only() {
        let expr = crate::syntax::parser::expr::parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::ExprStmt(expr), span));
    }
    if tokens[*pos..].windows(2)
        .take_while(|pair| pair[0].0 != Token::Semicolon)
        .any(|pair| pair[0].0 == Token::LBracket && pair[1].0 == Token::RBracket)
    {
        let value = crate::syntax::parser::expr::parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::ExprStmt(value), span));
    }
    let is_increment = tokens[*pos].0 == Token::PlusPlus;
    *pos += 1;

    let next_token = tokens.get(*pos).map(|(t, _)| t);
    let is_scoped = matches!(
        next_token,
        Some(Token::Identifier(_)) | Some(Token::Self_) | Some(Token::Parent) | Some(Token::Static)
    ) && tokens.get(*pos + 1).map(|(t, _)| t) == Some(&Token::DoubleColon)
        && tokens.get(*pos + 2).map(|(t, _)| t).is_some_and(|t| matches!(t, Token::Variable(_)));

    if is_scoped {
        let lhs_expr = crate::syntax::parser::expr::parse_expr(tokens, pos)
            .map_err(|_| CompileError::new(span, "Expected variable after '++'"))?;
        expect_semicolon(tokens, pos)?;
        return super::postfix::lower_postfix_incdec_assignment(lhs_expr, is_increment, span);
    }

    // `++$this->n;`, `++$obj->n;`, and `++$a[0];` target storage the simple local path
    // cannot name. Statement position discards the operator's value, so they lower
    // through the same read-modify-write shape as their postfix spellings.
    if starts_complex_incdec_target(tokens, *pos) {
        let lhs_expr = crate::syntax::parser::expr::parse_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        return super::postfix::lower_postfix_incdec_assignment(lhs_expr, is_increment, span);
    }

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(n)) => n.clone(),
        _ => {
            let op = if is_increment { "++" } else { "--" };
            return Err(CompileError::new(
                span,
                &format!("Expected variable after '{}'", op),
            ));
        }
    };
    *pos += 1;
    expect_semicolon(tokens, pos)?;

    let kind = if is_increment {
        ExprKind::PreIncrement(name)
    } else {
        ExprKind::PreDecrement(name)
    };
    let expr = Expr::new(kind, span);
    Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
}

/// Returns true when the tokens after a prefix `++`/`--` name a property, array element,
/// or `$this` member rather than a plain local variable.
///
/// `$this` always continues into a member access, and a variable is only a complex target
/// when it is followed by `->`, `?->`, or `[`. Everything else keeps the plain
/// `PreIncrement`/`PreDecrement` local path.
fn starts_complex_incdec_target(tokens: &[SpannedToken], pos: usize) -> bool {
    match tokens.get(pos).map(|(token, _)| token) {
        Some(Token::This | Token::Identifier(_) | Token::Backslash | Token::LParen) => true,
        Some(Token::Variable(_)) => matches!(
            tokens.get(pos + 1).map(|(token, _)| token),
            Some(Token::Arrow) | Some(Token::QuestionArrow) | Some(Token::LBracket)
        ),
        _ => false,
    }
}

/// Parses a `global $var, ...;` declaration statement.
/// Consumes the `global` keyword, then collects a comma-separated list of variable names
/// until a semicolon. Returns a `StmtKind::Global` node.
pub(in crate::syntax::parser::stmt) fn parse_global(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'global'

    let mut vars = Vec::new();
    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                vars.push(n.clone());
                *pos += 1;
            }
            _ => return Err(CompileError::new(span, "Expected variable after 'global'")),
        }
        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
        } else {
            break;
        }
    }

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Global { vars }, span))
}

/// Parses a `static $var = expr, $other;` declaration statement.
///
/// Consumes the `static` keyword, then every comma-separated declarator: a variable name
/// optionally followed by `=` and an initializer expression; a missing initializer desugars to
/// `= null` (PHP treats both forms identically). One declarator returns a `StaticVar` node and
/// several return a `Synthetic` body of them, in source order.
pub(in crate::syntax::parser::stmt) fn parse_static_var(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'static'

    // PHP allows several declarators in one statement: `static $a = 1, $b;`. Each becomes its
    // own `StaticVar`, in source order, and a list is returned as a `Synthetic` body so every
    // later pass sees the same flat shape separate statements produce.
    let mut declarations = Vec::new();
    loop {
        let name_span = tokens.get(*pos).map_or(span, |(_, metadata)| metadata.span);
        let name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => n.clone(),
            _ if declarations.is_empty() => {
                return Err(CompileError::new(span, "Expected variable after 'static'"))
            }
            _ => {
                return Err(CompileError::new(
                    name_span,
                    "Expected a variable after ',' in the static declaration list",
                ))
            }
        };
        *pos += 1;

        let init = if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Assign)) {
            *pos += 1;
            if crate::syntax::parser::runtime_fragment() {
                parse_expr(tokens, pos)?
            } else {
                parse_assignment_value_expr(tokens, pos)?
            }
        } else {
            Expr::new(ExprKind::Null, name_span)
        };
        let declarator_span = if declarations.is_empty() {
            span
        } else {
            crate::syntax::parser::expr::span_through_prev_token(tokens, *pos, name_span)
        };
        declarations.push(Stmt::new(StmtKind::StaticVar { name, init }, declarator_span));

        if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Comma)) {
            if crate::syntax::parser::runtime_fragment() {
                return Err(CompileError::expected_semicolon(tokens[*pos].1.span));
            }
            *pos += 1;
            continue;
        }
        expect_semicolon(tokens, pos)?;
        break;
    }

    if declarations.len() == 1 {
        return Ok(declarations.pop().expect("one declaration was just checked"));
    }
    Ok(Stmt::new(StmtKind::Synthetic(declarations), span))
}

/// Returns true if the token sequence at `pos` looks like a typed local assignment:
/// a type expression followed by a variable name. Performs a lookahead parse of the type
/// expression only; does not consume any tokens.
pub(in crate::syntax::parser::stmt) fn looks_like_typed_assign(tokens: &[SpannedToken], pos: usize) -> bool {
    // A leading `(` is a cast or grouped expression, never a typed local declaration; the
    // type grammar now accepts parenthesized DNF members, which would otherwise swallow
    // `(int) $x` as a type followed by a variable.
    if matches!(tokens.get(pos).map(|(token, _)| token), Some(Token::LParen)) {
        return false;
    }
    let mut probe = pos;
    match crate::syntax::parser::runtime_templates::lookahead(|| parse_type_expr(tokens, &mut probe, tokens[pos].1.span)) {
        Ok(_) => matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Variable(_))),
        Err(_) => false,
    }
}

/// Parses a typed local assignment: `Type $var = expr;`
/// Consumes a type expression, a variable name, the `=` token, and an initializer expression.
/// Returns a `StmtKind::TypedAssign` node.
#[allow(unreachable_code)]
pub(in crate::syntax::parser::stmt) fn parse_typed_assign(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    if validation_only() {
        return Err(CompileError::new(
            span,
            "Typed local declarations are not valid PHP",
        ));
    }
    let type_expr = parse_type_expr(tokens, pos, span)?;
    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Variable(name)) => {
            let name = name.clone();
            *pos += 1;
            name
        }
        _ => {
            return Err(CompileError::new(
                span,
                "Expected variable after type annotation",
            ))
        }
    };
    expect_token(
        tokens,
        pos,
        &Token::Assign,
        "Expected '=' after typed variable",
    )?;
    let value = parse_assignment_value_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        },
        span,
    ))
}
