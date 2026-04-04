use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::Name;
use crate::parser::ast::{TraitAdaptation, TraitUse, Visibility};
use crate::span::Span;

use super::super::{expect_semicolon, expect_token, parse_name};

pub(in crate::parser::stmt) fn parse_trait_use(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<TraitUse, CompileError> {
    *pos += 1; // consume 'use'
    let mut trait_names = Vec::new();
    loop {
        trait_names.push(parse_name(
            tokens,
            pos,
            span,
            "Expected trait name after 'use'",
        )?);
        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
            continue;
        }
        break;
    }

    let mut adaptations = Vec::new();
    if *pos < tokens.len() && tokens[*pos].0 == Token::LBrace {
        *pos += 1;
        while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
            let (trait_name, method) = parse_trait_adaptation_target(tokens, pos, span)?;
            if *pos >= tokens.len() {
                return Err(CompileError::new(
                    span,
                    "Unexpected end of trait adaptation block",
                ));
            }
            match &tokens[*pos].0 {
                Token::As => {
                    *pos += 1;
                    let visibility = match tokens.get(*pos).map(|(t, _)| t) {
                        Some(Token::Public) => {
                            *pos += 1;
                            Some(Visibility::Public)
                        }
                        Some(Token::Protected) => {
                            *pos += 1;
                            Some(Visibility::Protected)
                        }
                        Some(Token::Private) => {
                            *pos += 1;
                            Some(Visibility::Private)
                        }
                        _ => None,
                    };
                    let alias = match tokens.get(*pos).map(|(t, _)| t) {
                        Some(Token::Identifier(name)) => {
                            let name = name.clone();
                            *pos += 1;
                            Some(name)
                        }
                        _ => None,
                    };
                    if visibility.is_none() && alias.is_none() {
                        return Err(CompileError::new(
                            span,
                            "Trait alias adaptation requires a visibility and/or alias name",
                        ));
                    }
                    adaptations.push(TraitAdaptation::Alias {
                        trait_name,
                        method,
                        alias,
                        visibility,
                    });
                }
                Token::InsteadOf => {
                    *pos += 1;
                    let mut instead_of = Vec::new();
                    loop {
                        match tokens.get(*pos).map(|(t, _)| t) {
                            Some(Token::Identifier(_)) | Some(Token::Backslash) => {
                                instead_of.push(parse_name(
                                    tokens,
                                    pos,
                                    span,
                                    "Expected trait name after 'insteadof'",
                                )?);
                            }
                            _ => {
                                return Err(CompileError::new(
                                    span,
                                    "Expected trait name after 'insteadof'",
                                ))
                            }
                        }
                        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                            *pos += 1;
                            continue;
                        }
                        break;
                    }
                    if instead_of.is_empty() {
                        return Err(CompileError::new(
                            span,
                            "Trait insteadof adaptation requires at least one suppressed trait",
                        ));
                    }
                    adaptations.push(TraitAdaptation::InsteadOf {
                        trait_name,
                        method,
                        instead_of,
                    });
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "Expected 'as' or 'insteadof' inside trait adaptation block",
                    ))
                }
            }
            expect_semicolon(tokens, pos)?;
        }
        expect_token(
            tokens,
            pos,
            &Token::RBrace,
            "Expected '}' after trait adaptations",
        )?;
        if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
            *pos += 1;
        }
    } else {
        expect_semicolon(tokens, pos)?;
    }
    Ok(TraitUse {
        trait_names,
        adaptations,
        span,
    })
}

fn parse_trait_adaptation_target(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<(Option<Name>, String), CompileError> {
    let first = parse_name(
        tokens,
        pos,
        span,
        "Expected method or trait name in adaptation",
    )?;
    if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleColon {
        *pos += 1;
        let method = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => {
                let name = name.clone();
                *pos += 1;
                name
            }
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected method name after 'TraitName::' in adaptation",
                ))
            }
        };
        Ok((Some(first), method))
    } else {
        let method = first
            .last_segment()
            .map(str::to_string)
            .ok_or_else(|| CompileError::new(span, "Expected method name in adaptation"))?;
        Ok((None, method))
    }
}
