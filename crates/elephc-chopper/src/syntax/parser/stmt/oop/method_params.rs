//! Purpose:
//! Parses method and constructor parameters, including promoted properties.
//! Produces parameter metadata plus synthetic property and assignment statements for promoted parameters.
//!
//! Called from:
//! - `crate::syntax::parser::stmt::oop::body::parse_class_like_method()`.
//!
//! Key details:
//! - Promoted property lowering must keep constructor assignment order and member visibility metadata aligned.

use crate::syntax::parser::validation_only;
use crate::syntax::errors::CompileError;
use crate::syntax::lexer::{SpannedToken, Token};
use crate::syntax::parser::ast::{
    AttributeGroup, ClassProperty, Expr, ExprKind, PropertyHooks, RuntimeExpr, RuntimeWrite, Stmt, StmtKind, TypeExpr,
    Visibility,
};
use crate::syntax::parser::expr::parse_expr;
use crate::syntax::span::Span;

use super::super::expect_token;
use super::super::params::{
    looks_like_typed_param, parameter_error_span, parameter_variable_name, parse_type_expr,
};
use super::body::consume_set_marker;

type MethodParam = (String, Option<TypeExpr>, Option<Expr>, bool);
type ParsedMethodParams = (
    Vec<MethodParam>,
    Vec<Vec<AttributeGroup>>,
    Option<String>,
    bool,
    Option<TypeExpr>,
    Vec<ClassProperty>,
    Vec<Stmt>,
);

/// Modifiers that turn a constructor parameter into a promoted property: the read visibility,
/// the optional PHP 8.4 asymmetric write (`set`) visibility, `readonly`, and the span of the
/// first modifier token (used as the synthetic property's span).
struct PromotedModifiers {
    visibility: Visibility,
    set_visibility: Option<Visibility>,
    readonly: bool,
    is_final: bool,
    span: Span,
}

/// Parses method or constructor parameters from `(` to `)`, including PHP 8.0 promoted
/// properties. Returns the parameter list, optional variadic name, promoted property
/// declarations, and synthetic constructor assignments for promoted parameters.
///
/// - `method_name` only rejects promoted properties in non-constructor methods, and only in the
///   compiler build: TOKEN_PARSE accepts the declaration and PHP rejects it while compiling.
/// - Promoted properties are stored as `ClassProperty` with visibility, readonly, and type,
///   but with no default (PHP keeps defaults on the parameter itself).
/// - The caller is responsible for inserting the returned `promoted_assignments` statements
///   into the constructor body after parsing.
pub(super) fn parse_method_params(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    method_name: &str,
) -> Result<ParsedMethodParams, CompileError> {
    let mut params = Vec::new();
    let mut param_attributes = Vec::new();
    let mut param_lines = Vec::new();
    let mut variadic = None;
    let mut variadic_by_ref = false;
    let mut variadic_type = None;
    let mut promoted_properties = Vec::new();
    let mut promoted_assignments = Vec::new();

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
        // PHP 8.0 parameter attributes — also covers attributes preceding a
        // promoted-property modifier such as `#[Inject] public Foo $f`.
        let attributes = crate::syntax::parser::parse_attribute_lists(tokens, pos)?;
        param_lines.push(tokens.get(*pos).map_or(span.line, |(_, token)| token.span.line));
        // PHP accepts a parameter after the variadic and promoted properties on any method
        // through TOKEN_PARSE; both are rejected while COMPILING the declaration, so the
        // validation build must not fail them.
        if !validation_only() && variadic.is_some() {
            return Err(CompileError::runtime_unsupported(
                span,
                "Variadic parameter must be the last parameter",
            ));
        }

        let promotion = parse_promoted_param_modifiers(tokens, pos)?;
        if !validation_only()
            && promotion.is_some()
            && !method_name.eq_ignore_ascii_case("__construct")
        {
            return Err(CompileError::runtime_unsupported(
                span,
                "Cannot declare promoted property outside a constructor",
            ));
        }

        let type_ann = if looks_like_typed_param(tokens, *pos) {
            Some(if promotion.is_some() {
                super::body::parse_property_type_expr(tokens, pos, span)?
            } else {
                parse_type_expr(tokens, pos, span)?
            })
        } else {
            None
        };
        let (is_ref, ref_span) = if *pos < tokens.len() && tokens[*pos].0 == Token::Ampersand {
            let ref_span = tokens[*pos].1.span;
            *pos += 1;
            (true, Some(ref_span))
        } else {
            (false, None)
        };
        if *pos < tokens.len() && tokens[*pos].0 == Token::Ellipsis {
            if !validation_only() && promotion.is_some() {
                return Err(CompileError::runtime_unsupported(
                    span,
                    "Cannot declare variadic promoted property",
                ));
            }
            // A typed variadic (`int ...$xs`) is accepted; the declared element type is preserved
            // so call validation can check each argument collected into the variadic.
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

        match tokens
            .get(*pos)
            .and_then(|(token, metadata)| {
                parameter_variable_name(token).map(|name| (name, metadata.span))
            })
        {
            Some((n, param_span)) => {
                let n = n.to_owned();
                *pos += 1;
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                if let Some(PromotedModifiers {
                    visibility,
                    set_visibility,
                    readonly,
                    is_final,
                    span: property_span,
                }) = promotion
                {
                    // A promoted parameter is not implicitly nullable: php-src rejects the
                    // `null` default outright, since the property's type would not accept it.
                    if let (Some(declared), Some(ExprKind::Null)) =
                        (type_ann.as_ref(), default.as_ref().map(|default| &default.kind))
                    {
                        // An intersection rejects null too; it is only never widened.
                        if !validation_only() && !crate::syntax::parser::runtime_fragment()
                            && (declared.implicitly_nullable().is_some()
                                || matches!(declared, TypeExpr::Intersection(_)))
                        {
                            return Err(CompileError::new(
                                param_span,
                                &format!(
                                    "Cannot use null as default value for parameter ${} of type {}",
                                    n,
                                    crate::syntax::generics::describe_written_type(declared)
                                ),
                            ));
                        }
                    }
                    if !validation_only() && !crate::syntax::parser::runtime_fragment() && readonly && is_ref {
                        return Err(CompileError::new(
                            ref_span.unwrap_or(property_span),
                            "Readonly promoted property cannot be by-reference",
                        ));
                    }
                    promoted_properties.push(ClassProperty {
                        name: n.clone(),
                        visibility,
                        // The checker validates and records the `set` visibility exactly as for
                        // an ordinary `public private(set)` property declaration.
                        set_visibility,
                        type_expr: type_ann.clone(),
                        hooks: PropertyHooks::none(),
                        readonly,
                        is_final,
                        is_static: false,
                        is_abstract: false,
                        by_ref: is_ref,
                        is_promoted: true,
                        // PHP keeps constructor-promotion defaults on the parameter,
                        // not on the promoted property's default metadata.
                        default: None,
                        span: property_span,
                        attributes: attributes.clone(),
                    });
                    promoted_assignments.push(promoted_property_assignment(&n, is_ref, param_span));
                }
                param_attributes.push(attributes);
                params.push((n, type_ann, default, is_ref));
            }
            _ => {
                let error_span = parameter_error_span(tokens, *pos, span);
                return Err(CompileError::new(error_span, "Expected parameter variable"));
            }
        }
    }
    super::super::params::record_param_lines(span, param_lines);

    Ok((
        params,
        param_attributes,
        variadic,
        variadic_by_ref,
        variadic_type,
        promoted_properties,
        promoted_assignments,
    ))
}

