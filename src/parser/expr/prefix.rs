use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
use crate::span::Span;

use super::calls::{parse_scoped_static_call, peek_cast};
use super::prefix_complex::{
    parse_arrow_closure, parse_closure, parse_match_expr, parse_named_expr, parse_new_object,
};
use super::pratt::parse_expr_bp;
use super::{parse_args, parse_expr};

pub(super) fn parse_prefix(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Expr, CompileError> {
    if *pos >= tokens.len() {
        let span = tokens.last().map(|(_, span)| *span).unwrap_or(Span::dummy());
        return Err(CompileError::new(span, "Unexpected end of input"));
    }

    let span = tokens[*pos].1;

    match &tokens[*pos].0 {
        Token::Minus => parse_unary(tokens, pos, span, ExprKind::Negate, 27),
        Token::Bang => parse_unary(tokens, pos, span, ExprKind::Not, 27),
        Token::Tilde => parse_unary(tokens, pos, span, ExprKind::BitNot, 27),
        Token::Throw => parse_unary(tokens, pos, span, ExprKind::Throw, 0),
        Token::True => parse_simple(tokens, pos, span, ExprKind::BoolLiteral(true)),
        Token::False => parse_simple(tokens, pos, span, ExprKind::BoolLiteral(false)),
        Token::Null => parse_simple(tokens, pos, span, ExprKind::Null),
        Token::Inf => parse_simple(tokens, pos, span, ExprKind::FloatLiteral(f64::INFINITY)),
        Token::Nan => parse_simple(tokens, pos, span, ExprKind::FloatLiteral(f64::NAN)),
        Token::PhpIntMax => parse_simple(tokens, pos, span, ExprKind::IntLiteral(i64::MAX)),
        Token::PhpIntMin => parse_simple(tokens, pos, span, ExprKind::IntLiteral(i64::MIN)),
        Token::PhpFloatMax => parse_simple(tokens, pos, span, ExprKind::FloatLiteral(f64::MAX)),
        Token::MPi => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::PI),
        ),
        Token::ME => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::E),
        ),
        Token::MSqrt2 => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::SQRT_2),
        ),
        Token::MPi2 => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::FRAC_PI_2),
        ),
        Token::MPi4 => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::FRAC_PI_4),
        ),
        Token::MLog2e => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::LOG2_E),
        ),
        Token::MLog10e => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(std::f64::consts::LOG10_E),
        ),
        Token::PhpFloatMin => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(f64::MIN_POSITIVE),
        ),
        Token::PhpFloatEpsilon => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::FloatLiteral(f64::EPSILON),
        ),
        Token::Stdin => parse_simple(tokens, pos, span, ExprKind::IntLiteral(0)),
        Token::Stdout => parse_simple(tokens, pos, span, ExprKind::IntLiteral(1)),
        Token::Stderr => parse_simple(tokens, pos, span, ExprKind::IntLiteral(2)),
        Token::PhpEol => parse_simple(tokens, pos, span, ExprKind::StringLiteral("\n".to_string())),
        Token::PhpOs => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::StringLiteral("Darwin".to_string()),
        ),
        Token::DirectorySeparator => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::StringLiteral("/".to_string()),
        ),
        Token::PlusPlus => parse_prefix_inc_dec(tokens, pos, span, true),
        Token::MinusMinus => parse_prefix_inc_dec(tokens, pos, span, false),
        Token::StringLiteral(value) => {
            let value = value.clone();
            *pos += 1;
            Ok(Expr::new(ExprKind::StringLiteral(value), span))
        }
        Token::IntLiteral(value) => {
            let value = *value;
            *pos += 1;
            Ok(Expr::new(ExprKind::IntLiteral(value), span))
        }
        Token::FloatLiteral(value) => {
            let value = *value;
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(value), span))
        }
        Token::Variable(name) => parse_variable(tokens, pos, span, name.clone()),
        Token::LParen => parse_group_or_cast(tokens, pos, span),
        Token::LBracket => parse_array_literal(tokens, pos, span),
        Token::Match => parse_match_expr(tokens, pos, span),
        Token::Function => parse_closure(tokens, pos, span),
        Token::Fn => parse_arrow_closure(tokens, pos, span),
        Token::Identifier(_) | Token::Backslash => parse_named_expr(tokens, pos, span),
        Token::Self_ => {
            *pos += 1;
            parse_scoped_static_call(tokens, pos, span, StaticReceiver::Self_, "self")
        }
        Token::Static => {
            *pos += 1;
            parse_scoped_static_call(tokens, pos, span, StaticReceiver::Static, "static")
        }
        Token::Parent => {
            *pos += 1;
            parse_scoped_static_call(tokens, pos, span, StaticReceiver::Parent, "parent")
        }
        Token::New => parse_new_object(tokens, pos, span),
        Token::This => parse_simple(tokens, pos, span, ExprKind::This),
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token: {:?}", other),
        )),
    }
}

