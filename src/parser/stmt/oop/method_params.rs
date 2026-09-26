//! Purpose:
//! Parses method and constructor parameters, including promoted properties.
//! Produces parameter metadata plus synthetic property and assignment statements for promoted parameters.
//!
//! Called from:
//! - `crate::parser::stmt::oop::body::parse_class_like_method()`.
//!
//! Key details:
//! - Promoted property lowering must keep constructor assignment order and member visibility metadata aligned.

use crate::errors::CompileError;
use crate::lexer::{SpannedToken, Token};
use crate::parser::ast::{
    AttributeGroup, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt, StmtKind, TypeExpr,
    Visibility,
};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::super::expect_token;
use super::super::params::{looks_like_typed_param, parse_type_expr};
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
    span: Span,
}

/// Parses method or constructor parameters from `(` to `)`, including PHP 8.0 promoted
/// properties. Returns the parameter list, optional variadic name, promoted property
/// declarations, and synthetic constructor assignments for promoted parameters.
///
/// - `method_name` is used only to reject promoted properties in non-constructor methods.
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
        let attributes = crate::parser::parse_attribute_lists(tokens, pos)?;
        if variadic.is_some() {
            return Err(CompileError::new(
                span,
                "Variadic parameter must be the last parameter",
            ));
        }

        let promotion = parse_promoted_param_modifiers(tokens, pos)?;
        if promotion.is_some() && !method_name.eq_ignore_ascii_case("__construct") {
            return Err(CompileError::new(
                span,
                "Cannot declare promoted property outside a constructor",
            ));
        }

        let type_ann = if looks_like_typed_param(tokens, *pos) {
            Some(parse_type_expr(tokens, pos, span)?)
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
            if promotion.is_some() {
                return Err(CompileError::new(
                    span,
                    "Cannot declare variadic promoted property",
                ));
            }
            // A typed variadic (`int ...$xs`) is accepted; the declared element type is preserved
            // so call validation can check each argument collected into the variadic.
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

        match tokens
            .get(*pos)
            .map(|(token, metadata)| (token, metadata.span))
        {
            Some((Token::Variable(n), param_span)) => {
                let n = n.clone();
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
                    span: property_span,
                }) = promotion
                {
                    if readonly && is_ref {
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
                        is_final: false,
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
                    promoted_assignments.push(promoted_property_assignment(&n, param_span));
                }
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
            *pos += 1;
            // A visibility keyword immediately followed by `(set)` is the asymmetric write
            // visibility; otherwise it is the ordinary read visibility.
            let (slot, message) = if consume_set_marker(tokens, pos) {
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
                return Err(CompileError::new(
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
                return Err(CompileError::new(
                    token_span,
                    "Cannot use the final modifier on a parameter",
                ))
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
        span,
    }))
}

/// Builds a synthetic `PropertyAssign` statement: `$this-><name> = $<name>` using the
/// given variable name and span. The statement models the implicit assignment that
/// PHP performs for a promoted constructor parameter.
fn promoted_property_assignment(name: &str, span: Span) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, span)),
            property: name.to_string(),
            value: Expr::new(ExprKind::Variable(name.to_string()), span),
        },
        span,
    )
}
