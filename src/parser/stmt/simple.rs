//! Purpose:
//! Parses simple statement forms with minimal nested structure.
//! Handles includes, echo, expression statements, returns, throws, `$this` statements, and constants.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()`.
//!
//! Key details:
//! - Include statements preserve their path expression for resolver include discovery and loading.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::parser::expr::{parse_assignment_value_expr, parse_expr};
use crate::span::Span;

use super::assign::{try_parse_postfix_assignment, try_parse_postfix_incdec};
use super::{expect_semicolon, expect_token};

/// Parses `include`/`require` (with optional `_once`) statements.
///
/// Consumes the keyword, optionally the parentheses, then the path expression.
/// Produces a `StmtKind::Include` with `once` and `required` flags set.
/// The path expression is preserved for resolver include discovery.
pub(super) fn parse_include(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    once: bool,
    required: bool,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume include/require keyword

    // Support both: include 'file.php'; and include('file.php');
    let has_parens = *pos < tokens.len() && tokens[*pos].0 == Token::LParen;
    if has_parens {
        *pos += 1;
    }

    let path = parse_expr(tokens, pos)?;

    if has_parens {
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(span, "Expected ')' after include path"));
        }
        *pos += 1;
    }

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(
        StmtKind::Include {
            path,
            once,
            required,
        },
        span,
    ))
}

/// If the token at `*pos` begins an `include`/`require` (optionally `_once`), parses it as a
/// value-position include and returns an expression evaluating to the include's value, without
/// consuming a trailing semicolon. Returns `Ok(None)` when the next token is not an include
/// keyword.
///
/// PHP allows `include`/`require` in expression position (e.g. `return require X;` or
/// `$x = require X;`) and the expression evaluates to the included file's `return` value, or `1`
/// when the file has no explicit `return`. The included file runs in the *calling* scope, so it
/// can read and write the caller's variables. elephc represents this with a transient
/// `ExprKind::IncludeValue` marker that the resolver expands by inlining the included file's
/// statements into the caller's statement list (sharing scope) and capturing its top-level
/// `return` into a hidden temporary.
pub(in crate::parser::stmt) fn try_parse_value_include(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<Option<Expr>, CompileError> {
    let (once, required) = match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Include) => (false, false),
        Some(Token::IncludeOnce) => (true, false),
        Some(Token::Require) => (false, true),
        Some(Token::RequireOnce) => (true, true),
        _ => return Ok(None),
    };
    let span = tokens[*pos].1.span;
    *pos += 1; // consume the include/require keyword

    let has_parens = *pos < tokens.len() && tokens[*pos].0 == Token::LParen;
    if has_parens {
        *pos += 1;
    }
    let path = parse_expr(tokens, pos)?;
    if has_parens {
        expect_token(tokens, pos, &Token::RParen, "Expected ')' after include path")?;
    }

    Ok(Some(Expr::new(
        ExprKind::IncludeValue {
            path: Box::new(path),
            once,
            required,
        },
        span,
    )))
}

/// Parses `echo` statements with one or more comma-separated expressions.
///
/// Consumes `echo`, then parses expressions until a non-comma token is encountered.
/// Returns a single `Echo` statement for one expression, or a `Synthetic` wrapper
/// containing multiple `Echo` statements for PHP's multi-argument echo syntax.
pub(super) fn parse_echo(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let mut echoed = Vec::new();

    loop {
        let expr = parse_expr(tokens, pos)?;
        echoed.push(Stmt::new(StmtKind::Echo(expr), span));
        if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Comma)) {
            break;
        }
        *pos += 1;
    }

    expect_semicolon(tokens, pos)?;
    // Preserve the single-expression AST shape and reuse the existing
    // statement sequence wrapper for PHP's multi-argument echo syntax.
    if echoed.len() == 1 {
        Ok(echoed.remove(0))
    } else {
        Ok(Stmt::new(StmtKind::Synthetic(echoed), span))
    }
}

/// Parses a bare expression statement: an expression followed by a semicolon.
///
/// Parses the expression, consumes the trailing semicolon, and wraps it in
/// `StmtKind::ExprStmt`.
pub(super) fn parse_expr_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
}

/// Parses a statement prefixed with `@` error suppression.
///
/// Checks the next token to detect an include/require variant; if found,
/// delegates to `parse_include` with error-suppression semantics. Otherwise,
/// falls back to `parse_expr_stmt`.
pub(super) fn parse_error_suppressed_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    match tokens.get(*pos + 1).map(|(token, _)| token) {
        Some(Token::Include) => {
            *pos += 1;
            parse_include(tokens, pos, span, false, false)
        }
        Some(Token::IncludeOnce) => {
            *pos += 1;
            parse_include(tokens, pos, span, true, false)
        }
        Some(Token::Require) => {
            *pos += 1;
            parse_include(tokens, pos, span, false, true)
        }
        Some(Token::RequireOnce) => {
            *pos += 1;
            parse_include(tokens, pos, span, true, true)
        }
        _ => parse_expr_stmt(tokens, pos, span),
    }
}

