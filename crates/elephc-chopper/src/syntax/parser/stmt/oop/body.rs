//! Purpose:
//! Parses class-like member bodies for classes, interfaces, and traits.
//! Handles properties, methods, constants, promoted constructor properties, and member modifiers.
//!
//! Called from:
//! - `crate::syntax::parser::stmt::oop::declarations` and trait/interface declaration parsers.
//!
//! Key details:
//! - Modifier and member parsing must preserve PHP visibility and abstract/static/final/readonly rules.
//! - Every property and constant a declarator list emits carries the span of its own declarator
//!   (name plus optional initializer), not the span of the whole declaration.
//! - A member that fails to parse is skipped to its own end, so its error does not cascade into
//!   the rest of the class body.

use crate::syntax::parser::validation_only;
use crate::syntax::errors::CompileError;
use crate::syntax::lexer::{SpannedToken, Token};
use crate::syntax::names::{property_hook_get_method, property_hook_set_method};
use crate::syntax::parser::ast::{
    ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind, PropertyHooks, Stmt,
    StmtKind, TraitUse, TypeExpr, Visibility,
};
use crate::syntax::parser::expr::parse_expr;
use crate::syntax::span::Span;

use super::super::params::{
    looks_like_typed_param, parse_name_list, parse_type_expr, parse_type_param_list,
};
use super::super::{expect_semicolon, expect_token, parse_block, parse_unqualified_name};
use super::method_params::parse_method_params;
use super::traits::parse_trait_use;

/// Parses an `interface` declaration, including its name, `extends` clause, and body.
/// Consumes the `interface` keyword and expects a name followed by optional `extends` parents
/// and a body containing constants, properties, and method signatures.
pub(in crate::syntax::parser::stmt) fn parse_interface_decl(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'interface'

    let name = parse_unqualified_name(
        tokens,
        pos,
        span,
        "Expected interface name after 'interface'",
    )?;

    let _type_scope = crate::syntax::parser::runtime_templates::TypeScope::enter();
    let type_params = parse_type_param_list(tokens, pos, span)?;

    let (extends, interface_args) = if *pos < tokens.len() && tokens[*pos].0 == Token::Extends {
        *pos += 1;
        parse_name_list(
            tokens,
            pos,
            span,
            "Expected parent interface name after 'extends'",
        )?
    } else {
        (Vec::new(), Vec::new())
    };

    // An interface has no single parent, so its inherited type arguments all live in
    // `interface_args`, aligned with `extends`.
    let generics =
        crate::syntax::parser::ast::GenericDecl::new(type_params, Vec::new(), interface_args);

    expect_token(
        tokens,
        pos,
        &Token::LBrace,
        "Expected '{' after interface name",
    )?;
    let (properties, methods, constants) = parse_interface_body(tokens, pos)?;
    expect_token(
        tokens,
        pos,
        &Token::RBrace,
        "Expected '}' at end of interface",
    )?;
    let span = if crate::syntax::parser::runtime_fragment() {
        Span::with_end_from(span, tokens[*pos - 1].1.span)
    } else {
        span
    };

    Ok(Stmt::new(
        StmtKind::InterfaceDecl {
            name,
            generics,
            extends,
            properties,
            methods,
            constants,
        },
        span,
    ))
}

/// Parses a `trait` declaration, consuming the `trait` keyword, name, and body.
/// Trait bodies support `use` trait statements, properties, methods, and constants.
pub(in crate::syntax::parser::stmt) fn parse_trait_decl(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'trait'

    let name = parse_unqualified_name(tokens, pos, span, "Expected trait name after 'trait'")?;

    // A trait has no parent and implements nothing, so its type parameters are the whole of its
    // generic half.
    let _type_scope = crate::syntax::parser::runtime_templates::TypeScope::enter();
    let type_params = parse_type_param_list(tokens, pos, span)?;
    let generics = crate::syntax::parser::ast::GenericDecl::new(type_params, Vec::new(), Vec::new());

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after trait name")?;
    let (trait_uses, properties, methods, constants, _cases) =
        parse_class_like_body(tokens, pos, "trait", false)?;
    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of trait")?;
    let span = if crate::syntax::parser::runtime_fragment() {
        Span::with_end_from(span, tokens[*pos - 1].1.span)
    } else {
        span
    };

    Ok(Stmt::new(
        StmtKind::TraitDecl {
            name,
            generics,
            trait_uses,
            properties,
            methods,
            constants,
        },
        span,
    ))
}

/// Parses the body of a class, trait, or abstract class.
/// Returns four vectors: trait uses, properties, methods, and constants.
/// Handles modifiers (visibility, static, readonly, abstract, final), property hooks,
/// promoted constructor properties, and member attributes.
/// `owner_kind` is used only for error messages (e.g., "class", "trait").
/// `enclosing_is_abstract` controls whether abstract property declarations are permitted.
///
/// A member that fails to parse is skipped to its end (see [`parse_members_recovering`]), so
/// the error is reported once instead of cascading into the rest of the file.
pub(in crate::syntax::parser::stmt) fn parse_class_like_body(
    tokens: &[SpannedToken],
    pos: &mut usize,
    owner_kind: &str,
    enclosing_is_abstract: bool,
) -> Result<
    (
        Vec<TraitUse>,
        Vec<ClassProperty>,
        Vec<ClassMethod>,
        Vec<ClassConst>,
        Vec<EnumCaseDecl>,
    ),
    CompileError,
> {
    let mut members = Default::default();
    parse_members_recovering(tokens, pos, &mut members, |tokens, pos, members, member_start| {
        parse_class_like_members(
            tokens,
            pos,
            owner_kind,
            enclosing_is_abstract,
            members,
            member_start,
        )
    })?;
    Ok(members)
}

/// The members of a class-like body, in the order `parse_class_like_body` returns them.
type ClassLikeMembers = (
    Vec<TraitUse>,
    Vec<ClassProperty>,
    Vec<ClassMethod>,
    Vec<ClassConst>,
    Vec<EnumCaseDecl>,
);

