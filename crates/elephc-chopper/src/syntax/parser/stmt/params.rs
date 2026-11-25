//! Purpose:
//! Parses function parameters, return types, and reusable parsed type expressions.
//! Handles typed parameters, defaults, by-reference markers, variadics, and name lists.
//!
//! Called from:
//! - `crate::syntax::parser::stmt`, `crate::syntax::parser::control`, and closure/OOP parsers.
//!
//! Key details:
//! - Type-name parsing must allow namespace-qualified PHP names without resolving them here.

use crate::syntax::parser::validation_only;
use crate::syntax::errors::CompileError;
use crate::syntax::lexer::{SpannedToken, Token};
use crate::syntax::names::Name;
use crate::syntax::parser::ast::{AttributeGroup, Expr, Stmt, StmtKind, TypeExpr, TypeParam, Variance};
use crate::syntax::parser::expr::parse_expr;
use crate::syntax::span::Span;

use super::{expect_token, name_starts_at, parse_block, parse_name};

/// Parses a `function` declaration: name, parameters, optional return type, and body.
/// Consumes the `function` keyword at `*pos` and advances past the closing `}` of the body.
/// Returns `StmtKind::FunctionDecl` with params, variadic, return_type, and body.
pub(super) fn parse_function_decl(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1;

    // PHP `function &f()` returns a reference (alias) to the returned lvalue.
    let by_ref_return = matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Ampersand));
    if by_ref_return {
        *pos += 1;
    }

    let name = match tokens.get(*pos) {
        Some((Token::Identifier(n), _)) => n.clone(),
        // `readonly` is a keyword token but PHP's grammar allows it as a function name.
        Some((Token::ReadOnly, metadata)) => {
            crate::syntax::parser::tokenizer_feedback::record(metadata, "readonly");
            "readonly".to_string()
        }
        // PHP reports the offending token itself, with its fixed expectation.
        Some((_, metadata)) => {
            return Err(CompileError::new(metadata.span, "Expected function name"));
        }
        None => return Err(CompileError::new(span, "Expected function name")),
    };
    if validation_only() && crate::syntax::parser::keyword_name::is_reserved_construct_name(&name) {
        return Err(CompileError::new(
            tokens[*pos].1.span,
            "Expected function name",
        ));
    }
    *pos += 1;

    let _type_scope = crate::syntax::parser::runtime_templates::TypeScope::enter();
    let type_params = parse_type_param_list(tokens, pos, span)?;

    expect_token(
        tokens,
        pos,
        &Token::LParen,
        "Expected '(' after function name",
    )?;
    let (params, param_attributes, variadic, variadic_by_ref, variadic_type) =
        parse_params(tokens, pos, span)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')' after parameters")?;

    // Parse optional return type: `: TypeExpr`
    let return_type = if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        Some(parse_type_expr(tokens, pos, span)?)
    } else {
        None
    };

    let body = parse_block(tokens, pos)?;
    let span = if crate::syntax::parser::runtime_fragment() {
        Span::with_end_from(span, tokens[*pos - 1].1.span)
    } else {
        span
    };

    Ok(Stmt::new(
        StmtKind::FunctionDecl {
            name,
            type_params,
            params,
            param_attributes,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            by_ref_return,
            body,
        },
        span,
    ))
}

/// Tracks the unconsumed half of a `>>` token while closing nested type argument lists.
///
/// `Box<array<int>>` ends in ONE token: the lexer reads `>>` as `GreaterGreater`, PHP's
/// right-shift operator, because it cannot know it is inside a type. Splitting it in the lexer
/// is not an option — `$a >> $b` is the same two characters — so the parser does the splitting.
///
/// The innermost list that meets a `>>` consumes the whole token and leaves a credit here; the
/// enclosing list spends that credit instead of consuming a token. Three levels (`A<B<C<int>>>`)
/// lex as `>>` then `>`, and four as `>>` then `>>`, which this handles by construction: every
/// `>>` is consumed by the inner of the two lists it closes.
#[derive(Default)]
pub(super) struct AngleCloses {
    /// Number of type argument lists already closed by an earlier `>>` token.
    pending: u32,
}

impl AngleCloses {
    /// Returns the number of unmatched closing angle brackets.
    pub(super) fn pending_count(&self) -> u32 {
        self.pending
    }
}

/// Consumes the `>` that closes a type argument list, splitting a `>>` token when needed.
pub(super) fn expect_type_list_close(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    closes: &mut AngleCloses,
    message: &str,
) -> Result<(), CompileError> {
    if closes.pending > 0 {
        closes.pending -= 1;
        return Ok(());
    }
    match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Greater) => {
            *pos += 1;
            Ok(())
        }
        Some(Token::GreaterGreater) => {
            *pos += 1;
            closes.pending += 1;
            Ok(())
        }
        _ => Err(CompileError::new(span, message)),
    }
}

