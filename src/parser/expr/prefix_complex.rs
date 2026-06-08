//! Purpose:
//! Parses complex prefix expressions that need multi-token bodies or nested parser coordination.
//! Handles match expressions, closures, arrow functions, named expressions, and object construction.
//!
//! Called from:
//! - `crate::parser::expr::prefix::parse_prefix()`.
//!
//! Key details:
//! - Closure bodies and parameter defaults must preserve their own spans and PHP evaluation context.

use std::collections::HashSet;

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind,
};
use crate::parser::stmt::{looks_like_typed_param, parse_block, parse_name, parse_type_expr};
use crate::span::Span;

use super::calls::parse_first_class_callable_parens;
use super::{parse_args, parse_expr};

/// Parses a PHP `match` expression: `match ($subject) { pattern => result, default => fallback }`.
/// Consumes the `match` keyword, parenthesized subject expression, and the braced arm list.
/// Handles comma-separated patterns within a single arm, and an optional `default =>` fallback arm.
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

/// Parses a PHP 8.0 attributed closure or arrow function: `#[Attr] function() { … }` or `#[Attr] fn() => …`.
/// Consumes attribute lists (discarded for now), then dispatches to `parse_closure` or `parse_arrow_closure`.
/// Supports static and non-static variants, and the `static` keyword between attributes and the callable.
pub(super) fn parse_attributed_closure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    // PHP 8.0 allows `#[Foo] function() {…}`, `#[Foo] fn() => …`, and the
    // static variants. Attributes parse for shape only and are discarded.
    crate::parser::consume_attribute_lists(tokens, pos)?;
    let span = tokens.get(*pos).map(|(_, s)| *s).unwrap_or(span);
    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Function) => parse_closure(tokens, pos, span, false),
        Some(Token::Fn) => parse_arrow_closure(tokens, pos, span, false),
        Some(Token::Static) => match tokens.get(*pos + 1).map(|(t, _)| t) {
            Some(Token::Function) => {
                *pos += 1;
                parse_closure(tokens, pos, span, true)
            }
            Some(Token::Fn) => {
                *pos += 1;
                parse_arrow_closure(tokens, pos, span, true)
            }
            _ => Err(CompileError::new(
                span,
                "Expected closure or arrow function after attribute group",
            )),
        },
        _ => Err(CompileError::new(
            span,
            "Expected closure or arrow function after attribute group",
        )),
    }
}

/// Parses a PHP `function(...) use ($capture) { ... }` closure.
/// Consumes the `function` keyword and parameter list, an optional `use ($vars)` capture clause,
/// an optional `: ReturnType` annotation, and the block body. Sets `is_arrow: false`.
pub(super) fn parse_closure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    is_static: bool,
) -> Result<Expr, CompileError> {
    if *pos + 1 >= tokens.len() || tokens[*pos + 1].0 != Token::LParen {
        return Err(CompileError::new(span, "Unexpected token: Function"));
    }
    *pos += 2;
    let (params, variadic) = parse_closure_params(tokens, pos, span)?;
    let mut captures = Vec::new();
    let mut capture_refs = Vec::new();
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
            let is_ref = if tokens.get(*pos).map(|(token, _)| token) == Some(&Token::Ampersand) {
                *pos += 1;
                true
            } else {
                false
            };
            match tokens.get(*pos).map(|(token, _)| token) {
                Some(Token::Variable(name)) => {
                    if is_ref {
                        capture_refs.push(name.clone());
                    }
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
    let return_type = parse_optional_closure_return_type(tokens, pos, span)?;
    let body = parse_block(tokens, pos)?;
    Ok(Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow: false,
            is_static,
            captures,
            capture_refs,
        },
        span,
    ))
}

