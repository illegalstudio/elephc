//! Purpose:
//! Parses expression suffixes that involve calls, casts, static receivers, and first-class callables.
//! Recognizes scoped static call forms and PHP cast syntax around grouped expressions.
//!
//! Called from:
//! - `crate::parser::expr::pratt` and `crate::parser::expr::prefix`.
//!
//! Key details:
//! - Callable and cast disambiguation depends on PHP token spelling and case-insensitive type names.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{CallableTarget, CastType, Expr, ExprKind, StaticReceiver};
use crate::span::Span;

use super::parse_args;

/// Parses the `::` suffix of a scoped static access (static method call, static property
/// access, class constant, or scoped constant access) after the initial receiver has been
/// consumed. Consumes the `::` token, then dispatches on the following token to produce the
/// appropriate `ExprKind` variant. Returns a static method call if a `(` follows, a static
/// property access if a variable follows, a class constant if `class` follows, or a scoped
/// constant access otherwise.
pub(super) fn parse_scoped_static_call(
    tokens: &[SpannedToken],
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
    let method = match tokens.get(*pos).map(|(token, _)| token) {
        // `self::$method(args)` calls the method NAMED by the local `$method`; only without
        // the parentheses is `self::$name` a static property.
        Some(Token::Variable(method))
            if matches!(tokens.get(*pos + 1), Some((Token::LParen, _))) =>
        {
            let method = Expr::new(ExprKind::Variable(method.clone()), tokens[*pos].1.span);
            *pos += 1;
            return finish_scoped_dynamic_method_call(tokens, pos, span, receiver, method);
        }
        // `static::{'should' . $unit}(args)`: a computed method name.
        Some(Token::LBrace) => {
            let method = parse_braced_member_name(tokens, pos, span)?;
            return finish_scoped_dynamic_method_call(tokens, pos, span, receiver, method);
        }
        Some(Token::Variable(property)) => {
            let property = property.clone();
            *pos += 1;
            return Ok(Expr::new(
                ExprKind::StaticPropertyAccess { receiver, property },
                span,
            ));
        }
        Some(Token::Class) => {
            *pos += 1;
            return Ok(Expr::new(ExprKind::ClassConstant { receiver }, span));
        }
        // PHP 8 allows semi-reserved keywords as static method / class-constant names
        // (e.g. `Foo::self()`, `Foo::print`); `class` and `$var` are handled above.
        Some(t)
            if crate::parser::keyword_name::bareword_name_from_token(t, &tokens[*pos].1)
                .is_some() =>
        {
            let name =
                crate::parser::keyword_name::bareword_name_from_token(t, &tokens[*pos].1).unwrap();
            *pos += 1;
            name
        }
        _ => {
            return Err(CompileError::new(
                span,
                &format!("Expected method or property name after '{}::'", receiver_name),
            ))
        }
    };
    // If a `(` follows, this is a static method call; otherwise it's a
    // user-declared class-constant access (`MyClass::FOO`).
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Ok(Expr::new(
            ExprKind::ScopedConstantAccess {
                receiver,
                name: method,
            },
            span,
        ));
    }
    *pos += 1;
    if parse_first_class_callable_parens(tokens, pos)? {
        Ok(Expr::new(
            ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }),
            span,
        ))
    } else {
        let args = parse_args(tokens, pos, span)?;
        let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
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

/// Parses the braced `{expr}` member name after `::`, consuming through the `}`.
pub(super) fn parse_braced_member_name(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1; // consume '{'
    let member = super::parse_expr(tokens, pos)?;
    if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::RBrace)) {
        return Err(CompileError::new(span, "Expected '}'"));
    }
    *pos += 1;
    Ok(member)
}

