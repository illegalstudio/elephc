use super::*;

mod guards;
mod ifs;
mod methods;
mod state;
mod switches;
mod tries;
mod writes;

pub(crate) use methods::{dce_method, dce_method_without_context};
use guards::*;
use ifs::dce_if_stmt;
use state::{GuardLiteral, GuardState, TailSinkTarget};
use switches::direct_switch_entry_blocks;
use switches::{dce_switch_stmt, dce_switch_stmt_with_tail};
use tries::{dce_try_stmt, dce_try_stmt_with_tail};
use writes::*;

pub(crate) fn dce_block(body: Vec<Stmt>) -> Vec<Stmt> {
    dce_block_with_guards(body, GuardState::default())
}

fn dce_block_with_guards(body: Vec<Stmt>, mut guards: GuardState) -> Vec<Stmt> {
    let mut eliminated = Vec::new();
    let mut stmts = body.into_iter().peekable();
    while let Some(stmt) = stmts.next() {
        let has_tail = stmts.peek().is_some();
        let use_tail_sink = has_tail
            && matches!(
                stmt.kind,
                StmtKind::If { .. } | StmtKind::IfDef { .. } | StmtKind::Switch { .. } | StmtKind::Try { .. }
            );
        let dce_stmt = if use_tail_sink {
            let tail: Vec<Stmt> = stmts.clone().collect();
            dce_stmt_with_tail(stmt, tail, &guards)
        } else {
            dce_stmt_with_guards(stmt, &guards)
        };
        let stops_here = dce_stmt
            .last()
            .is_some_and(|stmt| !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough));
        for stmt in &dce_stmt {
            invalidate_guards_for_stmt(stmt, &mut guards);
        }
        eliminated.extend(dce_stmt);
        if stops_here {
            break;
        }
        if use_tail_sink {
            break;
        }
    }
    eliminated
}

fn append_tail_to_fallthrough_path(mut body: Vec<Stmt>, tail: Vec<Stmt>) -> Vec<Stmt> {
    if block_reaches_following_stmt(&body) {
        body.extend(tail);
    }
    body
}

fn block_matches_tail_target(body: &[Stmt], target: TailSinkTarget) -> bool {
    matches!(
        (block_terminal_effect(body), target),
        (TerminalEffect::FallsThrough, TailSinkTarget::FallsThrough)
            | (TerminalEffect::Breaks, TailSinkTarget::Breaks)
    )
}

fn sink_tail_into_terminal_path(
    mut body: Vec<Stmt>,
    tail: Vec<Stmt>,
    target: TailSinkTarget,
) -> Vec<Stmt> {
    let Some(stmt) = body.pop() else {
        return tail;
    };

    let rewritten = sink_tail_into_terminal_stmt(stmt, tail, target);
    body.extend(rewritten);
    body
}

fn sink_tail_into_terminal_stmt(stmt: Stmt, tail: Vec<Stmt>, target: TailSinkTarget) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let rewrite_branch = |body: Vec<Stmt>, target: TailSinkTarget, tail: &Vec<Stmt>| {
                if block_matches_tail_target(&body, target) {
                    sink_tail_into_terminal_path(body, tail.clone(), target)
                } else {
                    body
                }
            };
            let then_body = rewrite_branch(then_body, target, &tail);
            let elseif_clauses: Vec<_> = elseif_clauses
                .into_iter()
                .map(|(condition, body)| (condition, rewrite_branch(body, target, &tail)))
                .collect();
            let else_body = else_body.map(|body| rewrite_branch(body, target, &tail));
            vec![Stmt::new(
                StmtKind::If {
                    condition,
                    then_body,
                    elseif_clauses,
                    else_body,
                },
                span,
            )]
        }
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = if block_matches_tail_target(&then_body, target) {
                sink_tail_into_terminal_path(then_body, tail.clone(), target)
            } else {
                then_body
            };
            let else_body = else_body.map(|body| {
                if block_matches_tail_target(&body, target) {
                    sink_tail_into_terminal_path(body, tail.clone(), target)
                } else {
                    body
                }
            });
            vec![Stmt::new(
                StmtKind::IfDef {
                    symbol,
                    then_body,
                    else_body,
                },
                span,
            )]
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let try_body = if block_matches_tail_target(&try_body, target) {
                sink_tail_into_terminal_path(try_body, tail.clone(), target)
            } else {
                try_body
            };
            let catches = catches
                .into_iter()
                .map(|catch| crate::parser::ast::CatchClause {
                    body: if block_matches_tail_target(&catch.body, target) {
                        sink_tail_into_terminal_path(catch.body, tail.clone(), target)
                    } else {
                        catch.body
                    },
                    ..catch
                })
                .collect();
            vec![Stmt::new(
                StmtKind::Try {
                    try_body,
                    catches,
                    finally_body,
                },
                span,
            )]
        }
        _ if matches!(target, TailSinkTarget::FallsThrough)
            && matches!(stmt_terminal_effect(&stmt), TerminalEffect::FallsThrough) =>
        {
            let mut stmts = vec![stmt];
            stmts.extend(tail);
            stmts
        }
        StmtKind::Break if matches!(target, TailSinkTarget::Breaks) => {
            let mut stmts = tail;
            if block_reaches_following_stmt(&stmts) {
                stmts.push(Stmt::new(StmtKind::Break, span));
            }
            stmts
        }
        _ => vec![stmt],
    }
}

