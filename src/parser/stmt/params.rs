//! Purpose:
//! Parses function parameters, return types, and reusable parsed type expressions.
//! Handles typed parameters, defaults, by-reference markers, variadics, and name lists.
//!
//! Called from:
//! - `crate::parser::stmt`, `crate::parser::control`, and closure/OOP parsers.
//!
//! Key details:
//! - Type-name parsing must allow namespace-qualified PHP names without resolving them here.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::names::Name;
use crate::parser::ast::{AttributeGroup, Expr, Stmt, StmtKind, TypeExpr, TypeParam, Variance};
use crate::parser::expr::parse_expr;
use crate::span::Span;

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

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => n.clone(),
        _ => return Err(CompileError::new(span, "Expected function name")),
    };
    *pos += 1;

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
struct AngleCloses {
    /// Number of type argument lists already closed by an earlier `>>` token.
    pending: u32,
}

/// Consumes the `>` that closes a type argument list, splitting a `>>` token when needed.
fn expect_type_list_close(
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
pub(crate) fn parse_type_arguments_only(
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
pub(crate) fn type_arguments_end(tokens: &[SpannedToken], pos: usize) -> Option<usize> {
    if tokens.get(pos).map(|(token, _)| token) != Some(&Token::Less) {
        return None;
    }
    let mut probe = pos;
    let mut closes = AngleCloses::default();
    let span = tokens.get(pos)?.1.span;
    let args = parse_type_arguments(tokens, &mut probe, span, &mut closes).ok()?;
    if args.is_empty() || closes.pending > 0 {
        return None;
    }
    Some(probe)
}

/// Parses a class or interface name that may carry type arguments (`Repository<User>`).
///
/// Used for the inheritance clauses, where the name is a `Name` rather than a `TypeExpr`: the
/// arguments come back alongside it and the declaration stores them in its `GenericDecl`.
pub(crate) fn parse_inherited_name(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    message: &str,
) -> Result<(Name, Vec<TypeExpr>), CompileError> {
    let name = parse_name(tokens, pos, span, message)?;
    parse_inherited_name_at(tokens, pos, span, &name)
}

/// Reads the type arguments of a name the caller has ALREADY consumed.
///
/// The expression parser has to read the name before it can decide whether `<` begins a type
/// argument list or a comparison, so it cannot use [`parse_inherited_name`]; this is the same
/// function starting one step later, and the name comes back with it so both callers return
/// the same pair.
pub(crate) fn parse_inherited_name_at(
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
pub(crate) fn parse_type_param_list(
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
            return Err(CompileError::new(
                span,
                &format!("Duplicate type parameter '{}'", name),
            ));
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
    Ok(params)
}

/// Returns `true` if the token stream at `pos` begins with a type expression that could
/// be a parameter type annotation, `false` otherwise.
/// Checks for nullable/union types, pointer/buffer generics, and that the token sequence
/// ultimately resolves to a variable token (possibly after `&` or `...` markers).
pub(crate) fn looks_like_typed_param(tokens: &[SpannedToken], pos: usize) -> bool {
    let mut probe = pos;
    match parse_type_expr(tokens, &mut probe, tokens[pos].1.span) {
        Ok(_) => {
            if matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Ampersand)) {
                probe += 1;
            }
            if matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Ellipsis)) {
                probe += 1;
            }
            matches!(tokens.get(probe).map(|(t, _)| t), Some(Token::Variable(_)))
        }
        Err(_) => false,
    }
}

/// Parses a type expression: atomic type, nullable shorthand, or union of pipe-separated types.
/// Advances `*pos` past the consumed type tokens. Returns `TypeExpr::Atomic`, `Nullable`,
/// `Union`, `Ptr`, or `Buffer`. Does not resolve names — emits `TypeExpr::Named` with a
/// `Name` for class/interface/enum types.
pub(crate) fn parse_type_expr(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    let mut closes = AngleCloses::default();
    let ty = parse_type_expr_nested(tokens, pos, span, &mut closes)?;
    // A credit left over means a `>>` closed one more list than was open — the type ended with
    // a stray `>`. Rejecting here keeps the error at the type rather than at whatever the extra
    // token later fails to parse as.
    if closes.pending > 0 {
        return Err(CompileError::new(span, "Unbalanced '>' in type arguments"));
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
        let mut members = vec![ty];
        while matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Ampersand))
            && type_starts_at(tokens, *pos + 1)
        {
            *pos += 1; // consume '&'
            members.push(parse_atomic_type_expr(tokens, pos, span, closes)?);
        }
        return Ok(TypeExpr::Intersection(members));
    }

    let mut members = vec![ty];
    while matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Pipe)) {
        *pos += 1;
        members.push(parse_atomic_type_expr(tokens, pos, span, closes)?);
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
        )
    )
}

