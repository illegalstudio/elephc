use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::{canonical_name_for_decl, Name, NameKind};
use crate::parser::ast::{
    CallableTarget, CatchClause, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind,
    TraitAdaptation, TraitUse, UseItem, UseKind,
};

#[derive(Default, Clone)]
struct Imports {
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    constants: HashMap<String, String>,
}

#[derive(Default)]
struct Symbols {
    functions: HashSet<String>,
    classes: HashSet<String>,
    interfaces: HashSet<String>,
    traits: HashSet<String>,
    constants: HashSet<String>,
    extern_functions: HashSet<String>,
    extern_classes: HashSet<String>,
}

impl Symbols {
    fn has_function(&self, name: &str) -> bool {
        self.functions.contains(name) || self.extern_functions.contains(name) || is_builtin_function(name)
    }

    fn has_constant(&self, name: &str) -> bool {
        self.constants.contains(name)
    }
}

pub fn resolve(program: Program) -> Result<Program, CompileError> {
    let mut symbols = Symbols::default();
    collect_symbols(&program, None, &mut symbols);
    resolve_stmt_list(&program, None, &Imports::default(), &symbols)
}

fn collect_symbols(stmts: &[Stmt], current_namespace: Option<&str>, symbols: &mut Symbols) {
    let mut namespace = current_namespace.map(str::to_string);
    for stmt in stmts {
        match &stmt.kind {
            StmtKind::NamespaceDecl { name } => {
                namespace = Some(namespace_name(name));
            }
            StmtKind::NamespaceBlock { name, body } => {
                let block_namespace = Some(namespace_name(name));
                collect_symbols(body, block_namespace.as_deref(), symbols);
            }
            StmtKind::FunctionDecl { name, .. } => {
                symbols
                    .functions
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::ClassDecl { name, .. } => {
                symbols
                    .classes
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::EnumDecl { name, .. } => {
                symbols
                    .classes
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::InterfaceDecl { name, .. } => {
                symbols
                    .interfaces
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::TraitDecl { name, .. } => {
                symbols
                    .traits
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::ExternFunctionDecl { name, .. } => {
                symbols
                    .extern_functions
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::ExternClassDecl { name, .. } => {
                symbols
                    .extern_classes
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            StmtKind::ConstDecl { name, .. } => {
                symbols
                    .constants
                    .insert(canonical_name_for_decl(namespace.as_deref(), name));
            }
            _ => {}
        }
    }
}

fn resolve_stmt_list(
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
                let body = resolve_stmt_list(body, block_namespace.as_deref(), &Imports::default(), symbols)?;
                resolved.extend(body);
            }
            StmtKind::UseDecl { imports: use_items } => {
                register_imports(&mut imports, use_items, stmt.span)?;
            }
            StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                return_type,
                body,
            } => {
                let body = resolve_stmt_list(body, namespace.as_deref(), &imports, symbols)?;
                resolved.push(Stmt::new(
                    StmtKind::FunctionDecl {
                        name: canonical_name_for_decl(namespace.as_deref(), name),
                        params: params.clone(),
                        variadic: variadic.clone(),
                        return_type: return_type.clone(),
                        body,
                    },
                    stmt.span,
                ));
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
                        let body =
                            resolve_stmt_list(&method.body, namespace.as_deref(), &imports, symbols)?;
                        Ok(crate::parser::ast::ClassMethod {
                            body,
                            ..method.clone()
                        })
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let trait_uses = trait_uses
                    .iter()
                    .map(|trait_use| resolve_trait_use(trait_use, namespace.as_deref(), &imports))
                    .collect::<Result<Vec<_>, CompileError>>()?;
                resolved.push(Stmt::new(
                    StmtKind::ClassDecl {
                        name: canonical_name_for_decl(namespace.as_deref(), name),
                        extends: extends
                            .as_ref()
                            .map(|name| resolved_name(resolved_class_name(name, namespace.as_deref(), &imports))),
                        implements: implements
                            .iter()
                            .map(|name| resolved_name(resolved_class_name(name, namespace.as_deref(), &imports)))
                            .collect(),
                        is_abstract: *is_abstract,
                        is_readonly_class: *is_readonly_class,
                        trait_uses,
                        properties: properties.clone(),
                        methods: resolved_methods,
                    },
                    stmt.span,
                ));
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
                            .map(|expr| resolve_expr(expr, namespace.as_deref(), &imports, symbols)),
                        span: case.span,
                    })
                    .collect();
                resolved.push(Stmt::new(
                    StmtKind::EnumDecl {
                        name: canonical_name_for_decl(namespace.as_deref(), name),
                        backing_type: backing_type.clone(),
                        cases: resolved_cases,
                    },
                    stmt.span,
                ));
            }
            StmtKind::InterfaceDecl { name, extends, methods } => {
                let resolved_methods = methods
                    .iter()
                    .map(|method| {
                        let body =
                            resolve_stmt_list(&method.body, namespace.as_deref(), &imports, symbols)?;
                        Ok(crate::parser::ast::ClassMethod {
                            body,
                            ..method.clone()
                        })
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                resolved.push(Stmt::new(
                    StmtKind::InterfaceDecl {
                        name: canonical_name_for_decl(namespace.as_deref(), name),
                        extends: extends
                            .iter()
                            .map(|name| resolved_name(resolved_class_name(name, namespace.as_deref(), &imports)))
                            .collect(),
                        methods: resolved_methods,
                    },
                    stmt.span,
                ));
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
                        let body =
                            resolve_stmt_list(&method.body, namespace.as_deref(), &imports, symbols)?;
                        Ok(crate::parser::ast::ClassMethod {
                            body,
                            ..method.clone()
                        })
                    })
                    .collect::<Result<Vec<_>, CompileError>>()?;
                let trait_uses = trait_uses
                    .iter()
                    .map(|trait_use| resolve_trait_use(trait_use, namespace.as_deref(), &imports))
                    .collect::<Result<Vec<_>, CompileError>>()?;
                resolved.push(Stmt::new(
                    StmtKind::TraitDecl {
                        name: canonical_name_for_decl(namespace.as_deref(), name),
                        trait_uses,
                        properties: properties.clone(),
                        methods: resolved_methods,
                    },
                    stmt.span,
                ));
            }
            StmtKind::ExternFunctionDecl {
                name,
                params,
                return_type,
                library,
            } => {
                resolved.push(Stmt::new(
                    StmtKind::ExternFunctionDecl {
                        name: canonical_name_for_decl(namespace.as_deref(), name),
                        params: params.clone(),
                        return_type: return_type.clone(),
                        library: library.clone(),
                    },
                    stmt.span,
                ));
            }
            StmtKind::ExternClassDecl { name, fields } => {
                resolved.push(Stmt::new(
                    StmtKind::ExternClassDecl {
                        name: canonical_name_for_decl(namespace.as_deref(), name),
                        fields: fields.clone(),
                    },
                    stmt.span,
                ));
            }
            StmtKind::ConstDecl { name, value } => {
                resolved.push(Stmt::new(
                    StmtKind::ConstDecl {
                        name: canonical_name_for_decl(namespace.as_deref(), name),
                        value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                    },
                    stmt.span,
                ));
            }
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses,
                else_body,
            } => {
                resolved.push(Stmt::new(
                    StmtKind::If {
                        condition: resolve_expr(condition, namespace.as_deref(), &imports, symbols),
                        then_body: resolve_stmt_list(then_body, namespace.as_deref(), &imports, symbols)?,
                        elseif_clauses: elseif_clauses
                            .iter()
                            .map(|(cond, body)| {
                                Ok((
                                    resolve_expr(cond, namespace.as_deref(), &imports, symbols),
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
                                resolve_stmt_list(body, namespace.as_deref(), &imports, symbols)
                            })
                            .transpose()?,
                    },
                    stmt.span,
                ));
            }
            StmtKind::While { condition, body } => {
                resolved.push(Stmt::new(
                    StmtKind::While {
                        condition: resolve_expr(condition, namespace.as_deref(), &imports, symbols),
                        body: resolve_stmt_list(body, namespace.as_deref(), &imports, symbols)?,
                    },
                    stmt.span,
                ));
            }
            StmtKind::DoWhile { body, condition } => {
                resolved.push(Stmt::new(
                    StmtKind::DoWhile {
                        body: resolve_stmt_list(body, namespace.as_deref(), &imports, symbols)?,
                        condition: resolve_expr(condition, namespace.as_deref(), &imports, symbols),
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
                            .map(|stmt| resolve_one_stmt(stmt, namespace.as_deref(), &imports, symbols))
                            .transpose()?
                            .map(Box::new),
                        condition: condition
                            .as_ref()
                            .map(|expr| resolve_expr(expr, namespace.as_deref(), &imports, symbols)),
                        update: update
                            .as_ref()
                            .map(|stmt| resolve_one_stmt(stmt, namespace.as_deref(), &imports, symbols))
                            .transpose()?
                            .map(Box::new),
                        body: resolve_stmt_list(body, namespace.as_deref(), &imports, symbols)?,
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
                        array: resolve_expr(array, namespace.as_deref(), &imports, symbols),
                        key_var: key_var.clone(),
                        value_var: value_var.clone(),
                        body: resolve_stmt_list(body, namespace.as_deref(), &imports, symbols)?,
                    },
                    stmt.span,
                ));
            }
            StmtKind::Switch { subject, cases, default } => {
                resolved.push(Stmt::new(
                    StmtKind::Switch {
                        subject: resolve_expr(subject, namespace.as_deref(), &imports, symbols),
                        cases: cases
                            .iter()
                            .map(|(values, body)| {
                                Ok((
                                    values
                                        .iter()
                                        .map(|value| {
                                            resolve_expr(value, namespace.as_deref(), &imports, symbols)
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
                            .map(|body| resolve_stmt_list(body, namespace.as_deref(), &imports, symbols))
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
                        try_body: resolve_stmt_list(try_body, namespace.as_deref(), &imports, symbols)?,
                        catches: catches
                            .iter()
                            .map(|catch_clause| resolve_catch_clause(catch_clause, namespace.as_deref(), &imports, symbols))
                            .collect::<Result<Vec<_>, CompileError>>()?,
                        finally_body: finally_body
                            .as_ref()
                            .map(|body| resolve_stmt_list(body, namespace.as_deref(), &imports, symbols))
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
            StmtKind::Echo(expr) => {
                resolved.push(Stmt::new(
                    StmtKind::Echo(resolve_expr(expr, namespace.as_deref(), &imports, symbols)),
                    stmt.span,
                ));
            }
            StmtKind::Throw(expr) => {
                resolved.push(Stmt::new(
                    StmtKind::Throw(resolve_expr(expr, namespace.as_deref(), &imports, symbols)),
                    stmt.span,
                ));
            }
            StmtKind::ExprStmt(expr) => {
                resolved.push(Stmt::new(
                    StmtKind::ExprStmt(resolve_expr(expr, namespace.as_deref(), &imports, symbols)),
                    stmt.span,
                ));
            }
            StmtKind::Return(expr) => {
                resolved.push(Stmt::new(
                    StmtKind::Return(
                        expr.as_ref()
                            .map(|expr| resolve_expr(expr, namespace.as_deref(), &imports, symbols)),
                    ),
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
                        object: Box::new(resolve_expr(object, namespace.as_deref(), &imports, symbols)),
                        property: property.clone(),
                        value: resolve_expr(value, namespace.as_deref(), &imports, symbols),
                    },
                    stmt.span,
                ));
            }
            _ => resolved.push(stmt.clone()),
        }
    }

    Ok(resolved)
}

fn resolve_one_stmt(
    stmt: &Stmt,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Result<Stmt, CompileError> {
    let mut stmts = resolve_stmt_list(std::slice::from_ref(stmt), current_namespace, imports, symbols)?;
    Ok(stmts.remove(0))
}

fn resolve_trait_use(
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

fn resolve_catch_clause(
    catch_clause: &CatchClause,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Result<CatchClause, CompileError> {
    Ok(CatchClause {
        exception_types: catch_clause
            .exception_types
            .iter()
            .map(|name| resolved_name(resolve_special_or_class_name(name, current_namespace, imports)))
            .collect(),
        variable: catch_clause.variable.clone(),
        body: resolve_stmt_list(&catch_clause.body, current_namespace, imports, symbols)?,
    })
}

fn resolve_expr(expr: &Expr, current_namespace: Option<&str>, imports: &Imports, symbols: &Symbols) -> Expr {
    let kind = match &expr.kind {
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(resolve_expr(left, current_namespace, imports, symbols)),
            op: op.clone(),
            right: Box::new(resolve_expr(right, current_namespace, imports, symbols)),
        },
        ExprKind::Throw(inner) => {
            ExprKind::Throw(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(resolve_expr(value, current_namespace, imports, symbols)),
            default: Box::new(resolve_expr(default, current_namespace, imports, symbols)),
        },
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name: resolved_name(resolve_function_name(name, current_namespace, imports, symbols)),
            args: rewrite_callback_literal_args(
                name.as_str(),
                args,
                current_namespace,
                imports,
                symbols,
            )
            .into_iter()
            .map(|arg| resolve_expr(&arg, current_namespace, imports, symbols))
            .collect(),
        },
        ExprKind::ArrayLiteral(values) => ExprKind::ArrayLiteral(
            values
                .iter()
                .map(|value| resolve_expr(value, current_namespace, imports, symbols))
                .collect(),
        ),
        ExprKind::ArrayLiteralAssoc(values) => ExprKind::ArrayLiteralAssoc(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        resolve_expr(key, current_namespace, imports, symbols),
                        resolve_expr(value, current_namespace, imports, symbols),
                    )
                })
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(resolve_expr(subject, current_namespace, imports, symbols)),
            arms: arms
                .iter()
                .map(|(conds, value)| {
                    (
                        conds
                            .iter()
                            .map(|cond| resolve_expr(cond, current_namespace, imports, symbols))
                            .collect(),
                        resolve_expr(value, current_namespace, imports, symbols),
                    )
                })
                .collect(),
            default: default
                .as_ref()
                .map(|expr| Box::new(resolve_expr(expr, current_namespace, imports, symbols))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(resolve_expr(array, current_namespace, imports, symbols)),
            index: Box::new(resolve_expr(index, current_namespace, imports, symbols)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(resolve_expr(condition, current_namespace, imports, symbols)),
            then_expr: Box::new(resolve_expr(then_expr, current_namespace, imports, symbols)),
            else_expr: Box::new(resolve_expr(else_expr, current_namespace, imports, symbols)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target: target.clone(),
            expr: Box::new(resolve_expr(expr, current_namespace, imports, symbols)),
        },
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => ExprKind::Closure {
            params: params.clone(),
            variadic: variadic.clone(),
            body: resolve_stmt_list(body, current_namespace, imports, symbols)
                .expect("name resolver bug: closure body resolution failed"),
            is_arrow: *is_arrow,
            captures: captures.clone(),
        },
        ExprKind::Spread(inner) => {
            ExprKind::Spread(Box::new(resolve_expr(inner, current_namespace, imports, symbols)))
        }
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var: var.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(resolve_expr(callee, current_namespace, imports, symbols)),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::ConstRef(name) => {
            ExprKind::ConstRef(resolved_name(resolve_constant_name(name, current_namespace, imports, symbols)))
        }
        ExprKind::EnumCase { enum_name, case_name } => ExprKind::EnumCase {
            enum_name: resolved_name(resolve_special_or_class_name(
                enum_name,
                current_namespace,
                imports,
            )),
            case_name: case_name.clone(),
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name: resolved_name(resolve_special_or_class_name(
                class_name,
                current_namespace,
                imports,
            )),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            property: property.clone(),
        },
        ExprKind::MethodCall { object, method, args } => ExprKind::MethodCall {
            object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
            method: method.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver: match receiver {
                StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                    resolve_special_or_class_name(name, current_namespace, imports),
                )),
                _ => receiver.clone(),
            },
            method: method.clone(),
            args: args
                .iter()
                .map(|arg| resolve_expr(arg, current_namespace, imports, symbols))
                .collect(),
        },
        ExprKind::FirstClassCallable(target) => ExprKind::FirstClassCallable(match target {
            CallableTarget::Function(name) => CallableTarget::Function(resolved_name(
                resolve_function_name(name, current_namespace, imports, symbols),
            )),
            CallableTarget::StaticMethod { receiver, method } => CallableTarget::StaticMethod {
                receiver: match receiver {
                    StaticReceiver::Named(name) => StaticReceiver::Named(resolved_name(
                        resolve_special_or_class_name(name, current_namespace, imports),
                    )),
                    _ => receiver.clone(),
                },
                method: method.clone(),
            },
            CallableTarget::Method { object, method } => CallableTarget::Method {
                object: Box::new(resolve_expr(object, current_namespace, imports, symbols)),
                method: method.clone(),
            },
        }),
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type: target_type.clone(),
            expr: Box::new(resolve_expr(expr, current_namespace, imports, symbols)),
        },
        _ => expr.kind.clone(),
    };
    Expr::new(kind, expr.span)
}

fn register_imports(imports: &mut Imports, use_items: &[UseItem], span: crate::span::Span) -> Result<(), CompileError> {
    for item in use_items {
        let target = item.name.as_canonical();
        let alias_map = match item.kind {
            UseKind::Class => &mut imports.classes,
            UseKind::Function => &mut imports.functions,
            UseKind::Const => &mut imports.constants,
        };
        if alias_map.insert(item.alias.clone(), target).is_some() {
            return Err(CompileError::new(
                span,
                &format!("Duplicate import alias: {}", item.alias),
            ));
        }
    }
    Ok(())
}

fn resolve_special_or_class_name(name: &Name, current_namespace: Option<&str>, imports: &Imports) -> String {
    match name.as_canonical().as_str() {
        "self" | "parent" | "static" => name.as_canonical(),
        _ => resolved_class_name(name, current_namespace, imports),
    }
}

fn resolved_class_name(name: &Name, current_namespace: Option<&str>, imports: &Imports) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(alias) = name.last_segment().and_then(|segment| imports.classes.get(segment)) {
            return alias.clone();
        }
    } else if let Some(first) = name.parts.first() {
        if let Some(alias) = imports.classes.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return alias.clone();
            }
            return format!("{}\\{}", alias, suffix.join("\\"));
        }
    }
    if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, name.as_canonical());
        }
    }
    name.as_canonical()
}

fn resolve_function_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(alias) = name.last_segment().and_then(|segment| imports.functions.get(segment)) {
            return alias.clone();
        }
        let local = if let Some(namespace) = current_namespace {
            if !namespace.is_empty() {
                format!("{}\\{}", namespace, name.as_canonical())
            } else {
                name.as_canonical()
            }
        } else {
            name.as_canonical()
        };
        if current_namespace.is_some() && symbols.has_function(&local) {
            return local;
        }
        if symbols.has_function(&name.as_canonical()) {
            return name.as_canonical();
        }
        return local;
    }
    if let Some(first) = name.parts.first() {
        if let Some(alias) = imports.functions.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return alias.clone();
            }
            return format!("{}\\{}", alias, suffix.join("\\"));
        }
    }
    if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, name.as_canonical());
        }
    }
    name.as_canonical()
}