fn guard_literal_to_scalar(value: &GuardLiteral) -> ScalarValue {
    match value {
        GuardLiteral::Bool(value) => ScalarValue::Bool(*value),
        GuardLiteral::Null => ScalarValue::Null,
        GuardLiteral::Int(value) => ScalarValue::Int(*value),
        GuardLiteral::Float(bits) => ScalarValue::Float(f64::from_bits(*bits)),
        GuardLiteral::String(value) => ScalarValue::String(value.clone()),
    }
}

fn known_scalar_subject_value(subject: &Expr, guards: &GuardState) -> Option<ScalarValue> {
    scalar_value(subject).or_else(|| match &subject.kind {
        ExprKind::Variable(name) => known_exact_guard(guards, name).map(guard_literal_to_scalar),
        _ => None,
    })
}

fn known_subject_truthiness(subject: &Expr, guards: &GuardState) -> Option<bool> {
    if let Some(subject_value) = known_scalar_subject_value(subject, guards) {
        let guard_literal = match subject_value {
            ScalarValue::Bool(value) => GuardLiteral::Bool(value),
            ScalarValue::Null => GuardLiteral::Null,
            ScalarValue::Int(value) => GuardLiteral::Int(value),
            ScalarValue::Float(value) => GuardLiteral::Float(value.to_bits()),
            ScalarValue::String(value) => GuardLiteral::String(value),
        };
        return Some(guard_literal_truthy(&guard_literal));
    }

    let ExprKind::Variable(name) = &subject.kind else {
        return None;
    };

    if guards.bool_true_vars.iter().any(|known| known == name)
        || guards.truthy_vars.iter().any(|known| known == name)
    {
        return Some(true);
    }

    if guards.bool_false_vars.iter().any(|known| known == name)
        || guards.falsy_vars.iter().any(|known| known == name)
    {
        return Some(false);
    }

    None
}

fn dce_stmt_with_tail(stmt: Stmt, tail: Vec<Stmt>, guards: &GuardState) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let reachability = analyze_if_tail_paths(&then_body, &elseif_clauses, &else_body);
            let then_body = if reachability.then_sinks_tail {
                append_tail_to_fallthrough_path(then_body, tail.clone())
            } else {
                then_body
            };
            let elseif_clauses: Vec<_> = elseif_clauses
                .into_iter()
                .zip(reachability.elseif_sinks_tail)
                .map(|((condition, body), sinks_tail)| {
                    let body = if sinks_tail {
                        append_tail_to_fallthrough_path(body, tail.clone())
                    } else {
                        body
                    };
                    (condition, body)
                })
                .collect();
            let else_body = match else_body {
                Some(body) if reachability.else_sinks_tail => Some(append_tail_to_fallthrough_path(body, tail)),
                Some(body) => Some(body),
                None if reachability.implicit_else_sinks_tail => Some(tail),
                None => None,
            };
            dce_if_stmt(condition, then_body, elseif_clauses, else_body, span, guards)
        }
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let reachability = analyze_ifdef_tail_paths(&then_body, &else_body);
            let then_body = if reachability.then_sinks_tail {
                append_tail_to_fallthrough_path(then_body, tail.clone())
            } else {
                then_body
            };
            let else_body = match else_body {
                Some(body) if reachability.else_sinks_tail => Some(append_tail_to_fallthrough_path(body, tail)),
                Some(body) => Some(body),
                None if reachability.implicit_else_sinks_tail => Some(tail),
                None => None,
            };
            dce_stmt_with_guards(Stmt::new(
                StmtKind::IfDef {
                    symbol,
                    then_body,
                    else_body,
                },
                span,
            ), guards)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => dce_switch_stmt_with_tail(subject, cases, default, tail, span, guards),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => dce_try_stmt_with_tail(try_body, catches, finally_body, tail, span, guards),
        _ => {
            let mut stmts = dce_stmt_with_guards(stmt, guards);
            if stmts
                .last()
                .is_some_and(|stmt| matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough))
            {
                stmts.extend(dce_block_with_guards(tail, guards.clone()));
            }
            stmts
        }
    }
}

