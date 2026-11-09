//! Purpose:
//! Parses complex prefix expressions that need multi-token bodies or nested parser coordination.
//! Handles match expressions, closures, arrow functions, named expressions, and object construction.
//!
//! Called from:
//! - `crate::parser::expr::prefix::parse_prefix()`.
//!
//! Key details:
//! - Closure bodies and parameter defaults must preserve their own spans and PHP evaluation context.

use crate::fast_hash::FastSet as HashSet;

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::names::Name;
use crate::parser::ast::{
    CallableTarget, Expr, ExprKind, StaticReceiver, Stmt, StmtKind, TypeExpr,
};
use crate::parser::stmt::{
    looks_like_typed_param, parse_anonymous_class, parse_name, parse_type_arguments_only, parse_type_expr,
};
use crate::span::Span;

use super::calls::parse_first_class_callable_parens;
use super::pratt::{parse_object_member, ObjectMember};
use super::{parse_args, parse_expr};

/// Consumes a single leading `&` token if present, returning whether one was seen.
///
/// Used to detect the by-reference-return marker in `fn &()` / `function &()`.
pub(crate) fn consume_optional_ampersand(tokens: &[SpannedToken], pos: &mut usize) -> bool {
    if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Ampersand)) {
        *pos += 1;
        true
    } else {
        false
    }
}

/// Parses a PHP `match` expression: `match ($subject) { pattern => result, default => fallback }`.
/// Consumes the `match` keyword, parenthesized subject expression, and the braced arm list.
/// Handles comma-separated patterns within a single arm, and an optional `default =>` fallback arm.
pub(super) fn parse_match_expr(
    tokens: &[SpannedToken],
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
        // A docblock is whitespace here. The lexer keeps one that follows `{` because a
        // declaration could start there, so the first arm of `match (...) { /** ... */ 1 => ...`
        // arrives with it in front.
        if matches!(tokens[*pos].0, Token::DocComment(_)) {
            *pos += 1;
            continue;
        }
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
                    *pos += 1;
                    if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleArrow {
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
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    // PHP 8.0 allows `#[Foo] function() {…}`, `#[Foo] fn() => …`, and the
    // static variants. Attributes parse for shape only and are discarded.
    crate::parser::consume_attribute_lists(tokens, pos)?;
    let span = tokens
        .get(*pos)
        .map(|(_, metadata)| metadata.span)
        .unwrap_or(span);
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
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    is_static: bool,
) -> Result<Expr, CompileError> {
    // `function &(...) use(...) {...}` returns a reference (alias) to the body lvalue.
    let mut after_fn = *pos + 1;
    let by_ref_return = consume_optional_ampersand(tokens, &mut after_fn);
    if after_fn >= tokens.len() || tokens[after_fn].0 != Token::LParen {
        return Err(CompileError::new(span, "Unexpected token: Function"));
    }
    *pos = after_fn + 1;
    let (params, variadic, variadic_by_ref, variadic_type) =
        parse_closure_params(tokens, pos, span)?;
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
                        tokens[*pos].1.span,
                        "Expected ',' between captured variables",
                    ));
                }
                *pos += 1;
                // PHP 8.0 admits a trailing comma: `use ($a, $b,)`.
                if tokens.get(*pos).map(|(token, _)| token) == Some(&Token::RParen) {
                    break;
                }
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
    let body = crate::parser::stmt::parse_executable_block(tokens, pos)?;
    let span = closure_extent_span(tokens, *pos, span);
    Ok(Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            body,
            is_arrow: false,
            is_static,
            by_ref_return,
            captures,
            capture_refs,
        },
        span,
    ))
}

/// Widens a closure literal's span to end at the last token its body consumed.
///
/// php names a closure `{closure:SCOPE:LINE}` and answers `ReflectionFunction::getEndLine()`
/// with the line of its closing `}` (or, for an arrow function, of its body's last token); the
/// point span the literal kept answered the start line for both.
fn closure_extent_span(tokens: &[SpannedToken], pos: usize, span: Span) -> Span {
    let Some(last) = pos.checked_sub(1).and_then(|index| tokens.get(index)) else {
        return span;
    };
    if last.1.span.end_line < span.line {
        return span;
    }
    Span {
        line: span.line,
        col: span.col,
        end_line: last.1.span.end_line,
        end_col: last.1.span.end_col,
    }
}