fn resolve_constant_name(
    name: &Name,
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> String {
    if name.is_fully_qualified() {
        return name.as_canonical();
    }
    if name.is_unqualified() {
        if let Some(alias) = name.last_segment().and_then(|segment| imports.constants.get(segment)) {
            return alias.clone();
        }
        let local = if let Some(namespace) = current_namespace {
            if !namespace.is_empty() {
                format!("{}\\{}", namespace, name.as_canonical())
            } else {
                name.as_canonical()
            }
        } else {
            name.as_canonical()
        };
        if current_namespace.is_some() && symbols.has_constant(&local) {
            return local;
        }
        if symbols.has_constant(&name.as_canonical()) {
            return name.as_canonical();
        }
        return local;
    }
    if let Some(first) = name.parts.first() {
        if let Some(alias) = imports.constants.get(first) {
            let suffix = &name.parts[1..];
            if suffix.is_empty() {
                return alias.clone();
            }
            return format!("{}\\{}", alias, suffix.join("\\"));
        }
    }
    if let Some(namespace) = current_namespace {
        if !namespace.is_empty() {
            return format!("{}\\{}", namespace, name.as_canonical());
        }
    }
    name.as_canonical()
}

fn rewrite_callback_literal_args(
    function_name: &str,
    args: &[Expr],
    current_namespace: Option<&str>,
    imports: &Imports,
    symbols: &Symbols,
) -> Vec<Expr> {
    let callback_positions: &[usize] = match function_name {
        "function_exists" | "call_user_func" | "call_user_func_array" => &[0],
        "array_map" | "array_filter" | "array_reduce" | "array_walk" => &[0],
        "usort" | "uksort" | "uasort" => &[1],
        _ => &[],
    };

    args.iter()
        .enumerate()
        .map(|(idx, arg)| {
            if callback_positions.contains(&idx) {
                if let ExprKind::StringLiteral(raw_name) = &arg.kind {
                    let resolved = resolve_function_name(
                        &parse_callback_name(raw_name),
                        current_namespace,
                        imports,
                        symbols,
                    );
                    return Expr::new(ExprKind::StringLiteral(resolved), arg.span);
                }
            }
            arg.clone()
        })
        .collect()
}

fn parse_callback_name(raw_name: &str) -> Name {
    if let Some(stripped) = raw_name.strip_prefix('\\') {
        return Name::from_parts(
            NameKind::FullyQualified,
            stripped.split('\\').map(str::to_string).collect(),
        );
    }
    if raw_name.contains('\\') {
        return Name::from_parts(
            NameKind::Qualified,
            raw_name.split('\\').map(str::to_string).collect(),
        );
    }
    Name::unqualified(raw_name)
}

fn resolved_name(name: String) -> Name {
    Name::from_parts(NameKind::FullyQualified, name.split('\\').map(str::to_string).collect())
}

fn namespace_name(name: &Option<Name>) -> String {
    name.as_ref().map(Name::as_canonical).unwrap_or_default()
}

pub(crate) fn is_builtin_function(name: &str) -> bool {
    crate::types::checker::builtins::is_supported_builtin_function(name)
}