/// Collapses a parsed union member list into its canonical `TypeExpr`.
///
/// A lone member is unwrapped. A `null` member (lowered to `TypeExpr::Void`) reproduces the
/// nullable shorthand so that `T|null` is identical to `?T`: with a single remaining non-null
/// member the union becomes `Nullable`, while a wider union keeps exactly one null sentinel so
/// the checker's `union_contains_void` still recognizes it as nullable. Pure non-null unions
/// are returned unchanged.
fn normalize_union_members(members: Vec<TypeExpr>) -> TypeExpr {
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
            if *pos < tokens.len() && tokens[*pos].0 == Token::Less {
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
            Ok(TypeExpr::Named(crate::names::Name::unqualified("array")))
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("mixed") => {
            *pos += 1;
            Ok(TypeExpr::Named(crate::names::Name::unqualified("mixed")))
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("callable") => {
            *pos += 1;
            parse_callable_signature(tokens, pos, span)
        }
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("object") => {
            *pos += 1;
            Ok(TypeExpr::Named(crate::names::Name::unqualified("object")))
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
            if *pos < tokens.len() && tokens[*pos].0 == Token::Less {
                *pos += 1;
                let target = parse_name(
                    tokens,
                    pos,
                    span,
                    "Expected pointer target type inside ptr<...>",
                )?;
                expect_token(
                    tokens,
                    pos,
                    &Token::Greater,
                    "Expected '>' after ptr target type",
                )?;
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
            Ok(TypeExpr::Void)
        }
        // Preserve `false` as a literal subtype so `$x === false` can remove only the false
        // member from `T|false` without incorrectly removing a full `bool` member. Its runtime
        // representation remains identical to bool. `true` is conservatively widened to bool.
        Some(Token::False) => {
            *pos += 1;
            Ok(TypeExpr::False)
        }
        Some(Token::True) => {
            *pos += 1;
            Ok(TypeExpr::Bool)
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
        Some(_) if name_starts_at(tokens, *pos) => {
            let name = parse_name(tokens, pos, span, "Expected type name")?;
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
        _ => Err(CompileError::new(span, "Expected type expression")),
    }
}

/// Returns `true` if `name` matches any of the `keywords` case-insensitively.
fn ident_matches(name: &str, keywords: &[&str]) -> bool {
    keywords
        .iter()
        .any(|keyword| name.eq_ignore_ascii_case(keyword))
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
        let attributes = crate::parser::parse_attribute_lists(tokens, pos)?;
        if variadic.is_some() {
            return Err(CompileError::new(
                span,
                "Variadic parameter must be the last parameter",
            ));
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
            match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Variable(n)) => {
                    variadic = Some(n.clone());
                    variadic_by_ref = is_ref;
                    variadic_type = type_ann;
                    param_attributes.push(attributes);
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
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                param_attributes.push(attributes);
                params.push((n, type_ann, default, is_ref));
            }
            _ => return Err(CompileError::new(span, "Expected parameter variable")),
        }
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
fn parse_callable_signature(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    if *pos >= tokens.len() || tokens[*pos].0 != Token::LParen {
        return Ok(TypeExpr::Named(crate::names::Name::unqualified("callable")));
    }
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

pub(super) fn parse_name_list(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    first_error: &str,
) -> Result<(Vec<Name>, Vec<Vec<TypeExpr>>), CompileError> {
    let mut names = Vec::new();
    let mut type_args = Vec::new();
    loop {
        if !name_starts_at(tokens, *pos) {
            if names.is_empty() {
                return Err(CompileError::new(span, first_error));
            }
            return Err(CompileError::new(
                span,
                "Expected name after ',' in declaration list",
            ));
        }
        let (name, args) = parse_inherited_name(tokens, pos, span, first_error)?;
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
