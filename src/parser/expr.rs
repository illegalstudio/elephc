use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{
    BinOp, CallableTarget, CastType, Expr, ExprKind, StaticReceiver, Stmt, StmtKind,
};
use crate::parser::stmt::{parse_name, parse_type_expr};
use crate::span::Span;

pub fn parse_expr(tokens: &[(Token, Span)], pos: &mut usize) -> Result<Expr, CompileError> {
    parse_expr_bp(tokens, pos, 0)
}

/// Parse a comma-separated argument list. The opening `(` must already be consumed.
/// Consumes through the closing `)`.
pub(crate) fn parse_args(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    err_span: Span,
) -> Result<Vec<Expr>, CompileError> {
    let mut args = Vec::new();
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !args.is_empty() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1,
                    "Expected ',' between arguments",
                ));
            }
            *pos += 1;
        }
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            let spread_span = tokens[*pos].1;
            *pos += 1;
            let inner = parse_expr(tokens, pos)?;
            args.push(Expr::new(ExprKind::Spread(Box::new(inner)), spread_span));
        } else {
            args.push(parse_expr(tokens, pos)?);
        }
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
        return Err(CompileError::new(err_span, "Expected ')' after arguments"));
    }
    *pos += 1;
    Ok(args)
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

    // Postfix property/method access: $obj->prop or $obj->method(args)
    while *pos < tokens.len() && tokens[*pos].0 == Token::Arrow {
        let arrow_span = tokens[*pos].1;
        *pos += 1; // consume ->
        let member_name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(n)) => {
                let n = n.clone();
                *pos += 1;
                n
            }
            _ => {
                return Err(CompileError::new(
                    arrow_span,
                    "Expected property or method name after '->'",
                ))
            }
        };
        // Check if method call: ->method(args)
        if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
            *pos += 1;
            if parse_first_class_callable_parens(tokens, pos, arrow_span)? {
                lhs = Expr::new(
                    ExprKind::FirstClassCallable(CallableTarget::Method {
                        object: Box::new(lhs),
                        method: member_name,
                    }),
                    arrow_span,
                );
            } else {
                let args = parse_args(tokens, pos, arrow_span)?;
                lhs = Expr::new(
                    ExprKind::MethodCall {
                        object: Box::new(lhs),
                        method: member_name,
                        args,
                    },
                    arrow_span,
                );
            }
        } else {
            // Property access: ->prop
            lhs = Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(lhs),
                    property: member_name,
                },
                arrow_span,
            );
        }
    }

    // Postfix call on expression result: $arr[0](args), $f()(), etc.
    while *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
        if matches!(
            lhs.kind,
            ExprKind::ArrayAccess { .. }
                | ExprKind::ExprCall { .. }
                | ExprKind::ClosureCall { .. }
                | ExprKind::FunctionCall { .. }
        ) {
            let call_span = tokens[*pos].1;
            *pos += 1;
            let args = parse_args(tokens, pos, call_span)?;
            lhs = Expr::new(
                ExprKind::ExprCall {
                    callee: Box::new(lhs),
                    args,
                },
                call_span,
            );
        } else {
            break;
        }
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
        if op == BinOp::NullCoalesce {
            lhs = Expr::new(
                ExprKind::NullCoalesce {
                    value: Box::new(lhs),
                    default: Box::new(rhs),
                },
                span,
            );
        } else {
            lhs = Expr::new(
                ExprKind::BinaryOp {
                    left: Box::new(lhs),
                    op,
                    right: Box::new(rhs),
                },
                span,
            );
        }
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
///
/// PHP precedence (high to low):
///   ** (right-assoc) > unary ~ - ! > * / % > + - > . > << >> >
///   < <= > >= <=> > == != === !== > & > ^ > | > && > || > ??
fn infix_bp(token: &Token) -> Option<(BinOp, u8, u8)> {
    match token {
        Token::QuestionQuestion => Some((BinOp::NullCoalesce, 2, 1)), // right-associative, lowest binop
        Token::OrOr => Some((BinOp::Or, 3, 4)),
        Token::AndAnd => Some((BinOp::And, 5, 6)),
        Token::Pipe => Some((BinOp::BitOr, 7, 8)),
        Token::Caret => Some((BinOp::BitXor, 9, 10)),
        Token::Ampersand => Some((BinOp::BitAnd, 11, 12)),
        Token::EqualEqual => Some((BinOp::Eq, 13, 14)),
        Token::NotEqual => Some((BinOp::NotEq, 13, 14)),
        Token::EqualEqualEqual => Some((BinOp::StrictEq, 13, 14)),
        Token::NotEqualEqual => Some((BinOp::StrictNotEq, 13, 14)),
        Token::Less => Some((BinOp::Lt, 15, 16)),
        Token::Greater => Some((BinOp::Gt, 15, 16)),
        Token::LessEqual => Some((BinOp::LtEq, 15, 16)),
        Token::GreaterEqual => Some((BinOp::GtEq, 15, 16)),
        Token::Spaceship => Some((BinOp::Spaceship, 15, 16)),
        Token::LessLess => Some((BinOp::ShiftLeft, 17, 18)),
        Token::GreaterGreater => Some((BinOp::ShiftRight, 17, 18)),
        Token::Dot => Some((BinOp::Concat, 19, 20)),
        Token::Plus => Some((BinOp::Add, 21, 22)),
        Token::Minus => Some((BinOp::Sub, 21, 22)),
        Token::Star => Some((BinOp::Mul, 23, 24)),
        Token::Slash => Some((BinOp::Div, 23, 24)),
        Token::Percent => Some((BinOp::Mod, 23, 24)),
        Token::StarStar => Some((BinOp::Pow, 29, 28)), // right-associative, above unary
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
            let inner = parse_expr_bp(tokens, pos, 27)?;
            Ok(Expr::new(ExprKind::Negate(Box::new(inner)), span))
        }
        Token::Bang => {
            *pos += 1;
            let inner = parse_expr_bp(tokens, pos, 27)?;
            Ok(Expr::new(ExprKind::Not(Box::new(inner)), span))
        }
        Token::Tilde => {
            *pos += 1;
            let inner = parse_expr_bp(tokens, pos, 27)?;
            Ok(Expr::new(ExprKind::BitNot(Box::new(inner)), span))
        }
        Token::Throw => {
            *pos += 1;
            let inner = parse_expr_bp(tokens, pos, 0)?;
            Ok(Expr::new(ExprKind::Throw(Box::new(inner)), span))
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
            Ok(Expr::new(
                ExprKind::FloatLiteral(std::f64::consts::PI),
                span,
            ))
        }
        Token::ME => {
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(std::f64::consts::E), span))
        }
        Token::MSqrt2 => {
            *pos += 1;
            Ok(Expr::new(
                ExprKind::FloatLiteral(std::f64::consts::SQRT_2),
                span,
            ))
        }
        Token::MPi2 => {
            *pos += 1;
            Ok(Expr::new(
                ExprKind::FloatLiteral(std::f64::consts::FRAC_PI_2),
                span,
            ))
        }
        Token::MPi4 => {
            *pos += 1;
            Ok(Expr::new(
                ExprKind::FloatLiteral(std::f64::consts::FRAC_PI_4),
                span,
            ))
        }
        Token::MLog2e => {
            *pos += 1;
            Ok(Expr::new(
                ExprKind::FloatLiteral(std::f64::consts::LOG2_E),
                span,
            ))
        }
        Token::MLog10e => {
            *pos += 1;
            Ok(Expr::new(
                ExprKind::FloatLiteral(std::f64::consts::LOG10_E),
                span,
            ))
        }
        Token::PhpFloatMin => {
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(f64::MIN_POSITIVE), span))
        }
        Token::PhpFloatEpsilon => {
            *pos += 1;
            Ok(Expr::new(ExprKind::FloatLiteral(f64::EPSILON), span))
        }
        Token::Stdin => {
            *pos += 1;
            Ok(Expr::new(ExprKind::IntLiteral(0), span))
        }
        Token::Stdout => {
            *pos += 1;
            Ok(Expr::new(ExprKind::IntLiteral(1), span))
        }
        Token::Stderr => {
            *pos += 1;
            Ok(Expr::new(ExprKind::IntLiteral(2), span))
        }
        Token::PhpEol => {
            *pos += 1;
            Ok(Expr::new(ExprKind::StringLiteral("\n".to_string()), span))
        }
        Token::PhpOs => {
            *pos += 1;
            Ok(Expr::new(
                ExprKind::StringLiteral("Darwin".to_string()),
                span,
            ))
        }
        Token::DirectorySeparator => {
            *pos += 1;
            Ok(Expr::new(ExprKind::StringLiteral("/".to_string()), span))
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
                    // Closure call: $fn(args)
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
        Token::LParen => {
            // Check for type cast: (int), (float), (string), (bool), (array)
            if let Some(cast_ty) = peek_cast(tokens, *pos) {
                *pos += 3; // skip (, type, )
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
            // IIFE: (function() { ... })(args) — call expression result
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
        Token::LBracket => {
            *pos += 1;
            // Check if this is an associative array (first elem has =>)
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
                    // Allow trailing comma
                    if *pos < tokens.len() && tokens[*pos].0 == Token::RBracket {
                        break;
                    }
                }
                // Check for spread operator: ...expr
                if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
                    let spread_span = tokens[*pos].1;
                    *pos += 1;
                    let inner = parse_expr(tokens, pos)?;
                    elems.push(Expr::new(ExprKind::Spread(Box::new(inner)), spread_span));
                    first = false;
                    continue;
                }
                let expr = parse_expr(tokens, pos)?;
                // Check for => (associative array)
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
        Token::Match => {
            *pos += 1;
            // match (subject) { expr => result, ... }
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
                    // optional trailing comma
                    if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                        *pos += 1;
                    }
                } else {
                    // Parse one or more patterns separated by commas before =>
                    let mut patterns = Vec::new();
                    loop {
                        patterns.push(parse_expr(tokens, pos)?);
                        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                            // peek ahead to see if next token is => (then this comma separates patterns)
                            // or if it's something else (then the arm ended)
                            // Actually in PHP, comma before => separates multiple patterns for same arm
                            // Check: is there a => coming after more expressions?
                            // Simple approach: if after comma we see => then break, otherwise continue
                            let saved = *pos;
                            *pos += 1;
                            if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
                                *pos = saved; // undo — this comma is from between arms
                                break;
                            }
                            // This comma separates patterns for the same arm
                            // pos already advanced past comma, continue parsing next pattern
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
                    // optional trailing comma
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
        Token::Function => {
            // Anonymous function: function($x, $y) { ... }
            // Only parse as expression if next token is '(' (no name)
            if *pos + 1 < tokens.len() && tokens[*pos + 1].0 == Token::LParen {
                *pos += 1; // consume 'function'
                *pos += 1; // consume '('
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
                    let type_ann = if crate::parser::stmt::looks_like_typed_param(tokens, *pos) {
                        Some(crate::parser::stmt::parse_type_expr(tokens, pos, span)?)
                    } else {
                        None
                    };
                    let is_ref = if *pos < tokens.len() && tokens[*pos].0 == Token::Ampersand {
                        *pos += 1;
                        true
                    } else {
                        false
                    };
                    // Check for ... (variadic)
                    if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
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
                            _ => {
                                return Err(CompileError::new(
                                    span,
                                    "Expected variable after '...'",
                                ))
                            }
                        }
                        continue;
                    }
                    match tokens.get(*pos).map(|(t, _)| t) {
                        Some(Token::Variable(n)) => {
                            let n = n.clone();
                            *pos += 1;
                            // Check for default value
                            let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign
                            {
                                *pos += 1;
                                Some(parse_expr(tokens, pos)?)
                            } else {
                                None
                            };
                            params.push((n, type_ann, default, is_ref));
                        }
                        _ => return Err(CompileError::new(span, "Expected parameter variable")),
                    }
                }
                if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
                    return Err(CompileError::new(span, "Expected ')' after parameters"));
                }
                *pos += 1;
                // Parse optional `use ($var1, $var2, ...)` capture list
                let mut captures = Vec::new();
                if *pos < tokens.len() && tokens[*pos].0 == Token::Use {
                    *pos += 1; // consume 'use'
                    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
                        return Err(CompileError::new(span, "Expected '(' after 'use'"));
                    }
                    *pos += 1; // consume '('
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
                        match tokens.get(*pos).map(|(t, _)| t) {
                            Some(Token::Variable(n)) => {
                                captures.push(n.clone());
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
                    *pos += 1; // consume ')'
                }
                let body = crate::parser::stmt::parse_block(tokens, pos)?;
                return Ok(Expr::new(
                    ExprKind::Closure {
                        params,
                        variadic,
                        body,
                        is_arrow: false,
                        captures,
                    },
                    span,
                ));
            }
            Err(CompileError::new(span, "Unexpected token: Function"))
        }
        Token::Fn => {
            // Arrow function: fn($x) => expr
            *pos += 1; // consume 'fn'
            if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
                return Err(CompileError::new(span, "Expected '(' after 'fn'"));
            }
            *pos += 1; // consume '('
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
                let type_ann = if crate::parser::stmt::looks_like_typed_param(tokens, *pos) {
                    Some(crate::parser::stmt::parse_type_expr(tokens, pos, span)?)
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
                    match tokens.get(*pos).map(|(t, _)| t) {
                        Some(Token::Variable(n)) => {
                            variadic = Some(n.clone());
                            *pos += 1;
                        }
                        _ => return Err(CompileError::new(span, "Expected variable after '...'")),
                    }
                    continue;
                }
                match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Variable(n)) => {
                        let n = n.clone();
                        *pos += 1;
                        // Check for default value
                        let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                            *pos += 1;
                            Some(parse_expr(tokens, pos)?)
                        } else {
                            None
                        };
                        params.push((n, type_ann, default, is_ref));
                    }
                    _ => return Err(CompileError::new(span, "Expected parameter variable")),
                }
            }
            if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
                return Err(CompileError::new(
                    span,
                    "Expected ')' after arrow function parameters",
                ));
            }
            *pos += 1;
            // Expect =>
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
        Token::Identifier(_) | Token::Backslash => {
            let name = parse_name(tokens, pos, span, "Expected name")?;
            if name.parts.len() == 1
                && name.parts[0] == "buffer_new"
                && *pos < tokens.len()
                && tokens[*pos].0 == Token::Less
            {
                *pos += 1; // consume <
                let element_type = parse_type_expr(tokens, pos, span)?;
                if *pos >= tokens.len() || tokens[*pos].0 != Token::Greater {
                    return Err(CompileError::new(span, "Expected '>' after buffer_new<T"));
                }
                *pos += 1; // consume >
                if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
                    return Err(CompileError::new(span, "Expected '(' after buffer_new<T>"));
                }
                *pos += 1; // consume (
                let len = parse_expr(tokens, pos)?;
                if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
                    return Err(CompileError::new(
                        span,
                        "Expected ')' after buffer_new length",
                    ));
                }
                *pos += 1; // consume )
                return Ok(Expr::new(
                    ExprKind::BufferNew {
                        element_type,
                        len: Box::new(len),
                    },
                    span,
                ));
            }
            // ptr_cast<T>(expr) — generic pointer cast
            if name.parts.len() == 1
                && name.parts[0] == "ptr_cast"
                && *pos < tokens.len()
                && tokens[*pos].0 == Token::Less
            {
                *pos += 1; // consume <
                let target_type =
                    parse_name(tokens, pos, span, "Expected type name after 'ptr_cast<'")?
                        .as_canonical();
                if *pos >= tokens.len() || tokens[*pos].0 != Token::Greater {
                    return Err(CompileError::new(span, "Expected '>' after ptr_cast<T"));
                }
                *pos += 1; // consume >
                if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
                    return Err(CompileError::new(span, "Expected '(' after ptr_cast<T>"));
                }
                *pos += 1; // consume (
                let expr = parse_expr(tokens, pos)?;
                if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
                    return Err(CompileError::new(
                        span,
                        "Expected ')' after ptr_cast argument",
                    ));
                }
                *pos += 1; // consume )
                return Ok(Expr::new(
                    ExprKind::PtrCast {
                        target_type,
                        expr: Box::new(expr),
                    },
                    span,
                ));
            }
            // Function call: name(...)
            if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                *pos += 1;
                if parse_first_class_callable_parens(tokens, pos, span)? {
                    Ok(Expr::new(
                        ExprKind::FirstClassCallable(CallableTarget::Function(name)),
                        span,
                    ))
                } else {
                    let args = parse_args(tokens, pos, span)?;
                    Ok(Expr::new(ExprKind::FunctionCall { name, args }, span))
                }
            } else if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleColon {
                // Static member access: ClassName::method(args) or EnumName::Case
                *pos += 1; // consume ::
                let member = match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Identifier(m)) => {
                        let m = m.clone();
                        *pos += 1;
                        m
                    }
                    _ => return Err(CompileError::new(span, "Expected member name after '::'")),
                };
                if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                    *pos += 1;
                    if parse_first_class_callable_parens(tokens, pos, span)? {
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
                // Bare identifier — treat as constant reference (validated by type checker)
                Ok(Expr::new(ExprKind::ConstRef(name), span))
            }
        }
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
        Token::New => {
            *pos += 1; // consume 'new'
            let class_name = parse_name(tokens, pos, span, "Expected class name after 'new'")?;
            // Parse constructor arguments
            if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
                return Err(CompileError::new(span, "Expected '(' after class name"));
            }
            *pos += 1;
            let args = parse_args(tokens, pos, span)?;
            Ok(Expr::new(ExprKind::NewObject { class_name, args }, span))
        }
        Token::This => {
            *pos += 1;
            Ok(Expr::new(ExprKind::This, span))
        }
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token: {:?}", other),
        )),
    }
}

