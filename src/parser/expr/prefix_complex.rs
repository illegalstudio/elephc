use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind,
};
use crate::parser::stmt::{looks_like_typed_param, parse_block, parse_name, parse_type_expr};
use crate::span::Span;

use super::calls::parse_first_class_callable_parens;
use super::{parse_args, parse_expr};

pub(super) fn parse_match_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after 'match'"));
    }
    *pos += 1;
    let subject = parse_expr(tokens, pos)?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
        return Err(CompileError::new(span, "Expected ')' after match subject"));
    }
    *pos += 1;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LBrace {
        return Err(CompileError::new(span, "Expected '{' after match subject"));
    }
    *pos += 1;
    let mut arms = Vec::new();
    let mut default = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBrace {
        if tokens[*pos].0 == Token::Default {
            *pos += 1;
            if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleArrow {
                return Err(CompileError::new(span, "Expected '=>' after 'default'"));
            }
            *pos += 1;
            let result = parse_expr(tokens, pos)?;
            default = Some(Box::new(result));
            if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                *pos += 1;
            }
        } else {
            let mut patterns = Vec::new();
            loop {
                patterns.push(parse_expr(tokens, pos)?);
                if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                    let saved = *pos;
                    *pos += 1;
                    if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
                        *pos = saved;
                        break;
                    }
                } else {
                    break;
                }
            }
            if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleArrow {
                return Err(CompileError::new(span, "Expected '=>' in match arm"));
            }
            *pos += 1;
            let result = parse_expr(tokens, pos)?;
            arms.push((patterns, result));
            if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                *pos += 1;
            }
        }
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBrace {
        return Err(CompileError::new(span, "Expected '}' to close match"));
    }
    *pos += 1;
    Ok(Expr::new(
        ExprKind::Match {
            subject: Box::new(subject),
            arms,
            default,
        },
        span,
    ))
}

pub(super) fn parse_closure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    if *pos + 1 >= tokens.len() || tokens[*pos + 1].0 != Token::LParen {
        return Err(CompileError::new(span, "Unexpected token: Function"));
    }
    *pos += 2;
    let (params, variadic) = parse_closure_params(tokens, pos, span)?;
    let mut captures = Vec::new();
    if *pos < tokens.len() && tokens[*pos].0 == Token::Use {
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(span, "Expected '(' after 'use'"));
        }
        *pos += 1;
        while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
            if !captures.is_empty() {
                if tokens[*pos].0 != Token::Comma {
                    return Err(CompileError::new(
                        tokens[*pos].1,
                        "Expected ',' between captured variables",
                    ));
                }
                *pos += 1;
            }
            match tokens.get(*pos).map(|(token, _)| token) {
                Some(Token::Variable(name)) => {
                    captures.push(name.clone());
                    *pos += 1;
                }
                _ => {
                    return Err(CompileError::new(
                        span,
                        "Expected variable in use() capture list",
                    ))
                }
            }
        }
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(
                span,
                "Expected ')' after use() capture list",
            ));
        }
        *pos += 1;
    }
    let body = parse_block(tokens, pos)?;
    Ok(Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow: false,
            captures,
        },
        span,
    ))
}

pub(super) fn parse_arrow_closure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after 'fn'"));
    }
    *pos += 1;
    let (params, variadic) = parse_closure_params(tokens, pos, span)?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleArrow {
        return Err(CompileError::new(
            span,
            "Expected '=>' after arrow function parameters",
        ));
    }
    *pos += 1;
    let body_expr = parse_expr(tokens, pos)?;
    let body = vec![Stmt::new(StmtKind::Return(Some(body_expr)), span)];
    Ok(Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow: true,
            captures: vec![],
        },
        span,
    ))
}

