//! Purpose:
//! Parses prefix and primary PHP expressions before Pratt suffix handling takes over.
//! Covers literals, variables, names, arrays, magic constants, unary operators, and grouped expressions.
//!
//! Called from:
//! - `crate::parser::expr::pratt::parse_expr_bp()`.
//!
//! Key details:
//! - `__LINE__` is lowered at parse time while other magic constants remain AST nodes for later context passes.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind, MagicConstant, StaticReceiver};
use crate::span::Span;

use super::calls::{parse_scoped_static_call, peek_cast};
use super::prefix_complex::{
    parse_arrow_closure, parse_attributed_closure, parse_closure, parse_match_expr,
    parse_named_expr, parse_new_object,
};
use super::pratt::parse_expr_bp;
use super::{parse_args, parse_expr};

/// Parses a prefix (unary or primary) PHP expression and returns the resulting AST node.
/// Dispatched from `parse_expr_bp` for the leftmost expression in a binding-power loop.
/// Advances `pos` past all tokens consumed by the prefix; the caller continues with the
/// remaining token stream. Returns an error on unexpected end of input or unrecognized tokens.
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
        Token::Minus => parse_unary(tokens, pos, span, ExprKind::Negate, 35),
        Token::Bang => parse_unary(tokens, pos, span, ExprKind::Not, 35),
        Token::Tilde => parse_unary(tokens, pos, span, ExprKind::BitNot, 35),
        Token::At => parse_unary(tokens, pos, span, ExprKind::ErrorSuppress, 35),
        Token::Print => parse_unary(tokens, pos, span, ExprKind::Print, 7),
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
        Token::Stdin => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::ConstRef(Name::unqualified("STDIN")),
        ),
        Token::Stdout => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::ConstRef(Name::unqualified("STDOUT")),
        ),
        Token::Stderr => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::ConstRef(Name::unqualified("STDERR")),
        ),
        Token::PhpEol => parse_simple(tokens, pos, span, ExprKind::StringLiteral("\n".to_string())),
        Token::PhpOs => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::ConstRef(Name::unqualified("PHP_OS")),
        ),
        Token::DirectorySeparator => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::StringLiteral("/".to_string()),
        ),
        Token::DunderLine => {
            parse_simple(tokens, pos, span, ExprKind::IntLiteral(span.line as i64))
        }
        Token::DunderDir => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Dir),
        ),
        Token::DunderFile => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::File),
        ),
        Token::DunderFunction => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Function),
        ),
        Token::DunderClass => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Class),
        ),
        Token::DunderMethod => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Method),
        ),
        Token::DunderNamespace => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Namespace),
        ),
        Token::DunderTrait => parse_simple(
            tokens,
            pos,
            span,
            ExprKind::MagicConstant(MagicConstant::Trait),
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
        Token::Function => parse_closure(tokens, pos, span, false),
        Token::Fn => parse_arrow_closure(tokens, pos, span, false),
        Token::AttrOpen => parse_attributed_closure(tokens, pos, span),
        Token::Identifier(_) | Token::Backslash => parse_named_expr(tokens, pos, span),
        Token::Self_ => {
            *pos += 1;
            parse_scoped_static_call(tokens, pos, span, StaticReceiver::Self_, "self")
        }
        Token::Static => {
            // `static function() {}` and `static fn() => ...` — closures that
            // do not capture $this. Routed here before parse_scoped_static_call.
            match tokens.get(*pos + 1).map(|(t, _)| t) {
                Some(Token::Function) => {
                    *pos += 1; // consume `static`, leave `function` for parse_closure
                    parse_closure(tokens, pos, span, true)
                }
                Some(Token::Fn) => {
                    *pos += 1; // consume `static`, leave `fn` for parse_arrow_closure
                    parse_arrow_closure(tokens, pos, span, true)
                }
                _ => {
                    *pos += 1;
                    parse_scoped_static_call(tokens, pos, span, StaticReceiver::Static, "static")
                }
            }
        }
        Token::Parent => {
            *pos += 1;
            parse_scoped_static_call(tokens, pos, span, StaticReceiver::Parent, "parent")
        }
        Token::New => parse_new_object(tokens, pos, span),
        Token::This => parse_simple(tokens, pos, span, ExprKind::This),
        Token::Yield => parse_yield(tokens, pos, span),
        other => Err(CompileError::new(
            span,
            &format!("Unexpected token: {:?}", other),
        )),
    }
}