/// Parses an optional type ARGUMENT list (`<int>`, `<string, User>`) after a class name.
///
/// Returns an empty vec when the next token is not `<`, so an ordinary class type costs one
/// comparison. Distinct from [`parse_type_param_list`], which parses the DECLARATION side
/// (`class Box<T : Entity>`): arguments are types, parameters are names with bounds.
#[allow(unreachable_code)]
fn parse_type_arguments(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    closes: &mut AngleCloses,
) -> Result<Vec<TypeExpr>, CompileError> {
    // `Box<>` lexes as the single `<>` token (PHP's `!=` alias), the same trap `array<>`,
    // `ptr<>` and `buffer<>` each spring.
    if *pos < tokens.len() && tokens[*pos].0 == Token::LessGreater {
        return Err(CompileError::new(
            span,
            "Expected a type argument inside <...>",
        ));
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::Less {
        return Ok(Vec::new());
    }
    if validation_only() {
        return Err(CompileError::new(
            tokens[*pos].1.span,
            "Generic type parameters are not valid PHP",
        ));
    }
    crate::syntax::parser::runtime_templates::record_extension(tokens[*pos].1.span);
    *pos += 1;
    let mut args = vec![parse_type_expr_nested(tokens, pos, span, closes)?];
    while *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
        *pos += 1;
        args.push(parse_type_expr_nested(tokens, pos, span, closes)?);
    }
    expect_type_list_close(tokens, pos, span, closes, "Expected '>' after type arguments")?;
    Ok(args)
}

/// Consumes a type ARGUMENT list where no name precedes it in the AST.
///
/// The method-call form `$box->map<string>($f)` needs the arguments alone: the receiver is an
/// expression and the method is a bare identifier, so there is no `Name` to hang them on the way
/// [`parse_inherited_name_at`] does. Same grammar, same balance rule.
pub fn parse_type_arguments_only(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Vec<TypeExpr>, CompileError> {
    let mut closes = AngleCloses::default();
    let args = parse_type_arguments(tokens, pos, span, &mut closes)?;
    if closes.pending > 0 {
        return Err(CompileError::new(span, "Unbalanced '>' in type arguments"));
    }
    Ok(args)
}

/// Returns the position after a type argument list starting at `pos`, without consuming it.
///
/// A probe, for the one place that needs to RECOGNIZE `Name<...>` in expression position
/// without being able to parse it: static access on a generic class type. Runs the real
/// grammar on a copy, so what it recognizes and what the type parser accepts cannot drift.
pub fn type_arguments_end(tokens: &[SpannedToken], pos: usize) -> Option<usize> {
    if tokens.get(pos).map(|(token, _)| token) != Some(&Token::Less) {
        return None;
    }
    if validation_only() {
        // TOKEN_PARSE must follow PHP's grammar: `<...>` in expression position is a
        // comparison chain, never elephc type arguments. The generic spellings are invalid
        // PHP, and the comparison reading rejects every one of them as the non-associative
        // chain php-src rejects (`(Foo < Bar) > $c`); treating them as comparisons also keeps
        // valid PHP such as `Foo < Bar | Baz > ($c)` accepted.
        return None;
    }
    let mut probe = pos;
    let mut closes = AngleCloses::default();
    let span = tokens.get(pos)?.1.span;
    let args = crate::syntax::parser::runtime_templates::lookahead(|| {
        parse_type_arguments(tokens, &mut probe, span, &mut closes)
    }).ok()?;
    if args.is_empty() || closes.pending > 0 {
        return None;
    }
    Some(probe)
}

/// Parses a class or interface name that may carry type arguments (`Repository<User>`).
///
/// Used for the inheritance clauses, where the name is a `Name` rather than a `TypeExpr`: the
/// arguments come back alongside it and the declaration stores them in its `GenericDecl`.
pub fn parse_inherited_name(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    message: &str,
) -> Result<(Name, Vec<TypeExpr>), CompileError> {
    let name_start = *pos;
    let name = parse_name(tokens, pos, span, message)?;
    crate::syntax::parser::runtime_templates::record_reference(&name, tokens[name_start].1.span);
    parse_inherited_name_at(tokens, pos, span, &name)
}

/// Reads the type arguments of a name the caller has ALREADY consumed.
///
/// The expression parser has to read the name before it can decide whether `<` begins a type
/// argument list or a comparison, so it cannot use [`parse_inherited_name`]; this is the same
/// function starting one step later, and the name comes back with it so both callers return
/// the same pair.
pub fn parse_inherited_name_at(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    name: &Name,
) -> Result<(Name, Vec<TypeExpr>), CompileError> {
    let mut closes = AngleCloses::default();
    let args = parse_type_arguments(tokens, pos, span, &mut closes)?;
    if closes.pending > 0 {
        return Err(CompileError::new(span, "Unbalanced '>' in type arguments"));
    }
    Ok((name.clone(), args))
}

/// Parses an optional type parameter list (`<T>`, `<T, U>`) after a function name.
///
/// Returns an empty vec when the next token is not `<`, so an ordinary function costs one
/// comparison. The names are kept as plain strings: a type parameter is resolved against the
/// enclosing declaration's own list, never against the namespace, so it must not go through
/// `Name` and the import machinery.
///
/// `<>` lexes as the single `LessGreater` token (PHP's `!=` alias) and is rejected explicitly,
/// the same way `ptr<>` and `array<>` are.
#[allow(unreachable_code)]
pub fn parse_type_param_list(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Vec<TypeParam>, CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::LessGreater {
        return Err(CompileError::new(
            span,
            "Expected a type parameter name inside <...>",
        ));
    }
    if *pos >= tokens.len() || tokens[*pos].0 != Token::Less {
        return Ok(Vec::new());
    }
    if validation_only() {
        return Err(CompileError::new(
            tokens[*pos].1.span,
            "Generic type parameters are not valid PHP",
        ));
    }
    let header_start = *pos;
    let track_header = crate::syntax::parser::runtime_fragment() && header_start > 0;
    let mut parameter_spans = Vec::new();
    *pos += 1;
    // A bound may itself be a generic class (`<T : Box<int>>`), whose `>>` has to split the
    // same way it does inside a type argument list.
    let mut closes = AngleCloses::default();
    let mut params: Vec<TypeParam> = Vec::new();
    loop {
        let variance = parse_variance_marker(tokens, pos);
        let name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => name.clone(),
            _ => {
                return Err(CompileError::new(
                    span,
                    "Expected a type parameter name inside <...>",
                ))
            }
        };
        if params.iter().any(|existing| existing.name == name) {
            return Err(CompileError::runtime_unsupported(
                span,
                &format!("Duplicate type parameter '{}'", name),
            ));
        }
        if track_header {
            let span = tokens[*pos].1.span;
            crate::syntax::parser::runtime_templates::declare_parameter(&name, span);
            parameter_spans.push(span);
        }
        *pos += 1;
        // `<T : Entity>` — the RFC's spelling for an upper bound. Unambiguous here because a
        // type parameter list is its own grammar: the `:` of a return type comes after the
        // parameter list's `)`, and an enum's backing `:` never appears inside `<...>`.
        let bound = if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
            *pos += 1;
            Some(parse_type_expr_nested(tokens, pos, span, &mut closes)?)
        } else {
            None
        };
        // `<K = string>` — the type argument to use when nothing constrains the parameter.
        let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            Some(parse_type_expr_nested(tokens, pos, span, &mut closes)?)
        } else {
            None
        };
        params.push(TypeParam {
            name,
            bound,
            default,
            variance,
            from_docblock: false,
            native_hint: None,
        });
        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
            continue;
        }
        break;
    }
    expect_type_list_close(
        tokens,
        pos,
        span,
        &mut closes,
        "Expected '>' after type parameter list",
    )?;
    if track_header {
        if let Some((_, after)) = tokens.get(*pos) {
            crate::syntax::parser::runtime_templates::record_header(
                crate::syntax::parser::RuntimeTemplateHeader {
                    name: tokens[header_start - 1].1.span,
                    open: tokens[header_start].1.span,
                    after: after.span,
                    parameters: parameter_spans,
                },
            );
        }
    }
    Ok(params)
}