/// Parses a PHP arrow function: `fn(...) => expr`.
/// Consumes the `fn` keyword and parameter list, optional `: ReturnType`, then the `=>` and body expression.
/// Wraps the body expression in a `Return` statement and stores implicit by-value captures.
pub(super) fn parse_arrow_closure(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    is_static: bool,
) -> Result<Expr, CompileError> {
    *pos += 1;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after 'fn'"));
    }
    *pos += 1;
    let (params, variadic) = parse_closure_params(tokens, pos, span)?;
    let return_type = parse_optional_closure_return_type(tokens, pos, span)?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::DoubleArrow {
        return Err(CompileError::new(
            span,
            "Expected '=>' after arrow function parameters",
        ));
    }
    *pos += 1;
    let body_expr = parse_expr(tokens, pos)?;
    let captures = infer_arrow_captures(&body_expr, &params, variadic.as_ref());
    let body = vec![Stmt::new(StmtKind::Return(Some(body_expr)), span)];
    Ok(Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            return_type,
            body,
            is_arrow: true,
            is_static,
            captures,
            capture_refs: vec![],
        },
        span,
    ))
}

/// Infers PHP arrow-function implicit captures from free variable reads in the body expression.
fn infer_arrow_captures(
    body_expr: &Expr,
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&String>,
) -> Vec<String> {
    let mut bound = HashSet::new();
    for (name, _, _, _) in params {
        bound.insert(name.clone());
    }
    if let Some(name) = variadic {
        bound.insert(name.clone());
    }

    let mut captures = Vec::new();
    let mut seen = HashSet::new();
    collect_arrow_expr_captures(body_expr, &bound, &mut seen, &mut captures);
    captures
}

/// Records `name` as an arrow capture unless it is bound locally or already recorded.
fn push_arrow_capture(
    name: &str,
    bound: &HashSet<String>,
    seen: &mut HashSet<String>,
    captures: &mut Vec<String>,
) {
    if bound.contains(name) || name.starts_with("__elephc") {
        return;
    }
    if seen.insert(name.to_string()) {
        captures.push(name.to_string());
    }
}