/// Parses class-like members into `members` up to the closing `}`, stopping at the first
/// error. `member_start` is set to the first token of each member before it is parsed.
fn parse_class_like_members(
    tokens: &[SpannedToken],
    pos: &mut usize,
    owner_kind: &str,
    enclosing_is_abstract: bool,
    members: &mut ClassLikeMembers,
    member_start: &mut usize,
) -> Result<(), CompileError> {
    let (trait_uses, properties, methods, constants, cases) = members;

    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        *member_start = *pos;
        // Capture any `#[...]` attribute groups attached to the next member —
        // they're attached to the resulting property or method below.
        let member_attributes = crate::syntax::parser::parse_attribute_lists(tokens, pos)?;
        if *pos >= tokens.len() || matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
            break;
        }
        let member_span = tokens[*pos].1.span;
        if tokens[*pos].0 == Token::Use {
            if !member_attributes.is_empty() {
                return Err(CompileError::new(
                    member_span,
                    "Attributes are not supported on `use` trait declarations",
                ));
            }
            trait_uses.push(parse_trait_use(tokens, pos, member_span)?);
            continue;
        }

        // Enum cases. Cases carry no visibility modifiers, so they are handled before the
        // modifier scan; `case` outside an enum is a hard error.
        if tokens[*pos].0 == Token::Case {
            if !validation_only() && owner_kind != "enum" {
                return Err(CompileError::new(
                    member_span,
                    "'case' is only valid inside an enum",
                ));
            }
            *pos += 1; // consume 'case'
            let case_name = match tokens.get(*pos).map(|(t, _)| t) {
                // Zend's grammar accepts `case class;`; its compiler reserves `class`.
                Some(Token::Class) if !validation_only() => {
                    return Err(CompileError::new(
                        member_span,
                        "Cannot use 'class' as an enum case name",
                    ))
                }
                Some(t)
                    if crate::syntax::parser::keyword_name::bareword_name_from_token(
                        t,
                        &tokens[*pos].1,
                    )
                    .is_some() =>
                {
                    let name = crate::syntax::parser::keyword_name::bareword_name_from_token(
                        t,
                        &tokens[*pos].1,
                    )
                    .unwrap();
                    *pos += 1;
                    name
                }
                _ => {
                    return Err(CompileError::new(
                        member_span,
                        "Expected case name after 'case'",
                    ))
                }
            };
            let value = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                *pos += 1;
                Some(parse_expr(tokens, pos)?)
            } else {
                None
            };
            expect_semicolon(tokens, pos)?;
            cases.push(EnumCaseDecl {
                name: case_name,
                value,
                span: member_span,
                attributes: member_attributes,
            });
            continue;
        }

        let modifiers = parse_member_modifiers(tokens, pos)?;

        if *pos >= tokens.len() {
            return Err(CompileError::new(
                member_span,
                &format!("Unexpected end of {} body", owner_kind),
            ));
        }

        if tokens[*pos].0 == Token::Const {
            if modifiers.is_static {
                return Err(CompileError::new(
                    member_span,
                    "Class constants cannot be declared static",
                ));
            }
            if modifiers.is_readonly || modifiers.is_abstract {
                return Err(CompileError::new(
                    member_span,
                    "Class constants cannot be declared readonly or abstract",
                ));
            }
            parse_constant_declarator_list(
                tokens,
                pos,
                member_span,
                &modifiers,
                &member_attributes,
                constants,
                owner_kind,
            )?;
            continue;
        }

        if tokens[*pos].0 == Token::Function {
            if crate::syntax::parser::runtime_fragment() && modifiers.set_visibility.is_some() {
                return Err(CompileError::runtime_unsupported(member_span, "Set visibility is only valid on properties"));
            }
            if modifiers.is_readonly {
                return Err(CompileError::new(
                    member_span,
                    "Readonly methods are not supported",
                ));
            }
            let (mut method, promoted_properties) = parse_class_like_method(
                tokens,
                pos,
                member_span,
                modifiers.visibility,
                modifiers.is_static,
                modifiers.is_abstract,
                modifiers.is_final,
            )?;
            method.attributes = member_attributes;
            append_promoted_properties(properties, promoted_properties)?;
            methods.push(method);
            continue;
        }

        let type_expr = parse_optional_property_type(tokens, pos, member_span)?;

        if validation_only() && !modifiers.saw_modifier {
            return Err(CompileError::new(
                tokens.get(*pos).map_or(member_span, |(_, metadata)| metadata.span),
                "Property declarations require a modifier",
            ));
        }

        if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Variable(_))) {
            if !validation_only() && modifiers.is_static && modifiers.is_readonly {
                return Err(CompileError::runtime_unsupported(
                    member_span,
                    "Static properties cannot be readonly",
                ));
            }
            // One declaration may introduce SEVERAL properties: `public int $w = 40, $h = 22;`.
            // The type and every modifier are shared; each declarator carries its own optional
            // default. (Issue #684.)
            //
            // A hook block belongs to ONE property, so php-src rejects it on a list at all --
            // `public int $a = 1, $b { get => 1; }` is `unexpected token "{", expecting "," or
            // ";"`. `saw_comma` carries that: once the declaration has more than one name, the
            // tail must be a bare `;`.
            let mut saw_comma = false;
            loop {
                let Some(Token::Variable(prop_name)) = tokens.get(*pos).map(|(t, _)| t.clone())
                else {
                    return Err(CompileError::new(
                        tokens.get(*pos).map_or(member_span, |(_, meta)| meta.span),
                        "Expected a property name after ',' in the declaration list",
                    ));
                };
                let name_span = tokens[*pos].1.span;
                *pos += 1;
                // PHP's parser accepts duplicate properties; the compiler rejects them later.
                if !validation_only() && properties.iter().any(|property| property.name == prop_name) {
                    return Err(CompileError::new(
                        name_span,
                        &format!("Cannot redeclare property ${}", prop_name),
                    ));
                }
                let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                    *pos += 1;
                    Some(parse_expr(tokens, pos)?)
                } else {
                    None
                };
                // This declarator's own extent: `$b = 2` in `public int $a = 1, $b = 2;`, so a
                // diagnostic or an IDE range about `$b` points at `$b` (issue #1140).
                let declarator_span =
                    crate::syntax::parser::expr::span_through_prev_token(tokens, *pos, name_span);
                let more_declarators =
                    matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Comma));
                let (hooks, hook_accessors) = if more_declarators {
                    saw_comma = true;
                    *pos += 1;
                    (PropertyHooks::none(), Vec::new())
                } else {
                    if saw_comma && matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::LBrace))
                    {
                        return Err(CompileError::new(
                            tokens[*pos].1.span,
                            "A property hook block needs a declaration of its own: \
                             remove the other names from this one, or drop the hooks",
                        ));
                    }
                    parse_property_hooks(tokens, pos, member_span, &prop_name, type_expr.as_ref())?
                };
                let type_expr = type_expr.clone();
                if !validation_only() && modifiers.is_abstract && default.is_some() {
                    return Err(CompileError::new(
                        member_span,
                        &format!(
                            "Abstract property ${} cannot have a default value",
                            prop_name
                        ),
                    ));
                }
                if !validation_only() && modifiers.is_abstract && !hooks.any() {
                    return Err(CompileError::new(
                        member_span,
                        "Only hooked properties may be declared abstract",
                    ));
                }
                if !validation_only() {
                    if modifiers.is_static && hooks.any() {
                        return Err(CompileError::runtime_unsupported(
                            member_span,
                            "Cannot declare hooks for static property",
                        ));
                    }
                    if modifiers.is_readonly && hooks.any() {
                        return Err(CompileError::new(
                            member_span,
                            "Hooked properties cannot be readonly",
                        ));
                    }
                    if hooks.is_virtual() && default.is_some() {
                        return Err(CompileError::runtime_unsupported(
                            member_span,
                            "Virtual properties cannot have a default value",
                        ));
                    }
                }
                if !validation_only() {
                    if modifiers.is_abstract {
                        if owner_kind != "trait" && !enclosing_is_abstract {
                            return Err(CompileError::new(
                                member_span,
                                "Abstract properties can only be declared in abstract classes",
                            ));
                        }
                        if modifiers.is_static {
                            return Err(CompileError::new(
                                member_span,
                                "Abstract static properties are not supported",
                            ));
                        }
                        if modifiers.is_final {
                            return Err(CompileError::runtime_unsupported(
                                member_span,
                                "Cannot use the final modifier on an abstract property",
                            ));
                        }
                        if modifiers.visibility == Visibility::Private {
                            return Err(CompileError::new(
                                member_span,
                                "Private abstract properties are not supported",
                            ));
                        }
                    } else if hooks.any() && hook_accessors.is_empty() {
                        return Err(CompileError::new(
                            member_span,
                            "Non-abstract property hook must have a body",
                        ));
                    }
                }
                methods.extend(hook_accessors);
                properties.push(ClassProperty {
                    name: prop_name,
                    visibility: modifiers.visibility.clone(),
                    set_visibility: modifiers.set_visibility.clone(),
                    type_expr,
                    hooks,
                    readonly: modifiers.is_readonly,
                    is_final: modifiers.is_final,
                    is_static: modifiers.is_static,
                    is_abstract: modifiers.is_abstract,
                    by_ref: false,
                    is_promoted: false,
                    default,
                    span: declarator_span,
                    attributes: member_attributes.clone(),
                });
                if !more_declarators {
                    break;
                }
            }
            continue;
        }

        return Err(CompileError::new(
            member_span,
            &format!(
                "Expected trait use, property, or method declaration in {} body",
                owner_kind
            ),
        ));
    }

    Ok(())
}

