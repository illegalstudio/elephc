//! Purpose:
//! Parses class-like member bodies for classes, interfaces, and traits.
//! Handles properties, methods, constants, promoted constructor properties, and member modifiers.
//!
//! Called from:
//! - `crate::parser::stmt::oop::declarations` and trait/interface declaration parsers.
//!
//! Key details:
//! - Modifier and member parsing must preserve PHP visibility and abstract/static/final/readonly rules.

use crate::errors::CompileError;
use crate::lexer::Token;
use crate::names::{property_hook_get_method, property_hook_set_method};
use crate::parser::ast::{
    ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, PropertyHooks, Stmt, StmtKind, TraitUse,
    TypeExpr, Visibility,
};
use crate::parser::expr::parse_expr;
use crate::span::Span;

use super::super::params::{looks_like_typed_param, parse_name_list, parse_type_expr};
use super::super::{expect_semicolon, expect_token, parse_block};
use super::method_params::parse_method_params;
use super::traits::parse_trait_use;

/// Parses an `interface` declaration, including its name, `extends` clause, and body.
/// Consumes the `interface` keyword and expects a name followed by optional `extends` parents
/// and a body containing constants, properties, and method signatures.
pub(in crate::parser::stmt) fn parse_interface_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'interface'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => {
            return Err(CompileError::new(
                span,
                "Expected interface name after 'interface'",
            ))
        }
    };

    let extends = if *pos < tokens.len() && tokens[*pos].0 == Token::Extends {
        *pos += 1;
        parse_name_list(
            tokens,
            pos,
            span,
            "Expected parent interface name after 'extends'",
        )?
    } else {
        Vec::new()
    };

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

    Ok(Stmt::new(
        StmtKind::InterfaceDecl {
            name,
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
pub(in crate::parser::stmt) fn parse_trait_decl(
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Stmt, CompileError> {
    *pos += 1; // consume 'trait'

    let name = match tokens.get(*pos).map(|(t, _)| t) {
        Some(Token::Identifier(n)) => {
            let n = n.clone();
            *pos += 1;
            n
        }
        _ => return Err(CompileError::new(span, "Expected trait name after 'trait'")),
    };

    expect_token(tokens, pos, &Token::LBrace, "Expected '{' after trait name")?;
    let (trait_uses, properties, methods, constants, _cases) =
        parse_class_like_body(tokens, pos, "trait", false)?;
    expect_token(tokens, pos, &Token::RBrace, "Expected '}' at end of trait")?;

    Ok(Stmt::new(
        StmtKind::TraitDecl {
            name,
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
pub(in crate::parser::stmt) fn parse_class_like_body(
    tokens: &[(Token, Span)],
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
    let mut trait_uses = Vec::new();
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    let mut constants = Vec::new();
    let mut cases = Vec::new();

    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        // Capture any `#[...]` attribute groups attached to the next member —
        // they're attached to the resulting property or method below.
        let member_attributes = crate::parser::parse_attribute_lists(tokens, pos)?;
        if *pos >= tokens.len() || matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
            break;
        }
        let member_span = tokens[*pos].1;
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
            if owner_kind != "enum" {
                return Err(CompileError::new(
                    member_span,
                    "'case' is only valid inside an enum",
                ));
            }
            *pos += 1; // consume 'case'
            let case_name = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Identifier(name)) => {
                    let name = name.clone();
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

        let modifiers = parse_member_modifiers(tokens, pos);

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
            *pos += 1; // consume `const`
            // PHP 8 allows semi-reserved keywords as class-constant names, except `class`,
            // which is reserved for the `Foo::class` name fetch.
            let const_name = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Class) => {
                    return Err(CompileError::new(
                        member_span,
                        "Cannot use 'class' as a class constant name",
                    ))
                }
                Some(t) if crate::parser::keyword_name::bareword_name_from_token(t).is_some() => {
                    let n = crate::parser::keyword_name::bareword_name_from_token(t).unwrap();
                    *pos += 1;
                    n
                }
                _ => {
                    return Err(CompileError::new(
                        member_span,
                        "Expected class constant name after 'const'",
                    ))
                }
            };
            if constants.iter().any(|c: &ClassConst| c.name == const_name) {
                return Err(CompileError::new(
                    member_span,
                    &format!("Cannot redeclare class constant {}", const_name),
                ));
            }
            expect_token(
                tokens,
                pos,
                &Token::Assign,
                "Expected '=' after class constant name",
            )?;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            constants.push(ClassConst {
                name: const_name,
                visibility: modifiers.visibility,
                is_final: modifiers.is_final,
                value,
                span: member_span,
                attributes: member_attributes,
            });
            continue;
        }

        if tokens[*pos].0 == Token::Function {
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
            append_promoted_properties(&mut properties, promoted_properties)?;
            methods.push(method);
            continue;
        }

        let type_expr = parse_optional_property_type(tokens, pos, member_span)?;

        if let Some(Token::Variable(prop_name)) = tokens.get(*pos).map(|(t, _)| t.clone()) {
            if modifiers.is_static && modifiers.is_readonly {
                return Err(CompileError::new(
                    member_span,
                    "Static properties cannot be readonly",
                ));
            }
            let prop_name = prop_name.clone();
            *pos += 1;
            if properties.iter().any(|property| property.name == prop_name) {
                return Err(CompileError::new(
                    member_span,
                    &format!("Cannot redeclare property ${}", prop_name),
                ));
            }
            let default = if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                *pos += 1;
                Some(parse_expr(tokens, pos)?)
            } else {
                None
            };
            let (hooks, hook_accessors) =
                parse_property_hooks(tokens, pos, member_span, &prop_name, type_expr.as_ref())?;
            if modifiers.is_abstract && default.is_some() {
                return Err(CompileError::new(
                    member_span,
                    &format!("Abstract property ${} cannot have a default value", prop_name),
                ));
            }
            if modifiers.is_abstract && !hooks.any() {
                return Err(CompileError::new(
                    member_span,
                    "Only hooked properties may be declared abstract",
                ));
            }
            if modifiers.is_static && hooks.any() {
                return Err(CompileError::new(
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
            if hooks.any() && default.is_some() {
                return Err(CompileError::new(
                    member_span,
                    "Hooked properties cannot have a default value",
                ));
            }
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
                    return Err(CompileError::new(
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
            methods.extend(hook_accessors);
            properties.push(ClassProperty {
                name: prop_name,
                visibility: modifiers.visibility,
                set_visibility: modifiers.set_visibility,
                type_expr,
                hooks,
                readonly: modifiers.is_readonly,
                is_final: modifiers.is_final,
                is_static: modifiers.is_static,
                is_abstract: modifiers.is_abstract,
                by_ref: false,
                default,
                span: member_span,
                attributes: member_attributes,
            });
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

    Ok((trait_uses, properties, methods, constants, cases))
}

/// Appends promoted constructor properties to the class properties list.
/// Validates that no property with the same name already exists in the class,
/// returning an error on duplicate declaration.
fn append_promoted_properties(
    properties: &mut Vec<ClassProperty>,
    promoted_properties: Vec<ClassProperty>,
) -> Result<(), CompileError> {
    for promoted in promoted_properties {
        if properties.iter().any(|property| property.name == promoted.name) {
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
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
) -> Result<Option<TypeExpr>, CompileError> {
    if matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::Variable(_))) {
        return Ok(None);
    }
    if !matches!(
        tokens.get(*pos).map(|(t, _)| t),
        Some(Token::Identifier(_)) | Some(Token::Question) | Some(Token::Backslash)
    ) {
        return Ok(None);
    }
    Ok(Some(parse_type_expr(tokens, pos, span)?))
}

/// Holds parsed member modifiers for class-like members: visibility, static, readonly, abstract, final.
/// Used internally during class-like body parsing to collect modifiers before processing a member declaration.
pub(super) struct MemberModifiers {
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
fn parse_member_modifiers(tokens: &[(Token, Span)], pos: &mut usize) -> MemberModifiers {
    let mut visibility = Visibility::Public;
    let mut set_visibility = None;
    let mut is_static = false;
    let mut is_readonly = false;
    let mut is_abstract = false;
    let mut is_final = false;

    loop {
        // A visibility keyword immediately followed by `(set)` is a PHP 8.4 asymmetric write
        // visibility (`private(set)`); otherwise it is the ordinary read visibility.
        let visibility_keyword = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Public) => Some(Visibility::Public),
            Some(Token::Protected) => Some(Visibility::Protected),
            Some(Token::Private) => Some(Visibility::Private),
            _ => None,
        };
        if let Some(keyword) = visibility_keyword {
            *pos += 1;
            if consume_set_marker(tokens, pos) {
                set_visibility = Some(keyword);
            } else {
                visibility = keyword;
            }
            continue;
        }
        match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Static) => {
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

    MemberModifiers {
        visibility,
        set_visibility,
        is_static,
        is_readonly,
        is_abstract,
        is_final,
    }
}

/// Consumes a `(set)` marker at `*pos` (an `LParen`, the `set` identifier, and an `RParen`),
/// returning `true` when one was present. Leaves `*pos` unchanged otherwise. `set` is matched
/// case-insensitively, mirroring PHP's case-insensitive modifier keywords.
fn consume_set_marker(tokens: &[(Token, Span)], pos: &mut usize) -> bool {
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
    tokens: &[(Token, Span)],
    pos: &mut usize,
    span: Span,
    visibility: Visibility,
    is_static: bool,
    is_abstract: bool,
    is_final: bool,
) -> Result<(ClassMethod, Vec<ClassProperty>), CompileError> {
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
        .and_then(|(t, _)| crate::parser::keyword_name::bareword_name_from_token(t))
    {
        Some(n) => {
            *pos += 1;
            n
        }
        None => return Err(CompileError::new(span, "Expected method name")),
    };

    expect_token(
        tokens,
        pos,
        &Token::LParen,
        "Expected '(' after method name",
    )?;
    let (params, variadic, variadic_type, promoted_properties, promoted_assignments) =
        parse_method_params(tokens, pos, span, &method_name)?;
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
        if is_abstract || !has_body {
            return Err(CompileError::new(
                span,
                "Cannot declare promoted property in an abstract constructor",
            ));
        }
        if is_static {
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
    Ok((ClassMethod {
        name: method_name,
        visibility,
        is_static,
        is_abstract,
        is_final,
        has_body,
        params,
        variadic,
        variadic_type,
        return_type,
        by_ref_return,
        body,
        span,
        attributes: Vec::new(),
    }, promoted_properties))
}

/// Parses the body of an `interface` declaration.
/// Interface bodies may only contain constants, hooked properties, and method signatures (no bodies).
/// All properties are implicitly abstract and public; modifiers are validated but not stored as-is.
fn parse_interface_body(
    tokens: &[(Token, Span)],
    pos: &mut usize,
) -> Result<(Vec<ClassProperty>, Vec<ClassMethod>, Vec<ClassConst>), CompileError> {
    let mut properties = Vec::new();
    let mut methods = Vec::new();
    let mut constants = Vec::new();

    while *pos < tokens.len() && !matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
        // Attributes may decorate interface methods (e.g. `#[Deprecated]`).
        let member_attributes = crate::parser::parse_attribute_lists(tokens, pos)?;
        if *pos >= tokens.len() || matches!(tokens[*pos].0, Token::RBrace | Token::Eof) {
            break;
        }
        let member_span = tokens[*pos].1;
        let modifiers = parse_member_modifiers(tokens, pos);
        if *pos >= tokens.len() {
            return Err(CompileError::new(
                member_span,
                "Unexpected end of interface body",
            ));
        }
        if tokens[*pos].0 == Token::Const {
            *pos += 1; // consume `const`
            // PHP 8 allows semi-reserved keywords as class-constant names, except `class`,
            // which is reserved for the `Foo::class` name fetch.
            let const_name = match tokens.get(*pos).map(|(t, _)| t) {
                Some(Token::Class) => {
                    return Err(CompileError::new(
                        member_span,
                        "Cannot use 'class' as a class constant name",
                    ))
                }
                Some(t) if crate::parser::keyword_name::bareword_name_from_token(t).is_some() => {
                    let n = crate::parser::keyword_name::bareword_name_from_token(t).unwrap();
                    *pos += 1;
                    n
                }
                _ => {
                    return Err(CompileError::new(
                        member_span,
                        "Expected class constant name after 'const'",
                    ))
                }
            };
            if constants.iter().any(|c: &ClassConst| c.name == const_name) {
                return Err(CompileError::new(
                    member_span,
                    &format!("Cannot redeclare interface constant {}", const_name),
                ));
            }
            expect_token(
                tokens,
                pos,
                &Token::Assign,
                "Expected '=' after interface constant name",
            )?;
            let value = parse_expr(tokens, pos)?;
            expect_semicolon(tokens, pos)?;
            constants.push(ClassConst {
                name: const_name,
                visibility: modifiers.visibility,
                is_final: modifiers.is_final,
                value,
                span: member_span,
                attributes: member_attributes,
            });
            continue;
        }
        let type_expr = parse_optional_property_type(tokens, pos, member_span)?;
        if let Some(Token::Variable(prop_name)) = tokens.get(*pos).map(|(t, _)| t.clone()) {
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
            let prop_name = prop_name.clone();
            *pos += 1;
            if properties.iter().any(|property: &ClassProperty| property.name == prop_name) {
                return Err(CompileError::new(
                    member_span,
                    &format!("Cannot redeclare interface property ${}", prop_name),
                ));
            }
            if *pos < tokens.len() && tokens[*pos].0 == Token::Assign {
                return Err(CompileError::new(
                    member_span,
                    "Interface properties cannot have a default value",
                ));
            }
            let (hooks, hook_accessors) =
                parse_property_hooks(tokens, pos, member_span, &prop_name, type_expr.as_ref())?;
            if !hook_accessors.is_empty() {
                return Err(CompileError::new(
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
                return Err(CompileError::new(
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
            properties.push(ClassProperty {
                name: prop_name,
                visibility: Visibility::Public,
                set_visibility: None,
                type_expr,
                hooks,
                readonly: false,
                is_final: false,
                is_static: false,
                is_abstract: true,
                by_ref: false,
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
        let (mut method, promoted_properties) = parse_class_like_method(
            tokens,
            pos,
            member_span,
            modifiers.visibility,
            modifiers.is_static,
            true,
            modifiers.is_final,
        )?;
        if !promoted_properties.is_empty() {
            return Err(CompileError::new(
                member_span,
                "Cannot declare promoted property in an interface",
            ));
        }
        method.attributes = member_attributes;
        methods.push(method);
    }

    Ok((properties, methods, constants))
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
    tokens: &[(Token, Span)],
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
        let hook_span = tokens[*pos].1;
        let get_by_ref = if tokens[*pos].0 == Token::Ampersand {
            *pos += 1;
            true
        } else {
            false
        };
        let hook_name = match tokens.get(*pos).map(|(t, _)| t) {
            Some(Token::Identifier(name)) => name.clone(),
            _ => {
                return Err(CompileError::new(
                    hook_span,
                    "Expected property hook name",
                ))
            }
        };
        *pos += 1;
        let is_get = hook_name.eq_ignore_ascii_case("get");
        let is_set = hook_name.eq_ignore_ascii_case("set");
        if !is_get && !is_set {
            return Err(CompileError::new(
                hook_span,
                &format!("Unknown property hook '{}'", hook_name),
            ));
        }

        // Optional set-hook parameter list: `set(Type $value)` (the type is accepted but the
        // property type governs); the default parameter name is `$value`.
        let mut set_param = "value".to_string();
        if is_set && matches!(tokens.get(*pos).map(|(t, _)| t), Some(Token::LParen)) {
            *pos += 1;
            if looks_like_typed_param(tokens, *pos) {
                let _ = parse_type_expr(tokens, pos, hook_span)?;
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
                    return Err(CompileError::new(
                        hook_span,
                        "Short `set => expr` hooks require a backed property; use a block `set { ... }`",
                    ));
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

        if is_get {
            if hooks.requires_get() {
                return Err(CompileError::new(hook_span, "Duplicate get property hook"));
            }
            hooks.get = !get_by_ref;
            hooks.get_by_ref = get_by_ref;
            if let Some(body) = body {
                accessors.push(ClassMethod {
                    name: property_hook_get_method(prop_name),
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: Vec::new(),
                    variadic: None,
                    variadic_type: None,
                    return_type: prop_type.cloned(),
                    by_ref_return: get_by_ref,
                    body,
                    span: hook_span,
                    attributes: Vec::new(),
                });
            }
        } else {
            if get_by_ref {
                return Err(CompileError::new(
                    hook_span,
                    "Set property hook cannot return by reference",
                ));
            }
            if hooks.set {
                return Err(CompileError::new(hook_span, "Duplicate set property hook"));
            }
            hooks.set = true;
            if let Some(body) = body {
                accessors.push(ClassMethod {
                    name: property_hook_set_method(prop_name),
                    visibility: Visibility::Public,
                    is_static: false,
                    is_abstract: false,
                    is_final: false,
                    has_body: true,
                    params: vec![(set_param, prop_type.cloned(), None, false)],
                    variadic: None,
                    variadic_type: None,
                    return_type: Some(TypeExpr::Void),
                    by_ref_return: false,
                    body,
                    span: hook_span,
                    attributes: Vec::new(),
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
    if !hooks.any() {
        return Err(CompileError::new(span, "Expected property hook declaration"));
    }
    Ok((hooks, accessors))
}
