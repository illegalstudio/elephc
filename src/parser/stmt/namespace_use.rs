//! Purpose:
//! Parses PHP namespace declarations and use import statements.
//! Handles nested namespace bodies, grouped imports, aliases, and use-kind prefixes.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()`.
//!
//! Key details:
//! - Namespace and use syntax remains syntactic here; canonical resolution happens in `crate::name_resolver`.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::{Name, NameKind};
use crate::parser::ast::{Stmt, StmtKind, UseItem, UseKind};
use crate::span::Span;

use super::{expect_semicolon, expect_token, parse_name, parse_stmt, recover_to_statement_boundary};

/// Parses a `namespace` declaration or block.
///
/// Consumes the `namespace` keyword, then either:
/// - Parses a simple `namespace Name;` declaration, or
/// - Parses a `namespace Name { ... }` block containing statements.
///
/// Collects parse errors during block parsing and reports them all if the block
/// closes successfully.
pub(super) fn parse_namespace_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume namespace

    let name = if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        None
    } else {
        Some(parse_name(
            tokens,
            pos,
            span,
            "Expected namespace name after 'namespace'",
        )?)
    };

    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        return Ok(Stmt::new(StmtKind::NamespaceDecl { name }, span));
    }

    expect_token(
        tokens,
        pos,
        &Token::LBrace,
        "Expected ';' or '{' after namespace name",
    )?;
    let mut body = Vec::new();
    let mut errors = Vec::new();
    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        match parse_stmt(tokens, pos) {
            Ok(stmt) => body.push(stmt),
            Err(error) => {
                errors.extend(error.flatten());
                recover_to_statement_boundary(tokens, pos);
            }
        }
    }
    expect_token(
        tokens,
        pos,
        &Token::RBrace,
        "Expected '}' after namespace block",
    )?;
    if !errors.is_empty() {
        return Err(CompileError::from_many(errors));
    }
    Ok(Stmt::new(StmtKind::NamespaceBlock { name, body }, span))
}

/// Parses a `use` import statement.
///
/// Handles `use`, `use function`, and `use const` declarations, including:
/// - Simple single-item imports (`use Foo\Bar;`)
/// - Grouped imports (`use Foo\{Bar, Baz};`)
/// - Multiple comma-separated imports (`use Foo, Bar;`)
/// - Optional `as` aliasing on each imported name.
///
/// Emits a `StmtKind::UseDecl` containing the complete list of `UseItem`s.
pub(super) fn parse_use_stmt(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume use

    let default_kind = if *pos < tokens.len() && tokens[*pos].0 == Token::Function {
        *pos += 1;
        UseKind::Function
    } else if *pos < tokens.len() && tokens[*pos].0 == Token::Const {
        *pos += 1;
        UseKind::Const
    } else {
        UseKind::Class
    };

    let prefix = parse_use_prefix(tokens, pos, span)?;

    let imports = if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        parse_group_use_items(tokens, pos, span, prefix, default_kind.clone())?
    } else {
        vec![parse_single_use_item_after_name(
            tokens,
            pos,
            span,
            prefix,
            default_kind.clone(),
        )?]
    };

    let mut all_imports = imports;
    while *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
        *pos += 1;
        let item_kind = if *pos < tokens.len() && tokens[*pos].0 == Token::Function {
            *pos += 1;
            UseKind::Function
        } else if *pos < tokens.len() && tokens[*pos].0 == Token::Const {
            *pos += 1;
            UseKind::Const
        } else {
            default_kind.clone()
        };
        let name = parse_name(
            tokens,
            pos,
            span,
            "Expected imported name after ',' in use declaration",
        )?;
        all_imports.push(parse_single_use_item_after_name(
            tokens, pos, span, name, item_kind,
        )?);
    }

    expect_semicolon(tokens, pos)?;
    Ok(Stmt::new(
        StmtKind::UseDecl {
            imports: all_imports,
        },
        span,
    ))
}