/// Appends promoted constructor properties to the class properties list.
/// Validates that no property with the same name already exists in the class,
/// returning an error on duplicate declaration.
fn append_promoted_properties(
    properties: &mut Vec<ClassProperty>,
    promoted_properties: Vec<ClassProperty>,
) -> Result<(), CompileError> {
    for promoted in promoted_properties {
        // PHP's parser accepts duplicate promoted properties; the compiler rejects them later.
        if !validation_only() && properties
            .iter()
            .any(|property| property.name == promoted.name)
        {
            return Err(CompileError::new(
                promoted.span,
                &format!("Cannot redeclare promoted property ${}", promoted.name),
            ));
        }
        properties.push(promoted);
    }
    Ok(())
}

/// Parses an optional type expression for class properties.
/// Returns `None` if the next token is a variable (no type given), or a `Some(TypeExpr)` otherwise.
/// Does not consume the variable token itself; the caller handles that.
fn parse_optional_property_type(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<Option<TypeExpr>, CompileError> {
    if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Variable(_))) {
        return Ok(None);
    }
    // PHP 8.2's standalone `false` / `true` / `null` are KEYWORD tokens, not identifiers.
    // Property parsing preserves null's declaration and rejects unimplemented true-only slots.
    // Excluding these tokens here made `public false $v;` parse
    // as an untyped property whose NAME is a keyword — an immediate statement-level error.
    let starts_type = matches!(
        tokens.get(*pos).map(|(t, _)| t),
        Some(Token::Identifier(_))
            | Some(Token::Question)
            | Some(Token::Backslash)
            | Some(Token::False)
            | Some(Token::True)
            | Some(Token::Null)
    ) || crate::syntax::parser::relative_name_starts_at(tokens, *pos)
        // A `(` only starts a property type as a DNF intersection group such as `(A&B)|D`.
        // Other parenthesized forms stay on the ordinary class-body error path, which keeps
        // Zend's diagnostic for markers like `public (set) $x;`.
        || (matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::LParen))
            && parenthesized_intersection_starts_at(tokens, *pos));
    if !starts_type {
        return Ok(None);
    }
    Ok(Some(parse_property_type_expr(tokens, pos, span)?))
}

/// Returns true when the parenthesized group at `pos` contains a top-level `&` intersection.
fn parenthesized_intersection_starts_at(tokens: &[SpannedToken], pos: usize) -> bool {
    let mut depth = 0usize;
    for (token, _) in tokens.iter().skip(pos) {
        match token {
            Token::LParen => depth += 1,
            Token::RParen => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return false;
                }
            }
            Token::Ampersand if depth == 1 => return true,
            _ => {}
        }
    }
    false
}