/// Parses a PHP arrow function: `fn(...) => expr`.
/// Consumes the `fn` keyword and parameter list, optional `: ReturnType`, then the `=>` and body expression.
/// Wraps the body expression in a `Return` statement and stores implicit by-value captures.
pub(super) fn parse_arrow_closure(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    is_static: bool,
) -> Result<Expr, CompileError> {
    *pos += 1;
    // PHP `fn &() => …` returns a reference (alias) to the body lvalue.
    let by_ref_return = consume_optional_ampersand(tokens, pos);
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Err(CompileError::new(span, "Expected '(' after 'fn'"));
    }
    *pos += 1;
    let (params, variadic, variadic_by_ref, variadic_type) =
        parse_closure_params(tokens, pos, span)?;
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
    let span = closure_extent_span(tokens, *pos, span);
    Ok(Expr::new(
        ExprKind::Closure {
            params,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            body,
            is_arrow: true,
            is_static,
            by_ref_return,
            captures,
            capture_refs: vec![],
        },
        span,
    ))
}

/// Builds the Closure `$receiver->$name(...)` / `$receiver::$name(...)` stands for.
///
/// PHP's first-class callable syntax on a RUNTIME method name has no static target to bind, so it
/// becomes what it means: a closure forwarding every argument to that method,
/// `fn(...$args) => call_user_func_array([$receiver, $name], $args)`, capturing the receiver and
/// the name by value as an arrow function does. Symfony's DeepClone polyfill rebuilds named
/// closures exactly this way (`$obj->$name(...)`, `$obj::$name(...)`).
pub(super) fn dynamic_member_closure(receiver: Expr, member: Expr, span: Span) -> Expr {
    let args = "__elephc_fcc_args".to_string();
    let body_expr = Expr::new(
        ExprKind::FunctionCall {
            name: crate::names::Name::unqualified("call_user_func_array"),
            args: vec![
                Expr::new(ExprKind::ArrayLiteral(vec![receiver, member]), span),
                Expr::new(ExprKind::Variable(args.clone()), span),
            ],
        },
        span,
    );
    let captures = infer_arrow_captures(&body_expr, &[], Some(&args));
    Expr::new(
        ExprKind::Closure {
            params: Vec::new(),
            variadic: Some(args),
            variadic_by_ref: false,
            variadic_type: None,
            return_type: None,
            body: vec![Stmt::new(StmtKind::Return(Some(body_expr)), span)],
            is_arrow: true,
            is_static: false,
            by_ref_return: false,
            captures,
            capture_refs: vec![],
        },
        span,
    )
}

/// Infers PHP arrow-function implicit captures from free variable reads in the body expression.
fn infer_arrow_captures(
    body_expr: &Expr,
    params: &[(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)],
    variadic: Option<&String>,
) -> Vec<String> {
    let mut bound = HashSet::default();
    for (name, _, _, _) in params {
        bound.insert(name.clone());
    }
    if let Some(name) = variadic {
        bound.insert(name.clone());
    }

    let mut captures = Vec::new();
    let mut seen = HashSet::default();
    collect_arrow_expr_captures(body_expr, &mut bound, &mut seen, &mut captures);
    // Superglobals (and `$GLOBALS`) are visible in every scope, so an arrow function reads them
    // directly instead of capturing a by-value copy of the enclosing scope's binding. Capturing
    // `$_SERVER` made the closure re-store the global from its copy on entry, and the
    // immediately invoked form returned a garbage string.
    captures.retain(|name| {
        !crate::globals_array::is_alias(name)
            && !crate::superglobals::is_superglobal(name)
            && name != "GLOBALS"
    });
    captures
}

/// Records `name` as an arrow capture unless it is bound locally or already recorded.
///
/// A compiler-generated temporary is recognised through [`crate::names::is_generated_local_name`],
/// the one shared predicate, rather than through an `__elephc` spelling. The prefix is forgeable
/// from source: `$__elephc_tag` is a perfectly legal PHP variable, and filtering on the prefix
/// silently dropped it from an arrow function's capture list, so the body read an unset variable.
pub(super) fn push_arrow_capture(
    name: &str,
    bound: &HashSet<String>,
    seen: &mut HashSet<String>,
    captures: &mut Vec<String>,
) {
    if bound.contains(name) || crate::names::is_generated_local_name(name) {
        return;
    }
    if seen.insert(name.to_string()) {
        captures.push(name.to_string());
    }
}

