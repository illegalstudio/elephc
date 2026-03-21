use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{BinOp, Expr, ExprKind};
use crate::span::Span;

pub fn parse_expr(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Expr, CompileError> {
    parse_expr_bp(tokens, pos, 0)
}

/// Pratt parser: parses expressions with binding power `min_bp` or higher.
fn parse_expr_bp(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    min_bp: u8,
) -> Result<Expr, CompileError> {
    let mut lhs = parse_prefix(tokens, pos)?;

    loop {
        if *pos >= tokens.len() {
            break;
        }

        let (op, l_bp, r_bp) = match infix_bp(&tokens[*pos].0) {
            Some(v) => v,
            None => break,
        };

        if l_bp < min_bp {
            break;
        }

        let span = tokens[*pos].1;
        *pos += 1;
        let rhs = parse_expr_bp(tokens, pos, r_bp)?;
        lhs = Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(lhs),
                op,
                right: Box::new(rhs),
            },
            span,
        );
    }

    Ok(lhs)
}

/// Infix operator binding powers.
/// To add a new operator, add a line here.
fn infix_bp(token: &Token) -> Option<(BinOp, u8, u8)> {
    match token {
        Token::Dot          => Some((BinOp::Concat, 1, 2)),
        Token::EqualEqual   => Some((BinOp::Eq,     3, 4)),
        Token::NotEqual     => Some((BinOp::NotEq,  3, 4)),
        Token::Less         => Some((BinOp::Lt,     5, 6)),
        Token::Greater      => Some((BinOp::Gt,     5, 6)),
        Token::LessEqual    => Some((BinOp::LtEq,   5, 6)),
        Token::GreaterEqual => Some((BinOp::GtEq,   5, 6)),
        Token::Plus         => Some((BinOp::Add,    7, 8)),
        Token::Minus        => Some((BinOp::Sub,    7, 8)),
        Token::Star         => Some((BinOp::Mul,    9, 10)),
        Token::Slash        => Some((BinOp::Div,    9, 10)),
        Token::Percent      => Some((BinOp::Mod,    9, 10)),
        _ => None,
    }
}

/// Prefix expressions: literals, variables, unary operators, parentheses.
fn parse_prefix(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Expr, CompileError> {
    if *pos >= tokens.len() {
        let span = tokens.last().map(|(_, s)| *s).unwrap_or(Span::dummy());
        return Err(CompileError::new(span, "Unexpected end of input"));
    }

    let span = tokens[*pos].1;

    match &tokens[*pos].0 {
        Token::Minus => {
            *pos += 1;
            let inner = parse_expr_bp(tokens, pos, 11)?;
            Ok(Expr::new(ExprKind::Negate(Box::new(inner)), span))
        }
        Token::PlusPlus => {
            *pos += 1;
            if *pos < tokens.len() {
                if let Token::Variable(name) = &tokens[*pos].0 {
                    let name = name.clone();
                    *pos += 1;
                    return Ok(Expr::new(ExprKind::PreIncrement(name), span));
                }
            }
            Err(CompileError::new(span, "Expected variable after '++'"))
        }
        Token::MinusMinus => {
            *pos += 1;
            if *pos < tokens.len() {
                if let Token::Variable(name) = &tokens[*pos].0 {
                    let name = name.clone();
                    *pos += 1;
                    return Ok(Expr::new(ExprKind::PreDecrement(name), span));
                }
            }
            Err(CompileError::new(span, "Expected variable after '--'"))
        }
        Token::StringLiteral(s) => {
            let s = s.clone();
            *pos += 1;
            Ok(Expr::new(ExprKind::StringLiteral(s), span))
        }
        Token::IntLiteral(n) => {
            let n = *n;
            *pos += 1;
            Ok(Expr::new(ExprKind::IntLiteral(n), span))
        }
        Token::Variable(name) => {
            let name = name.clone();
            *pos += 1;
            // Check for postfix ++/--
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
                    _ => {}
                }
            }
            Ok(Expr::new(ExprKind::Variable(name), span))
        }
        Token::LParen => {
            *pos += 1;
            let expr = parse_expr(tokens, pos)?;
            if *pos < tokens.len() && tokens[*pos].0 == Token::RParen {
                *pos += 1;
                Ok(expr)
            } else {
                Err(CompileError::new(span, "Expected closing ')'"))
            }
        }
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token: {:?}", other),
        )),
    }
}