/// Keeps null-only property declarations distinct from void and refuses true-only storage.
pub(super) fn parse_property_type_expr(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Result<TypeExpr, CompileError> {
    let start = *pos;
    let parsed = parse_type_expr(tokens, pos, span)?;
    if tokens[start..*pos].iter().any(|(token, _)| matches!(token, Token::True)) {
        return Err(CompileError::new(span, "Literal true property types are not supported"));
    }
    if matches!(tokens.get(start).map(|(token, _)| token), Some(Token::Null))
        && matches!(parsed, TypeExpr::Void)
    {
        return Ok(TypeExpr::Named(crate::syntax::names::Name::unqualified("null")));
    }
    Ok(parsed)
}

/// Parses the optional PHP 8.3 type between `const` and a class-constant name.
/// A token followed immediately by `=` is the untyped constant name, including
/// semi-reserved names such as `string`.
fn parse_optional_class_const_type(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
) -> Option<TypeExpr> {
    if matches!(tokens.get(*pos + 1).map(|(token, _)| token), Some(Token::Assign)) {
        return None;
    }
    let before_type = *pos;
    match parse_type_expr(tokens, pos, span) {
        Ok(type_expr) => Some(type_expr),
        Err(_) => {
            *pos = before_type;
            None
        }
    }
}

/// Holds parsed member modifiers for class-like members: visibility, static, readonly, abstract, final.
/// Used internally during class-like body parsing to collect modifiers before processing a member declaration.
pub(super) struct MemberModifiers {
    /// Whether any modifier token was written. PHP's grammar requires at least one on a
    /// property declaration; a bare `$x;` member is a parse error.
    saw_modifier: bool,
    visibility: Visibility,
    /// PHP 8.4 asymmetric visibility: the write (`set`) visibility from a `private(set)` /
    /// `protected(set)` modifier, when present. Only meaningful for properties.
    set_visibility: Option<Visibility>,
    is_static: bool,
    is_readonly: bool,
    is_abstract: bool,
    is_final: bool,
}

/// Scans tokens to collect member modifiers (visibility, static, readonly, abstract, final).
/// Consumes any matching modifier tokens and returns a `MemberModifiers` struct.
/// Default visibility is `Public` if no visibility modifier is present.
fn parse_member_modifiers(tokens: &[SpannedToken], pos: &mut usize) -> Result<MemberModifiers, CompileError> {
    let modifier_start = *pos;
    let mut var_modifier = None;
    let mut visibility = Visibility::Public;
    let mut set_visibility = None;
    let mut is_static = false;
    let mut is_readonly = false;
    let mut is_abstract = false;
    let mut is_final = false;

    loop {
        // `var $x;` is the legacy public-property modifier.
        if matches!(
            tokens.get(*pos).map(|(t, _)| t),
            Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("var")
        ) {
            var_modifier = Some(tokens[*pos].1.span);
            visibility = Visibility::Public;
            *pos += 1;
            continue;
        }
        // A visibility keyword immediately followed by `(set)` is a PHP 8.4 asymmetric write
        // visibility (`private(set)`); otherwise it is the ordinary read visibility.
        let visibility_keyword = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Public) => Some(Visibility::Public),
            Some(Token::Protected) => Some(Visibility::Protected),
            Some(Token::Private) => Some(Visibility::Private),
            _ => None,
        };
        if let Some(keyword) = visibility_keyword {
            let modifier_span = tokens[*pos].1.span;
            *pos += 1;
            if consume_set_marker(tokens, pos, modifier_span) {
                if set_visibility.is_some() {
                    return Err(CompileError::new(modifier_span, "Multiple set visibility modifiers"));
                }
                set_visibility = Some(keyword);
            } else {
                visibility = keyword;
            }
            continue;
        }
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Static) => {
                if is_static && crate::syntax::parser::runtime_fragment() {
                    return Err(CompileError::runtime_unsupported(tokens[*pos].1.span, "Multiple static modifiers"));
                }
                is_static = true;
                *pos += 1;
            }
            Some(Token::ReadOnly) => {
                is_readonly = true;
                *pos += 1;
            }
            Some(Token::Abstract) => {
                is_abstract = true;
                *pos += 1;
            }
            Some(Token::Final) => {
                is_final = true;
                *pos += 1;
            }
            _ => break,
        }
    }

    if let Some(span) = var_modifier {
        if crate::syntax::parser::runtime_fragment() && *pos - modifier_start > 1 {
            return Err(CompileError::runtime_unsupported(span, "The 'var' modifier cannot be combined with other modifiers"));
        }
    }
    Ok(MemberModifiers {
        saw_modifier: *pos != modifier_start,
        visibility,
        set_visibility,
        is_static,
        is_readonly,
        is_abstract,
        is_final,
    })
}

/// Consumes a `(set)` marker at `*pos` (an `LParen`, the `set` identifier, and an `RParen`),
/// returning `true` when one was present. Leaves `*pos` unchanged otherwise. `set` is matched
/// case-insensitively, mirroring PHP's case-insensitive modifier keywords. Shared with the
/// promoted constructor parameter modifier parser so both declaration forms read `(set)` alike.
pub(crate) fn consume_set_marker(
    tokens: &[SpannedToken],
    pos: &mut usize,
    modifier_span: Span,
) -> bool {
    if !crate::syntax::parser::runtime_fragment() && !tokens.get(*pos).is_some_and(|(_, metadata)| {
        metadata.span.line == modifier_span.end_line
            && metadata.span.col == modifier_span.end_column()
    }) {
        return false;
    }
    let is_set_ident = matches!(
        tokens.get(*pos + 1).map(|(t, _)| t),
        Some(Token::Identifier(name)) if name.eq_ignore_ascii_case("set")
    );
    if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::LParen))
        && is_set_ident
        && matches!(tokens.get(*pos + 2).map(|(t, _)| t), Some(Token::RParen))
    {
        *pos += 3;
        true
    } else {
        false
    }
}