fn parse_scoped_static_call(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    receiver: StaticReceiver,
    receiver_name: &str,
) -> Result<Expr, CompileError> {
    if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleColon {
        return Err(CompileError::new(
            span,
            &format!("Expected '::' after '{}'", receiver_name),
        ));
    }
    *pos += 1;
    let method = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(method)) => {
            let method = method.clone();
            *pos += 1;
            method
        }
        _ => {
            return Err(CompileError::new(
                span,
                &format!("Expected method name after '{}::'", receiver_name),
            ))
        }
    };
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(
            span,
            &format!("Expected '(' after {} method name", receiver_name),
        ));
    }
    *pos += 1;
    if parse_first_class_callable_parens(tokens, pos, span)? {
        Ok(Expr::new(
            ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }),
            span,
        ))
    } else {
        let args = parse_args(tokens, pos, span)?;
        Ok(Expr::new(
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args,
            },
            span,
        ))
    }
}

fn parse_first_class_callable_parens(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    _span: Span,
) -> Result<bool, CompileError> {
    if *pos + 1 < tokens.len()
        && tokens[*pos].0 == Token::Ellipsis
        && tokens[*pos + 1].0 == Token::RParen
    {
        *pos += 2; // consume ... )
        return Ok(true);
    }
    Ok(false)
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
