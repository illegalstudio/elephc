use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::{Name, NameKind};
use crate::parser::ast::{Stmt, StmtKind, UseItem, UseKind};
use crate::span::Span;

use super::{expect_semicolon, expect_token, parse_name, parse_stmt, recover_to_statement_boundary};

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