/// Parses a method declaration inside a class-like body (class, trait, interface).
/// Consumes the `function` keyword, name, parameters, optional return type, and body.
/// Returns the `ClassMethod` and any promoted constructor properties found in the parameters.
/// Modifier flags (visibility, static, abstract, final) are passed in and stored on the method;
/// the function itself only consumes the `function` keyword and subsequent syntax.
fn parse_class_like_method(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    visibility: Visibility,
    is_static: bool,
    is_abstract: bool,
    is_final: bool,
) -> Result<(ClassMethod, Vec<ClassProperty>), CompileError> {
    let method_start = tokens[*pos].1.span;
    *pos += 1; // consume 'function'
    // PHP `function &m()` returns a reference (alias) to the returned lvalue.
    let by_ref_return = matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Ampersand));
    if by_ref_return {
        *pos += 1;
    }
    // PHP 8 allows identifiers and any semi-reserved keyword as a method name (e.g. `self`,
    // `parent`, `static`, `list`, `print`).
    let method_name = match tokens
        .get(*pos)
        .and_then(|(token, metadata)| {
            crate::syntax::parser::keyword_name::bareword_name_from_token(token, metadata)
        })
    {
        Some(n) => {
            *pos += 1;
            n
        }
        None => return Err(CompileError::new(span, "Expected method name")),
    };

    // A method may declare type parameters of its OWN, distinct from its class's: `map<U>` on a
    // `Box<T>` binds `T` when the class is instantiated and `U` when the method is called. The
    // list answers empty when there is no `<`, so an ordinary method costs one token peek.
    let _type_scope = crate::syntax::parser::runtime_templates::TypeScope::enter();
    let type_params = parse_type_param_list(tokens, pos, span)?;

    expect_token(
        tokens,
        pos,
        &Token::LParen,
        "Expected '(' after method name",
    )?;
    let (
        params,
        param_attributes,
        variadic,
        variadic_by_ref,
        variadic_type,
        promoted_properties,
        promoted_assignments,
    ) = parse_method_params(tokens, pos, span, &method_name)?;
    expect_token(tokens, pos, &Token::RParen, "Expected ')'")?;
    // Parse optional return type: `: TypeExpr`
    let return_type = if *pos < tokens.len() && tokens[*pos].0 == Token::Colon {
        *pos += 1;
        Some(parse_type_expr(tokens, pos, span)?)
    } else {
        None
    };
    let (has_body, body) = if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        (false, Vec::new())
    } else {
        (true, parse_block(tokens, pos)?)
    };
    if !promoted_properties.is_empty() {
        // TOKEN_PARSE accepts an abstract constructor's promoted properties; PHP rejects them
        // while compiling the declaration, after the parse phase.
        if !validation_only() && (is_abstract || !has_body) {
            return Err(CompileError::runtime_unsupported(
                span,
                "Cannot declare promoted property in an abstract constructor",
            ));
        }
        // TOKEN_PARSE accepts a static constructor's promoted properties; PHP rejects them
        // while compiling the declaration, after the parse phase.
        if !validation_only() && is_static {
            return Err(CompileError::new(
                span,
                "Constructor promotion cannot be used on static constructors",
            ));
        }
    }
    let body = if promoted_assignments.is_empty() {
        body
    } else {
        promoted_assignments.into_iter().chain(body).collect()
    };
    let span = if crate::syntax::parser::runtime_fragment() {
        Span::with_end_from(method_start, tokens[*pos - 1].1.span)
    } else {
        span
    };
    Ok((
        ClassMethod {
            type_params,
            name: method_name,
            visibility,
            is_static,
            is_abstract,
            is_final,
            has_body,
            params,
            param_attributes,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            by_ref_return,
            body,
            span,
            attributes: Vec::new(),
        },
        promoted_properties,
    ))
}

/// Parses the body of an `interface` declaration.
/// Interface bodies may only contain constants, hooked properties, and method signatures (no bodies).
/// All properties are implicitly abstract and public; modifiers are validated but not stored as-is.
///
/// Like a class body, a member that fails to parse is skipped to its end.
fn parse_interface_body(
    tokens: &[SpannedToken],
    pos: &mut usize,
) -> Result<(Vec<ClassProperty>, Vec<ClassMethod>, Vec<ClassConst>), CompileError> {
    let mut members = Default::default();
    parse_members_recovering(tokens, pos, &mut members, parse_interface_members)?;
    Ok(members)
}

