//! Purpose:
//! Parses local variable statement forms beyond ordinary assignment.
//! Handles increment/decrement statements, global declarations, static locals, and typed assignments.
//!
//! Called from:
//! - `crate::parser::stmt::assign::simple::parse_variable_stmt()` and statement dispatch.
//!
//! Key details:
//! - Typed local syntax is a parser-level distinction that later passes use for declaration semantics.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::parse_assignment_value_expr;
use crate::span::Span;

use super::super::params::parse_type_expr;
use super::super::{expect_semicolon, expect_token};

/// Handle ++$var; or --$var; as standalone statements.
pub(in crate::parser::stmt) fn parse_incdec_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let is_increment = tokens[*pos].0 == Token::PlusPlus;
    *pos += 1;

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

/// Parses a `global $var, ...;` declaration statement.
/// Consumes the `global` keyword, then collects a comma-separated list of variable names
/// until a semicolon. Returns a `StmtKind::Global` node.
pub(in crate::parser::stmt) fn parse_global(
    tokens: &[(Token, Span)],
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

/// Parses a `static` declaration statement, e.g. `static $a = 1, $b, $c = f();`.
///
/// Each variable may carry an `= expr` initializer; an omitted initializer defaults to `null`,
/// matching PHP (`static $x;` is equivalent to `static $x = null;`). Multiple comma-separated
/// variables produce one `StmtKind::StaticVar` per name, wrapped in a `Synthetic` block when there
/// is more than one so the single-statement callers stay unchanged.
pub(in crate::parser::stmt) fn parse_static_var(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'static'

    let mut declarations = Vec::new();
    loop {
        let var_span = tokens.get(*pos).map(|(_, s)| *s).unwrap_or(span);
        let name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => n.clone(),
            _ => return Err(CompileError::new(span, "Expected variable after 'static'")),
        };
        *pos += 1;

        // An explicit `= expr` initializer is optional; a bare `static $x;` defaults to null.
        let init = if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Assign)) {
            *pos += 1; // consume '='
            parse_assignment_value_expr(tokens, pos)?
        } else {
            Expr::new(ExprKind::Null, var_span)
        };

        declarations.push(Stmt::new(StmtKind::StaticVar { name, init }, var_span));

        // Continue the comma-separated list, otherwise require the terminating semicolon.
        if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Comma)) {
            *pos += 1; // consume ','
            continue;
        }
        expect_semicolon(tokens, pos)?;
        break;
    }

    if declarations.len() == 1 {
        Ok(declarations.pop().expect("one declaration present"))
    } else {
        Ok(Stmt::new(StmtKind::Synthetic(declarations), span))
    }
}

/// Returns true if the token sequence at `pos` looks like a typed local assignment:
/// a type expression followed by a variable name. Performs a lookahead parse of the type
/// expression only; does not consume any tokens.
pub(in crate::parser::stmt) fn looks_like_typed_assign(tokens: &[(Token, Span)], pos: usize) -> bool {
    let mut probe = pos;
    match parse_type_expr(tokens, &mut probe, tokens[pos].1) {
        Ok(_) => matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Variable(_))),
        Err(_) => false,
    }
}

/// Parses a typed local assignment: `Type $var = expr;`
/// Consumes a type expression, a variable name, the `=` token, and an initializer expression.
/// Returns a `StmtKind::TypedAssign` node.
pub(in crate::parser::stmt) fn parse_typed_assign(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
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
