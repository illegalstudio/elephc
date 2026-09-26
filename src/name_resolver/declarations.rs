//! Purpose:
//! Resolves names inside class-like, function, constant, trait-use, and extern declarations.
//! Applies namespace/import context to declaration children while preserving declaration identity.
//!
//! Called from:
//! - `crate::name_resolver::statements::list::resolve_stmt_list()`.
//!
//! Key details:
//! - Declaration names become canonical before type checking and codegen symbol collection.

use crate::errors::CompileError;
use crate::names::{canonical_name_for_decl, php_symbol_key};
use crate::parser::ast::{
    Attribute, AttributeGroup, ClassConst, ClassMethod, ClassProperty, GenericDecl, Stmt, StmtKind,
    TraitAdaptation, TraitUse, TypeParam,
};

use super::expressions::resolve_expr;
use super::names::{resolve_type_expr, resolved_class_name};
use super::statements::{resolve_params, resolve_stmt_list};
use super::{resolved_name, Imports, Symbols};

/// Returns the type parameters a class or interface declares, or an empty slice.
fn declared_type_params(generics: &Option<Box<GenericDecl>>) -> &[TypeParam] {
    generics
        .as_ref()
        .map(|generics| generics.type_params.as_slice())
        .unwrap_or(&[])
}

/// Restores the bare spelling of each type parameter after namespace resolution.
///
/// `T` is not a class: it resolves against its own declaration's type parameter list, never
/// against the namespace. The resolver cannot know that while it canonicalizes names, so
/// `class Box<T>` written inside `namespace App` comes back with every `T` qualified to
/// `App\T` — a class nothing declares, which the checker then reports as an unknown type.
/// Mapping that spelling back is what makes a generic declaration mean the same thing inside a
/// namespace as at the top level.
///
/// The rewrite is exactly what a type parameter SHADOWING a same-named class should do, so a
/// real `App\T` is correctly invisible inside `Box<T>`'s body.
///
/// Applied to the already-resolved declaration, and through the same substitution helper
/// instantiation uses, so it reaches every type position rather than the signature alone.
fn restore_type_parameter_names(
    stmt: Stmt,
    type_params: &[TypeParam],
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Stmt {
    let bindings: Vec<(String, crate::parser::ast::TypeExpr)> = type_params
        .iter()
        .filter_map(|param| {
            let bare = crate::names::Name::unqualified(&param.name);
            let resolved = resolve_type_expr(
                &crate::parser::ast::TypeExpr::Named(bare),
                namespace,
                imports,
                symbols,
            );
            let crate::parser::ast::TypeExpr::Named(resolved) = resolved else {
                return None;
            };
            // At the top level the name is already bare and substituting it for itself would
            // be a no-op walk over the whole declaration.
            if resolved.as_str() == param.name {
                return None;
            }
            Some((
                resolved.as_str().to_string(),
                crate::parser::ast::TypeExpr::Named(crate::names::Name::unqualified(&param.name)),
            ))
        })
        .collect();
    if bindings.is_empty() {
        return stmt;
    }
    crate::generics::substitute_in_body(vec![stmt], &bindings)
        .pop()
        .expect("substituting one declaration yields one declaration")
}

/// Resolves every name inside a declaration's generic half.
///
/// A bound (`T : Entity`), a default (`K = Status`) and an inherited type argument
/// (`implements Repository<User>`) all name classes, and all of them are compared against the
/// canonical class table later. Leaving any of them unqualified would make
/// `implements Repository<User>` inside a namespace instantiate a DIFFERENT `Repository` from
/// the one the file declares.
///
/// The type parameter NAMES are carried through untouched: `T` resolves against its own
/// declaration's list, never against the namespace.
/// Resolves the names a type parameter list mentions, leaving the list's shape alone.
///
/// A bound and a default are NAMES: `<T : Entity>` has to find `Entity` through the same
/// namespace and imports as any other class mention, or the bound names a class that does not
/// exist. The variance marker is carried verbatim — resolution says nothing about how two
/// instantiations relate.
fn resolve_type_params(
    type_params: &[TypeParam],
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<TypeParam> {
    type_params
        .iter()
        .map(|param| TypeParam {
            name: param.name.clone(),
            bound: param
                .bound
                .as_ref()
                .map(|ty| resolve_type_expr(ty, namespace, imports, symbols)),
            default: param
                .default
                .as_ref()
                .map(|ty| resolve_type_expr(ty, namespace, imports, symbols)),
            variance: param.variance,
        })
        .collect()
}

fn resolve_generic_decl(
    generics: &Option<Box<GenericDecl>>,
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Option<Box<GenericDecl>> {
    let generics = generics.as_ref()?;
    Some(Box::new(GenericDecl {
        type_params: resolve_type_params(&generics.type_params, namespace, imports, symbols),
        extends_args: generics
            .extends_args
            .iter()
            .map(|ty| resolve_type_expr(ty, namespace, imports, symbols))
            .collect(),
        interface_args: generics
            .interface_args
            .iter()
            .map(|args| {
                args.iter()
                    .map(|ty| resolve_type_expr(ty, namespace, imports, symbols))
                    .collect()
            })
            .collect(),
    }))
}

/// Resolves names within top-level declaration statements.
///
/// Dispatches on `StmtKind` variants to resolve functions, classes, enums,
/// traits, interfaces, packed classes, extern declarations, and constants.
/// Applies `canonical_name_for_decl` to declaration names and resolves all
/// nested expressions, types, parameters, and attribute groups using the
/// provided namespace/import context. Returns `Ok(None)` for non-declaration
/// statements to signal passthrough.
pub(super) fn resolve_decl_stmt(
    stmt: &Stmt,
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Result<Option<Stmt>, CompileError> {
    let stmt_attributes = resolve_attribute_groups(&stmt.attributes, namespace, imports, symbols);
    match &stmt.kind {
        StmtKind::FunctionDecl {
            by_ref_return,
            name,
            type_params,
            params,
            param_attributes,
            variadic,
            variadic_by_ref,
            variadic_type,
            return_type,
            body,
        } => {
            let body = resolve_stmt_list(body, namespace, imports, symbols)?;
            let resolved = Stmt::with_attributes(
                StmtKind::FunctionDecl {
                    by_ref_return: *by_ref_return,
                    name: canonical_name_for_decl(namespace, name),
                    // Type parameter names are scoped to their own declaration and never
                    // namespace-qualified, so they are carried through untouched.
                    type_params: type_params.clone(),
                    params: resolve_params(params, namespace, imports, symbols),
                    param_attributes: param_attributes
                        .iter()
                        .map(|groups| resolve_attribute_groups(groups, namespace, imports, symbols))
                        .collect(),
                    variadic: variadic.clone(),
                    variadic_by_ref: *variadic_by_ref,
                    variadic_type: variadic_type.clone(),
                    return_type: return_type
                        .as_ref()
                        .map(|ty| resolve_type_expr(ty, namespace, imports, symbols)),
                    body,
                },
                stmt.span,
                stmt_attributes,
            );
            Ok(Some(restore_type_parameter_names(
                resolved,
                type_params,
                namespace,
                imports,
                symbols,
            )))
        }
        StmtKind::ClassDecl {
            generics,
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
            constants,
        } => {
            let resolved_methods = resolve_methods(methods, namespace, imports, symbols)?;
            let trait_uses = trait_uses
                .iter()
                .map(|trait_use| resolve_trait_use(trait_use, namespace, imports, symbols))
                .collect::<Result<Vec<_>, CompileError>>()?;
            let resolved = Stmt::with_attributes(
                StmtKind::ClassDecl {
                    generics: resolve_generic_decl(generics, namespace, imports, symbols),
                    name: canonical_name_for_decl(namespace, name),
                    extends: extends.as_ref().map(|name| {
                        resolved_name(resolved_class_name(name, namespace, imports, symbols))
                    }),
                    implements: implements
                        .iter()
                        .map(|name| {
                            resolved_name(resolved_class_name(name, namespace, imports, symbols))
                        })
                        .collect(),
                    is_abstract: *is_abstract,
                    is_final: *is_final,
                    is_readonly_class: *is_readonly_class,
                    trait_uses,
                    properties: resolve_properties(properties, namespace, imports, symbols),
                    methods: resolved_methods,
                    constants: resolve_class_consts(constants, namespace, imports, symbols),
                },
                stmt.span,
                stmt_attributes,
            );
            Ok(Some(restore_type_parameter_names(
                resolved,
                declared_type_params(generics),
                namespace,
                imports,
                symbols,
            )))
        }
        StmtKind::EnumDecl {
            name,
            generics,
            backing_type,
            cases,
            implements,
            trait_uses,
            methods,
            constants,
        } => {
            let trait_uses = trait_uses
                .iter()
                .map(|trait_use| resolve_trait_use(trait_use, namespace, imports, symbols))
                .collect::<Result<Vec<_>, CompileError>>()?;
            let resolved_cases = cases
                .iter()
                .map(|case| crate::parser::ast::EnumCaseDecl {
                    name: case.name.clone(),
                    value: case
                        .value
                        .as_ref()
                        .map(|expr| resolve_expr(expr, namespace, imports, symbols)),
                    span: case.span,
                    attributes: resolve_attribute_groups(
                        &case.attributes,
                        namespace,
                        imports,
                        symbols,
                    ),
                })
                .collect();
            let resolved_methods = resolve_methods(methods, namespace, imports, symbols)?;
            Ok(Some(Stmt::with_attributes(
                StmtKind::EnumDecl {
                    name: canonical_name_for_decl(namespace, name),
                    // The interface arguments NAME classes, and they resolve through the same
                    // namespace and imports as the interface itself — `implements Labelled<User>`
                    // inside a namespace means that namespace's `User`.
                    generics: resolve_generic_decl(generics, namespace, imports, symbols),
                    backing_type: backing_type.clone(),
                    cases: resolved_cases,
                    implements: implements
                        .iter()
                        .map(|name| {
                            resolved_name(resolved_class_name(name, namespace, imports, symbols))
                        })
                        .collect(),
                    trait_uses,
                    methods: resolved_methods,
                    constants: resolve_class_consts(constants, namespace, imports, symbols),
                },
                stmt.span,
                stmt_attributes,
            )))
        }
        StmtKind::PackedClassDecl { name, fields } => {
            let resolved_fields = fields
                .iter()
                .map(|field| crate::parser::ast::PackedField {
                    name: field.name.clone(),
                    type_expr: resolve_type_expr(&field.type_expr, namespace, imports, symbols),
                    span: field.span,
                })
                .collect();
            Ok(Some(Stmt::with_attributes(
                StmtKind::PackedClassDecl {
                    name: canonical_name_for_decl(namespace, name),
                    fields: resolved_fields,
                },
                stmt.span,
                stmt_attributes,
            )))
        }
        StmtKind::InterfaceDecl {
            generics,
            name,
            extends,
            properties,
            methods,
            constants,
        } => {
            let resolved_methods = resolve_methods(methods, namespace, imports, symbols)?;
            let resolved = Stmt::with_attributes(
                StmtKind::InterfaceDecl {
                    generics: resolve_generic_decl(generics, namespace, imports, symbols),
                    name: canonical_name_for_decl(namespace, name),
                    extends: extends
                        .iter()
                        .map(|name| {
                            resolved_name(resolved_class_name(name, namespace, imports, symbols))
                        })
                        .collect(),
                    properties: resolve_properties(properties, namespace, imports, symbols),
                    methods: resolved_methods,
                    constants: resolve_class_consts(constants, namespace, imports, symbols),
                },
                stmt.span,
                stmt_attributes,
            );
            Ok(Some(restore_type_parameter_names(
                resolved,
                declared_type_params(generics),
                namespace,
                imports,
                symbols,
            )))
        }
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
            constants,
        } => {
            let resolved_methods = resolve_methods(methods, namespace, imports, symbols)?;
            let trait_uses = trait_uses
                .iter()
                .map(|trait_use| resolve_trait_use(trait_use, namespace, imports, symbols))
                .collect::<Result<Vec<_>, CompileError>>()?;
            Ok(Some(Stmt::with_attributes(
                StmtKind::TraitDecl {
                    name: canonical_name_for_decl(namespace, name),
                    trait_uses,
                    properties: resolve_properties(properties, namespace, imports, symbols),
                    methods: resolved_methods,
                    constants: resolve_class_consts(constants, namespace, imports, symbols),
                },
                stmt.span,
                stmt_attributes,
            )))
        }
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => Ok(Some(Stmt::with_attributes(
            StmtKind::ExternFunctionDecl {
                name: canonical_name_for_decl(namespace, name),
                params: params.clone(),
                return_type: return_type.clone(),
                library: library.clone(),
            },
            stmt.span,
            stmt_attributes,
        ))),
        StmtKind::FunctionVariantGroup { name, variants } => Ok(Some(Stmt::with_attributes(
            StmtKind::FunctionVariantGroup {
                name: name.clone(),
                variants: variants.clone(),
            },
            stmt.span,
            stmt_attributes,
        ))),
        StmtKind::ExternClassDecl { name, fields } => Ok(Some(Stmt::with_attributes(
            StmtKind::ExternClassDecl {
                name: canonical_name_for_decl(namespace, name),
                fields: fields.clone(),
            },
            stmt.span,
            stmt_attributes,
        ))),
        StmtKind::ConstDecl { name, value } => Ok(Some(Stmt::with_attributes(
            StmtKind::ConstDecl {
                name: canonical_name_for_decl(namespace, name),
                value: resolve_expr(value, namespace, imports, symbols),
            },
            stmt.span,
            stmt_attributes,
        ))),
        _ => Ok(None),
    }
}

/// Resolves attribute groups by rewriting each attribute's name through
/// `resolved_class_name` and each attribute argument through `resolve_expr`.
///
/// - `groups`: slice of attribute groups to resolve.
/// - `namespace`, `imports`, `symbols`: standard name resolution context.
fn resolve_attribute_groups(
    groups: &[AttributeGroup],
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<AttributeGroup> {
    groups
        .iter()
        .map(|group| AttributeGroup {
            attributes: group
                .attributes
                .iter()
                .map(|attr| Attribute {
                    name: resolved_name(resolved_class_name(
                        &attr.name, namespace, imports, symbols,
                    )),
                    args: attr
                        .args
                        .iter()
                        .map(|arg| resolve_expr(arg, namespace, imports, symbols))
                        .collect(),
                    span: attr.span,
                })
                .collect(),
            span: group.span,
        })
        .collect()
}

/// Resolves a slice of class methods by resolving their parameter types,
/// return types, bodies, and attributes with the given namespace/import context.
fn resolve_methods(
    methods: &[ClassMethod],
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Result<Vec<ClassMethod>, CompileError> {
    methods
        .iter()
        .map(|method| {
            let body = resolve_stmt_list(&method.body, namespace, imports, symbols)?;
            Ok(ClassMethod {
                // `..method.clone()` below would carry these verbatim, but a bound and a default
                // are NAMES: `<U : Entity>` has to resolve `Entity` the same way the class-level
                // type parameters do, or the bound names a class that does not exist.
                type_params: resolve_type_params(&method.type_params, namespace, imports, symbols),
                params: resolve_params(&method.params, namespace, imports, symbols),
                param_attributes: method
                    .param_attributes
                    .iter()
                    .map(|groups| resolve_attribute_groups(groups, namespace, imports, symbols))
                    .collect(),
                return_type: method
                    .return_type
                    .as_ref()
                    .map(|ty| resolve_type_expr(ty, namespace, imports, symbols)),
                body,
                attributes: resolve_attribute_groups(
                    &method.attributes,
                    namespace,
                    imports,
                    symbols,
                ),
                ..method.clone()
            })
        })
        .collect()
}

/// Resolves a slice of class constants by resolving their value expressions
/// and attributes with the given namespace/import context.
fn resolve_class_consts(
    constants: &[ClassConst],
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<ClassConst> {
    constants
        .iter()
        .map(|constant| ClassConst {
            type_expr: constant
                .type_expr
                .as_ref()
                .map(|ty| resolve_type_expr(ty, namespace, imports, symbols)),
            value: resolve_expr(&constant.value, namespace, imports, symbols),
            attributes: resolve_attribute_groups(&constant.attributes, namespace, imports, symbols),
            ..constant.clone()
        })
        .collect()
}

/// Resolves a slice of class properties by resolving their type expressions,
/// default value expressions, and attributes with the given namespace/import context.
fn resolve_properties(
    properties: &[ClassProperty],
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<ClassProperty> {
    properties
        .iter()
        .map(|property| ClassProperty {
            type_expr: property
                .type_expr
                .as_ref()
                .map(|ty| resolve_type_expr(ty, namespace, imports, symbols)),
            default: property
                .default
                .as_ref()
                .map(|expr| resolve_expr(expr, namespace, imports, symbols)),
            attributes: resolve_attribute_groups(&property.attributes, namespace, imports, symbols),
            ..property.clone()
        })
        .collect()
}

/// Resolves a trait use statement by rewriting trait names and method selectors.
///
/// Alias display names keep their declared spelling because Reflection exposes
/// them, while later method lookup still normalizes through `php_symbol_key`.
pub(super) fn resolve_trait_use(
    trait_use: &TraitUse,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Result<TraitUse, CompileError> {
    Ok(TraitUse {
        trait_names: trait_use
            .trait_names
            .iter()
            .map(|name| {
                resolved_name(resolved_class_name(
                    name,
                    current_namespace,
                    imports,
                    symbols,
                ))
            })
            .collect(),
        adaptations: trait_use
            .adaptations
            .iter()
            .map(|adaptation| match adaptation {
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => Ok(TraitAdaptation::Alias {
                    trait_name: trait_name.as_ref().map(|name| {
                        resolved_name(resolved_class_name(
                            name,
                            current_namespace,
                            imports,
                            symbols,
                        ))
                    }),
                    method: php_symbol_key(method),
                    alias: alias.clone(),
                    visibility: visibility.clone(),
                }),
                TraitAdaptation::InsteadOf {
                    trait_name,
                    method,
                    instead_of,
                } => Ok(TraitAdaptation::InsteadOf {
                    trait_name: trait_name.as_ref().map(|name| {
                        resolved_name(resolved_class_name(
                            name,
                            current_namespace,
                            imports,
                            symbols,
                        ))
                    }),
                    method: php_symbol_key(method),
                    instead_of: instead_of
                        .iter()
                        .map(|name| {
                            resolved_name(resolved_class_name(
                                name,
                                current_namespace,
                                imports,
                                symbols,
                            ))
                        })
                        .collect(),
                }),
            })
            .collect::<Result<Vec<_>, CompileError>>()?,
        span: trait_use.span,
    })
}
