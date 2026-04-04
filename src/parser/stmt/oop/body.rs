use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{ClassMethod, ClassProperty, Stmt, StmtKind, TraitUse, Visibility};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::super::params::{parse_name_list, parse_params, parse_type_expr};
use super::super::{expect_semicolon, expect_token, parse_block};
use super::traits::parse_trait_use;

pub(in crate::parser::stmt) fn parse_interface_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'interface'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => {
            return Err(CompileError::new(
                span,
                "Expected interface name after 'interface'",
            ))
        }
    };

    let extends = if *pos < tokens.len() && tokens[*pos].0 == Token::Extends {
        *pos += 1;
        parse_name_list(
            tokens,
            pos,
            span,
            "Expected parent interface name after 'extends'",
        )?
    } else {
        Vec::new()
    };

    expect_token(
        tokens,
        pos,
        &Token::LBrace,
        "Expected '{' after interface name",
    )?;
    let methods = parse_interface_body(tokens, pos)?;
    expect_token(
        tokens,
        pos,
        &Token::RBrace,
        "Expected '}' at end of interface",
    )?;

    Ok(Stmt::new(
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        },
        span,
    ))
}

/// Parse a trait declaration: trait Name { use OtherTrait; properties and methods }
pub(in crate::parser::stmt) fn parse_trait_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'trait'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected trait name after 'trait'")),
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after trait name")?;
    let (trait_uses, properties, methods) = parse_class_like_body(tokens, pos, "trait")?;
    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of trait")?;

    Ok(Stmt::new(
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        },
        span,
    ))
}

pub(in crate::parser::stmt) fn parse_class_like_body(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    owner_kind: &str,
) -> Result<(Vec<TraitUse>, Vec<ClassProperty>, Vec<ClassMethod>), CompileError> {
    let mut trait_uses = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();

    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        let member_span = tokens[*pos].1;
        if tokens[*pos].0 == Token::Use {
            trait_uses.push(parse_trait_use(tokens, pos, member_span)?);
            continue;
        }

        let modifiers = parse_member_modifiers(tokens, pos);

        if *pos >= tokens.len() {
            return Err(CompileError::new(
                member_span,
                &format!("Unexpected end of {} body", owner_kind),
            ));
        }

        if tokens[*pos].0 == Token::Function {
            if modifiers.is_readonly {
                return Err(CompileError::new(
                    member_span,
                    "Readonly methods are not supported",
                ));
            }
            methods.push(parse_class_like_method(
                tokens,
                pos,
                member_span,
                modifiers.visibility,
                modifiers.is_static,
                modifiers.is_abstract,
            )?);
            continue;
        }

        if let Some(Token::Variable(prop_name)) = tokens.get(*pos).map(|(t, _)| t.clone()) {
            if modifiers.is_static {
                return Err(CompileError::new(
                    member_span,
                    "Static properties are not supported",
                ));
            }
            if modifiers.is_abstract {
                return Err(CompileError::new(
                    member_span,
                    "Abstract properties are not supported",
                ));
            }
            let prop_name = prop_name.clone();
            *pos += 1;
            let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                *pos += 1;
                Some(parse_expr(tokens, pos)?)
            } else {
                None
            };
            expect_semicolon(tokens, pos)?;
            properties.push(ClassProperty {
                name: prop_name,
                visibility: modifiers.visibility,
                readonly: modifiers.is_readonly,
                default,
                span: member_span,
            });
            continue;
        }

        return Err(CompileError::new(
            member_span,
            &format!(
                "Expected trait use, property, or method declaration in {} body",
                owner_kind
            ),
        ));
    }

    Ok((trait_uses, properties, methods))
}

pub(super) struct MemberModifiers {
    visibility: Visibility,
    is_static: bool,
    is_readonly: bool,
    is_abstract: bool,
}

fn parse_member_modifiers(tokens: &[(Token, Span)], pos: &mut usize) -> MemberModifiers {
    let mut visibility = Visibility::Public;
    let mut is_static = false;
    let mut is_readonly = false;
    let mut is_abstract = false;

    loop {
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Public) => {
                visibility = Visibility::Public;
                *pos += 1;
            }
            Some(Token::Protected) => {
                visibility = Visibility::Protected;
                *pos += 1;
            }
            Some(Token::Private) => {
                visibility = Visibility::Private;
                *pos += 1;
            }
            Some(Token::Static) => {
                is_static = true;
                *pos += 1;
            }
            Some(Token::ReadOnly) => {
                is_readonly = true;
                *pos += 1;
            }
            Some(Token::Abstract) => {
                is_abstract = true;
                *pos += 1;
            }
            _ => break,
        }
    }

    MemberModifiers {
        visibility,
        is_static,
        is_readonly,
        is_abstract,
    }
}

fn parse_class_like_method(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    visibility: Visibility,
    is_static: bool,
    is_abstract: bool,
) -> Result<ClassMethod, CompileError> {
    *pos += 1; // consume 'function'
    let method_name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected method name")),
    };

    expect_token(
        tokens,
        pos,
        &Token::LParen,
        "Expected '(' after method name",
    )?;
    let (params, variadic) = parse_params(tokens, pos, span)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')'")?;
    // Parse optional return type: `: TypeExpr`
    let return_type = if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        Some(parse_type_expr(tokens, pos, span)?)
    } else {
        None
    };
    let (has_body, body) = if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        (false, Vec::new())
    } else {
        (true, parse_block(tokens, pos)?)
    };
    Ok(ClassMethod {
        name: method_name,
        visibility,
        is_static,
        is_abstract,
        has_body,
        params,
        variadic,
        return_type,
        body,
        span,
    })
}

fn parse_interface_body(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Vec<ClassMethod>, CompileError> {
    let mut methods = Vec::new();

    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        let member_span = tokens[*pos].1;
        let modifiers = parse_member_modifiers(tokens, pos);
        if *pos >= tokens.len() || tokens[*pos].0 != Token::Function {
            return Err(CompileError::new(
                member_span,
                "Interfaces may only contain method declarations",
            ));
        }
        methods.push(parse_class_like_method(
            tokens,
            pos,
            member_span,
            modifiers.visibility,
            modifiers.is_static,
            true,
        )?);
    }

    Ok(methods)
}
