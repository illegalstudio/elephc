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
use crate::lexer::Token;
use crate::parser::ast::{
    ClassProperty, Expr, ExprKind, PropertyHooks, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::super::params::{looks_like_typed_param, parse_type_expr};
use super::super::expect_token;

type MethodParam = (String, Option<TypeExpr>, Option<Expr>, bool);
type ParsedMethodParams = (Vec<MethodParam>, Option<String>, Vec<ClassProperty>, Vec<Stmt>);

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
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    method_name: &str,
) -> Result<ParsedMethodParams, CompileError> {
    let mut params = Vec::new();
    let mut variadic = None;
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
        }
        // PHP 8.0 parameter attributes — also covers attributes preceding a
        // promoted-property modifier such as `#[Inject] public Foo $f`.
        crate::parser::consume_attribute_lists(tokens, pos)?;
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
            let ref_span = tokens[*pos].1;
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

        match tokens.get(*pos).map(|(t, s)| (t, *s)) {
            Some((Token::Variable(n), param_span)) => {
                let n = n.clone();
                *pos += 1;
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                if let Some((visibility, readonly, property_span)) = promotion {
                    if readonly && is_ref {
                        return Err(CompileError::new(
                            ref_span.unwrap_or(property_span),
                            "Readonly promoted property cannot be by-reference",
                        ));
                    }
                    promoted_properties.push(ClassProperty {
                        name: n.clone(),
                        visibility,
                        type_expr: type_ann.clone(),
                        hooks: PropertyHooks::none(),
                        readonly,
                        is_final: false,
                        is_static: false,
                        is_abstract: false,
                        by_ref: is_ref,
                        // PHP keeps constructor-promotion defaults on the parameter,
                        // not on the promoted property's default metadata.
                        default: None,
                        span: property_span,
                        attributes: Vec::new(),
                    });
                    promoted_assignments.push(promoted_property_assignment(&n, param_span));
                }
                params.push((n, type_ann, default, is_ref));
            }
            _ => return Err(CompileError::new(span, "Expected parameter variable")),
        }
    }

    Ok((params, variadic, promoted_properties, promoted_assignments))
}

/// Scans the token stream for visibility modifiers (`public`/`protected`/`private`)
/// and `readonly` in any order, returning `(Visibility, readonly, first_token_span)`.
/// Returns `Ok(None)` if none are present. Rejects `static`/`abstract`/`final` with an
/// error. Visibility defaults to `Public` if only `readonly` is present.
fn parse_promoted_param_modifiers(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<Option<(Visibility, bool, Span)>, CompileError> {
    let mut visibility = None;
    let mut readonly = false;
    let mut first_span = None;

    loop {
        match tokens.get(*pos).map(|(t, s)| (t, *s)) {
            Some((Token::Public, token_span)) => {
                if visibility.is_some() {
                    return Err(CompileError::new(token_span, "Duplicate parameter visibility"));
                }
                first_span.get_or_insert(token_span);
                visibility = Some(Visibility::Public);
                *pos += 1;
            }
            Some((Token::Protected, token_span)) => {
                if visibility.is_some() {
                    return Err(CompileError::new(token_span, "Duplicate parameter visibility"));
                }
                first_span.get_or_insert(token_span);
                visibility = Some(Visibility::Protected);
                *pos += 1;
            }
            Some((Token::Private, token_span)) => {
                if visibility.is_some() {
                    return Err(CompileError::new(token_span, "Duplicate parameter visibility"));
                }
                first_span.get_or_insert(token_span);
                visibility = Some(Visibility::Private);
                *pos += 1;
            }
            Some((Token::ReadOnly, token_span)) => {
                if readonly {
                    return Err(CompileError::new(token_span, "Duplicate readonly modifier"));
                }
                first_span.get_or_insert(token_span);
                readonly = true;
                *pos += 1;
            }
            Some((Token::Static, token_span)) => {
                return Err(CompileError::new(
                    token_span,
                    "Cannot use the static modifier on a parameter",
                ))
            }
            Some((Token::Abstract, token_span)) => {
                return Err(CompileError::new(
                    token_span,
                    "Cannot use the abstract modifier on a parameter",
                ))
            }
            Some((Token::Final, token_span)) => {
                return Err(CompileError::new(
                    token_span,
                    "Cannot use the final modifier on a parameter",
                ))
            }
            _ => break,
        }
    }

    let Some(property_span) = first_span else {
        return Ok(None);
    };

    Ok(Some((
        visibility.unwrap_or(Visibility::Public),
        readonly,
        property_span,
    )))
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
