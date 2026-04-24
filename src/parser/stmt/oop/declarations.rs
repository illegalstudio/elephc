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
    is_final: bool,
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
            is_final,
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
                | Token::Abstract
                | Token::Final,
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
    parse_modified_class_decl(tokens, pos, span)
}

pub(in crate::parser::stmt) fn parse_readonly_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    parse_modified_class_decl(tokens, pos, span)
}

pub(in crate::parser::stmt) fn parse_final_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    parse_modified_class_decl(tokens, pos, span)
}

fn parse_modified_class_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    let mut is_abstract = false;
    let mut is_final = false;
    let mut is_readonly_class = false;

    while *pos < tokens.len() {
        match tokens[*pos].0 {
            Token::Abstract => {
                if is_abstract {
                    return Err(CompileError::new(span, "Duplicate class modifier: abstract"));
                }
                is_abstract = true;
                *pos += 1;
            }
            Token::Final => {
                if is_final {
                    return Err(CompileError::new(span, "Duplicate class modifier: final"));
                }
                is_final = true;
                *pos += 1;
            }
            Token::ReadOnly => {
                if is_readonly_class {
                    return Err(CompileError::new(span, "Duplicate class modifier: readonly"));
                }
                is_readonly_class = true;
                *pos += 1;
            }
            Token::Class => {
                if is_abstract && is_final {
                    return Err(CompileError::new(
                        span,
                        "Cannot use the final modifier on an abstract class",
                    ));
                }
                return parse_class_decl(tokens, pos, span, is_abstract, is_final, is_readonly_class);
            }
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected 'class' after class modifier at statement position",
                ))
            }
        }
    }

    Err(CompileError::new(
        span,
        "Expected 'class' after class modifier at statement position",
    ))
}