/// Parses interface members into `members` up to the closing `}`, stopping at the first
/// error. `member_start` is set to the first token of each member before it is parsed.
fn parse_interface_members(
    tokens: &[SpannedToken],
    pos: &mut usize,
    members: &mut (Vec<ClassProperty>, Vec<ClassMethod>, Vec<ClassConst>),
    member_start: &mut usize,
) -> Result<(), CompileError> {
    let (properties, methods, constants) = members;

    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        *member_start = *pos;
        // Attributes may decorate interface methods (e.g. `#[Deprecated]`).
        let member_attributes = crate::syntax::parser::parse_attribute_lists(tokens, pos)?;
        if *pos >= tokens.len() || matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
            break;
        }
        let member_span = tokens[*pos].1.span;
        let modifiers = parse_member_modifiers(tokens, pos)?;
        if *pos >= tokens.len() {
            return Err(CompileError::new(
                member_span,
                "Unexpected end of interface body",
            ));
        }
        if tokens[*pos].0 == Token::Const {
            parse_constant_declarator_list(
                tokens,
                pos,
                member_span,
                &modifiers,
                &member_attributes,
                constants,
                "interface",
            )?;
            continue;
        }
        let type_expr = parse_optional_property_type(tokens, pos, member_span)?;
        if validation_only() && !modifiers.saw_modifier && tokens[*pos].0 != Token::Function {
            // Interface methods are implicitly public, so `function f();` has no modifier.
            return Err(CompileError::new(
                tokens.get(*pos).map_or(member_span, |(_, metadata)| metadata.span),
                "Property declarations require a modifier",
            ));
        }
        if let Some(Token::Variable(prop_name)) = tokens.get(*pos).map(|(t, _)| t.clone()) {
            // PHP accepts every one of these declarations through TOKEN_PARSE and rejects them
            // while compiling the interface, so validation mode must not fail them.
            if !validation_only() {
                if modifiers.is_abstract {
                    return Err(CompileError::new(
                        member_span,
                        "Property in interface cannot be explicitly abstract",
                    ));
                }
                if modifiers.visibility != Visibility::Public {
                    return Err(CompileError::new(
                        member_span,
                        "Property in interface cannot be protected or private",
                    ));
                }
                if modifiers.is_final {
                    return Err(CompileError::new(
                        member_span,
                        "Interface properties cannot be final",
                    ));
                }
            }
            let prop_name = prop_name.clone();
            *pos += 1;
            if !validation_only() && properties
                .iter()
                .any(|property: &ClassProperty| property.name == prop_name)
            {
                return Err(CompileError::new(
                    member_span,
                    &format!("Cannot redeclare interface property ${}", prop_name),
                ));
            }
            if !validation_only() && *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                return Err(CompileError::runtime_unsupported(
                    member_span,
                    "Interface properties cannot have a default value",
                ));
            }
            if validation_only() && *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                *pos += 1;
                let _ = parse_expr(tokens, pos)?;
            }
            let (hooks, hook_accessors) =
                parse_property_hooks(tokens, pos, member_span, &prop_name, type_expr.as_ref())?;
            if !validation_only() {
                if !hook_accessors.is_empty() {
                    return Err(CompileError::runtime_unsupported(
                        member_span,
                        "Interface property hooks cannot have a body",
                    ));
                }
                if !hooks.any() {
                    return Err(CompileError::new(
                        member_span,
                        "Interfaces may only include hooked properties",
                    ));
                }
                if modifiers.is_static {
                    return Err(CompileError::runtime_unsupported(
                        member_span,
                        "Cannot declare hooks for static property",
                    ));
                }
                if modifiers.is_readonly {
                    return Err(CompileError::new(
                        member_span,
                        "Hooked properties cannot be readonly",
                    ));
                }
            }
            properties.push(ClassProperty {
                name: prop_name,
                visibility: Visibility::Public,
                set_visibility: if crate::syntax::parser::runtime_fragment() {
                    modifiers.set_visibility
                } else {
                    None
                },
                type_expr,
                hooks,
                readonly: false,
                is_final: false,
                is_static: false,
                is_abstract: true,
                by_ref: false,
                is_promoted: false,
                default: None,
                span: member_span,
                attributes: member_attributes,
            });
            continue;
        }
        if tokens[*pos].0 != Token::Function {
            return Err(CompileError::new(
                member_span,
                "Interfaces may only contain method, property, or constant declarations",
            ));
        }
        if crate::syntax::parser::runtime_fragment() && modifiers.set_visibility.is_some() {
            return Err(CompileError::runtime_unsupported(member_span, "Set visibility is only valid on properties"));
        }
        let (mut method, promoted_properties) = parse_class_like_method(
            tokens,
            pos,
            member_span,
            modifiers.visibility,
            modifiers.is_static,
            true,
            modifiers.is_final,
        )?;
        if !validation_only() && !promoted_properties.is_empty() {
            return Err(CompileError::new(
                member_span,
                "Cannot declare promoted property in an interface",
            ));
        }
        method.attributes = member_attributes;
        methods.push(method);
    }

    Ok(())
}

