use crate::errors::CompileError;
use crate::parser::ast::{CatchClause, Stmt, StmtKind, TypeExpr};

use super::declarations::resolve_decl_stmt;
use super::expressions::resolve_expr;
use super::names::{
    register_imports, resolve_special_or_class_name, resolve_type_expr,
};
use super::{namespace_name, resolved_name, Imports, Symbols};

pub(super) fn resolve_stmt_list(
    stmts: &[Stmt],
    current_namespace: Option<&str>,
    incoming_imports: &Imports,
    symbols: &Symbols,
) -> Result<Vec<Stmt>, CompileError> {
    let mut resolved = Vec::new();
    let mut namespace = current_namespace.map(str::to_string);
    let mut imports = incoming_imports.clone();

    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => {
                namespace = Some(namespace_name(name));
                imports = Imports::default();
            }
            StmtKind::NamespaceBlock { name, body } => {
                let block_namespace = Some(namespace_name(name));
                let body =
                    resolve_stmt_list(body, block_namespace.as_deref(), &Imports::default(), symbols)?;
                resolved.extend(body);
            }
            StmtKind::UseDecl { imports: use_items } => {
                register_imports(&mut imports, use_items, stmt.span)?;
            }
            _ => {
                if let Some(resolved_stmt) =
                    resolve_decl_stmt(stmt, namespace.as_deref(), &imports, symbols)?
                {
                    resolved.push(resolved_stmt);
                    continue;
                }
                match &stmt.kind {
                    StmtKind::If {
                        condition,
                        then_body,
                        elseif_clauses,
                        else_body,
                    } => {
                        resolved.push(Stmt::new(
                            StmtKind::If {
                                condition: resolve_expr(
                                    condition,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                ),
                                then_body: resolve_stmt_list(
                                    then_body,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                )?,
                                elseif_clauses: elseif_clauses
                                    .iter()
                                    .map(|(cond, body)| {
                                        Ok((
                                            resolve_expr(
                                                cond,
                                                namespace.as_deref(),
                                                &imports,
                                                symbols,
                                            ),
                                            resolve_stmt_list(
                                                body,
                                                namespace.as_deref(),
                                                &imports,
                                                symbols,
                                            )?,
                                        ))
                                    })
                                    .collect::<Result<Vec<_>, CompileError>>()?,
                                else_body: else_body
                                    .as_ref()
                                    .map(|body| {
                                        resolve_stmt_list(
                                            body,
                                            namespace.as_deref(),
                                            &imports,
                                            symbols,
                                        )
                                    })
                                    .transpose()?,
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::While { condition, body } => {
                        resolved.push(Stmt::new(
                            StmtKind::While {
                                condition: resolve_expr(
                                    condition,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                ),
                                body: resolve_stmt_list(
                                    body,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                )?,
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::DoWhile { body, condition } => {
                        resolved.push(Stmt::new(
                            StmtKind::DoWhile {
                                body: resolve_stmt_list(
                                    body,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                )?,
                                condition: resolve_expr(
                                    condition,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                ),
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::For {
                        init,
                        condition,
                        update,
                        body,
                    } => {
                        resolved.push(Stmt::new(
                            StmtKind::For {
                                init: init
                                    .as_ref()
                                    .map(|stmt| {
                                        resolve_one_stmt(
                                            stmt,
                                            namespace.as_deref(),
                                            &imports,
                                            symbols,
                                        )
                                    })
                                    .transpose()?
                                    .map(Box::new),
                                condition: condition.as_ref().map(|expr| {
                                    resolve_expr(expr, namespace.as_deref(), &imports, symbols)
                                }),
                                update: update
                                    .as_ref()
                                    .map(|stmt| {
                                        resolve_one_stmt(
                                            stmt,
                                            namespace.as_deref(),
                                            &imports,
                                            symbols,
                                        )
                                    })
                                    .transpose()?
                                    .map(Box::new),
                                body: resolve_stmt_list(
                                    body,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                )?,
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::Foreach {
                        array,
                        key_var,
                        value_var,
                        body,
                    } => {
                        resolved.push(Stmt::new(
                            StmtKind::Foreach {
                                array: resolve_expr(
                                    array,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                ),
                                key_var: key_var.clone(),
                                value_var: value_var.clone(),
                                body: resolve_stmt_list(
                                    body,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                )?,
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::Switch {
                        subject,
                        cases,
                        default,
                    } => {
                        resolved.push(Stmt::new(
                            StmtKind::Switch {
                                subject: resolve_expr(
                                    subject,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                ),
                                cases: cases
                                    .iter()
                                    .map(|(values, body)| {
                                        Ok((
                                            values
                                                .iter()
                                                .map(|value| {
                                                    resolve_expr(
                                                        value,
                                                        namespace.as_deref(),
                                                        &imports,
                                                        symbols,
                                                    )
                                                })
                                                .collect(),
                                            resolve_stmt_list(
                                                body,
                                                namespace.as_deref(),
                                                &imports,
                                                symbols,
                                            )?,
                                        ))
                                    })
                                    .collect::<Result<Vec<_>, CompileError>>()?,
                                default: default
                                    .as_ref()
                                    .map(|body| {
                                        resolve_stmt_list(
                                            body,
                                            namespace.as_deref(),
                                            &imports,
                                            symbols,
                                        )
                                    })
                                    .transpose()?,
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::Try {
                        try_body,
                        catches,
                        finally_body,
                    } => {
                        resolved.push(Stmt::new(
                            StmtKind::Try {
                                try_body: resolve_stmt_list(
                                    try_body,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                )?,
                                catches: catches
                                    .iter()
                                    .map(|catch_clause| {
                                        resolve_catch_clause(
                                            catch_clause,
                                            namespace.as_deref(),
                                            &imports,
                                            symbols,
                                        )
                                    })
                                    .collect::<Result<Vec<_>, CompileError>>()?,
                                finally_body: finally_body
                                    .as_ref()
                                    .map(|body| {
                                        resolve_stmt_list(
                                            body,
                                            namespace.as_deref(),
                                            &imports,
                                            symbols,
                                        )
                                    })
                                    .transpose()?,
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::Assign { name, value } => {
                        resolved.push(Stmt::new(
                            StmtKind::Assign {
                                name: name.clone(),
                                value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::TypedAssign {
                        type_expr,
                        name,
                        value,
                    } => {
                        resolved.push(Stmt::new(
                            StmtKind::TypedAssign {
                                type_expr: resolve_type_expr(
                                    type_expr,
                                    namespace.as_deref(),
                                    &imports,
                                ),
                                name: name.clone(),
                                value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::Echo(expr) => {
                        resolved.push(Stmt::new(
                            StmtKind::Echo(resolve_expr(
                                expr,
                                namespace.as_deref(),
                                &imports,
                                symbols,
                            )),
                            stmt.span,
                        ));
                    }
                    StmtKind::Throw(expr) => {
                        resolved.push(Stmt::new(
                            StmtKind::Throw(resolve_expr(
                                expr,
                                namespace.as_deref(),
                                &imports,
                                symbols,
                            )),
                            stmt.span,
                        ));
                    }
                    StmtKind::ExprStmt(expr) => {
                        resolved.push(Stmt::new(
                            StmtKind::ExprStmt(resolve_expr(
                                expr,
                                namespace.as_deref(),
                                &imports,
                                symbols,
                            )),
                            stmt.span,
                        ));
                    }
                    StmtKind::Return(expr) => {
                        resolved.push(Stmt::new(
                            StmtKind::Return(expr.as_ref().map(|expr| {
                                resolve_expr(expr, namespace.as_deref(), &imports, symbols)
                            })),
                            stmt.span,
                        ));
                    }
                    StmtKind::ListUnpack { vars, value } => {
                        resolved.push(Stmt::new(
                            StmtKind::ListUnpack {
                                vars: vars.clone(),
                                value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::ArrayAssign { array, index, value } => {
                        resolved.push(Stmt::new(
                            StmtKind::ArrayAssign {
                                array: array.clone(),
                                index: resolve_expr(index, namespace.as_deref(), &imports, symbols),
                                value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::ArrayPush { array, value } => {
                        resolved.push(Stmt::new(
                            StmtKind::ArrayPush {
                                array: array.clone(),
                                value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::PropertyAssign {
                        object,
                        property,
                        value,
                    } => {
                        resolved.push(Stmt::new(
                            StmtKind::PropertyAssign {
                                object: Box::new(resolve_expr(
                                    object,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                )),
                                property: property.clone(),
                                value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                            },
                            stmt.span,
                        ));
                    }
                    StmtKind::PropertyArrayPush {
                        object,
                        property,
                        value,
                    } => {
                        resolved.push(Stmt::new(
                            StmtKind::PropertyArrayPush {
                                object: Box::new(resolve_expr(
                                    object,
                                    namespace.as_deref(),
                                    &imports,
                                    symbols,
                                )),
                                property: property.clone(),
                                value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                            },
                            stmt.span,
                        ));
                    }
                    _ => resolved.push(stmt.clone()),
                }
            }
        }
    }

    Ok(resolved)
}

pub(super) fn resolve_one_stmt(
    stmt: &Stmt,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Result<Stmt, CompileError> {
    let mut stmts = resolve_stmt_list(std::slice::from_ref(stmt), current_namespace, imports, symbols)?;
    Ok(stmts.remove(0))
}

pub(super) fn resolve_catch_clause(
    catch_clause: &CatchClause,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Result<CatchClause, CompileError> {
    Ok(CatchClause {
        exception_types: catch_clause
            .exception_types
            .iter()
            .map(|name| {
                resolved_name(resolve_special_or_class_name(
                    name,
                    current_namespace,
                    imports,
                ))
            })
            .collect(),
        variable: catch_clause.variable.clone(),
        body: resolve_stmt_list(&catch_clause.body, current_namespace, imports, symbols)?,
    })
}

pub(super) fn resolve_params(
    params: &[(String, Option<TypeExpr>, Option<crate::parser::ast::Expr>, bool)],
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<(String, Option<TypeExpr>, Option<crate::parser::ast::Expr>, bool)> {
    params
        .iter()
        .map(|(name, type_ann, default, is_ref)| {
            (
                name.clone(),
                type_ann
                    .as_ref()
                    .map(|ty| resolve_type_expr(ty, current_namespace, imports)),
                default
                    .as_ref()
                    .map(|expr| resolve_expr(expr, current_namespace, imports, symbols)),
                *is_ref,
            )
        })
        .collect()
}
