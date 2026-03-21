use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{BinOp, Expr};

pub fn parse_expr(tokens: &[Token], pos: &mut usize) -> Result<Expr, CompileError> {
    parse_concat(tokens, pos)
}

fn parse_concat(tokens: &[Token], pos: &mut usize) -> Result<Expr, CompileError> {
    let mut left = parse_additive(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Dot => {
                *pos += 1;
                let right = parse_additive(tokens, pos)?;
                left = Expr::BinaryOp {
                    left: Box::new(left),
                    op: BinOp::Concat,
                    right: Box::new(right),
                };
            }
            _ => break,
        }
    }

    Ok(left)
}

fn parse_additive(tokens: &[Token], pos: &mut usize) -> Result<Expr, CompileError> {
    let mut left = parse_multiplicative(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Plus => {
                *pos += 1;
                let right = parse_multiplicative(tokens, pos)?;
                left = Expr::BinaryOp {
                    left: Box::new(left),
                    op: BinOp::Add,
                    right: Box::new(right),
                };
            }
            Token::Minus => {
                *pos += 1;
                let right = parse_multiplicative(tokens, pos)?;
                left = Expr::BinaryOp {
                    left: Box::new(left),
                    op: BinOp::Sub,
                    right: Box::new(right),
                };
            }
            _ => break,
        }
    }

    Ok(left)
}

fn parse_multiplicative(tokens: &[Token], pos: &mut usize) -> Result<Expr, CompileError> {
    let mut left = parse_unary(tokens, pos)?;

    while *pos < tokens.len() {
        match &tokens[*pos] {
            Token::Star => {
                *pos += 1;
                let right = parse_unary(tokens, pos)?;
                left = Expr::BinaryOp {
                    left: Box::new(left),
                    op: BinOp::Mul,
                    right: Box::new(right),
                };
            }
            Token::Slash => {
                *pos += 1;
                let right = parse_unary(tokens, pos)?;
                left = Expr::BinaryOp {
                    left: Box::new(left),
                    op: BinOp::Div,
                    right: Box::new(right),
                };
            }
            _ => break,
        }
    }

    Ok(left)
}

fn parse_unary(tokens: &[Token], pos: &mut usize) -> Result<Expr, CompileError> {
    if *pos < tokens.len() && tokens[*pos] == Token::Minus {
        *pos += 1;
        let expr = parse_primary(tokens, pos)?;
        return Ok(Expr::Negate(Box::new(expr)));
    }
    parse_primary(tokens, pos)
}

fn parse_primary(tokens: &[Token], pos: &mut usize) -> Result<Expr, CompileError> {
    if *pos >= tokens.len() {
        return Err(CompileError::at(0, 0, "Unexpected end of input"));
    }

    match &tokens[*pos] {
        Token::StringLiteral(s) => {
            let s = s.clone();
            *pos += 1;
            Ok(Expr::StringLiteral(s))
        }
        Token::IntLiteral(n) => {
            let n = *n;
            *pos += 1;
            Ok(Expr::IntLiteral(n))
        }
        Token::Variable(name) => {
            let name = name.clone();
            *pos += 1;
            Ok(Expr::Variable(name))
        }
        Token::LParen => {
            *pos += 1;
            let expr = parse_expr(tokens, pos)?;
            if *pos < tokens.len() && tokens[*pos] == Token::RParen {
                *pos += 1;
                Ok(expr)
            } else {
                Err(CompileError::at(0, 0, "Expected closing ')'"))
            }
        }
        other => Err(CompileError::at(
            0,
            0,
            &format!("Unexpected token: {:?}", other),
        )),
    }
}