/// Runs `parse_members` over a class-like body until it reaches the closing `}`, collecting
/// every member's error.
///
/// When a member fails, parsing resumes after that member (see [`skip_class_member`]) rather
/// than at the statement level. Statement recovery would resume inside the failed declaration,
/// so `public function __construct(public string $s = null) {}` would also report its `{}` and
/// the class's `}` as unexpected tokens, where php reports the one error.
fn parse_members_recovering<T>(
    tokens: &[SpannedToken],
    pos: &mut usize,
    members: &mut T,
    mut parse_members: impl FnMut(
        &[SpannedToken],
        &mut usize,
        &mut T,
        &mut usize,
    ) -> Result<(), CompileError>,
) -> Result<(), CompileError> {
    let mut errors = Vec::new();
    loop {
        let mut member_start = *pos;
        match parse_members(tokens, pos, members, &mut member_start) {
            Ok(()) => break,
            Err(error) => {
                errors.extend(error.flatten());
                *pos = skip_class_member(tokens, member_start);
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompileError::from_many(errors))
    }
}

/// Returns the index just past the class-like member that starts at `start`: past its `;`, or
/// past the `}` that closes its body (a method body, property hooks, a trait-use adaptation
/// block), whichever comes first outside parentheses and brackets. Stops on the body's own
/// closing `}` or at the end of input without consuming it, so the caller's `}` check still
/// sees it.
///
/// Open delimiters are kept on a stack. A `}` closes the nearest `{` and drops any `(` or `[`
/// left open after it, so an unbalanced `echo (;` inside a method body still ends the skip at
/// that body's `}` instead of swallowing the next member. A `)` or `]` only closes its own
/// kind, and a `{` opened inside parentheses (a `match` in a parameter default) does not end
/// the member when it closes.
///
/// After a top-level `=` (a property or constant initializer), the `}` closing a top-level `{`
/// may end an initializer expression (`const X = match (1) { default => 1 };`) or a hook block
/// (`public int $p = 1 { get => 2; }`, or a malformed `{}`). The token after it decides: the
/// start of another member ends the skip before that member, and anything else (`;`, `} + 1;`)
/// continues the skip up to the declaration's `;`. So that `;` is not reported as a stray
/// member, and a broken hook block does not swallow the next member.
fn skip_class_member(tokens: &[SpannedToken], start: usize) -> usize {
    let mut open: Vec<&Token> = Vec::new();
    let mut in_initializer = false;
    let mut index = start;
    while let Some((token, _)) = tokens.get(index) {
        match token {
            Token::Eof => return index,
            // No `{` of this member is open: this is the enclosing body's `}`, even when a `(`
            // or `[` was left unclosed before it.
            Token::RBrace if !open.contains(&&Token::LBrace) => return index,
            Token::Assign if open.is_empty() => in_initializer = true,
            Token::LParen | Token::LBracket | Token::LBrace => open.push(token),
            Token::RParen | Token::RBracket => {
                let opener = if matches!(token, Token::RParen) {
                    Token::LParen
                } else {
                    Token::LBracket
                };
                if open.last() == Some(&&opener) {
                    open.pop();
                }
            }
            Token::RBrace => {
                while let Some(opener) = open.pop() {
                    if matches!(opener, Token::LBrace) {
                        break;
                    }
                }
                if open.is_empty() {
                    let next = tokens.get(index + 1).map(|(t, _)| t);
                    if !in_initializer || starts_class_member(next) {
                        return index + 1;
                    }
                }
            }
            Token::Semicolon if open.is_empty() => return index + 1,
            _ => {}
        }
        index += 1;
    }
    index
}

/// Whether `token` can only start a new class-like member (a modifier, `function`, `const`,
/// `case`, `use`, `var` or an attribute group), so it cannot continue an initializer expression
/// that ended at the `}` before it. `static` and `function` also start closures, but never right
/// after a `}`.
fn starts_class_member(token: Option<&Token>) -> bool {
    match token {
        Some(Token::Identifier(name)) => name.eq_ignore_ascii_case("var"),
        Some(
            Token::Public
            | Token::Protected
            | Token::Private
            | Token::Static
            | Token::Abstract
            | Token::Final
            | Token::ReadOnly
            | Token::Function
            | Token::Const
            | Token::Case
            | Token::Use
            | Token::AttrOpen,
        ) => true,
        _ => false,
    }
}

/// Parses everything after `const` in a class, interface, trait or enum body.
///
/// One `const` may introduce SEVERAL constants: `const A = 1, B = 2;`. The optional declared type
/// and every modifier are shared by the whole list; each declarator carries its own required
/// value. (Issue #684.)
///
/// Shared by the class-like and interface parsers because they had already drifted: the interface
/// copy still required a semicolon straight after the first value, so
/// `interface Limits { const MIN = 1, MAX = 10; }` reported `Expected ';'` while the identical
/// class declaration compiled. `owner_kind` (`class`, `trait`, `enum` or `interface`) only names
/// the owner in the duplicate-name diagnostic; everything else is the same language rule.
///
/// Each emitted `ClassConst` carries the span of its own declarator, `B = 2` in
/// `const A = 1, B = 2;`, rather than the span of the whole declaration (issue #1140).
///
/// Expects `*pos` on the `const` token and leaves it past the terminating `;`.
#[allow(clippy::too_many_arguments)]
fn parse_constant_declarator_list(
    tokens: &[SpannedToken],
    pos: &mut usize,
    member_span: Span,
    modifiers: &MemberModifiers,
    member_attributes: &[crate::syntax::parser::ast::AttributeGroup],
    constants: &mut Vec<ClassConst>,
    owner_kind: &str,
) -> Result<(), CompileError> {
    *pos += 1; // consume `const`
    let type_expr = parse_optional_class_const_type(tokens, pos, member_span);
    let initial_constant_count = constants.len();
    loop {
        let name_span = tokens.get(*pos).map_or(member_span, |(_, meta)| meta.span);
        // PHP 8 allows semi-reserved keywords as class-constant names, but its compiler
        // reserves `class` for `Foo::class`; the grammar itself accepts the declaration.
        let const_name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Class) if !validation_only() => {
                return Err(CompileError::runtime_unsupported(
                    member_span,
                    "Cannot use 'class' as a class constant name",
                ))
            }
            Some(t)
                if crate::syntax::parser::keyword_name::bareword_name_from_token(t, &tokens[*pos].1)
                    .is_some() =>
            {
                let n =
                    crate::syntax::parser::keyword_name::bareword_name_from_token(t, &tokens[*pos].1)
                        .unwrap();
                *pos += 1;
                n
            }
            _ if constants.len() == initial_constant_count => {
                return Err(CompileError::new(
                    member_span,
                    "Expected class constant name after 'const'",
                ))
            }
            _ => {
                return Err(CompileError::new(
                    name_span,
                    "Expected a constant name after ',' in the declaration list",
                ))
            }
        };
        // PHP's parser accepts duplicate class constants; the compiler rejects them later.
        if !validation_only() && !crate::syntax::parser::runtime_fragment()
            && constants.iter().any(|c: &ClassConst| c.name == const_name)
        {
            return Err(CompileError::new(
                name_span,
                &format!("Cannot redeclare {} constant {}", owner_kind, const_name),
            ));
        }
        expect_token(
            tokens,
            pos,
            &Token::Assign,
            "Expected '=' after class constant name",
        )?;
        let value = parse_expr(tokens, pos)?;
        let declarator_span =
            crate::syntax::parser::expr::span_through_prev_token(tokens, *pos, name_span);
        let more_declarators = matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Comma));
        if more_declarators {
            *pos += 1;
        } else {
            expect_semicolon(tokens, pos)?;
        }
        constants.push(ClassConst {
            name: const_name,
            visibility: modifiers.visibility.clone(),
            is_final: modifiers.is_final,
            type_expr: type_expr.clone(),
            value,
            span: declarator_span,
            attributes: member_attributes.to_vec(),
        });
        if !more_declarators {
            return Ok(());
        }
    }
}