pub(crate) fn dce_stmt(stmt: Stmt) -> Vec<Stmt> {
    dce_stmt_with_guards(stmt, &GuardState::default())
}

fn dce_stmt_with_guards(stmt: Stmt, guards: &GuardState) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Echo(expr) => vec![Stmt {
            kind: StmtKind::Echo(prune_expr(expr)),
            span,
        }],
        StmtKind::Assign { name, value } => vec![Stmt {
            kind: StmtKind::Assign {
                name,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::TypedAssign {
            name,
            type_expr,
            value,
        } => vec![Stmt {
            kind: StmtKind::TypedAssign {
                name,
                type_expr,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyAssign {
                object: Box::new(prune_expr(*object)),
                property,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::StaticPropertyAssign {
                receiver,
                property,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::StaticPropertyArrayPush {
                receiver,
                property,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => vec![Stmt {
            kind: StmtKind::StaticPropertyArrayAssign {
                receiver,
                property,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyArrayAssign {
                object: Box::new(prune_expr(*object)),
                property,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => vec![Stmt {
            kind: StmtKind::PropertyArrayPush {
                object: Box::new(prune_expr(*object)),
                property,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ArrayAssign { array, index, value } => vec![Stmt {
            kind: StmtKind::ArrayAssign {
                array,
                index: prune_expr(index),
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ArrayPush { array, value } => vec![Stmt {
            kind: StmtKind::ArrayPush {
                array,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::ListUnpack { vars, value } => vec![Stmt {
            kind: StmtKind::ListUnpack {
                vars,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::StaticVar { name, init } => vec![Stmt {
            kind: StmtKind::StaticVar {
                name,
                init: prune_expr(init),
            },
            span,
        }],
        StmtKind::ConstDecl { name, value } => vec![Stmt {
            kind: StmtKind::ConstDecl {
                name,
                value: prune_expr(value),
            },
            span,
        }],
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => dce_if_stmt(condition, then_body, elseif_clauses, else_body, span, guards),
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = dce_block_with_guards(then_body, guards.clone());
            let else_body =
                normalize_optional_block(else_body.map(|body| dce_block_with_guards(body, guards.clone())));
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
                }]
            }
        }
        StmtKind::While { condition, body } => vec![Stmt {
            kind: StmtKind::While {
                condition: prune_expr(condition),
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
        }],
        StmtKind::DoWhile { body, condition } => vec![Stmt {
            kind: StmtKind::DoWhile {
                body: dce_block_with_guards(body, guards.clone()),
                condition: prune_expr(condition),
            },
            span,
        }],
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => vec![Stmt {
            kind: StmtKind::For {
                init: init.and_then(|stmt| dce_stmt(*stmt).into_iter().next().map(Box::new)),
                condition: condition.map(prune_expr),
                update: update.and_then(|stmt| dce_stmt(*stmt).into_iter().next().map(Box::new)),
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
        }],
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
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
        }],
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => dce_switch_stmt(subject, cases, default, span, guards),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => dce_try_stmt(try_body, catches, finally_body, span, guards),
        StmtKind::NamespaceBlock { name, body } => vec![Stmt {
            kind: StmtKind::NamespaceBlock {
                name,
                body: dce_block_with_guards(body, guards.clone()),
            },
            span,
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
                body: dce_block_with_guards(body, GuardState::default()),
            },
            span,
        }],
        StmtKind::Return(expr) => vec![Stmt {
            kind: StmtKind::Return(expr.map(prune_expr)),
            span,
        }],
        StmtKind::Throw(expr) => vec![Stmt {
            kind: StmtKind::Throw(prune_expr(expr)),
            span,
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
        } => {
            let parent_name = extends.as_ref().map(|parent| parent.as_str().to_string());
            let methods = methods
                .into_iter()
                .map(|method| dce_method(method, &name, parent_name.as_deref()))
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
                },
                span,
            }]
        }
        StmtKind::ExprStmt(expr) => {
            let expr = prune_expr(expr);
            if expr_has_side_effects(&expr) {
                vec![Stmt {
                    kind: StmtKind::ExprStmt(expr),
                    span,
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
        }],
        StmtKind::PackedClassDecl { name, fields } => vec![Stmt {
            kind: StmtKind::PackedClassDecl { name, fields },
            span,
        }],
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => vec![Stmt {
            kind: StmtKind::InterfaceDecl {
                name,
                extends,
                methods: methods
                    .into_iter()
                    .map(dce_method_without_context)
                    .collect(),
            },
            span,
        }],
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => vec![Stmt {
            kind: StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods: methods
                    .into_iter()
                    .map(dce_method_without_context)
                    .collect(),
            },
            span,
        }],
        kind => vec![Stmt { kind, span }],
    }
}