/// Parses an optional `as Alias` clause after a use item name.
///
/// Returns `None` if no `as` keyword is present. Otherwise consumes `as` and the
/// following identifier, returning the alias name.
fn parse_optional_alias(tokens: &[(Token, Span)], pos: &mut usize) -> Option<String> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::As {
        *pos += 1;
        if let Some(Token::Identifier(alias)) = tokens.get(*pos).map(|(t, _)| t) {
            let alias = alias.clone();
            *pos += 1;
            return Some(alias);
        }
    }
    None
}

/// Builds a `UseItem` from an already-parsed name and kind.
///
/// If an explicit `as Alias` clause follows, that alias is used; otherwise the
/// last segment of `name` becomes the alias. Returns an error if the name is empty.
fn parse_single_use_item_after_name(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    name: Name,
    kind: UseKind,
) -> Result<UseItem, CompileError> {
    let alias = parse_optional_alias(tokens, pos)
        .or_else(|| name.last_segment().map(str::to_string))
        .ok_or_else(|| CompileError::new(span, "Imported name cannot be empty"))?;
    Ok(UseItem { kind, name, alias })
}

/// Parses a grouped use block `Prefix { ... }`.
///
/// Consumes the opening `{`, then parses a comma-separated list of use items relative
/// to `prefix`. Each item may override the `default_kind` with `function` or `const`.
/// Fully-qualified items inside the group are rejected. Returns the list of `UseItem`s.
fn parse_group_use_items(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    prefix: Name,
    default_kind: UseKind,
) -> Result<Vec<UseItem>, CompileError> {
    expect_token(
        tokens,
        pos,
        &Token::LBrace,
        "Expected '{' in group use declaration",
    )?;
    let mut imports = Vec::new();
    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        if !imports.is_empty() {
            expect_token(
                tokens,
                pos,
                &Token::Comma,
                "Expected ',' in group use declaration",
            )?;
        }

        let kind = if *pos < tokens.len() && tokens[*pos].0 == Token::Function {
            *pos += 1;
            UseKind::Function
        } else if *pos < tokens.len() && tokens[*pos].0 == Token::Const {
            *pos += 1;
            UseKind::Const
        } else {
            default_kind.clone()
        };

        let suffix = parse_name(
            tokens,
            pos,
            span,
            "Expected imported name inside group use declaration",
        )?;
        if suffix.is_fully_qualified() {
            return Err(CompileError::new(
                span,
                "Group use items must be relative to the shared prefix",
            ));
        }
        let mut parts = prefix.parts.clone();
        parts.extend(suffix.parts);
        let name = Name::qualified(parts);
        imports.push(parse_single_use_item_after_name(
            tokens, pos, span, name, kind,
        )?);
    }
    expect_token(
        tokens,
        pos,
        &Token::RBrace,
        "Expected '}' after group use declaration",
    )?;
    Ok(imports)
}

/// Parses the name prefix before the optional grouped-use `{`.
///
/// Reads a sequence of identifiers separated by `\`. If the sequence begins with `\`,
/// the name kind is `FullyQualified`; otherwise it is `Unqualified` or `Qualified`
/// depending on whether an intermediate `\ ` was seen. Stops when a trailing `\`
/// followed by `{` is encountered (the opening brace is consumed by the caller).
fn parse_use_prefix(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Name, CompileError> {
    let mut kind = NameKind::Unqualified;
    if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
        kind = NameKind::FullyQualified;
        *pos += 1;
    }

    let mut parts = Vec::new();
    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => {
                parts.push(name.clone());
                *pos += 1;
            }
            _ if parts.is_empty() => {
                return Err(CompileError::new(
                    span,
                    "Expected imported name after 'use'",
                ))
            }
            _ => break,
        }

        if *pos < tokens.len() && tokens[*pos].0 == Token::Backslash {
            if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::LBrace {
                *pos += 1;
                break;
            }
            if kind != NameKind::FullyQualified {
                kind = NameKind::Qualified;
            }
            *pos += 1;
            continue;
        }
        break;
    }

    Ok(Name::from_parts(kind, parts))
}