/// Parses a `return` statement, with or without a value.
///
/// Consumes `return`. If the next token is a semicolon, returns `Return(None)`.
/// Otherwise parses an expression and consumes the trailing semicolon.
pub(super) fn parse_return(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        return Ok(Stmt::new(StmtKind::Return(None), span));
    }

    // `return require X;` returns the included file's value (its top-level `return`, or `1`).
    if let Some(include_value) = try_parse_value_include(tokens, pos)? {
        expect_semicolon(tokens, pos)?;
        return Ok(Stmt::new(StmtKind::Return(Some(include_value)), span));
    }

    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Return(Some(expr)), span))
}

/// Parses a `throw` statement: consumes `throw`, then the exception expression.
///
/// Consumes the trailing semicolon and wraps the expression in `StmtKind::Throw`.
pub(super) fn parse_throw(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;
    let expr = parse_expr(tokens, pos)?;
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::Throw(expr), span))
}

/// Parses statements starting with `$this`.
///
/// First attempts to parse a postfix assignment via `try_parse_postfix_assignment`.
/// If that yields nothing, parses the expression. If the next token after the
/// expression is `=` and the expression is a property access, produces a
/// `StmtKind::PropertyAssign`; otherwise returns an `ExprStmt`. Consumes the
/// trailing semicolon in all cases.
pub(super) fn parse_this_stmt(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    if let Some(stmt) = try_parse_postfix_assignment(tokens, pos, span)? {
        return Ok(stmt);
    }
    // `$this->n++` and `$this->arr[0]++` are read-modify-write statements, exactly like
    // `$obj->n++`; without this the `++` would be left for `expect_semicolon` to reject.
    if let Some(stmt) = try_parse_postfix_incdec(tokens, pos, span)? {
        return Ok(stmt);
    }

    // Parse as expression first
    let expr = parse_expr(tokens, pos)?;
    // Check if followed by assignment
    if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
        *pos += 1;
        let value = parse_assignment_value_expr(tokens, pos)?;
        expect_semicolon(tokens, pos)?;
        if let ExprKind::PropertyAccess { object, property } = expr.kind {
            return Ok(Stmt::new(
                StmtKind::PropertyAssign {
                    object,
                    property,
                    value,
                },
                span,
            ));
        }
        return Err(CompileError::new(
            span,
            "Invalid assignment target after $this",
        ));
    }
    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(StmtKind::ExprStmt(expr), span))
}

/// Parses a `const` declaration: `const NAME = value;` or a declarator list
/// `const A = 1, B = 2;`.
///
/// Consumes `const`, then for each declarator an identifier for the name, `=`, and the
/// value expression, and consumes the trailing semicolon. Returns a `StmtKind::ConstDecl`
/// for a single declarator, or a `StmtKind::Synthetic` holding one `ConstDecl` per name for a
/// list.
pub(super) fn parse_const_decl(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'const'

    // One statement may declare SEVERAL constants, `const A = 1, B = A + 1;`, exactly like a
    // class body (issue #1142). Each declarator becomes its own `ConstDecl`, in source order, so
    // the list behaves like one statement per name: a later value can read an earlier name. The
    // first declaration keeps the statement's span, as a lone `const` always did; each further
    // one carries the span of its own declarator (`B = A + 1`).
    let mut declarations = Vec::new();
    loop {
        let name_span = tokens.get(*pos).map_or(span, |(_, metadata)| metadata.span);
        let name = match tokens.get(*pos) {
            Some((Token::Identifier(n), _)) => n.clone(),
            // `NAN`, `INF`, `PHP_EOL` and the other predefined constants the lexer turns into
            // dedicated tokens are still legal names for a namespaced user constant (`Demo\NAN`).
            Some((token, metadata))
                if super::namespace_use::token_as_import_name(token, metadata).is_some() =>
            {
                super::namespace_use::token_as_import_name(token, metadata)
                    .expect("constant-like token was checked immediately above")
            }
            _ if declarations.is_empty() => {
                return Err(CompileError::new(
                    span,
                    "Expected constant name after 'const'",
                ))
            }
            _ => {
                return Err(CompileError::new(
                    name_span,
                    "Expected a constant name after ',' in the declaration list",
                ))
            }
        };
        *pos += 1;

        expect_token(
            tokens,
            pos,
            &Token::Assign,
            "Expected '=' after constant name",
        )?;

        let value = parse_expr(tokens, pos)?;
        let declaration_span = if declarations.is_empty() {
            span
        } else {
            crate::parser::expr::span_through_prev_token(tokens, *pos, name_span)
        };
        declarations.push(Stmt::new(StmtKind::ConstDecl { name, value }, declaration_span));
        if matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Comma)) {
            *pos += 1;
            continue;
        }
        expect_semicolon(tokens, pos)?;
        break;
    }

    // A single declarator is the statement itself. A list is handed back as one `Synthetic`
    // holding a `ConstDecl` per name; the file- and namespace-scope statement loops splice it
    // back into the enclosing list (`crate::parser::stmt::push_parsed_stmt`), so every later
    // pass sees the flat shape separate `const` statements produce.
    if declarations.len() == 1 {
        return Ok(declarations.pop().expect("one declaration was just checked"));
    }
    Ok(Stmt::new(StmtKind::Synthetic(declarations), span))
}