fn parse_closure_params(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<
    (
        Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
        Option<String>,
    ),
    CompileError,
> {
    let mut params = Vec::new();
    let mut variadic = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() || variadic.is_some() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Expected ',' between parameters",
                ));
            }
            *pos += 1;
        }
        if variadic.is_some() {
            return Err(CompileError::new(
                span,
                "Variadic parameter must be the last parameter",
            ));
        }
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
            match tokens.get(*pos).map(|(token, _)| token) {
                Some(Token::Variable(name)) => {
                    variadic = Some(name.clone());
                    *pos += 1;
                }
                _ => return Err(CompileError::new(span, "Expected variable after '...'")),
            }
            continue;
        }
        match tokens.get(*pos).map(|(token, _)| token) {
            Some(Token::Variable(name)) => {
                let name = name.clone();
                *pos += 1;
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                params.push((name, type_ann, default, is_ref));
            }
            _ => return Err(CompileError::new(span, "Expected parameter variable")),
        }
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
        return Err(CompileError::new(span, "Expected ')' after parameters"));
    }
    *pos += 1;
    Ok((params, variadic))
}

pub(super) fn parse_named_expr(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    let name = parse_name(tokens, pos, span, "Expected name")?;
    if name.parts.len() == 1
        && name.parts[0] == "buffer_new"
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::Less
    {
        *pos += 1;
        let element_type = parse_type_expr(tokens, pos, span)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::Greater {
            return Err(CompileError::new(span, "Expected '>' after buffer_new<T"));
        }
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(span, "Expected '(' after buffer_new<T>"));
        }
        *pos += 1;
        let len = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(
                span,
                "Expected ')' after buffer_new length",
            ));
        }
        *pos += 1;
        return Ok(Expr::new(
            ExprKind::BufferNew {
                element_type,
                len: Box::new(len),
            },
            span,
        ));
    }
    if name.parts.len() == 1
        && name.parts[0] == "ptr_cast"
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::Less
    {
        *pos += 1;
        let target_type = parse_name(tokens, pos, span, "Expected type name after 'ptr_cast<'")?
            .as_canonical();
        if *pos >= tokens.len() || tokens[*pos].0 != Token::Greater {
            return Err(CompileError::new(span, "Expected '>' after ptr_cast<T"));
        }
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(span, "Expected '(' after ptr_cast<T>"));
        }
        *pos += 1;
        let expr = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(
                span,
                "Expected ')' after ptr_cast argument",
            ));
        }
        *pos += 1;
        return Ok(Expr::new(
            ExprKind::PtrCast {
                target_type,
                expr: Box::new(expr),
            },
            span,
        ));
    }
    if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
        *pos += 1;
        if parse_first_class_callable_parens(tokens, pos)? {
            Ok(Expr::new(
                ExprKind::FirstClassCallable(CallableTarget::Function(name)),
                span,
            ))
        } else {
            let args = parse_args(tokens, pos, span)?;
            Ok(Expr::new(ExprKind::FunctionCall { name, args }, span))
        }
    } else if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleColon {
        *pos += 1;
        let member = match tokens.get(*pos).map(|(token, _)| token) {
            Some(Token::Identifier(member)) => {
                let member = member.clone();
                *pos += 1;
                member
            }
            _ => return Err(CompileError::new(span, "Expected member name after '::'")),
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
            *pos += 1;
            if parse_first_class_callable_parens(tokens, pos)? {
                Ok(Expr::new(
                    ExprKind::FirstClassCallable(CallableTarget::StaticMethod {
                        receiver: StaticReceiver::Named(name),
                        method: member,
                    }),
                    span,
                ))
            } else {
                let args = parse_args(tokens, pos, span)?;
                Ok(Expr::new(
                    ExprKind::StaticMethodCall {
                        receiver: StaticReceiver::Named(name),
                        method: member,
                        args,
                    },
                    span,
                ))
            }
        } else {
            Ok(Expr::new(
                ExprKind::EnumCase {
                    enum_name: name,
                    case_name: member,
                },
                span,
            ))
        }
    } else {
        Ok(Expr::new(ExprKind::ConstRef(name), span))
    }
}

pub(super) fn parse_new_object(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;
    let class_name = parse_name(tokens, pos, span, "Expected class name after 'new'")?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after class name"));
    }
    *pos += 1;
    let args = parse_args(tokens, pos, span)?;
    Ok(Expr::new(ExprKind::NewObject { class_name, args }, span))
}
