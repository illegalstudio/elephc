//! Purpose:
//! Parses function parameters, return types, and reusable parsed type expressions.
//! Handles typed parameters, defaults, by-reference markers, variadics, and name lists.
//!
//! Called from:
//! - `crate::parser::stmt`, `crate::parser::control`, and closure/OOP parsers.
//!
//! Key details:
//! - Type-name parsing must allow namespace-qualified PHP names without resolving them here.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::Name;
use crate::parser::ast::{Expr, Stmt, StmtKind, TypeExpr};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::{expect_token, name_starts_at, parse_block, parse_name};

/// Parses a `function` declaration: name, parameters, optional return type, and body.
/// Consumes the `function` keyword at `*pos` and advances past the closing `}` of the body.
/// Returns `StmtKind::FunctionDecl` with params, variadic, return_type, and body.
pub(super) fn parse_function_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected function name")),
    };
    *pos += 1;

    expect_token(
        tokens,
        pos,
        &Token::LParen,
        "Expected '(' after function name",
    )?;
    let (params, variadic) = parse_params(tokens, pos, span)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after parameters")?;

    // Parse optional return type: `: TypeExpr`
    let return_type = if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        Some(parse_type_expr(tokens, pos, span)?)
    } else {
        None
    };

    let body = parse_block(tokens, pos)?;

    Ok(Stmt::new(
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        },
        span,
    ))
}

/// Returns `true` if the token stream at `pos` begins with a type expression that could
/// be a parameter type annotation, `false` otherwise.
/// Checks for nullable/union types, pointer/buffer generics, and that the token sequence
/// ultimately resolves to a variable token (possibly after `&` or `...` markers).
pub(crate) fn looks_like_typed_param(tokens: &[(Token, Span)], pos: usize) -> bool {
    let mut probe = pos;
    match parse_type_expr(tokens, &mut probe, tokens[pos].1) {
        Ok(_) => {
            if matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Ampersand)) {
                probe += 1;
            }
            if matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Ellipsis)) {
                probe += 1;
            }
            matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Variable(_)))
        }
        Err(_) => false,
    }
}

/// Parses a type expression: atomic type, nullable shorthand, or union of pipe-separated types.
/// Advances `*pos` past the consumed type tokens. Returns `TypeExpr::Atomic`, `Nullable`,
/// `Union`, `Ptr`, or `Buffer`. Does not resolve names — emits `TypeExpr::Named` with a
/// `Name` for class/interface/enum types.
pub(crate) fn parse_type_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    let mut ty = if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Question)) {
        *pos += 1;
        TypeExpr::Nullable(Box::new(parse_atomic_type_expr(tokens, pos, span)?))
    } else {
        parse_atomic_type_expr(tokens, pos, span)?
    };

    if matches!(ty, TypeExpr::Nullable(_))
        && matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Pipe))
    {
        return Err(CompileError::new(
            span,
            "Nullable shorthand cannot be combined directly with union types; write T|null",
        ));
    }

    let mut members = vec![ty];
    while matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Pipe)) {
        *pos += 1;
        members.push(parse_atomic_type_expr(tokens, pos, span)?);
    }

    if members.len() == 1 {
        ty = members.pop().expect("type member exists");
        Ok(ty)
    } else {
        Ok(TypeExpr::Union(members))
    }
}