/// Scans the token stream for visibility modifiers (`public`/`protected`/`private`), PHP 8.4
/// asymmetric write visibilities (`private(set)`/`protected(set)`/`public(set)`), and `readonly`
/// in any order. Returns `Ok(None)` if none are present. Rejects duplicate read or write
/// visibilities and `static`/`abstract`/`final` with an error. The read visibility defaults to
/// `Public` when only `readonly` or a `(set)` visibility is present, as in PHP.
fn parse_promoted_param_modifiers(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<Option<PromotedModifiers>, CompileError> {
    let mut visibility = None;
    let mut set_visibility = None;
    let mut readonly = false;
    let mut is_final = false;
    let mut first_span = None;

    loop {
        let Some((token, token_span)) = tokens
            .get(*pos)
            .map(|(token, metadata)| (token, metadata.span))
        else {
            break;
        };
        let keyword = match token {
            Token::Public => Some(Visibility::Public),
            Token::Protected => Some(Visibility::Protected),
            Token::Private => Some(Visibility::Private),
            _ => None,
        };
        if let Some(keyword) = keyword {
            let modifier_span = token_span;
            *pos += 1;
            // A visibility keyword immediately followed by `(set)` is the asymmetric write
            // visibility; otherwise it is the ordinary read visibility.
            let (slot, message) = if consume_set_marker(tokens, pos, modifier_span) {
                (&mut set_visibility, "Duplicate parameter set visibility")
            } else {
                (&mut visibility, "Duplicate parameter visibility")
            };
            if slot.is_some() {
                return Err(CompileError::new(token_span, message));
            }
            *slot = Some(keyword);
            first_span.get_or_insert(token_span);
            continue;
        }
        match token {
            Token::ReadOnly => {
                if readonly {
                    return Err(CompileError::new(token_span, "Duplicate readonly modifier"));
                }
                first_span.get_or_insert(token_span);
                readonly = true;
                *pos += 1;
            }
            Token::Static => {
                return Err(CompileError::runtime_unsupported(
                    token_span,
                    "Cannot use the static modifier on a parameter",
                ))
            }
            Token::Abstract => {
                return Err(CompileError::new(
                    token_span,
                    "Cannot use the abstract modifier on a parameter",
                ))
            }
            Token::Final => {
                if is_final {
                    return Err(CompileError::new(token_span, "Duplicate final modifier"));
                }
                first_span.get_or_insert(token_span);
                is_final = true;
                *pos += 1;
            }
            _ => break,
        }
    }

    let Some(span) = first_span else {
        return Ok(None);
    };

    Ok(Some(PromotedModifiers {
        visibility: visibility.unwrap_or(Visibility::Public),
        set_visibility,
        readonly,
        is_final,
        span,
    }))
}

/// Builds a synthetic `PropertyAssign` statement: `$this-><name> = $<name>` using the
/// given variable name and span. The statement models the implicit assignment that
/// PHP performs for a promoted constructor parameter.
fn promoted_property_assignment(name: &str, by_ref: bool, span: Span) -> Stmt {
    if by_ref && crate::syntax::parser::runtime_fragment() {
        let target = Expr::new(ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, span)),
            property: name.to_owned(),
        }, span);
        let write = Expr::new(ExprKind::Runtime(Box::new(RuntimeExpr::Write {
            target,
            operation: RuntimeWrite::BindReference(name.to_owned()),
        })), span);
        return Stmt::new(StmtKind::ExprStmt(write), span);
    }
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, span)),
            property: name.to_string(),
            value: Expr::new(ExprKind::Variable(name.to_string()), span),
        },
        span,
    )
}
