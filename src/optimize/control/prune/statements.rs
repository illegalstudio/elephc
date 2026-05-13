//! Purpose:
//! Prunes constant control-flow statements cases.
//! Rewrites statements or expressions whose compile-time condition is known while preserving required effects.
//!
//! Called from:
//! - `crate::optimize::control::prune`
//!
//! Key details:
//! - Loop exits, empty bodies, and effectful conditions must be handled before removing structural statements.

use super::super::*;
use super::expr::{expr_has_side_effects, prune_expr};
use super::loop_exit::block_contains_loop_exit;

pub(crate) fn prune_block(body: Vec<Stmt>) -> Vec<Stmt> {
    let mut pruned = Vec::new();
    for stmt in body {
        let pruned_stmt = prune_stmt(stmt);
        let stops_here = pruned_stmt
            .last()
            .is_some_and(|stmt| !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough));
        pruned.extend(pruned_stmt);
        if stops_here {
            break;
        }
    }
    pruned
}

pub(crate) fn prune_stmt(stmt: Stmt) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Echo(expr) => vec![Stmt {
            kind: StmtKind::Echo(prune_expr(expr)),
            span,
            attributes: Vec::new(),
        }],
        StmtKind::Assign { name, value } => vec![Stmt {
            kind: StmtKind::Assign {
                name,
                value: prune_expr(value),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => prune_if_chain(condition, then_body, elseif_clauses, else_body),
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = prune_block(then_body);
            let else_body = normalize_optional_block(else_body.map(prune_block));
            if then_body.is_empty() && else_body.is_none() {
                Vec::new()
            } else {
                vec![Stmt {
                    kind: StmtKind::IfDef {
                        symbol,
                        then_body,
                        else_body,
                    },
                    span,
                    attributes: Vec::new(),
                }]
            }
        }
        StmtKind::While { condition, body } => {
            let condition = prune_expr(condition);
            match scalar_value(&condition) {
                Some(value) if !value.truthy() => Vec::new(),
                _ => vec![Stmt {
                    kind: StmtKind::While {
                        condition,
                        body: prune_block(body),
                    },
                    span,
                    attributes: Vec::new(),
                }],
            }
        }
        StmtKind::DoWhile { body, condition } => {
            let condition = prune_expr(condition);
            let body = prune_block(body);
            match scalar_value(&condition) {
                Some(value) if !value.truthy() && !block_contains_loop_exit(&body) => body,
                _ => vec![Stmt {
                    kind: StmtKind::DoWhile {
                        body,
                        condition,
                    },
                    span,
                    attributes: Vec::new(),
                }],
            }
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let init = prune_for_clause(init);
            let condition = condition.map(prune_expr);
            let update = prune_for_clause(update);
            match condition.as_ref().and_then(scalar_value) {
                Some(value) if !value.truthy() => init.map(|stmt| vec![*stmt]).unwrap_or_default(),
                _ => vec![Stmt {
                    kind: StmtKind::For {
                        init,
                        condition,
                        update,
                        body: prune_block(body),
                    },
                    span,
                    attributes: Vec::new(),
                }],
            }
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => vec![Stmt {
            kind: StmtKind::Foreach {
                array: prune_expr(array),
                key_var,
                value_var,
                body: prune_block(body),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => prune_switch_stmt(subject, cases, default, span),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let try_body = prune_block(try_body);
            let catches = normalize_catch_clauses(catches
                .into_iter()
                .map(|catch| crate::parser::ast::CatchClause {
                    exception_types: catch.exception_types,
                    variable: catch.variable,
                    body: prune_block(catch.body),
                })
                .collect());
            let finally_body = normalize_optional_block(finally_body.map(prune_block));

            if catches.is_empty() && finally_body.is_some() && try_body.len() == 1 {
                if let StmtKind::Try {
                    try_body: inner_try_body,
                    catches: inner_catches,
                    finally_body: None,
                } = &try_body[0].kind
                {
                    return vec![Stmt {
                        kind: StmtKind::Try {
                            try_body: inner_try_body.clone(),
                            catches: inner_catches.clone(),
                            finally_body,
                        },
                        span,
                        attributes: Vec::new(),
                    }];
                }
            }

            let (mut hoisted_prefix, try_body) = split_hoistable_try_prefix(try_body);

            let mut remaining = if try_body.is_empty() {
                finally_body.unwrap_or_default()
            } else if !block_may_throw(&try_body) {
                if let Some(finally_body) = finally_body {
                    if matches!(block_terminal_effect(&try_body), TerminalEffect::FallsThrough) {
                        let mut stmts = try_body;
                        stmts.extend(finally_body);
                        stmts
                    } else {
                        vec![Stmt {
                            kind: StmtKind::Try {
                                try_body,
                                catches,
                                finally_body: Some(finally_body),
                            },
                            span,
                            attributes: Vec::new(),
                        }]
                    }
                } else {
                    try_body
                }
            } else if catches.is_empty() && finally_body.is_none() {
                try_body
            } else {
                vec![Stmt {
                    kind: StmtKind::Try {
                        try_body,
                        catches,
                        finally_body,
                    },
                    span,
                    attributes: Vec::new(),
                }]
            };
            hoisted_prefix.append(&mut remaining);
            hoisted_prefix
        }
        StmtKind::NamespaceBlock { name, body } => vec![Stmt {
            kind: StmtKind::NamespaceBlock {
                name,
                body: prune_block(body),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => vec![Stmt {
            kind: StmtKind::FunctionDecl {
                name,
                params,
                variadic,
                return_type,
                body: prune_block(body),
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::Return(expr) => vec![Stmt {
            kind: StmtKind::Return(expr.map(prune_expr)),
            span,
            attributes: Vec::new(),
        }],
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
            let parent_name = extends.as_ref().map(|parent| parent.as_str().to_string());
            let methods = methods
                .into_iter()
                .map(|method| prune_method(method, &name, parent_name.as_deref()))
                .collect();
            vec![Stmt {
                kind: StmtKind::ClassDecl {
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
                },
                span,
                attributes: Vec::new(),
            }]
        }
        StmtKind::ExprStmt(expr) => {
            let expr = prune_expr(expr);
            if expr_has_side_effects(&expr) {
                vec![Stmt {
                    kind: StmtKind::ExprStmt(expr),
                    span,
                    attributes: Vec::new(),
                }]
            } else {
                Vec::new()
            }
        }
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => vec![Stmt {
            kind: StmtKind::EnumDecl {
                name,
                backing_type,
                cases,
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::PackedClassDecl { name, fields } => vec![Stmt {
            kind: StmtKind::PackedClassDecl { name, fields },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        constants,
        } => vec![Stmt {
            kind: StmtKind::InterfaceDecl {
                name,
                extends,
                methods: methods
                    .into_iter()
                    .map(prune_method_without_context)
                    .collect(),
            constants,
            },
            span,
            attributes: Vec::new(),
        }],
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        constants,
        } => vec![Stmt {
            kind: StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods: methods
                    .into_iter()
                    .map(prune_method_without_context)
                    .collect(),
            constants,
            },
            span,
            attributes: Vec::new(),
        }],
        kind => vec![Stmt { kind, span, attributes: Vec::new() }],
    }
}

pub(crate) fn prune_method(
    method: ClassMethod,
    class_name: &str,
    parent_name: Option<&str>,
) -> ClassMethod {
    let context = ClassEffectContext {
        class_name: class_name.to_string(),
        parent_name: parent_name.map(str::to_string),
    };
    ClassMethod {
        body: with_class_effect_context(Some(context), || prune_block(method.body)),
        ..method
    }
}

pub(crate) fn prune_method_without_context(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        body: with_class_effect_context(None, || prune_block(method.body)),
        ..method
    }
}

pub(crate) fn prune_for_clause(stmt: Option<Box<Stmt>>) -> Option<Box<Stmt>> {
    let stmt = stmt?;
    prune_stmt(*stmt).into_iter().next().map(Box::new)
}