/// Recursively collects variables that an arrow function body reads from its enclosing scope.
fn collect_arrow_expr_captures(
    expr: &Expr,
    bound: &HashSet<String>,
    seen: &mut HashSet<String>,
    captures: &mut Vec<String>,
) {
    match &expr.kind {
        ExprKind::Variable(name)
        | ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => push_arrow_capture(name, bound, seen, captures),
        ExprKind::BinaryOp { left, right, .. } => {
            collect_arrow_expr_captures(left, bound, seen, captures);
            collect_arrow_expr_captures(right, bound, seen, captures);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_arrow_expr_captures(value, bound, seen, captures);
            if let crate::parser::ast::InstanceOfTarget::Expr(target_expr) = target {
                collect_arrow_expr_captures(target_expr, bound, seen, captures);
            }
        }
        ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::YieldFrom(inner) => collect_arrow_expr_captures(inner, bound, seen, captures),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            collect_arrow_expr_captures(value, bound, seen, captures);
            collect_arrow_expr_captures(default, bound, seen, captures);
        }
        ExprKind::Pipe { value, callable } => {
            collect_arrow_expr_captures(value, bound, seen, captures);
            collect_arrow_expr_captures(callable, bound, seen, captures);
        }
        ExprKind::Assignment { target, value, .. } => {
            if !matches!(target.kind, ExprKind::Variable(_)) {
                collect_arrow_expr_captures(target, bound, seen, captures);
            }
            collect_arrow_expr_captures(value, bound, seen, captures);
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewScopedObject { args, .. }
        | ExprKind::StaticMethodCall { args, .. } => {
            for arg in args {
                collect_arrow_expr_captures(arg, bound, seen, captures);
            }
        }
        ExprKind::NewDynamicObject {
            class_name, args, ..
        } => {
            collect_arrow_expr_captures(class_name, bound, seen, captures);
            for arg in args {
                collect_arrow_expr_captures(arg, bound, seen, captures);
            }
        }
        ExprKind::NewDynamic { name_expr, args } => {
            collect_arrow_expr_captures(name_expr, bound, seen, captures);
            for arg in args {
                collect_arrow_expr_captures(arg, bound, seen, captures);
            }
        }
        ExprKind::ClosureCall { var, args } => {
            push_arrow_capture(var, bound, seen, captures);
            for arg in args {
                collect_arrow_expr_captures(arg, bound, seen, captures);
            }
        }
        ExprKind::ExprCall { callee, args } => {
            collect_arrow_expr_captures(callee, bound, seen, captures);
            for arg in args {
                collect_arrow_expr_captures(arg, bound, seen, captures);
            }
        }
        ExprKind::ArrayLiteral(items) => {
            for item in items {
                collect_arrow_expr_captures(item, bound, seen, captures);
            }
        }
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_arrow_expr_captures(key, bound, seen, captures);
                collect_arrow_expr_captures(value, bound, seen, captures);
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_arrow_expr_captures(subject, bound, seen, captures);
            for (patterns, result) in arms {
                for pattern in patterns {
                    collect_arrow_expr_captures(pattern, bound, seen, captures);
                }
                collect_arrow_expr_captures(result, bound, seen, captures);
            }
            if let Some(default) = default {
                collect_arrow_expr_captures(default, bound, seen, captures);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_arrow_expr_captures(array, bound, seen, captures);
            collect_arrow_expr_captures(index, bound, seen, captures);
        }
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => {
            collect_arrow_expr_captures(condition, bound, seen, captures);
            collect_arrow_expr_captures(then_expr, bound, seen, captures);
            collect_arrow_expr_captures(else_expr, bound, seen, captures);
        }
        ExprKind::Closure { captures: nested, .. } => {
            for name in nested {
                push_arrow_capture(name, bound, seen, captures);
            }
        }
        ExprKind::NamedArg { value, .. } => {
            collect_arrow_expr_captures(value, bound, seen, captures);
        }
        ExprKind::PropertyAccess { object, .. }
        | ExprKind::NullsafePropertyAccess { object, .. } => {
            collect_arrow_expr_captures(object, bound, seen, captures);
        }
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_arrow_expr_captures(object, bound, seen, captures);
            collect_arrow_expr_captures(property, bound, seen, captures);
        }
        ExprKind::MethodCall { object, args, .. }
        | ExprKind::NullsafeMethodCall { object, args, .. } => {
            collect_arrow_expr_captures(object, bound, seen, captures);
            for arg in args {
                collect_arrow_expr_captures(arg, bound, seen, captures);
            }
        }
        ExprKind::FirstClassCallable(CallableTarget::Method { object, .. }) => {
            collect_arrow_expr_captures(object, bound, seen, captures);
        }
        ExprKind::BufferNew { len, .. } => {
            collect_arrow_expr_captures(len, bound, seen, captures);
        }
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_arrow_expr_captures(key, bound, seen, captures);
            }
            if let Some(value) = value {
                collect_arrow_expr_captures(value, bound, seen, captures);
            }
        }
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::FirstClassCallable(_)
        | ExprKind::This
        | ExprKind::MagicConstant(_) => {}
    }
}

/// Parses the optional `: ReturnType` clause after a closure's parameter list.
/// Returns `Some(TypeExpr)` if a colon is present, otherwise `None`.
fn parse_optional_closure_return_type(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Option<crate::parser::ast::TypeExpr>, CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        Ok(Some(parse_type_expr(tokens, pos, span)?))
    } else {
        Ok(None)
    }
}

/// Parses the comma-separated parameter list inside a closure's `(` `)`.
/// Consumes typed parameters, by-reference `&`, variadic `...`, default values, and PHP 8.0 parameter attributes.
/// Returns the parameter list and an optional variadic parameter name.
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
            // Allow a trailing comma before the closing paren (PHP 8.0+).
            if *pos < tokens.len() && tokens[*pos].0 == Token::RParen {
                break;
            }
        }
        // PHP 8.0 closure-parameter attributes (`fn(#[X] $a) => …`).
        crate::parser::consume_attribute_lists(tokens, pos)?;
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