/// Recursively collects variables that an arrow function body reads from its enclosing scope.
pub(super) fn collect_arrow_expr_captures(
    expr: &Expr,
    bound: &mut HashSet<String>,
    seen: &mut HashSet<String>,
    captures: &mut Vec<String>,
) {
    match &expr.kind {
        ExprKind::Variable(name)
        | ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => push_arrow_capture(name, bound, seen, captures),
        ExprKind::IncludeValue { path, .. } => {
            collect_arrow_expr_captures(path, bound, seen, captures);
        }
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
        | ExprKind::ArrayReference(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::Clone(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::PtrCast { expr: inner, .. }
        | ExprKind::Cast { expr: inner, .. }
        | ExprKind::ObjectClassName { object: inner }
        | ExprKind::YieldFrom(inner) => collect_arrow_expr_captures(inner, bound, seen, captures),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            collect_arrow_expr_captures(value, bound, seen, captures);
            let mut default_bound = bound.clone();
            collect_arrow_expr_captures(default, &mut default_bound, seen, captures);
        }
        ExprKind::Pipe { value, callable } => {
            collect_arrow_expr_captures(value, bound, seen, captures);
            collect_arrow_expr_captures(callable, bound, seen, captures);
        }
        ExprKind::Assignment { target, value, prelude, result_target, .. } => {
            for statement in prelude {
                super::arrow_preludes::collect(statement, bound, seen, captures);
            }
            if let ExprKind::Variable(name) = &target.kind {
                collect_arrow_expr_captures(value, bound, seen, captures);
                bound.insert(name.clone());
            } else {
                collect_arrow_expr_captures(target, bound, seen, captures);
                collect_arrow_expr_captures(value, bound, seen, captures);
            }
            if let Some(result) = result_target {
                collect_arrow_expr_captures(result, bound, seen, captures);
            }
        }
        ExprKind::FunctionCall { args, .. }
        | ExprKind::NewObject { args, .. }
        | ExprKind::NewGeneric { args, .. }
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
        ExprKind::ArrayLiteralMixed(entries) => {
            for entry in entries.iter() {
                for expr in entry.exprs() {
                    collect_arrow_expr_captures(expr, bound, seen, captures);
                }
            }
        }
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            collect_arrow_expr_captures(subject, bound, seen, captures);
            let branch_entry = bound.clone();
            let mut branch_exit: Option<HashSet<String>> = None;
            for (patterns, result) in arms {
                let mut arm_bound = branch_entry.clone();
                for pattern in patterns {
                    collect_arrow_expr_captures(pattern, &mut arm_bound, seen, captures);
                }
                collect_arrow_expr_captures(result, &mut arm_bound, seen, captures);
                intersect_arrow_bound(&mut branch_exit, arm_bound);
            }
            if let Some(default) = default {
                let mut default_bound = branch_entry.clone();
                collect_arrow_expr_captures(default, &mut default_bound, seen, captures);
                intersect_arrow_bound(&mut branch_exit, default_bound);
            }
            if let Some(branch_exit) = branch_exit {
                *bound = branch_exit;
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
            let mut then_bound = bound.clone();
            let mut else_bound = bound.clone();
            collect_arrow_expr_captures(then_expr, &mut then_bound, seen, captures);
            collect_arrow_expr_captures(else_expr, &mut else_bound, seen, captures);
            then_bound.retain(|name| else_bound.contains(name));
            *bound = then_bound;
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
        ExprKind::NullsafeDynamicMethodCall {
            object,
            method,
            args,
        } => {
            collect_arrow_expr_captures(object, bound, seen, captures);
            collect_arrow_expr_captures(method, bound, seen, captures);
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
        ExprKind::DynamicStaticPropertyAccess { property, .. } => {
            collect_arrow_expr_captures(property, bound, seen, captures);
        }
        ExprKind::DynamicScopedConstantAccess { receiver, .. } => {
            collect_arrow_expr_captures(receiver, bound, seen, captures);
        }
    }
}

/// Intersects one arrow-function branch exit with the exits collected so far.
fn intersect_arrow_bound(
    branch_exit: &mut Option<HashSet<String>>,
    current: HashSet<String>,
) {
    if let Some(existing) = branch_exit {
        existing.retain(|name| current.contains(name));
    } else {
        *branch_exit = Some(current);
    }
}

/// Parses the optional `: ReturnType` clause after a closure's parameter list.
/// Returns `Some(TypeExpr)` if a colon is present, otherwise `None`.
fn parse_optional_closure_return_type(
    tokens: &[SpannedToken],
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
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<
    (
        Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
        Option<String>,
        bool,
        Option<crate::parser::ast::TypeExpr>,
    ),
    CompileError,
> {
    let mut params = Vec::new();
    let mut variadic = None;
    let mut variadic_by_ref = false;
    let mut variadic_type = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() || variadic.is_some() {
            if tokens[*pos].0 != Token::Comma {
                return Err(CompileError::new(
                    tokens[*pos].1.span,
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
            // A typed variadic (`int ...$xs`) is accepted on closures/arrow functions too; the
            // declared element type is preserved so call validation can check collected args.
            *pos += 1;
            match tokens.get(*pos).map(|(token, _)| token) {
                Some(Token::Variable(name)) => {
                    variadic = Some(name.clone());
                    variadic_by_ref = is_ref;
                    variadic_type = type_ann;
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
                // `T $x = null` is implicitly `?T` (see `parse_params`).
                let type_ann = match (type_ann, &default) {
                    (Some(ty), Some(default)) if matches!(default.kind, ExprKind::Null) => {
                        Some(crate::parser::stmt::implicitly_nullable_type(ty))
                    }
                    (ty, _) => ty,
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
    Ok((params, variadic, variadic_by_ref, variadic_type))
}

/// Parses the tail of a function-style call once the name and its opening `(` have been
/// consumed: `...)` yields the first-class callable for `name`, anything else goes through
/// the shared argument parser and becomes a `FunctionCall`. Shared by plain named calls and
/// the PHP 8.5 `clone(...)` function form.
pub(super) fn parse_function_call_or_callable(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    name: Name,
) -> Result<Expr, CompileError> {
    if parse_first_class_callable_parens(tokens, pos)? {
        Ok(Expr::new(
            ExprKind::FirstClassCallable(CallableTarget::Function(name)),
            span,
        ))
    } else {
        let args = parse_args(tokens, pos, span)?;
        let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
        Ok(Expr::new(ExprKind::FunctionCall { name, args }, span))
    }
}

/// Returns true for the construct-like names whose `<T>` is part of their own grammar.
///
/// `buffer_new<int>(8)` and `ptr_cast<Foo>($p)` take a type argument the way `new` takes a class
/// name: it belongs to the construct and produces a dedicated AST node, not an instantiation of
/// a template. They are matched unqualified because that is how they are declared — neither is
/// namespaced, and a userland `App\buffer_new` is an ordinary function.
fn has_dedicated_type_argument_grammar(name: &Name) -> bool {
    name.parts.len() == 1
        && matches!(name.parts[0].as_str(), "buffer_new" | "ptr_cast")
}

/// Parses a named expression that could be a constant reference, function call, buffer_new<T>, ptr_cast<T>, or static/class method access.
/// Disambiguates based on the token that follows the name: `(` for calls, `<T>` for buffer_new/ptr_cast, `::` for static access.
/// On `new` after a name, delegates to `parse_new_object`; otherwise returns a `ConstRef` if no suffix matches.
pub(super) fn parse_named_expr(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    let name = parse_name(tokens, pos, span, "Expected name")?;
    // `Box<int>::of(1)` — static access through a generic class. The type arguments are read
    // here rather than where `::` is handled, because `Name<...>` has to be told apart from a
    // comparison chain FIRST: `::` cannot follow a comparison, so the `::` after a balanced
    // argument list is what makes this unambiguous. Without the lookahead the expression parser
    // reads `Box < int > ::` as two comparisons and fails on a stray `::` several tokens later.
    let mut static_type_args: Vec<TypeExpr> = Vec::new();
    // `identity<int>(41)` — a call that WRITES its type arguments instead of leaving them to
    // inference. Disambiguated exactly like the `::` form above: a balanced argument list
    // followed by `(`. The comparison reading of the same tokens is `(identity < int) > (41)`,
    // which php-src rejects outright — `<` is non-associative in PHP 8 — so nothing a PHP
    // program could have meant is claimed here.
    //
    // The arguments go into the NAME, because the instantiated name is this compiler's encoding
    // for an instantiation everywhere else too (`Box<int>` is a real class). The checker splits
    // it back with `generics::instantiated_type`, so the two spellings cannot drift.
    let mut name = name;
    if let Some(after_args) = crate::parser::stmt::type_arguments_end(tokens, *pos) {
        match tokens.get(after_args).map(|(token, _)| token) {
            Some(&Token::DoubleColon) => {
                let (_, args) =
                    crate::parser::stmt::parse_inherited_name_at(tokens, pos, span, &name)?;
                static_type_args = args;
            }
            // `buffer_new<int>(8)` and `ptr_cast<Foo>($p)` are the same token shape with their
            // own grammar and their own AST nodes, parsed a few lines below. Their type argument
            // is part of the CONSTRUCT, not an instantiation of a template, so this branch must
            // leave them alone.
            Some(&Token::LParen) if !has_dedicated_type_argument_grammar(&name) => {
                let (_, args) =
                    crate::parser::stmt::parse_inherited_name_at(tokens, pos, span, &name)?;
                if !args.is_empty() {
                    name = name.with_written_type_arguments(&args);
                }
            }
            _ => {}
        }
    }
    let name = name;
    // PHP's legacy `array(...)` construct is an array literal, not a call:
    // its elements may use `key => value` pairs that call arguments reject.
    if name.parts.len() == 1
        && name.parts[0].eq_ignore_ascii_case("array")
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::LParen
    {
        return super::prefix::parse_legacy_array_literal(tokens, pos, span);
    }
    // `exit` and `die` are language constructs whose parentheses are optional: a bare `exit;`
    // is `exit()`. Read as a name it became a reference to the undefined constant `exit`, which
    // throws instead of ending the script -- and, since no branch ending in it was terminal,
    // `if ($u === false) { exit; }` stopped narrowing `$u` for the statements after it.
    if name.parts.len() == 1
        && (name.parts[0].eq_ignore_ascii_case("exit") || name.parts[0].eq_ignore_ascii_case("die"))
        && !(*pos < tokens.len() && tokens[*pos].0 == Token::LParen)
    {
        return Ok(Expr::new(
            ExprKind::FunctionCall {
                name,
                args: Vec::new(),
            },
            span,
        ));
    }
    // `buffer_new<>` lexes as the single `<>` token (PHP's `!=` alias).
    if name.parts.len() == 1
        && name.parts[0] == "buffer_new"
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::LessGreater
    {
        return Err(CompileError::new(
            span,
            "Expected buffer element type after 'buffer_new<'",
        ));
    }
    if name.parts.len() == 1
        && name.parts[0] == "buffer_new"
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::Less
    {
        let mut args = parse_type_arguments_only(tokens, pos, span)?;
        if args.len() != 1 {
            return Err(CompileError::new(span, "Expected one buffer element type"));
        }
        let element_type = args.remove(0);
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
    // `ptr_cast<>` lexes as the single `<>` token (PHP's `!=` alias), so the empty
    // type list is recognized here to keep reporting the missing type name.
    if name.parts.len() == 1
        && name.parts[0] == "ptr_cast"
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::LessGreater
    {
        return Err(CompileError::new(
            span,
            "Expected type name after 'ptr_cast<'",
        ));
    }
    if name.parts.len() == 1
        && name.parts[0] == "ptr_cast"
        && *pos < tokens.len()
        && tokens[*pos].0 == Token::Less
    {
        *pos += 1;
        let target_name = parse_name(tokens, pos, span, "Expected type name after 'ptr_cast<'")?;
        // Keep the leading `\` of a fully qualified target: the name resolver applies the current
        // namespace to every other spelling, so `ptr_cast<\Pair>` must stay distinguishable.
        let target_type = if target_name.is_fully_qualified() {
            format!("\\{}", target_name.as_canonical())
        } else {
            target_name.as_canonical()
        };
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
        parse_function_call_or_callable(tokens, pos, span, name)
    } else if *pos < tokens.len() && tokens[*pos].0 == Token::DoubleColon {
        *pos += 1;
        // Built once so every form below — method call, property, constant, `::class` — agrees
        // about whether this receiver is generic.
        let receiver = static_receiver(name, static_type_args);
        if matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Dollar)) {
            let property = super::calls::parse_dynamic_static_property_name(tokens, pos, span)?;
            return Ok(Expr::new(
                ExprKind::DynamicStaticPropertyAccess {
                    receiver: receiver.clone(),
                    property: Box::new(property),
                },
                span,
            ));
        }
        if let Some(expr) = super::calls::parse_braced_static_member(tokens, pos, span, &receiver)? {
            return Ok(expr);
        }
        let member = match tokens.get(*pos).map(|(token, _)| token) {
            Some(Token::Variable(property)) => {
                let property = property.clone();
                *pos += 1;
                if *pos < tokens.len() && tokens[*pos].0 == Token::LParen {
                    // `C::$method(args)` is a dynamic static method call. Desugar to
                    // `call_user_func([C::class, $method], ...args)` so it reuses the runtime
                    // dynamic-dispatch path, exactly like the variable-receiver form `$c::$m()`.
                    *pos += 1; // consume '('
                    let dynamic_args = parse_args(tokens, pos, span)?;
                    let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
                    let receiver = Expr::new(
                        ExprKind::ClassConstant {
                            receiver: receiver.clone(),
                        },
                        span,
                    );
                    let method_expr = Expr::new(ExprKind::Variable(property), span);
                    if crate::parser::expr::pratt::has_named_args(&dynamic_args) {
                        let target =
                            Expr::new(ExprKind::ArrayLiteral(vec![receiver, method_expr]), span);
                        return Ok(crate::parser::expr::pratt::named_args_dynamic_call(
                            target,
                            &dynamic_args,
                            span,
                        ));
                    }
                    let mut call_args =
                        vec![Expr::new(ExprKind::ArrayLiteral(vec![receiver, method_expr]), span)];
                    call_args.extend(dynamic_args);
                    return Ok(Expr::new(
                        ExprKind::FunctionCall {
                            name: crate::names::Name::unqualified("call_user_func"),
                            args: call_args,
                        },
                        span,
                    ));
                }
                return Ok(Expr::new(
                    ExprKind::StaticPropertyAccess {
                        receiver: receiver.clone(),
                        property,
                    },
                    span,
                ));
            }
            Some(Token::Class) => {
                *pos += 1;
                return Ok(Expr::new(
                    ExprKind::ClassConstant {
                        receiver: receiver.clone(),
                    },
                    span,
                ));
            }
            // PHP 8 allows semi-reserved keywords as static method / class-constant names
            // (e.g. `Foo::self()`, `Foo::print`); `class` and `$var` are handled above.
            Some(t)
                if crate::parser::keyword_name::bareword_name_from_token(t, &tokens[*pos].1)
                    .is_some() =>
            {
                let member =
                    crate::parser::keyword_name::bareword_name_from_token(t, &tokens[*pos].1)
                        .unwrap();
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
                        receiver: receiver.clone(),
                        method: member,
                    }),
                    span,
                ))
            } else {
                let args = parse_args(tokens, pos, span)?;
                let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
                Ok(Expr::new(
                    ExprKind::StaticMethodCall {
                        receiver: receiver.clone(),
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
                    receiver: receiver.clone(),
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
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Expr, CompileError> {
    *pos += 1;

    // `new class { ... }` / `new readonly class { ... }` — anonymous class. The body is hoisted
    // to a synthetic top-level class and this becomes `new <synthetic name>(args)`.
    let anonymous_readonly = matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::ReadOnly))
        && matches!(tokens.get(*pos + 1).map(|(t, _)| t), Some(Token::Class));
    if anonymous_readonly {
        *pos += 1; // consume `readonly`; `parse_anonymous_class` consumes `class`
    }
    if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Class)) {
        return parse_anonymous_class(tokens, pos, span, anonymous_readonly);
    }

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
        if let Some(name_expr) = parse_new_static_property_reference(tokens, pos, &receiver)? {
            return finish_new_dynamic(tokens, pos, name_expr, span);
        }
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            reject_parenthesis_free_new_postfix(tokens, *pos)?;
            return Ok(Expr::new(
                ExprKind::NewScopedObject { receiver, args: Vec::new() },
                span,
            ));
        }
        *pos += 1;
        let args = parse_args(tokens, pos, span)?;
        let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
        return Ok(Expr::new(
            ExprKind::NewScopedObject { receiver, args },
            span,
        ));
    }

    // A dynamic class name may dereference a variable through array offsets or properties.
    // PHP's `new_variable` production starts from any `simple_variable`, and `$this` is one of
    // them: `new $this->options['matcher_class'](...)` is how Symfony's routing component picks
    // its matcher class. `$this` lexes as its own token rather than `Token::Variable`, so a base
    // matched only against `Token::Variable` sends every `new $this->…` down the class-name path
    // and rejects the whole file.
    if let Some(base) = new_variable_base(tokens, *pos) {
        *pos += 1;
        let name_expr = parse_new_variable_chain(tokens, pos, base)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            reject_dynamic_new_class_reference(tokens, *pos)?;
            return Ok(Expr::new(
                ExprKind::NewDynamic {
                    name_expr: Box::new(name_expr),
                    args: Vec::new(),
                },
                span,
            ));
        }
        *pos += 1;
        let args = parse_args(tokens, pos, span)?;
        let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
        return Ok(Expr::new(
            ExprKind::NewDynamic {
                name_expr: Box::new(name_expr),
                args,
            },
            span,
        ));
    }

    // `new (expression)(args)` — PHP 8.0's parenthesized class-name reference. The parentheses
    // are unambiguous here: no other `new` form starts with one, and what they hold is an ordinary
    // expression evaluated to a class name or object. Twig's
    // `BinaryOperatorExpressionParser::parse` is `new ($this->nodeClass)($left, $right, …)`.
    if matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::LParen)) {
        *pos += 1;
        let name_expr = parse_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::RParen {
            return Err(CompileError::new(span, "Expected ')' after new class expression"));
        }
        *pos += 1;
        if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
            reject_dynamic_new_class_reference(tokens, *pos)?;
            return Ok(Expr::new(
                ExprKind::NewDynamic {
                    name_expr: Box::new(name_expr),
                    args: Vec::new(),
                },
                span,
            ));
        }
        *pos += 1;
        let args = parse_args(tokens, pos, span)?;
        let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
        return Ok(Expr::new(
            ExprKind::NewDynamic {
                name_expr: Box::new(name_expr),
                args,
            },
            span,
        ));
    }

    let (class_name, type_args) = crate::parser::stmt::parse_inherited_name(
        tokens,
        pos,
        span,
        "Expected class name after 'new'",
    )?;
    if type_args.is_empty() {
        let named_receiver = StaticReceiver::Named(class_name.clone());
        if let Some(name_expr) =
            parse_new_static_property_reference(tokens, pos, &named_receiver)?
        {
            return finish_new_dynamic(tokens, pos, name_expr, span);
        }
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        reject_parenthesis_free_new_postfix(tokens, *pos)?;
        return Ok(Expr::new(
            new_object_kind(class_name, type_args, Vec::new()),
            span,
        ));
    }
    *pos += 1;
    let args = parse_args(tokens, pos, span)?;
    let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
    Ok(Expr::new(new_object_kind(class_name, type_args, args), span))
}

/// Builds a static receiver, generic or not.
///
/// Written once so `Box<int>::of()`, `Box<int>::COUNT` and `Box<int>::class` cannot disagree
/// about when a receiver carries type arguments.
fn static_receiver(name: Name, type_args: Vec<TypeExpr>) -> StaticReceiver {
    if type_args.is_empty() {
        return StaticReceiver::Named(name);
    }
    StaticReceiver::Generic(TypeExpr::GenericClass {
        name,
        args: type_args,
    })
}

/// Builds the construction node for `new Name(...)`, generic or not.
///
/// Written once so the parenthesis-free form and the ordinary form cannot disagree about when a
/// construction is generic.
fn new_object_kind(class_name: Name, type_args: Vec<TypeExpr>, args: Vec<Expr>) -> ExprKind {
    if type_args.is_empty() {
        return ExprKind::NewObject { class_name, args };
    }
    ExprKind::NewGeneric {
        class_type: TypeExpr::GenericClass {
            name: class_name,
            args: type_args,
        },
        args,
    }
}

/// Parses a static property used as a `new` class name: `new static::$builder(...)`.
///
/// PHP's `new_variable` production includes `class_name '::' simple_variable`, so the class a
/// `new` instantiates may be the VALUE of a static property, optionally dereferenced further
/// (`new self::$map['key'](...)`). Laravel's `Application::configure()` is
/// `new static::$applicationBuilder(new static($basePath))`. Without this form the parser saw
/// `new static` followed by `::` and refused the whole file. Returns `None`, consuming nothing,
/// when the tokens at `pos` are not `:: $name`, so `new static` / `new Foo(...)` are unchanged.
fn parse_new_static_property_reference(
    tokens: &[SpannedToken],
    pos: &mut usize,
    receiver: &StaticReceiver,
) -> Result<Option<Expr>, CompileError> {
    if !matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::DoubleColon)) {
        return Ok(None);
    }
    let Some((Token::Variable(property), metadata)) = tokens.get(*pos + 1) else {
        return Ok(None);
    };
    let property_span = metadata.span;
    let base = Expr::new(
        ExprKind::StaticPropertyAccess {
            receiver: receiver.clone(),
            property: property.clone(),
        },
        property_span,
    );
    *pos += 2;
    parse_new_variable_chain(tokens, pos, base).map(Some)
}

/// Finishes a `new` whose class is named by an expression: optional constructor arguments, then
/// a `NewDynamic` node. Rejects a parenthesis-free form followed by a member access, as PHP does.
fn finish_new_dynamic(
    tokens: &[SpannedToken],
    pos: &mut usize,
    name_expr: Expr,
    span: Span,
) -> Result<Expr, CompileError> {
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        reject_dynamic_new_class_reference(tokens, *pos)?;
        return Ok(Expr::new(
            ExprKind::NewDynamic {
                name_expr: Box::new(name_expr),
                args: Vec::new(),
            },
            span,
        ));
    }
    *pos += 1;
    let args = parse_args(tokens, pos, span)?;
    let span = crate::parser::expr::span_through_prev_token(tokens, *pos, span);
    Ok(Expr::new(
        ExprKind::NewDynamic {
            name_expr: Box::new(name_expr),
            args,
        },
        span,
    ))
}