/// Finishes `Receiver::$method(args)` / `Receiver::{expr}(args)` on a class receiver that is
/// named in the source (`self`, `static`, `parent`, `C`) with a method name only known at run
/// time. Positioned at the `(`.
///
/// A literal braced name (`C::{'m'}()`) is the ordinary call. Otherwise the call becomes
/// `__elephc_dynamic_static_call(<Receiver>::class, $name, ...args)`: a `ClassConstant`
/// receiver marks the scoped form, which `crate::ir_lower::expr::dynamic_class_access` expands
/// into a runtime method-NAME dispatch over the receiver class's methods, each arm lowering the
/// ordinary `Receiver::m(...)` so late static binding and `$this` forwarding stay PHP's.
pub(super) fn finish_scoped_dynamic_method_call(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    receiver: StaticReceiver,
    method: Expr,
) -> Result<Expr, CompileError> {
    if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::LParen)) {
        // `C::{expr}` without a call is PHP 8.3's dynamic class-constant fetch.
        return Err(CompileError::new(
            span,
            "Dynamic class constant fetch with a computed name (`Class::{expr}`) is not supported",
        ));
    }
    *pos += 1; // consume '('
    if parse_first_class_callable_parens(tokens, pos)? {
        if let ExprKind::StringLiteral(method) = method.kind {
            return Ok(Expr::new(
                ExprKind::FirstClassCallable(CallableTarget::StaticMethod { receiver, method }),
                span,
            ));
        }
        return Err(CompileError::new(
            span,
            "First-class callable syntax on a static method with a runtime name (`Class::$method(...)`) is not supported; use `Closure::fromCallable([Class::class, $method])`",
        ));
    }
    let args = parse_args(tokens, pos, span)?;
    let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
    if let ExprKind::StringLiteral(method) = method.kind {
        return Ok(Expr::new(
            ExprKind::StaticMethodCall { receiver, method, args },
            span,
        ));
    }
    super::pratt::reject_named_args_in_dynamic_call(&args, span)?;
    let mut call_args = vec![
        Expr::new(ExprKind::ClassConstant { receiver }, span),
        method,
    ];
    call_args.extend(args);
    Ok(Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("__elephc_dynamic_static_call"),
            args: call_args,
        },
        span,
    ))
}

/// Checks whether `...)` appears at the current position, indicating PHP's first-class callable
/// syntax `Name::method(...)`. Returns `true` and consumes both the ellipsis and `)` tokens if
/// found; returns `false` and advances nothing otherwise. Called after the initial `(` of a
/// method-call-like form has already been consumed by the caller.
pub(super) fn parse_first_class_callable_parens(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<bool, CompileError> {
    if *pos + 1 < tokens.len()
        && tokens[*pos].0 == Token::Ellipsis
        && tokens[*pos + 1].0 == Token::RParen
    {
        *pos += 2;
        return Ok(true);
    }
    Ok(false)
}

/// Peeks ahead to detect a PHP cast expression `(type)` where the middle token is an
/// identifier matching a valid cast type name. Does not consume any tokens. Returns the
/// matching `CastType` variant or `None` if the three-token window does not form a cast.
pub(super) fn peek_cast(tokens: &[SpannedToken], pos: usize) -> Option<CastType> {
    if pos + 2 >= tokens.len() {
        return None;
    }
    if tokens[pos].0 != Token::LParen || tokens[pos + 2].0 != Token::RParen {
        return None;
    }
    match &tokens[pos + 1].0 {
        Token::Identifier(name) if matches_case_insensitive(name, &["int", "integer"]) => {
            Some(CastType::Int)
        }
        Token::Identifier(name) if matches_case_insensitive(name, &["float", "double", "real"]) => {
            Some(CastType::Float)
        }
        Token::Identifier(name) if name.eq_ignore_ascii_case("string") => Some(CastType::String),
        Token::Identifier(name) if matches_case_insensitive(name, &["bool", "boolean"]) => {
            Some(CastType::Bool)
        }
        Token::Identifier(name) if name.eq_ignore_ascii_case("array") => Some(CastType::Array),
        Token::Identifier(name) if name.eq_ignore_ascii_case("object") => Some(CastType::Object),
        _ => None,
    }
}

/// Returns `true` if `name` matches any of the `keywords` case-insensitively.
fn matches_case_insensitive(name: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .any(|keyword| name.eq_ignore_ascii_case(keyword))
}
