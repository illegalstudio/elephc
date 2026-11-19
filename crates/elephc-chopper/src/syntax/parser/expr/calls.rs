//! Purpose:
//! Parses expression suffixes that involve calls, casts, static receivers, and first-class callables.
//! Recognizes scoped static call forms and PHP cast syntax around grouped expressions.
//!
//! Called from:
//! - `crate::syntax::parser::expr::pratt` and `crate::syntax::parser::expr::prefix`.
//!
//! Key details:
//! - Callable and cast disambiguation depends on PHP token spelling and case-insensitive type names.

use crate::syntax::parser::validation_only;
use crate::syntax::errors::CompileError;
use crate::syntax::lexer::{SpannedToken, Token};
use crate::syntax::parser::ast::{CallableTarget, CastType, Expr, ExprKind, StaticReceiver};
use crate::syntax::span::Span;

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
    if crate::syntax::parser::runtime_fragment() && super::runtime_members::has_computed_name(tokens, *pos) {
        return super::runtime_members::parse_static_member(tokens, pos, span,
            crate::syntax::parser::ast::RuntimeClassReceiver::Named(receiver));
    }
    let method = match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Variable(property)) => {
            let property = property.clone();
            *pos += 1;
            // TOKEN_PARSE accepts `self::$method(args)`; the compiler does not yet preserve
            // both the declaring scope and the late-static called class for a dynamic method
            // name, so it keeps this as a static-property access and refuses the trailing call
            // instead of lowering it to a class-name callback that would bind `static` to the
            // wrong class.
            if validation_only() && *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                *pos += 1; // consume '('
                let dynamic_args = parse_args(tokens, pos, span)?;
                let span = crate::syntax::parser::expr::span_through_prev_token(tokens, *pos, span);
                crate::syntax::parser::expr::pratt::reject_named_args_in_dynamic_call(
                    &dynamic_args,
                    span,
                )?;
                let receiver = Expr::new(
                    ExprKind::ClassConstant {
                        receiver: receiver.clone(),
                    },
                    span,
                );
                let method = Expr::new(ExprKind::Variable(property), span);
                let mut args =
                    vec![Expr::new(ExprKind::ArrayLiteral(vec![receiver, method]), span)];
                args.extend(dynamic_args);
                return Ok(Expr::new(
                    ExprKind::FunctionCall {
                        name: crate::syntax::names::Name::unqualified("call_user_func"),
                        args,
                    },
                    span,
                ));
            }
            return Ok(Expr::new(
                ExprKind::StaticPropertyAccess { receiver, property },
                span,
            ));
        }
        Some(Token::Class) => {
            crate::syntax::parser::tokenizer_feedback::record(&tokens[*pos].1, "class");
            *pos += 1;
            if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                // `A::class(...)` is a static method call to a method literally named
                // `class`, not a call of the `A::class` class-name string.
                *pos += 1;
                let args = parse_args(tokens, pos, span)?;
                let span = crate::syntax::parser::expr::span_through_prev_token(tokens, *pos, span);
                return Ok(Expr::new(
                    ExprKind::StaticMethodCall {
                        receiver,
                        method: "class".to_string(),
                        args,
                    },
                    span,
                ));
            }
            return Ok(Expr::new(ExprKind::ClassConstant { receiver }, span));
        }
        // PHP 8 allows semi-reserved keywords as static method / class-constant names
        // (e.g. `Foo::self()`, `Foo::print`); `class` and `$var` are handled above.
        Some(t)
            if crate::syntax::parser::keyword_name::bareword_name_from_token(t, &tokens[*pos].1)
                .is_some() =>
        {
            let name =
                crate::syntax::parser::keyword_name::bareword_name_from_token(t, &tokens[*pos].1).unwrap();
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
    if crate::syntax::parser::runtime_fragment() {
        if let Some(call) = super::runtime_members::try_parse_static_generic_method(tokens, pos, span, &receiver, &method)? {
            return Ok(call);
        }
    }
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
        let span = crate::syntax::parser::expr::span_through_prev_token(tokens, *pos, span);
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

/// Converts a dynamic callable expression once through the global Closure factory.
/// Keeping an ordinary AST call lets existing walkers preserve evaluation and ownership.
pub(super) fn dynamic_first_class_callable(callee: Expr, span: Span) -> Expr {
    Expr::new(
        ExprKind::StaticMethodCall {
            receiver: StaticReceiver::Named(crate::syntax::names::Name::from_parts(
                crate::syntax::names::NameKind::FullyQualified,
                vec!["Closure".to_string()],
            )),
            method: crate::syntax::parser::ast::DYNAMIC_CALLABLE_FACTORY.to_string(),
            args: vec![callee],
        },
        span,
    )
}