/// Returns `true` if the token stream at `pos` begins with a type expression that could
/// be a parameter type annotation, `false` otherwise.
/// Checks for nullable/union types, pointer/buffer generics, and that the token sequence
/// ultimately resolves to a variable token (possibly after `&` or `...` markers).
pub fn looks_like_typed_param(tokens: &[SpannedToken], pos: usize) -> bool {
    let mut probe = pos;
    match parse_type_expr(tokens, &mut probe, tokens[pos].1.span) {
        Ok(_) => {
            if matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Ampersand)) {
                probe += 1;
            }
            if matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Ellipsis)) {
                probe += 1;
            }
            tokens.get(probe).and_then(|(token, _)| parameter_variable_name(token)).is_some()
        }
        Err(_) => false,
    }
}

/// Consumes the promoted-property modifiers PHP accepts as grammar for any parameter.
///
/// The compiler rejects `function f(public int $x) {}` while compiling the declaration, but
/// `TOKEN_PARSE` accepts it. `(set)` only counts when it touches its visibility word
/// (`public(set)`), exactly like the method-parameter parser: `public (set)`, `final(set)` and
/// `readonly(set)` stay syntax errors.
pub(crate) fn skip_promoted_parameter_modifiers(tokens: &[SpannedToken], pos: &mut usize) {
    loop {
        let Some((token, metadata)) = tokens.get(*pos) else {
            return;
        };
        let visibility = matches!(token, Token::Public | Token::Protected | Token::Private);
        if !visibility && !matches!(token, Token::Final | Token::ReadOnly) {
            return;
        }
        let modifier_span = metadata.span;
        *pos += 1;
        if visibility {
            let _ = super::oop::consume_set_marker(tokens, pos, modifier_span);
        }
    }
}