/// Parses a single (non-union) type expression: builtin keyword, `ptr<T>`, `buffer<T>`,
/// or a qualified/unqualified name. Does not handle `?T` (nullable) — that is handled by
/// the caller `parse_type_expr`. Advances `*pos` past the consumed token(s).
fn parse_atomic_type_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(name)) if ident_matches(name, &["int", "integer"]) => {
            *pos += 1;
            Ok(TypeExpr::Int)
        }
        Some(Token::Identifier(name)) if ident_matches(name, &["float", "double", "real"]) => {
            *pos += 1;
            Ok(TypeExpr::Float)
        }
        Some(Token::Identifier(name)) if ident_matches(name, &["bool", "boolean"]) => {
            *pos += 1;
            Ok(TypeExpr::Bool)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("string") => {
            *pos += 1;
            Ok(TypeExpr::Str)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("void") => {
            *pos += 1;
            Ok(TypeExpr::Void)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("never") => {
            *pos += 1;
            Ok(TypeExpr::Never)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("iterable") => {
            *pos += 1;
            Ok(TypeExpr::Iterable)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("array") => {
            *pos += 1;
            Ok(TypeExpr::Named(crate::names::Name::unqualified("array")))
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("mixed") => {
            *pos += 1;
            Ok(TypeExpr::Named(crate::names::Name::unqualified("mixed")))
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("callable") => {
            *pos += 1;
            Ok(TypeExpr::Named(crate::names::Name::unqualified("callable")))
        }
        Some(Token::Identifier(name)) if matches!(name.as_str(), "ptr" | "pointer") => {
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == Token::Less {
                *pos += 1;
                let target = parse_name(
                    tokens,
                    pos,
                    span,
                    "Expected pointer target type inside ptr<...>",
                )?;
                expect_token(
                    tokens,
                    pos,
                    &Token::Greater,
                    "Expected '>' after ptr target type",
                )?;
                Ok(TypeExpr::Ptr(Some(target)))
            } else {
                Ok(TypeExpr::Ptr(None))
            }
        }
        Some(Token::Identifier(name)) if name == "buffer" => {
            *pos += 1;
            expect_token(tokens, pos, &Token::Less, "Expected '<' after buffer")?;
            let inner = parse_type_expr(tokens, pos, span)?;
            expect_token(
                tokens,
                pos,
                &Token::Greater,
                "Expected '>' after buffer element type",
            )?;
            Ok(TypeExpr::Buffer(Box::new(inner)))
        }
        Some(Token::Identifier(_)) | Some(Token::Backslash) => Ok(TypeExpr::Named(parse_name(
            tokens,
            pos,
            span,
            "Expected type name",
        )?)),
        _ => Err(CompileError::new(span, "Expected type expression")),
    }
}

/// Returns `true` if `name` matches any of the `keywords` case-insensitively.
fn ident_matches(name: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .any(|keyword| name.eq_ignore_ascii_case(keyword))
}

/// Parses a parenthesized parameter list (not including the surrounding `(` and `)`).
/// Handles typed parameters, defaults, `&` by-reference markers, `...` variadic markers,
/// and PHP 8.0 `#[...]` attributes. Returns a vec of `(name, type, default, is_ref)` tuples
/// and an optional variadic parameter name. Advances `*pos` to the token after `)`.
/// Errors if a variadic parameter appears after another parameter or if a typed variadic
/// is present.
pub(super) fn parse_params(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<
    (
        Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
        Option<String>,
    ),
    CompileError,
> {
    let mut params = Vec::new();
    let mut variadic = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() || variadic.is_some() {
            expect_token(
                tokens,
                pos,
                &Token::Comma,
                "Expected ',' between parameters",
            )?;
        }
        // PHP 8.0 parameter attributes (`function f(#[Sensitive] $s)`).
        crate::parser::consume_attribute_lists(tokens, pos)?;
        if variadic.is_some() {
            return Err(CompileError::new(
                span,
                "Variadic parameter must be the last parameter",
            ));
        }
        // Try to parse optional type annotation before $variable
        let type_ann = if looks_like_typed_param(tokens, *pos) {
            Some(parse_type_expr(tokens, pos, span)?)
        } else {
            None
        };
        let is_ref = if *pos < tokens.len() && tokens[*pos].0 == Token::Ampersand {
            *pos += 1;
            true
        } else {
            false
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            if type_ann.is_some() {
                return Err(CompileError::new(
                    span,
                    "Typed variadic parameters are not supported yet",
                ));
            }
            *pos += 1;
            match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Variable(n)) => {
                    variadic = Some(n.clone());
                    *pos += 1;
                }
                _ => return Err(CompileError::new(span, "Expected variable after '...'")),
            }
            continue;
        }
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(n)) => {
                let n = n.clone();
                *pos += 1;
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                params.push((n, type_ann, default, is_ref));
            }
            _ => return Err(CompileError::new(span, "Expected parameter variable")),
        }
    }
    Ok((params, variadic))
}

/// Parses a comma-separated list of `Name`s until a token that does not start a name is
/// seen. `first_error` is used when the list is empty; a more specific error is used when
/// a comma is found but no name follows. Advances `*pos` to the first non-name token.
pub(super) fn parse_name_list(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    first_error: &str,
) -> Result<Vec<Name>, CompileError> {
    let mut names = Vec::new();
    loop {
        if !name_starts_at(tokens, *pos) {
            if names.is_empty() {
                return Err(CompileError::new(span, first_error));
            }
            return Err(CompileError::new(
                span,
                "Expected name after ',' in declaration list",
            ));
        }
        names.push(parse_name(tokens, pos, span, first_error)?);

        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
            continue;
        }
        break;
    }
    Ok(names)
}
