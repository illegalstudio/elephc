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
    Attribute, AttributeGroup, ClassConst, ClassMethod, ClassProperty, Stmt, StmtKind,
    TraitAdaptation, TraitUse,
};

use super::expressions::resolve_expr;
use super::names::{resolve_type_expr, resolved_class_name};
use super::statements::{resolve_params, resolve_stmt_list};
use super::{resolved_name, Imports, Symbols};

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
            name,
            params,
            variadic,
            return_type,
            body,
        } => {
            let body = resolve_stmt_list(body, namespace, imports, symbols)?;
            Ok(Some(Stmt::with_attributes(
                StmtKind::FunctionDecl {
                    name: canonical_name_for_decl(namespace, name),
                    params: resolve_params(params, namespace, imports, symbols),
                    variadic: variadic.clone(),
                    return_type: return_type
                        .as_ref()
                        .map(|ty| resolve_type_expr(ty, namespace, imports, symbols)),
                    body,
                },
                stmt.span,
                stmt_attributes,
            )))
        }
        StmtKind::ClassDecl {
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
            Ok(Some(Stmt::with_attributes(
                StmtKind::ClassDecl {
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
            )))
        }
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => {
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
            Ok(Some(Stmt::with_attributes(
                StmtKind::EnumDecl {
                    name: canonical_name_for_decl(namespace, name),
                    backing_type: backing_type.clone(),
                    cases: resolved_cases,
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
            name,
            extends,
            properties,
            methods,
            constants,
        } => {
            let resolved_methods = resolve_methods(methods, namespace, imports, symbols)?;
            Ok(Some(Stmt::with_attributes(
                StmtKind::InterfaceDecl {
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
                        &attr.name,
                        namespace,
                        imports,
                        symbols,
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
                params: resolve_params(&method.params, namespace, imports, symbols),
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
            value: resolve_expr(&constant.value, namespace, imports, symbols),
            attributes: resolve_attribute_groups(
                &constant.attributes,
                namespace,
                imports,
                symbols,
            ),
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
            attributes: resolve_attribute_groups(
                &property.attributes,
                namespace,
                imports,
                symbols,
            ),
            ..property.clone()
        })
        .collect()
}

/// Resolves a trait use statement by rewriting its trait names and adaptations
/// (aliases and instead-of rules) through `resolved_class_name` and `php_symbol_key`.
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
                    trait_name: trait_name
                        .as_ref()
                        .map(|name| {
                            resolved_name(resolved_class_name(
                                name,
                                current_namespace,
                                imports,
                                symbols,
                            ))
                        }),
                    method: php_symbol_key(method),
                    alias: alias.as_ref().map(|alias| php_symbol_key(alias)),
                    visibility: visibility.clone(),
                }),
                TraitAdaptation::InsteadOf {
                    trait_name,
                    method,
                    instead_of,
                } => Ok(TraitAdaptation::InsteadOf {
                    trait_name: trait_name
                        .as_ref()
                        .map(|name| {
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
