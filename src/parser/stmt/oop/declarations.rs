use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{EnumCaseDecl, PackedField, Stmt, StmtKind};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::super::params::{parse_name_list, parse_type_expr};
use super::super::{expect_semicolon, expect_token, parse_name};
use super::body::parse_class_like_body;

/// Parse a class declaration: class Name { use TraitName; properties and methods }
pub(in crate::parser::stmt) fn parse_class_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    is_abstract: bool,
    is_readonly_class: bool,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'class'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected class name after 'class'")),
    };

    let extends = if *pos < tokens.len() && tokens[*pos].0 == Token::Extends {
        *pos += 1;
        Some(parse_name(
            tokens,
            pos,
            span,
            "Expected parent class name after 'extends'",
        )?)
    } else {
        None
    };

    let implements = if *pos < tokens.len() && tokens[*pos].0 == Token::Implements {
        *pos += 1;
        parse_name_list(
            tokens,
            pos,
            span,
            "Expected interface name after 'implements'",
        )?
    } else {
        Vec::new()
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after class name")?;

    let (trait_uses, properties, methods) = parse_class_like_body(tokens, pos, "class")?;

    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of class")?;

    Ok(Stmt::new(
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        },
        span,
    ))
}

pub(in crate::parser::stmt) fn parse_enum_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'enum'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected enum name after 'enum'")),
    };

    let backing_type = if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        Some(parse_type_expr(tokens, pos, span)?)
    } else {
        None
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after enum name")?;
    let mut cases = Vec::new();
    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        let case_span = tokens[*pos].1;
        expect_token(tokens, pos, &Token::Case, "Expected 'case' in enum body")?;
        let case_name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => {
                let name = name.clone();
                *pos += 1;
                name
            }
            _ => {
                return Err(CompileError::new(
                    case_span,
                    "Expected case name after 'case'",
                ))
            }
        };
        let value = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            Some(parse_expr(tokens, pos)?)
        } else {
            None
        };
        expect_semicolon(tokens, pos)?;
        cases.push(EnumCaseDecl {
            name: case_name,
            value,
            span: case_span,
        });
    }

    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of enum")?;
    Ok(Stmt::new(
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        },
        span,
    ))
}

pub(in crate::parser::stmt) fn parse_packed_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'packed'
    expect_token(
        tokens,
        pos,
        &Token::Class,
        "Expected 'class' after 'packed'",
    )?;

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => {
            return Err(CompileError::new(
                span,
                "Expected class name after 'packed class'",
            ))
        }
    };

    expect_token(
        tokens,
        pos,
        &Token::LBrace,
        "Expected '{' after packed class name",
    )?;
    let mut fields = Vec::new();
    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        let field_span = tokens[*pos].1;
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Public) => *pos += 1,
            Some(
                Token::Protected
                | Token::Private
                | Token::Static
                | Token::ReadOnly
                | Token::Abstract,
            ) => {
                return Err(CompileError::new(
                    field_span,
                    "Packed class fields may only use public visibility",
                ))
            }
            _ => {}
        }

        let type_expr = parse_type_expr(tokens, pos, field_span)?;
        let field_name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Variable(name)) => {
                let name = name.clone();
                *pos += 1;
                name
            }
            _ => {
                return Err(CompileError::new(
                    field_span,
                    "Expected $field after packed field type",
                ))
            }
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            return Err(CompileError::new(
                field_span,
                "Packed class fields cannot have default values",
            ));
        }
        expect_semicolon(tokens, pos)?;
        fields.push(PackedField {
            name: field_name,
            type_expr,
            span: field_span,
        });
    }

    expect_token(
        tokens,
        pos,
        &Token::RBrace,
        "Expected '}' at end of packed class",
    )?;
    Ok(Stmt::new(StmtKind::PackedClassDecl { name, fields }, span))
}

pub(in crate::parser::stmt) fn parse_abstract_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'abstract'
    if *pos < tokens.len() && tokens[*pos].0 == Token::ReadOnly {
        *pos += 1; // consume 'readonly'
        if *pos < tokens.len() && tokens[*pos].0 == Token::Class {
            return parse_class_decl(tokens, pos, span, true, true);
        }
        return Err(CompileError::new(
            span,
            "Expected 'class' after 'abstract readonly' at statement position",
        ));
    }
    if *pos < tokens.len() && tokens[*pos].0 == Token::Class {
        return parse_class_decl(tokens, pos, span, true, false);
    }
    Err(CompileError::new(
        span,
        "Expected 'class' after 'abstract' at statement position",
    ))
}

pub(in crate::parser::stmt) fn parse_readonly_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'readonly'
    if *pos < tokens.len() && tokens[*pos].0 == Token::Abstract {
        *pos += 1; // consume 'abstract'
        if *pos < tokens.len() && tokens[*pos].0 == Token::Class {
            return parse_class_decl(tokens, pos, span, true, true);
        }
        return Err(CompileError::new(
            span,
            "Expected 'class' after 'readonly abstract' at statement position",
        ));
    }
    if *pos < tokens.len() && tokens[*pos].0 == Token::Class {
        return parse_class_decl(tokens, pos, span, false, true);
    }
    Err(CompileError::new(
        span,
        "Expected 'class' after 'readonly' at statement position",
    ))
}