/// Returns the `simple_variable` a dynamic `new` class name dereferences from, if the token at
/// `pos` starts one.
///
/// PHP's `new_variable` production bottoms out in `simple_variable`, which covers both a plain
/// `$name` and `$this`; the two differ only in how they lex.
fn new_variable_base(tokens: &[SpannedToken], pos: usize) -> Option<Expr> {
    match tokens.get(pos) {
        Some((Token::Variable(name), metadata)) => {
            Some(Expr::new(ExprKind::Variable(name.clone()), metadata.span))
        }
        Some((Token::This, metadata)) => Some(Expr::new(ExprKind::This, metadata.span)),
        _ => None,
    }
}

/// Parses array-offset and property dereferences used as a dynamic `new` class name.
fn parse_new_variable_chain(
    tokens: &[SpannedToken],
    pos: &mut usize,
    base: Expr,
) -> Result<Expr, CompileError> {
    let mut expression = base;
    loop {
        match tokens.get(*pos).map(|(token, _)| token) {
            Some(Token::LBracket) => {
                let bracket_span = tokens[*pos].1.span;
                *pos += 1;
                let index = parse_expr(tokens, pos)?;
                if *pos >= tokens.len() || tokens[*pos].0 != Token::RBracket {
                    return Err(CompileError::new(bracket_span, "Expected ']'"));
                }
                *pos += 1;
                expression = Expr::new(
                    ExprKind::ArrayAccess {
                        array: Box::new(expression),
                        index: Box::new(index),
                    },
                    bracket_span,
                );
            }
            Some(Token::Arrow) => {
                let arrow_span = tokens[*pos].1.span;
                *pos += 1;
                expression = match parse_object_member(tokens, pos, arrow_span, false)? {
                    ObjectMember::Named(property) => Expr::new(
                        ExprKind::PropertyAccess {
                            object: Box::new(expression),
                            property,
                        },
                        arrow_span,
                    ),
                    ObjectMember::Dynamic(property) => Expr::new(
                        ExprKind::DynamicPropertyAccess {
                            object: Box::new(expression),
                            property: Box::new(property),
                        },
                        arrow_span,
                    ),
                };
            }
            _ => break,
        }
    }
    Ok(expression)
}