/// Parses a named expression that could be a constant reference, function call, buffer_new<T>, ptr_cast<T>, or static/class method access.
/// Disambiguates based on the token that follows the name: `(` for calls, `<T>` for buffer_new/ptr_cast, `::` for static access.
/// On `new` after a name, delegates to `parse_new_object`; otherwise returns a `ConstRef` if no suffix matches.
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
            Some(Token::Variable(property)) => {
                let property = property.clone();
                *pos += 1;
                return Ok(Expr::new(
                    ExprKind::StaticPropertyAccess {
                        receiver: StaticReceiver::Named(name),
                        property,
                    },
                    span,
                ));
            }
            Some(Token::Class) => {
                *pos += 1;
                return Ok(Expr::new(
                    ExprKind::ClassConstant {
                        receiver: StaticReceiver::Named(name),
                    },
                    span,
                ));
            }
            Some(Token::Identifier(member)) => {
                let member = member.clone();
                *pos += 1;
                member
            }
            Some(Token::Match) => {
                *pos += 1;
                "MATCH".to_string()
            }
            // PHP 8 allows semi-reserved keywords as static method / class-constant names
            // (e.g. `Foo::self()`, `Foo::print`); `class` and `$var` are handled above.
            Some(t) if crate::parser::keyword_name::bareword_name_from_token(t).is_some() => {
                let member = crate::parser::keyword_name::bareword_name_from_token(t).unwrap();
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
            // `Foo::BAR` (no parens) is either an enum case access or a
            // user-declared class constant. Disambiguation is done by the
            // type checker, which falls back from enum lookup to class const.
            Ok(Expr::new(
                ExprKind::ScopedConstantAccess {
                    receiver: StaticReceiver::Named(name),
                    name: member,
                },
                span,
            ))
        }
    } else {
        Ok(Expr::new(ExprKind::ConstRef(name), span))
    }
}

/// Parses a PHP `new` object construction: `new Class(...)`, `new self(...)`, `new static(...)`, or `new parent(...)`.
/// Consumes the `new` keyword, then handles late-static-binding receivers (`self`/`static`/`parent`) separately from class-name construction.
/// Returns `NewScopedObject` for the former and `NewObject` for the latter.
pub(super) fn parse_new_object(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;

    // `new self()`, `new static()`, `new parent()` — late-static-binding
    // factory pattern. Parsed as a NewScopedObject so codegen can apply LSB
    // for `static`.
    let scoped_receiver = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Self_) => Some(StaticReceiver::Self_),
        Some(Token::Static) => Some(StaticReceiver::Static),
        Some(Token::Parent) => Some(StaticReceiver::Parent),
        _ => None,
    };
    if let Some(receiver) = scoped_receiver {
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(
                span,
                "Expected '(' after self/static/parent",
            ));
        }
        *pos += 1;
        let args = parse_args(tokens, pos, span)?;
        return Ok(Expr::new(
            ExprKind::NewScopedObject { receiver, args },
            span,
        ));
    }

    // `new $variable(args)` — the class name is held in a variable; we'll
    // resolve it through the runtime class table at codegen time.
    if let Some((Token::Variable(name), _)) = tokens.get(*pos) {
        let var_name = name.clone();
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            return Err(CompileError::new(
                span,
                "Expected '(' after class-name variable in 'new $var('",
            ));
        }
        *pos += 1;
        let args = parse_args(tokens, pos, span)?;
        return Ok(Expr::new(
            ExprKind::NewDynamic {
                name_expr: Box::new(Expr::new(ExprKind::Variable(var_name), span)),
                args,
            },
            span,
        ));
    }

    let class_name = parse_name(tokens, pos, span, "Expected class name after 'new'")?;
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after class name"));
    }
    *pos += 1;
    let args = parse_args(tokens, pos, span)?;
    Ok(Expr::new(ExprKind::NewObject { class_name, args }, span))
}
