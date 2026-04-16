use crate::errors::CompileError;
use crate::names::canonical_name_for_decl;
use crate::parser::ast::{Stmt, StmtKind, TraitAdaptation, TraitUse};

use super::expressions::resolve_expr;
use super::names::{resolve_type_expr, resolved_class_name};
use super::statements::{resolve_params, resolve_stmt_list};
use super::{resolved_name, Imports, Symbols};

pub(super) fn resolve_decl_stmt(
    stmt: &Stmt,
    namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Result<Option<Stmt>, CompileError> {
    match &stmt.kind {
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => {
            let body = resolve_stmt_list(body, namespace, imports, symbols)?;
            Ok(Some(Stmt::new(
                StmtKind::FunctionDecl {
                    name: canonical_name_for_decl(namespace, name),
                    params: resolve_params(params, namespace, imports, symbols),
                    variadic: variadic.clone(),
                    return_type: return_type
                        .as_ref()
                        .map(|ty| resolve_type_expr(ty, namespace, imports)),
                    body,
                },
                stmt.span,
            )))
        }
        StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } => {
            let resolved_methods = methods
                .iter()
                .map(|method| {
                    let body = resolve_stmt_list(&method.body, namespace, imports, symbols)?;
                    Ok(crate::parser::ast::ClassMethod {
                        params: resolve_params(&method.params, namespace, imports, symbols),
                        return_type: method
                            .return_type
                            .as_ref()
                            .map(|ty| resolve_type_expr(ty, namespace, imports)),
                        body,
                        ..method.clone()
                    })
                })
                .collect::<Result<Vec<_>, CompileError>>()?;
            let trait_uses = trait_uses
                .iter()
                .map(|trait_use| resolve_trait_use(trait_use, namespace, imports))
                .collect::<Result<Vec<_>, CompileError>>()?;
            Ok(Some(Stmt::new(
                StmtKind::ClassDecl {
                    name: canonical_name_for_decl(namespace, name),
                    extends: extends
                        .as_ref()
                        .map(|name| resolved_name(resolved_class_name(name, namespace, imports))),
                    implements: implements
                        .iter()
                        .map(|name| resolved_name(resolved_class_name(name, namespace, imports)))
                        .collect(),
                    is_abstract: *is_abstract,
                    is_readonly_class: *is_readonly_class,
                    trait_uses,
                    properties: properties.clone(),
                    methods: resolved_methods,
                },
                stmt.span,
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
                })
                .collect();
            Ok(Some(Stmt::new(
                StmtKind::EnumDecl {
                    name: canonical_name_for_decl(namespace, name),
                    backing_type: backing_type.clone(),
                    cases: resolved_cases,
                },
                stmt.span,
            )))
        }
        StmtKind::PackedClassDecl { name, fields } => {
            let resolved_fields = fields
                .iter()
                .map(|field| crate::parser::ast::PackedField {
                    name: field.name.clone(),
                    type_expr: resolve_type_expr(&field.type_expr, namespace, imports),
                    span: field.span,
                })
                .collect();
            Ok(Some(Stmt::new(
                StmtKind::PackedClassDecl {
                    name: canonical_name_for_decl(namespace, name),
                    fields: resolved_fields,
                },
                stmt.span,
            )))
        }
        StmtKind::InterfaceDecl { name, extends, methods } => {
            let resolved_methods = methods
                .iter()
                .map(|method| {
                    let body = resolve_stmt_list(&method.body, namespace, imports, symbols)?;
                    Ok(crate::parser::ast::ClassMethod {
                        params: resolve_params(&method.params, namespace, imports, symbols),
                        return_type: method
                            .return_type
                            .as_ref()
                            .map(|ty| resolve_type_expr(ty, namespace, imports)),
                        body,
                        ..method.clone()
                    })
                })
                .collect::<Result<Vec<_>, CompileError>>()?;
            Ok(Some(Stmt::new(
                StmtKind::InterfaceDecl {
                    name: canonical_name_for_decl(namespace, name),
                    extends: extends
                        .iter()
                        .map(|name| resolved_name(resolved_class_name(name, namespace, imports)))
                        .collect(),
                    methods: resolved_methods,
                },
                stmt.span,
            )))
        }
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => {
            let resolved_methods = methods
                .iter()
                .map(|method| {
                    let body = resolve_stmt_list(&method.body, namespace, imports, symbols)?;
                    Ok(crate::parser::ast::ClassMethod {
                        params: resolve_params(&method.params, namespace, imports, symbols),
                        return_type: method
                            .return_type
                            .as_ref()
                            .map(|ty| resolve_type_expr(ty, namespace, imports)),
                        body,
                        ..method.clone()
                    })
                })
                .collect::<Result<Vec<_>, CompileError>>()?;
            let trait_uses = trait_uses
                .iter()
                .map(|trait_use| resolve_trait_use(trait_use, namespace, imports))
                .collect::<Result<Vec<_>, CompileError>>()?;
            Ok(Some(Stmt::new(
                StmtKind::TraitDecl {
                    name: canonical_name_for_decl(namespace, name),
                    trait_uses,
                    properties: properties.clone(),
                    methods: resolved_methods,
                },
                stmt.span,
            )))
        }
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => Ok(Some(Stmt::new(
            StmtKind::ExternFunctionDecl {
                name: canonical_name_for_decl(namespace, name),
                params: params.clone(),
                return_type: return_type.clone(),
                library: library.clone(),
            },
            stmt.span,
        ))),
        StmtKind::ExternClassDecl { name, fields } => Ok(Some(Stmt::new(
            StmtKind::ExternClassDecl {
                name: canonical_name_for_decl(namespace, name),
                fields: fields.clone(),
            },
            stmt.span,
        ))),
        StmtKind::ConstDecl { name, value } => Ok(Some(Stmt::new(
            StmtKind::ConstDecl {
                name: canonical_name_for_decl(namespace, name),
                value: resolve_expr(value, namespace, imports, symbols),
            },
            stmt.span,
        ))),
        _ => Ok(None),
    }
}

pub(super) fn resolve_trait_use(
    trait_use: &TraitUse,
    current_namespace: Option<&str>,
    imports: &Imports,
) -> Result<TraitUse, CompileError> {
    Ok(TraitUse {
        trait_names: trait_use
            .trait_names
            .iter()
            .map(|name| resolved_name(resolved_class_name(name, current_namespace, imports)))
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
                        .map(|name| resolved_name(resolved_class_name(name, current_namespace, imports))),
                    method: method.clone(),
                    alias: alias.clone(),
                    visibility: visibility.clone(),
                }),
                TraitAdaptation::InsteadOf {
                    trait_name,
                    method,
                    instead_of,
                } => Ok(TraitAdaptation::InsteadOf {
                    trait_name: trait_name
                        .as_ref()
                        .map(|name| resolved_name(resolved_class_name(name, current_namespace, imports))),
                    method: method.clone(),
                    instead_of: instead_of
                        .iter()
                        .map(|name| resolved_name(resolved_class_name(name, current_namespace, imports)))
                        .collect(),
                }),
            })
            .collect::<Result<Vec<_>, CompileError>>()?,
        span: trait_use.span,
    })
}