fn parse_simple(
    _tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    kind: ExprKind,
) -> Result<Expr, CompileError> {
    *pos += 1;
    Ok(Expr::new(kind, span))
}

fn parse_unary(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    ctor: fn(Box<Expr>) -> ExprKind,
    bp: u8,
) -> Result<Expr, CompileError> {
    *pos += 1;
    let inner = parse_expr_bp(tokens, pos, bp)?;
    Ok(Expr::new(ctor(Box::new(inner)), span))
}

fn parse_prefix_inc_dec(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    increment: bool,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos < tokens.len() {
        if let Token::Variable(name) = &tokens[*pos].0 {
            let name = name.clone();
            *pos += 1;
            return Ok(Expr::new(
                if increment {
                    ExprKind::PreIncrement(name)
                } else {
                    ExprKind::PreDecrement(name)
                },
                span,
            ));
        }
    }
    Err(CompileError::new(
        span,
        if increment {
            "Expected variable after '++'"
        } else {
            "Expected variable after '--'"
        },
    ))
}

fn parse_variable(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    name: String,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos < tokens.len() {
        match &tokens[*pos].0 {
            Token::PlusPlus => {
                *pos += 1;
                return Ok(Expr::new(ExprKind::PostIncrement(name), span));
            }
            Token::MinusMinus => {
                *pos += 1;
                return Ok(Expr::new(ExprKind::PostDecrement(name), span));
            }
            Token::LParen => {
                *pos += 1;
                let args = parse_args(tokens, pos, span)?;
                return Ok(Expr::new(ExprKind::ClosureCall { var: name, args }, span));
            }
            _ => {}
        }
    }
    Ok(Expr::new(ExprKind::Variable(name), span))
}

fn parse_group_or_cast(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    if let Some(cast_ty) = peek_cast(tokens, *pos) {
        *pos += 3;
        let inner = parse_expr_bp(tokens, pos, 27)?;
        return Ok(Expr::new(
            ExprKind::Cast {
                target: cast_ty,
                expr: Box::new(inner),
            },
            span,
        ));
    }

    *pos += 1;
    let inner = parse_expr(tokens, pos)?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
        return Err(CompileError::new(span, "Expected closing ')'"));
    }
    *pos += 1;
    if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
        let call_span = tokens[*pos].1;
        *pos += 1;
        let args = parse_args(tokens, pos, call_span)?;
        return Ok(Expr::new(
            ExprKind::ExprCall {
                callee: Box::new(inner),
                args,
            },
            call_span,
        ));
    }
    Ok(inner)
}

fn parse_array_literal(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;
    let mut elems = Vec::new();
    let mut assoc_elems = Vec::new();
    let mut is_assoc = false;
    let mut first = true;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RBracket {
        if !first {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Expected ',' between array elements",
                ));
            }
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == Token::RBracket {
                break;
            }
        }
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            let spread_span = tokens[*pos].1;
            *pos += 1;
            let inner = parse_expr(tokens, pos)?;
            elems.push(Expr::new(ExprKind::Spread(Box::new(inner)), spread_span));
            first = false;
            continue;
        }
        let expr = parse_expr(tokens, pos)?;
        if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
            is_assoc = true;
            *pos += 1;
            let value = parse_expr(tokens, pos)?;
            assoc_elems.push((expr, value));
        } else if is_assoc {
            return Err(CompileError::new(
                span,
                "Cannot mix associative and indexed array elements",
            ));
        } else {
            elems.push(expr);
        }
        first = false;
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
        return Err(CompileError::new(span, "Expected ']'"));
    }
    *pos += 1;
    if is_assoc {
        Ok(Expr::new(ExprKind::ArrayLiteralAssoc(assoc_elems), span))
    } else {
        Ok(Expr::new(ExprKind::ArrayLiteral(elems), span))
    }
}