/// Parses `yield` and `yield from` expressions. Consumes the `yield` token and optionally
/// parses a following expression or key => value pair. Returns a `Yield` or `YieldFrom` node
/// using the given span. On end of input or a terminating token, returns bare `Yield { key: None, value: None }`.
fn parse_yield(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;

    if *pos >= tokens.len() {
        return Ok(Expr::new(
            ExprKind::Yield {
                key: None,
                value: None,
            },
            span,
        ));
    }

    if let Token::Identifier(name) = &tokens[*pos].0 {
        if name.eq_ignore_ascii_case("from") {
            *pos += 1;
            let inner = parse_expr_bp(tokens, pos, 0)?;
            return Ok(Expr::new(ExprKind::YieldFrom(Box::new(inner)), span));
        }
    }

    match &tokens[*pos].0 {
        Token::Semicolon
        | Token::RParen
        | Token::RBracket
        | Token::RBrace
        | Token::Comma
        | Token::Eof => {
            return Ok(Expr::new(
                ExprKind::Yield {
                    key: None,
                    value: None,
                },
                span,
            ));
        }
        _ => {}
    }

    let first = parse_expr_bp(tokens, pos, 0)?;
    if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
        *pos += 1;
        let value = parse_expr_bp(tokens, pos, 0)?;
        return Ok(Expr::new(
            ExprKind::Yield {
                key: Some(Box::new(first)),
                value: Some(Box::new(value)),
            },
            span,
        ));
    }
    Ok(Expr::new(
        ExprKind::Yield {
            key: None,
            value: Some(Box::new(first)),
        },
        span,
    ))
}

/// Advances `pos` by one and wraps the given `ExprKind` and `Span` in a new `Expr`.
fn parse_simple(
    _tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    kind: ExprKind,
) -> Result<Expr, CompileError> {
    *pos += 1;
    Ok(Expr::new(kind, span))
}

/// Parses a unary operator expression. Consumes the operator token, advances `pos`,
/// then recursively parses the inner expression with the given binding power `bp` to
/// enforce precedence. The `ctor` function constructs the target `ExprKind` variant
/// (e.g., `Negate`, `Not`, `BitNot`). Returns the wrapped unary expression.
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

/// Parses a prefix `++` or `--` increment/decrement operator. Consumes the operator,
/// then expects a `Variable` token next. Returns `PreIncrement` or `PreDecrement` with the
/// variable name. Returns an error if a variable does not follow the operator.
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

/// Parses a variable expression starting with a `Variable` token. Consumes the variable name,
/// then checks for a following `++`, `--` (postfix form), or `(` (closure-call syntax like `$var(...)`).
/// Returns `Variable`, `PostIncrement`, `PostDecrement`, or `ClosureCall`. Advances `pos` past
/// any consumed postfix tokens.
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

/// Parses a grouped expression `(...)` or a type cast `(type) expr`. If `peek_cast` detects
/// a cast, consumes the cast syntax and returns a `Cast` node with the target type and inner
/// expression parsed at binding power 35 (the unary-operator level). This makes a cast bind
/// tighter than `* / % + - .` and the comparison/logical operators — so `(int)$x + 3` parses
/// as `((int)$x) + 3`, matching PHP — while `**` (left bp 37) still binds tighter than the cast.
/// Otherwise parses as a grouped expression: consumes `(` and `)`, then checks for an immediate
/// call (`inner(args)`) to support expression-call syntax.
fn parse_group_or_cast(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    if let Some(cast_ty) = peek_cast(tokens, *pos) {
        *pos += 3;
        let inner = parse_expr_bp(tokens, pos, 35)?;
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

/// Parses a `[...]` array literal.
///
/// Distinguishes indexed (`[a, b]`) and associative (`[key => value]`) forms while
/// preserving leading positional elements that appear before the first keyed entry.
/// Supports spread elements via `...`; spreads in keyed literals are parsed for
/// source-order progress but remain limited by the associative-array representation.
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
    let mut next_auto_key = 0i64;
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
            if !is_assoc {
                elems.push(Expr::new(ExprKind::Spread(Box::new(inner)), spread_span));
            }
            first = false;
            continue;
        }
        let expr = parse_expr(tokens, pos)?;
        if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
            if !is_assoc {
                promote_indexed_array_items_to_assoc(&mut elems, &mut assoc_elems);
            }
            is_assoc = true;
            *pos += 1;
            let value = parse_expr(tokens, pos)?;
            update_next_auto_key_from_explicit_key(&expr, &mut next_auto_key);
            assoc_elems.push((expr, value));
        } else if is_assoc {
            let key = Expr::new(ExprKind::IntLiteral(next_auto_key), expr.span);
            assoc_elems.push((key, expr));
            next_auto_key += 1;
        } else {
            elems.push(expr);
            next_auto_key += 1;
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

/// Converts positional items parsed before a keyed array entry into integer-keyed pairs.
fn promote_indexed_array_items_to_assoc(
    elems: &mut Vec<Expr>,
    assoc_elems: &mut Vec<(Expr, Expr)>,
) {
    let mut auto_key = 0i64;
    for elem in std::mem::take(elems) {
        if matches!(elem.kind, ExprKind::Spread(_)) {
            continue;
        }
        let key = Expr::new(ExprKind::IntLiteral(auto_key), elem.span);
        assoc_elems.push((key, elem));
        auto_key += 1;
    }
}

/// Advances the automatic integer key cursor after a statically known integer key.
fn update_next_auto_key_from_explicit_key(key: &Expr, next_auto_key: &mut i64) {
    if let ExprKind::IntLiteral(value) = &key.kind {
        if *value >= *next_auto_key {
            *next_auto_key = *value + 1;
        }
    }
}