/// Returns the span of the token that broke a parameter after its optional type.
///
/// The caller has already failed to find a parameter variable at `pos`, which is usually the
/// type itself (`func(callable(int) $cb)`). Zend reports the token that ends the type, so the
/// probe re-reads the type and points just past it.
pub(crate) fn parameter_error_span(tokens: &[SpannedToken], pos: usize, span: Span) -> Span {
    let mut probe = pos;
    if parse_type_expr(tokens, &mut probe, span).is_ok() {
        if matches!(
            tokens.get(probe).map(|(token, _)| token),
            Some(Token::Ampersand | Token::Ellipsis)
        ) {
            probe += 1;
        }
    } else {
        probe = pos;
    }
    tokens
        .get(probe)
        .map_or(span, |(_, metadata)| metadata.span)
}

/// Parses a type expression: atomic type, nullable shorthand, or union of pipe-separated types.
/// Advances `*pos` past the consumed type tokens. Returns `TypeExpr::Atomic`, `Nullable`,
/// `Union`, `Ptr`, or `Buffer`. Does not resolve names — emits `TypeExpr::Named` with a
/// `Name` for class/interface/enum types.
pub fn parse_type_expr(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    let starts_with_null = matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Null));
    let mut closes = AngleCloses::default();
    let ty = parse_type_expr_nested(tokens, pos, span, &mut closes)?;
    // A credit left over means a `>>` closed one more list than was open — the type ended with
    // a stray `>`. Rejecting here keeps the error at the type rather than at whatever the extra
    // token later fails to parse as.
    if closes.pending > 0 {
        return Err(CompileError::new(span, "Unbalanced '>' in type arguments"));
    }
    if starts_with_null && matches!(ty, TypeExpr::Void) {
        return Ok(TypeExpr::Named(crate::syntax::names::Name::unqualified("null")));
    }
    Ok(ty)
}

/// Parses a type expression while sharing one `>>`-splitting budget with its enclosing lists.
fn parse_type_expr_nested(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    closes: &mut AngleCloses,
) -> Result<TypeExpr, CompileError> {
    let ty = if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Question)) {
        *pos += 1;
        TypeExpr::Nullable(Box::new(parse_atomic_type_expr(tokens, pos, span, closes)?))
    } else {
        parse_atomic_type_expr(tokens, pos, span, closes)?
    };

    if matches!(ty, TypeExpr::Nullable(_))
        && matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Pipe))
    {
        return Err(CompileError::new(
            span,
            "Nullable shorthand cannot be combined directly with union types; write T|null",
        ));
    }

    // `?A&B` is a syntax error in PHP: the nullable shorthand may not be combined with an
    // intersection. Reject it rather than silently dropping a member.
    if matches!(ty, TypeExpr::Nullable(_))
        && matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Ampersand))
        && type_starts_at(tokens, *pos + 1)
    {
        return Err(CompileError::new(
            span,
            "Nullable shorthand cannot be combined with intersection types",
        ));
    }

    // Intersection type `A&B`: an `&` immediately followed by another type. A bare `&` followed
    // by a `$variable`/`...` is the by-reference marker, handled by the parameter parser, so it is
    // left in place here.
    if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Ampersand))
        && type_starts_at(tokens, *pos + 1)
    {
        // PHP's DNF grammar only allows `(A&B)|C`: an intersection may not contain a
        // parenthesized member (union or another intersection).
        if matches!(ty, TypeExpr::Intersection(_)) {
            return Err(CompileError::new(
                span,
                "Intersection types cannot be nested without a union",
            ));
        }
        let mut members = vec![ty];
        while matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Ampersand))
            && type_starts_at(tokens, *pos + 1)
        {
            *pos += 1; // consume '&'
            if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::LParen)) {
                return Err(CompileError::new(
                    span,
                    "Parenthesized types cannot be intersection members",
                ));
            }
            members.push(parse_atomic_type_expr(tokens, pos, span, closes)?);
        }
        if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Pipe)) {
            return Err(CompileError::new(
                span,
                "Intersection types cannot be part of a union without parentheses",
            ));
        }
        return Ok(TypeExpr::Intersection(members));
    }

    let mut members = vec![ty];
    while matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Pipe)) {
        *pos += 1;
        members.push(parse_atomic_type_expr(tokens, pos, span, closes)?);
        if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Ampersand))
            && type_starts_at(tokens, *pos + 1)
        {
            return Err(CompileError::new(
                span,
                "Intersection types cannot be part of a union without parentheses",
            ));
        }
    }
    Ok(normalize_union_members(members))
}

/// Returns true if the token at `index` can begin a (non-nullable) type — used to tell an
/// intersection `A&B` apart from a by-reference parameter `A &$x`.
fn type_starts_at(tokens: &[SpannedToken], index: usize) -> bool {
    matches!(
        tokens.get(index).map(|(token, _)| token),
        Some(
            Token::Identifier(_)
                | Token::Backslash
                | Token::Self_
                | Token::Static
                | Token::Parent
                | Token::LParen
        )
    ) || crate::syntax::parser::relative_name_starts_at(tokens, index)
}