/// Parses a property hook block (`{ get; set; }`) following a class property declaration.
/// Returns a `PropertyHooks` indicating which hooks are present (`get`, `set`, by-ref variants).
/// Consumes the opening `{`, hook declarations, and closing `}`.
/// Returns an error if no hook declarations appear or if the block is malformed.
/// Parses the property-hook tail of a property declaration: either a bare `;` (no hooks), or a
/// `{ get ...; set ...; }` block. Concrete hook bodies are compiled into synthetic accessor methods
/// (`__propget_<name>()` / `__propset_<name>($value)`) returned alongside the [`PropertyHooks`]
/// flags, so the bodies flow through every later pass as ordinary methods. `prop_name`/`prop_type`
/// name and type the property the hooks belong to; the get accessor returns `prop_type` and the set
/// accessor receives it. Abstract/interface hooked properties (a hook ending in `;`) produce flags
/// but no accessor methods.
fn parse_property_hooks(
    tokens: &[SpannedToken],
    pos: &mut usize,
    span: Span,
    prop_name: &str,
    prop_type: Option<&TypeExpr>,
) -> Result<(PropertyHooks, Vec<ClassMethod>), CompileError> {
    if *pos < tokens.len() && tokens[*pos].0 == Token::Semicolon {
        *pos += 1;
        return Ok((PropertyHooks::none(), Vec::new()));
    }
    if !matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::LBrace)) {
        return Err(CompileError::new(
            span,
            "Expected ';' or property hook block after property declaration",
        ));
    }
    *pos += 1;

    let mut hooks = PropertyHooks::none();
    let mut accessors: Vec<ClassMethod> = Vec::new();
    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        let hook_attributes = crate::syntax::parser::parse_attribute_lists(tokens, pos)?;
        let mut hook_is_final = false;
        while matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Final)) {
            hook_is_final = true;
            *pos += 1;
        }
        if *pos >= tokens.len() || matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
            // Attributes or `final` were consumed and no hook name followed.
            let span = tokens.get(*pos).map_or(span, |(_, metadata)| metadata.span);
            return Err(CompileError::new(span, "Expected property hook name"));
        }
        let hook_span = tokens[*pos].1.span;
        let get_by_ref = if tokens[*pos].0 == Token::Ampersand {
            *pos += 1;
            true
        } else {
            false
        };
        let hook_name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => name.clone(),
            _ => return Err(CompileError::new(hook_span, "Expected property hook name")),
        };
        *pos += 1;
        let is_get = hook_name.eq_ignore_ascii_case("get");
        let is_set = hook_name.eq_ignore_ascii_case("set");
        if !is_get && !is_set {
            if !validation_only() {
                return Err(CompileError::new(
                    hook_span,
                    &format!("Unknown property hook '{}'", hook_name),
                ));
            }
            if validation_only() {
                match tokens.get(*pos).map(|(t, _)| t) {
                    Some(Token::Semicolon) => {
                        *pos += 1;
                    }
                    Some(Token::DoubleArrow) => {
                        *pos += 1;
                        let _ = parse_expr(tokens, pos)?;
                        expect_semicolon(tokens, pos)?;
                    }
                    Some(Token::LBrace) => {
                        let _ = parse_block(tokens, pos)?;
                    }
                    _ => {
                        return Err(CompileError::new(
                            hook_span,
                            "Expected '=>', '{', or ';' in property hook",
                        ))
                    }
                }
                continue;
            }
        }

        // Optional set-hook parameter list: `set(Type $value)` (the type is accepted but the
        // property type governs); the default parameter name is `$value`.
        let mut set_param = "value".to_string();
        let mut set_param_type = None;
        if is_set && matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::LParen)) {
            *pos += 1;
            if looks_like_typed_param(tokens, *pos) {
                set_param_type = Some(parse_type_expr(tokens, pos, hook_span)?);
            } else if crate::syntax::parser::runtime_fragment() && prop_type.is_some() {
                return Err(CompileError::runtime_unsupported(hook_span, "Typed property requires a typed set hook parameter"));
            }
            match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Variable(name)) => {
                    set_param = name.clone();
                    *pos += 1;
                }
                _ => {
                    return Err(CompileError::new(
                        hook_span,
                        "Expected '$value' parameter in set hook",
                    ))
                }
            }
            expect_token(
                tokens,
                pos,
                &Token::RParen,
                "Expected ')' after set hook parameter",
            )?;
        }

        // Hook body: `;` (abstract), `=> expr;` (short), or `{ ... }` (block).
        let body: Option<Vec<Stmt>> = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Semicolon) => {
                *pos += 1;
                None
            }
            Some(Token::DoubleArrow) => {
                *pos += 1;
                let expr = parse_expr(tokens, pos)?;
                expect_semicolon(tokens, pos)?;
                if is_get {
                    Some(vec![Stmt::new(StmtKind::Return(Some(expr)), hook_span)])
                } else {
                    Some(vec![Stmt::new(
                        StmtKind::PropertyAssign {
                            object: Box::new(Expr::new(ExprKind::This, hook_span)),
                            property: prop_name.to_string(),
                            value: expr,
                        },
                        hook_span,
                    )])
                }
            }
            Some(Token::LBrace) => Some(parse_block(tokens, pos)?),
            _ => {
                return Err(CompileError::new(
                    hook_span,
                    "Expected '=>', '{', or ';' in property hook",
                ))
            }
        };
        let runtime_fragment = crate::syntax::parser::runtime_fragment();
        let hook_span = if runtime_fragment {
            Span::with_end_from(hook_span, tokens[*pos - 1].1.span)
        } else {
            hook_span
        };

        if is_get {
            // PHP's parser accepts duplicate hooks; the compiler rejects them later.
            if !validation_only() && hooks.requires_get() {
                return Err(CompileError::runtime_unsupported(hook_span, "Duplicate get property hook"));
            }
            hooks.get = !get_by_ref;
            hooks.get_by_ref = get_by_ref;
            if let Some(body) = body {
                accessors.push(ClassMethod {
                    type_params: Vec::new(),
                    name: property_hook_get_method(prop_name),
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_final: hook_is_final,
                    has_body: true,
                    params: Vec::new(),
                    param_attributes: Vec::new(),
                    variadic: None,
                    variadic_by_ref: false,
                    variadic_type: None,
                    return_type: prop_type.cloned(),
                    by_ref_return: get_by_ref,
                    body,
                    span: hook_span,
                    attributes: hook_attributes,
                });
            }
        } else {
            if get_by_ref {
                return Err(CompileError::runtime_unsupported(
                    hook_span,
                    "Set property hook cannot return by reference",
                ));
            }
            if !validation_only() && hooks.set {
                return Err(CompileError::runtime_unsupported(hook_span, "Duplicate set property hook"));
            }
            hooks.set = true;
            if let Some(body) = body {
                accessors.push(ClassMethod {
                    type_params: Vec::new(),
                    name: property_hook_set_method(prop_name),
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_final: hook_is_final,
                    has_body: true,
                    params: vec![(
                        set_param,
                        if runtime_fragment { set_param_type } else { prop_type.cloned() },
                        None,
                        false,
                    )],
                    param_attributes: vec![Vec::new()],
                    variadic: None,
                    variadic_by_ref: false,
                    variadic_type: None,
                    return_type: Some(TypeExpr::Void),
                    by_ref_return: false,
                    body,
                    span: hook_span,
                    attributes: hook_attributes,
                });
            }
        }
    }

    expect_token(
        tokens,
        pos,
        &Token::RBrace,
        "Expected '}' at end of property hook block",
    )?;
    if !validation_only() && !hooks.any() {
        return Err(CompileError::runtime_unsupported(
            span,
            "Expected property hook declaration",
        ));
    }
    if !validation_only() {
        // `uses_backing_slot` only feeds `is_virtual()`, a compiler-only check below. Walking
        // every hook body under TOKEN_PARSE would build no parser decision, so skip it.
        hooks.uses_backing_slot = accessors.iter().any(|method| {
            super::property_hook_storage::body_uses_backing_slot(&method.body, prop_name)
        });
    }
    Ok((hooks, accessors))
}
