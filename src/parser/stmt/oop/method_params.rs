use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{ClassProperty, Expr, ExprKind, Stmt, StmtKind, TypeExpr, Visibility};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::super::params::{looks_like_typed_param, parse_type_expr};
use super::super::expect_token;

type MethodParam = (String, Option<TypeExpr>, Option<Expr>, bool);
type ParsedMethodParams = (Vec<MethodParam>, Option<String>, Vec<ClassProperty>, Vec<Stmt>);

pub(super) fn parse_method_params(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    method_name: &str,
) -> Result<ParsedMethodParams, CompileError> {
    let mut params = Vec::new();
    let mut variadic = None;
    let mut promoted_properties = Vec::new();
    let mut promoted_assignments = Vec::new();

    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() || variadic.is_some() {
            expect_token(
                tokens,
                pos,
                &Token::Comma,
                "Expected ',' between parameters",
            )?;
        }
        if variadic.is_some() {
            return Err(CompileError::new(
                span,
                "Variadic parameter must be the last parameter",
            ));
        }

        let promotion = parse_promoted_param_modifiers(tokens, pos)?;
        if promotion.is_some() && method_name != "__construct" {
            return Err(CompileError::new(
                span,
                "Cannot declare promoted property outside a constructor",
            ));
        }

        let type_ann = if looks_like_typed_param(tokens, *pos) {
            Some(parse_type_expr(tokens, pos, span)?)
        } else {
            None
        };
        let is_ref = if *pos < tokens.len() && tokens[*pos].0 == Token::Ampersand {
            if promotion.as_ref().is_some_and(|(_, readonly, _)| *readonly) {
                return Err(CompileError::new(
                    span,
                    "Readonly promoted by-reference properties are not supported",
                ));
            }
            *pos += 1;
            true
        } else {
            false
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            if promotion.is_some() {
                return Err(CompileError::new(
                    span,
                    "Cannot declare variadic promoted property",
                ));
            }
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

        match tokens.get(*pos).map(|(t, s)| (t, *s)) {
            Some((Token::Variable(n), param_span)) => {
                let n = n.clone();
                *pos += 1;
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                if is_ref && promotion.is_some() && default.is_some() {
                    return Err(CompileError::new(
                        span,
                        "Promoted by-reference properties cannot use default values yet",
                    ));
                }
                if let Some((visibility, readonly, property_span)) = promotion {
                    promoted_properties.push(ClassProperty {
                        name: n.clone(),
                        visibility,
                        type_expr: type_ann.clone(),
                        readonly,
                        is_final: false,
                        is_static: false,
                        by_ref: is_ref,
                        default: None,
                        span: property_span,
                    });
                    promoted_assignments.push(promoted_property_assignment(&n, param_span));
                }
                params.push((n, type_ann, default, is_ref));
            }
            _ => return Err(CompileError::new(span, "Expected parameter variable")),
        }
    }

    Ok((params, variadic, promoted_properties, promoted_assignments))
}

fn parse_promoted_param_modifiers(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Option<(Visibility, bool, Span)>, CompileError> {
    let mut visibility = None;
    let mut readonly = false;
    let mut first_span = None;

    loop {
        match tokens.get(*pos).map(|(t, s)| (t, *s)) {
            Some((Token::Public, token_span)) => {
                if visibility.is_some() {
                    return Err(CompileError::new(token_span, "Duplicate parameter visibility"));
                }
                first_span.get_or_insert(token_span);
                visibility = Some(Visibility::Public);
                *pos += 1;
            }
            Some((Token::Protected, token_span)) => {
                if visibility.is_some() {
                    return Err(CompileError::new(token_span, "Duplicate parameter visibility"));
                }
                first_span.get_or_insert(token_span);
                visibility = Some(Visibility::Protected);
                *pos += 1;
            }
            Some((Token::Private, token_span)) => {
                if visibility.is_some() {
                    return Err(CompileError::new(token_span, "Duplicate parameter visibility"));
                }
                first_span.get_or_insert(token_span);
                visibility = Some(Visibility::Private);
                *pos += 1;
            }
            Some((Token::ReadOnly, token_span)) => {
                if readonly {
                    return Err(CompileError::new(token_span, "Duplicate readonly modifier"));
                }
                first_span.get_or_insert(token_span);
                readonly = true;
                *pos += 1;
            }
            Some((Token::Static, token_span)) => {
                return Err(CompileError::new(
                    token_span,
                    "Cannot use the static modifier on a parameter",
                ))
            }
            Some((Token::Abstract, token_span)) => {
                return Err(CompileError::new(
                    token_span,
                    "Cannot use the abstract modifier on a parameter",
                ))
            }
            Some((Token::Final, token_span)) => {
                return Err(CompileError::new(
                    token_span,
                    "Cannot use the final modifier on a parameter",
                ))
            }
            _ => break,
        }
    }

    let Some(property_span) = first_span else {
        return Ok(None);
    };

    Ok(Some((
        visibility.unwrap_or(Visibility::Public),
        readonly,
        property_span,
    )))
}

fn promoted_property_assignment(name: &str, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, span)),
            property: name.to_string(),
            value: Expr::new(ExprKind::Variable(name.to_string()), span),
        },
        span,
    )
}