/// Collapses a parsed union member list into its canonical `TypeExpr`.
///
/// A lone member is unwrapped. A `null` member (lowered to `TypeExpr::Void`) reproduces the
/// nullable shorthand so that `T|null` is identical to `?T`: with a single remaining non-null
/// member the union becomes `Nullable`, while a wider union keeps exactly one null sentinel so
/// the checker's `union_contains_void` still recognizes it as nullable. Pure non-null unions
/// are returned unchanged.
fn normalize_union_members(members: Vec<TypeExpr>) -> TypeExpr {
    if crate::syntax::parser::runtime_fragment() && members.len() > 1 {
        return TypeExpr::Union(members);
    }
    let null_count = members
        .iter()
        .filter(|member| matches!(member, TypeExpr::Void))
        .count();
    if null_count > 0 && members.len() > null_count {
        let mut non_null: Vec<TypeExpr> = members
            .into_iter()
            .filter(|member| !matches!(member, TypeExpr::Void))
            .collect();
        if non_null.len() == 1 {
            return TypeExpr::Nullable(Box::new(
                non_null.pop().expect("non-null member exists"),
            ));
        }
        non_null.push(TypeExpr::Void);
        return TypeExpr::Union(non_null);
    }
    if members.len() == 1 {
        members.into_iter().next().expect("type member exists")
    } else {
        TypeExpr::Union(members)
    }
}

/// Parses a pointer target and its closing bracket, preserving nested pointer targets.
fn parse_pointer_type_target(
    tokens: &[SpannedToken], pos: &mut usize, span: Span, closes: &mut AngleCloses,
) -> Result<Name, CompileError> {
    let target = parse_name(tokens, pos, span, "Expected pointer target type inside ptr<...>")?;
    let target = if target.is_unqualified() && matches!(target.as_str(), "ptr" | "pointer")
        && matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Less))
    {
        *pos += 1;
        let inner = parse_pointer_type_target(tokens, pos, span, closes)?;
        let prefix = if inner.kind == crate::syntax::names::NameKind::FullyQualified { "\\" } else { "" };
        Name::unqualified(format!("ptr<{}{}>", prefix, inner.as_str()))
    } else {
        target
    };
    expect_type_list_close(tokens, pos, span, closes, "Expected '>' after ptr target type")?;
    Ok(target)
}