/// Rejects postfix access that would otherwise bind to `new Foo` without constructor parentheses.
fn reject_parenthesis_free_new_postfix(
    tokens: &[SpannedToken],
    pos: usize,
) -> Result<(), CompileError> {
    if let Some((token, token_span)) = tokens.get(pos) {
        if is_new_without_parens_postfix(token) {
            return Err(CompileError::new(
                token_span.span,
                "Parentheses are required before accessing a parenthesis-free new expression",
            ));
        }
    }
    Ok(())
}

/// Rejects unsupported dynamic class-name references before they become object postfix access.
fn reject_dynamic_new_class_reference(
    tokens: &[SpannedToken],
    pos: usize,
) -> Result<(), CompileError> {
    if let Some((token, token_span)) = tokens.get(pos) {
        if is_new_without_parens_postfix(token) {
            return Err(CompileError::new(
                token_span.span,
                "Dynamic class-name expressions after 'new' are not supported",
            ));
        }
    }
    Ok(())
}

/// Returns true for postfix tokens that cannot follow a parenthesis-free `new Foo` in PHP.
fn is_new_without_parens_postfix(token: &Token) -> bool {
    matches!(
        token,
        Token::Arrow | Token::QuestionArrow | Token::DoubleColon | Token::LBracket
    )
}
