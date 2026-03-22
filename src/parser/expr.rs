use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{BinOp, CastType, Expr, ExprKind};
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

    // Postfix array access: $expr[index]
    while *pos < tokens.len() && tokens[*pos].0 == Token::LBracket {
        let span = tokens[*pos].1;
        *pos += 1;
        let index = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
            return Err(CompileError::new(span, "Expected ']'"));
        }
        *pos += 1;
        lhs = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(lhs),
                index: Box::new(index),
            },
            span,
        );
    }

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

    // Check for ternary operator (lowest precedence)
    if *pos < tokens.len() && tokens[*pos].0 == Token::Question && min_bp == 0 {
        let span = tokens[*pos].1;
        *pos += 1;
        let then_expr = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::Colon {
            return Err(CompileError::new(span, "Expected ':' in ternary operator"));
        }
        *pos += 1;
        let else_expr = parse_expr_bp(tokens, pos, 0)?;
        lhs = Expr::new(
            ExprKind::Ternary {
                condition: Box::new(lhs),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
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
        Token::OrOr         => Some((BinOp::Or,     1, 2)),
        Token::AndAnd       => Some((BinOp::And,    3, 4)),
        Token::Dot          => Some((BinOp::Concat, 5, 6)),
        Token::EqualEqual      => Some((BinOp::Eq,         7, 8)),
        Token::NotEqual        => Some((BinOp::NotEq,      7, 8)),
        Token::EqualEqualEqual => Some((BinOp::StrictEq,   7, 8)),
        Token::NotEqualEqual   => Some((BinOp::StrictNotEq,7, 8)),
        Token::Less         => Some((BinOp::Lt,     9, 10)),
        Token::Greater      => Some((BinOp::Gt,     9, 10)),
        Token::LessEqual    => Some((BinOp::LtEq,   9, 10)),
        Token::GreaterEqual => Some((BinOp::GtEq,   9, 10)),
        Token::Plus         => Some((BinOp::Add,   11, 12)),
        Token::Minus        => Some((BinOp::Sub,   11, 12)),
        Token::Star         => Some((BinOp::Mul,   13, 14)),
        Token::Slash        => Some((BinOp::Div,   13, 14)),
        Token::Percent      => Some((BinOp::Mod,   13, 14)),
        Token::StarStar     => Some((BinOp::Pow,   17, 16)), // right-associative, above unary
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
            let inner = parse_expr_bp(tokens, pos, 15)?;
            Ok(Expr::new(ExprKind::Negate(Box::new(inner)), span))
        }
        Token::Bang => {
            *pos += 1;
            let inner = parse_expr_bp(tokens, pos, 15)?;
            Ok(Expr::new(ExprKind::Not(Box::new(inner)), span))
        }
        Token::True => {
            *pos += 1;
            Ok(Expr::new(ExprKind::BoolLiteral(true), span))
        }
        Token::False => {
            *pos += 1;
            Ok(Expr::new(ExprKind::BoolLiteral(false), span))
        }
        Token::Null => {
            *pos += 1;
            Ok(Expr::new(ExprKind::Null, span))
        }
        Token::Inf => {
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(f64::INFINITY), span))
        }
        Token::Nan => {
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(f64::NAN), span))
        }
        Token::PhpIntMax => {
            *pos += 1;
            Ok(Expr::new(ExprKind::IntLiteral(i64::MAX), span))
        }
        Token::PhpIntMin => {
            *pos += 1;
            Ok(Expr::new(ExprKind::IntLiteral(i64::MIN), span))
        }
        Token::PhpFloatMax => {
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(f64::MAX), span))
        }
        Token::MPi => {
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(std::f64::consts::PI), span))
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
        Token::FloatLiteral(f) => {
            let f = *f;
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(f), span))
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
            // Check for type cast: (int), (float), (string), (bool), (array)
            if let Some(cast_ty) = peek_cast(tokens, *pos) {
                *pos += 3; // skip (, type, )
                let inner = parse_expr_bp(tokens, pos, 15)?;
                return Ok(Expr::new(
                    ExprKind::Cast { target: cast_ty, expr: Box::new(inner) },
                    span,
                ));
            }
            *pos += 1;
            let expr = parse_expr(tokens, pos)?;
            if *pos < tokens.len() && tokens[*pos].0 == Token::RParen {
                *pos += 1;
                Ok(expr)
            } else {
                Err(CompileError::new(span, "Expected closing ')'"))
            }
        }
        Token::LBracket => {
            *pos += 1;
            let mut elems = Vec::new();
            while *pos < tokens.len() && tokens[*pos].0 != Token::RBracket {
                if !elems.is_empty() {
                    if tokens[*pos].0 != Token::Comma {
                        return Err(CompileError::new(tokens[*pos].1, "Expected ',' between array elements"));
                    }
                    *pos += 1;
                    // Allow trailing comma
                    if *pos < tokens.len() && tokens[*pos].0 == Token::RBracket {
                        break;
                    }
                }
                elems.push(parse_expr(tokens, pos)?);
            }
            if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
                return Err(CompileError::new(span, "Expected ']'"));
            }
            *pos += 1;
            Ok(Expr::new(ExprKind::ArrayLiteral(elems), span))
        }
        Token::Identifier(name) => {
            let name = name.clone();
            *pos += 1;
            // Must be a function call
            if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                *pos += 1;
                let mut args = Vec::new();
                while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
                    if !args.is_empty() {
                        if tokens[*pos].0 != Token::Comma {
                            return Err(CompileError::new(tokens[*pos].1, "Expected ',' between arguments"));
                        }
                        *pos += 1;
                    }
                    args.push(parse_expr(tokens, pos)?);
                }
                if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
                    return Err(CompileError::new(span, "Expected ')' after arguments"));
                }
                *pos += 1;
                Ok(Expr::new(ExprKind::FunctionCall { name, args }, span))
            } else {
                Err(CompileError::new(span, &format!("Unexpected identifier: '{}'", name)))
            }
        }
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token: {:?}", other),
        )),
    }
}

/// Check if tokens at `pos` form a type cast: (int), (float), (string), (bool), (array)
fn peek_cast(tokens: &[(Token, Span)], pos: usize) -> Option<CastType> {
    if pos + 2 >= tokens.len() {
        return None;
    }
    if tokens[pos].0 != Token::LParen {
        return None;
    }
    if tokens[pos + 2].0 != Token::RParen {
        return None;
    }
    match &tokens[pos + 1].0 {
        Token::Identifier(name) => match name.as_str() {
            "int" | "integer" => Some(CastType::Int),
            "float" | "double" | "real" => Some(CastType::Float),
            "string" => Some(CastType::String),
            "bool" | "boolean" => Some(CastType::Bool),
            "array" => Some(CastType::Array),
            _ => None,
        },
        _ => None,
    }
}