/// Parses a single (non-union) type expression: builtin keyword, `ptr<T>`, `buffer<T>`,
/// or a qualified/unqualified name. Does not handle `?T` (nullable) — that is handled by
/// the caller `parse_type_expr`. Advances `*pos` past the consumed token(s).
fn parse_atomic_type_expr(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    closes: &mut AngleCloses,
) -> Result<TypeExpr, CompileError> {
    match tokens.get(*pos).map(|(t, _)| t) {
        // A builtin type word followed by `\` is the first segment of a class name, as in
        // `Int\Money $m` or `Array\Cursor $c` (#826): it is a class type, not `int`/`array`.
        Some(Token::Identifier(_))
            if matches!(tokens.get(*pos + 1).map(|(t, _)| t), Some(Token::Backslash)) =>
        {
            parse_named_type_expr(tokens, pos, span, closes)
        }
        Some(Token::Identifier(name)) if ident_matches(name, &["int", "integer"]) => {
            *pos += 1;
            Ok(TypeExpr::Int)
        }
        Some(Token::Identifier(name)) if ident_matches(name, &["float", "double", "real"]) => {
            *pos += 1;
            Ok(TypeExpr::Float)
        }
        Some(Token::Identifier(name)) if ident_matches(name, &["bool", "boolean"]) => {
            *pos += 1;
            Ok(TypeExpr::Bool)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("string") => {
            *pos += 1;
            Ok(TypeExpr::Str)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("void") => {
            *pos += 1;
            Ok(TypeExpr::Void)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("never") => {
            *pos += 1;
            Ok(TypeExpr::Never)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("iterable") => {
            *pos += 1;
            Ok(TypeExpr::Iterable)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("array") => {
            *pos += 1;
            // `array<>` lexes as the single `<>` token (PHP's `!=` alias), so the empty
            // type list is recognized here instead of reading as a bare `array`.
            if *pos < tokens.len() && tokens[*pos].0 == Token::LessGreater {
                return Err(CompileError::new(
                    span,
                    "Expected array element type inside array<...>",
                ));
            }
            if validation_only() && *pos < tokens.len() && tokens[*pos].0 == Token::Less {
                return Err(CompileError::new(tokens[*pos].1.span, "Generic type arguments are not valid PHP"));
            }
            if *pos < tokens.len() && tokens[*pos].0 == Token::Less {
                crate::syntax::parser::runtime_templates::record_extension(tokens[*pos].1.span);
                *pos += 1;
                let first = parse_type_expr_nested(tokens, pos, span, closes)?;
                // One argument is the indexed form `array<V>`; two is the associative
                // `array<K, V>`, which has hash storage rather than a packed element vector.
                if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                    *pos += 1;
                    let value = parse_type_expr_nested(tokens, pos, span, closes)?;
                    expect_type_list_close(
                        tokens,
                        pos,
                        span,
                        closes,
                        "Expected '>' after array value type",
                    )?;
                    return Ok(TypeExpr::AssocArray {
                        key: Box::new(first),
                        value: Box::new(value),
                    });
                }
                expect_type_list_close(
                    tokens,
                    pos,
                    span,
                    closes,
                    "Expected '>' after array element type",
                )?;
                return Ok(TypeExpr::Array(Box::new(first)));
            }
            Ok(TypeExpr::Named(crate::syntax::names::Name::unqualified("array")))
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("mixed") => {
            *pos += 1;
            Ok(TypeExpr::Named(crate::syntax::names::Name::unqualified("mixed")))
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("callable") => {
            *pos += 1;
            parse_callable_signature(tokens, pos, span)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("object") => {
            *pos += 1;
            Ok(TypeExpr::Named(crate::syntax::names::Name::unqualified("object")))
        }
        Some(Token::Identifier(name)) if matches!(name.as_str(), "ptr" | "pointer") => {
            *pos += 1;
            // `ptr<>` lexes as the single `<>` token (PHP's `!=` alias), so the empty
            // type list is recognized here instead of reading as a bare `ptr`.
            if *pos < tokens.len() && tokens[*pos].0 == Token::LessGreater {
                return Err(CompileError::new(
                    span,
                    "Expected pointer target type inside ptr<...>",
                ));
            }
            if validation_only() && *pos < tokens.len() && tokens[*pos].0 == Token::Less {
                return Err(CompileError::new(tokens[*pos].1.span, "Generic type arguments are not valid PHP"));
            }
            if *pos < tokens.len() && tokens[*pos].0 == Token::Less {
                *pos += 1;
                let target = parse_pointer_type_target(tokens, pos, span, closes)?;
                Ok(TypeExpr::Ptr(Some(target)))
            } else {
                Ok(TypeExpr::Ptr(None))
            }
        }
        Some(Token::Identifier(name)) if name == "buffer" => {
            *pos += 1;
            // `buffer<>` lexes as the single `<>` token (PHP's `!=` alias).
            if *pos < tokens.len() && tokens[*pos].0 == Token::LessGreater {
                return Err(CompileError::new(
                    span,
                    "Expected buffer element type after 'buffer<'",
                ));
            }
            if validation_only() {
                // `buffer<T>` is an elephc extension; in PHP `buffer` is an ordinary class name.
                return Ok(TypeExpr::Named(crate::syntax::names::Name::unqualified("buffer")));
            }
            expect_token(tokens, pos, &Token::Less, "Expected '<' after buffer")?;
            let inner = parse_type_expr_nested(tokens, pos, span, closes)?;
            expect_type_list_close(
                tokens,
                pos,
                span,
                closes,
                "Expected '>' after buffer element type",
            )?;
            Ok(TypeExpr::Buffer(Box::new(inner)))
        }
        // `null` is a first-class type that only ever means "the null value". It shares the
        // runtime null sentinel with `void`/`?T`, so it lowers to `TypeExpr::Void`; the caller
        // folds a `null` union member back into the canonical `Nullable` shorthand.
        Some(Token::Null) => {
            *pos += 1;
            if crate::syntax::parser::runtime_fragment() {
                Ok(TypeExpr::Named(Name::unqualified("null")))
            } else {
                Ok(TypeExpr::Void)
            }
        }
        // Preserve `false` as a literal subtype so `$x === false` can remove only the false
        // member from `T|false` without incorrectly removing a full `bool` member. Its runtime
        // representation remains identical to bool. `true` is conservatively widened to bool.
        Some(Token::False) => {
            *pos += 1;
            Ok(TypeExpr::False)
        }
        Some(Token::True) => {
            let name = tokens[*pos].0.word_spelling(&tokens[*pos].1).unwrap_or("true");
            *pos += 1;
            if crate::syntax::parser::runtime_fragment() {
                Ok(TypeExpr::Named(Name::unqualified(name)))
            } else {
                Ok(TypeExpr::Bool)
            }
        }
        // `self`, `static`, and `parent` are relative class types. They are kept symbolic here
        // (their concrete class is not known until inheritance/trait flattening) and resolved to
        // the enclosing class by `substitute_relative_class_types` before type checking.
        Some(Token::Self_) => {
            *pos += 1;
            Ok(TypeExpr::Named(Name::unqualified("self")))
        }
        Some(Token::Static) => {
            *pos += 1;
            Ok(TypeExpr::Named(Name::unqualified("static")))
        }
        Some(Token::Parent) => {
            *pos += 1;
            Ok(TypeExpr::Named(Name::unqualified("parent")))
        }
        // DNF types: `(A&B)|C` and `A|(B&C)` need a parenthesized member. PHP only accepts a
        // group when it contains an intersection AND the group is one side of a union.
        Some(Token::LParen) => {
            let open = *pos;
            *pos += 1;
            let inner = parse_type_expr_nested(tokens, pos, span, closes)?;
            expect_token(
                tokens,
                pos,
                &Token::RParen,
                "Expected ')' after parenthesized type",
            )?;
            if !matches!(inner, TypeExpr::Intersection(_)) {
                return Err(CompileError::new(
                    span,
                    "A parenthesized type must contain an intersection",
                ));
            }
            let joins_union = (open > 0 && matches!(&tokens[open - 1].0, Token::Pipe))
                || matches!(tokens.get(*pos).map(|(token, _)| token), Some(Token::Pipe));
            if !joins_union {
                return Err(CompileError::new(
                    span,
                    "A parenthesized type must be part of a union",
                ));
            }
            Ok(inner)
        }
        Some(_) if name_starts_at(tokens, *pos) => {
            parse_named_type_expr(tokens, pos, span, closes)
        }
        _ => Err(CompileError::new(span, "Expected type expression")),
    }
}

/// Parses a class type name and its optional generic arguments in any qualified form.
fn parse_named_type_expr(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    closes: &mut AngleCloses,
) -> Result<TypeExpr, CompileError> {
    let name_start = *pos;
    let name = parse_name(tokens, pos, span, "Expected type name")?;
    crate::syntax::parser::runtime_templates::record_reference(&name, tokens[name_start].1.span);
    if validation_only() && name.kind == crate::syntax::names::NameKind::Unqualified
        && name.parts.len() == 1
        && crate::syntax::parser::keyword_name::is_reserved_construct_name(&name.parts[0])
    {
        // `empty`/`isset`/... are not class names unless qualified (`\empty`, `Foo\empty`).
        return Err(CompileError::new(
            tokens
                .get(name_start)
                .map_or(span, |(_, metadata)| metadata.span),
            "Reserved type name",
        ));
    }
    // `Box<>` lexes as the single `<>` token (PHP's `!=` alias), the same trap
    // `array<>`, `ptr<>` and `buffer<>` each spring.
    if *pos < tokens.len() && tokens[*pos].0 == Token::LessGreater {
        return Err(CompileError::new(
            span,
            &format!("Expected a type argument inside {}<...>", name.as_str()),
        ));
    }
    let args = parse_type_arguments(tokens, pos, span, closes)?;
    if args.is_empty() {
        return Ok(TypeExpr::Named(name));
    }
    Ok(TypeExpr::GenericClass { name, args })
}

/// Returns `true` if `name` matches any of the `keywords` case-insensitively.
fn ident_matches(name: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .any(|keyword| name.eq_ignore_ascii_case(keyword))
}

/// Returns the parameter name, allowing `$this` only when checking PHP grammar for TOKEN_PARSE.
pub(in crate::syntax::parser) fn parameter_variable_name(token: &Token) -> Option<&str> {
    match token {
        Token::Variable(name) => Some(name),
        Token::This if validation_only() => Some("this"),
        _ => None,
    }
}

/// Parses a parenthesized parameter list (not including the surrounding `(` and `)`).
/// Handles typed parameters, defaults, `&` by-reference markers, `...` variadic markers,
/// and PHP 8.0 `#[...]` attributes. Returns a vec of `(name, type, default, is_ref)` tuples
/// and an optional variadic parameter name. Advances `*pos` to the token after `)`.
/// Errors if a variadic parameter appears after another parameter or if a typed variadic
/// is present.
pub(super) fn parse_params(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<
    (
        Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
        Vec<Vec<AttributeGroup>>,
        Option<String>,
        bool,
        Option<TypeExpr>,
    ),
    CompileError,
> {
    let mut params = Vec::new();
    let mut param_attributes = Vec::new();
    let mut variadic = None;
    let mut variadic_by_ref = false;
    let mut variadic_type = None;
    while *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        if !params.is_empty() || variadic.is_some() {
            expect_token(
                tokens,
                pos,
                &Token::Comma,
                "Expected ',' between parameters",
            )?;
            // Allow a trailing comma before the closing paren (PHP 8.0+).
            if *pos < tokens.len() && tokens[*pos].0 == Token::RParen {
                break;
            }
        }
        // PHP 8.0 parameter attributes (`function f(#[Sensitive] $s)`).
        let attributes = crate::syntax::parser::parse_attribute_lists(tokens, pos)?;
        if !validation_only() && variadic.is_some() {
            return Err(CompileError::runtime_unsupported(
                span,
                "Variadic parameter must be the last parameter",
            ));
        }
        // Promoted-property modifiers are grammar-valid on any function parameter; PHP
        // rejects them only when compiling a non-constructor declaration.
        if validation_only() {
            skip_promoted_parameter_modifiers(tokens, pos);
        }
        // Try to parse optional type annotation before $variable
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
            // A type annotation on a variadic (`int ...$xs`) constrains each passed argument; the
            // declared element type is preserved so call validation can check every collected arg.
            *pos += 1;
            let Some(name) = tokens.get(*pos).and_then(|(token, _)| parameter_variable_name(token)) else {
                return Err(CompileError::expected_variable(span, "Expected variable after '...'"));
            };
            variadic = Some(name.to_owned());
            variadic_by_ref = is_ref;
            variadic_type = type_ann;
            param_attributes.push(attributes);
            *pos += 1;
            continue;
        }
        let Some(name) = tokens.get(*pos).and_then(|(token, _)| parameter_variable_name(token)) else {
            let error_span = parameter_error_span(tokens, *pos, span);
            return Err(CompileError::new(error_span, "Expected parameter variable"));
        };
        *pos += 1;
        let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
            *pos += 1;
            Some(parse_expr(tokens, pos)?)
        } else {
            None
        };
        param_attributes.push(attributes);
        params.push((name.to_owned(), type_ann, default, is_ref));
    }
    Ok((
        params,
        param_attributes,
        variadic,
        variadic_by_ref,
        variadic_type,
    ))
}

/// Parses a comma-separated list of `Name`s until a token that does not start a name is
/// seen. `first_error` is used when the list is empty; a more specific error is used when
/// a comma is found but no name follows. Advances `*pos` to the first non-name token.
/// Reads the variance marker a type parameter may carry, leaving `pos` on its name.
///
/// Two spellings reach the same marker: the symbols `+T` / `-T`, and the words `out T` / `in T`.
///
/// The words need one token of lookahead and cannot have it any other way. `out` and `in` are
/// ordinary identifiers in PHP, so `<out>` is a type parameter NAMED `out` while `<out T>` is a
/// covariant `T` — the difference is only whether an identifier follows. Nothing else in a type
/// parameter list puts two identifiers in a row, so the lookahead is decisive rather than a
/// guess, and a parameter may still be called `out`.
fn parse_variance_marker(tokens: &[SpannedToken], pos: &mut usize) -> Variance {
    let variance = match tokens.get(*pos).map(|(token, _)| token) {
        Some(Token::Plus) => Variance::Covariant,
        Some(Token::Minus) => Variance::Contravariant,
        Some(Token::Identifier(word)) if word_marker(word).is_some() => {
            if !matches!(tokens.get(*pos + 1).map(|(token, _)| token), Some(Token::Identifier(_))) {
                return Variance::Invariant;
            }
            word_marker(word).expect("checked by the guard")
        }
        _ => return Variance::Invariant,
    };
    *pos += 1;
    variance
}

/// The marker a word spells, or `None` when the word is just an identifier.
fn word_marker(word: &str) -> Option<Variance> {
    match word {
        "out" => Some(Variance::Covariant),
        "in" => Some(Variance::Contravariant),
        _ => None,
    }
}

/// Reads the optional signature after the `callable` keyword.
///
/// `callable` alone stays exactly what it was — `Named("callable")` — so nothing about existing
/// code changes. `callable(int): string` gains the declared shape, which is what lets a type
/// parameter reach through a callback: `map<U>(callable(T): U $f)` can bind `U` from the closure
/// the call passes, where a bare `callable` carries no types and can bind nothing.
///
/// The return type is REQUIRED. A signature that declares only its parameters says nothing about
/// what comes back, which is the half inference actually needs, and PHPStan spells it the same
/// way. The `:` is unambiguous here because it follows this signature's own `)`, while a
/// function's return `:` follows the parameter LIST's.
///
/// PHP has no callable signatures, so TOKEN_PARSE only accepts the bare `callable` type and
/// leaves the `(` to the surrounding grammar, which rejects it exactly like Zend.
fn parse_callable_signature(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    if validation_only() || *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Ok(TypeExpr::Named(crate::syntax::names::Name::unqualified("callable")));
    }
    crate::syntax::parser::runtime_templates::record_extension(tokens[*pos].1.span);
    *pos += 1;
    let mut params = Vec::new();
    if *pos < tokens.len() && tokens[*pos].0 != Token::RParen {
        loop {
            params.push(parse_type_expr(tokens, pos, span)?);
            if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
                *pos += 1;
                continue;
            }
            break;
        }
    }
    expect_token(
        tokens,
        pos,
        &Token::RParen,
        "Expected ')' after callable parameter types",
    )?;
    expect_token(
        tokens,
        pos,
        &Token::Colon,
        "Expected ':' and a return type after a callable signature",
    )?;
    let ret = parse_type_expr(tokens, pos, span)?;
    Ok(TypeExpr::CallableSig {
        params,
        ret: Box::new(ret),
    })
}

/// Parses a comma-separated declaration name list with aligned optional generic type arguments.
pub(super) fn parse_name_list(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    first_error: &str,
) -> Result<(Vec<Name>, Vec<Vec<TypeExpr>>), CompileError> {
    let mut names = Vec::new();
    let mut type_args = Vec::new();
    loop {
        let error = if names.is_empty() { first_error } else { "Expected name after ',' in declaration list" };
        let (name, args) = parse_inherited_name(tokens, pos, span, error)?;
        names.push(name);
        type_args.push(args);

        if *pos < tokens.len() && tokens[*pos].0 == Token::Comma {
            *pos += 1;
            continue;
        }
        break;
    }
    Ok((names, type_args))
}
