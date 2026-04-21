use super::*;

#[derive(Clone, Copy)]
enum TailSinkTarget {
    FallsThrough,
}

pub(crate) fn dce_block(body: Vec<Stmt>) -> Vec<Stmt> {
    let mut eliminated = Vec::new();
    let mut stmts = body.into_iter().peekable();
    while let Some(stmt) = stmts.next() {
        let has_tail = stmts.peek().is_some();
        let use_tail_sink = has_tail
            && matches!(stmt.kind, StmtKind::If { .. } | StmtKind::Switch { .. });
        let dce_stmt = if use_tail_sink {
            let tail: Vec<Stmt> = stmts.clone().collect();
            dce_stmt_with_tail(stmt, tail)
        } else {
            dce_stmt(stmt)
        };
        let stops_here = dce_stmt
            .last()
            .is_some_and(|stmt| !matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough));
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
    if matches!(block_terminal_effect(&body), TerminalEffect::FallsThrough) {
        body.extend(tail);
    }
    body
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
                let effect = block_terminal_effect(&body);
                if matches!(effect, TerminalEffect::FallsThrough) {
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
            let then_body = if matches!(block_terminal_effect(&then_body), TerminalEffect::FallsThrough) {
                sink_tail_into_terminal_path(then_body, tail.clone(), target)
            } else {
                then_body
            };
            let else_body = else_body.map(|body| {
                if matches!(block_terminal_effect(&body), TerminalEffect::FallsThrough) {
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
            let try_body = if matches!(block_terminal_effect(&try_body), TerminalEffect::FallsThrough) {
                sink_tail_into_terminal_path(try_body, tail.clone(), target)
            } else {
                try_body
            };
            let catches = catches
                .into_iter()
                .map(|catch| crate::parser::ast::CatchClause {
                    body: if matches!(block_terminal_effect(&catch.body), TerminalEffect::FallsThrough) {
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
        _ => vec![stmt],
    }
}

fn dce_if_tail(
    mut elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let Some((condition, body)) = elseif_clauses.first().cloned() else {
        return else_body.unwrap_or_default();
    };
    elseif_clauses.remove(0);
    let rest = dce_if_tail(elseif_clauses, else_body, span);

    if body.is_empty() {
        if rest.is_empty() {
            expr_to_effect_stmt(condition)
        } else {
            vec![build_if_stmt(
                invert_condition(condition),
                rest,
                Vec::new(),
                None,
                span,
            )]
        }
    } else {
        vec![build_if_stmt(
            condition,
            body,
            Vec::new(),
            normalize_optional_block(Some(rest)),
            span,
        )]
    }
}

fn dce_if_stmt(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let condition = prune_expr(condition);
    let then_body = dce_block(then_body);
    let elseif_clauses: Vec<_> = elseif_clauses
        .into_iter()
        .map(|(condition, body)| (prune_expr(condition), dce_block(body)))
        .collect();
    let else_body = normalize_optional_block(else_body.map(dce_block));
    let tail = dce_if_tail(elseif_clauses.clone(), else_body.clone(), span);

    if tail.is_empty() {
        if then_body.is_empty() {
            return expr_to_effect_stmt(condition);
        }

        return vec![build_if_stmt(
            condition,
            then_body,
            Vec::new(),
            None,
            span,
        )];
    }

    if elseif_clauses.is_empty() {
        if then_body.is_empty() && else_body.is_none() {
            return expr_to_effect_stmt(condition);
        }

        if then_body.is_empty() {
            if let Some(else_body) = else_body {
                return vec![build_if_stmt(
                    invert_condition(condition),
                    else_body,
                    Vec::new(),
                    None,
                    span,
                )];
            }
        }

        if tail == then_body {
            let mut stmts = expr_to_effect_stmt(condition);
            stmts.extend(then_body);
            return stmts;
        }
    }

    if then_body.is_empty() {
        return vec![build_if_stmt(
            invert_condition(condition),
            tail,
            Vec::new(),
            None,
            span,
        )];
    }

    vec![Stmt::new(
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses: Vec::new(),
            else_body: normalize_optional_block(Some(tail)),
        },
        span,
    )]
}

fn dce_switch_stmt(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let trim_switch_noop_break = |body: Vec<Stmt>| {
        if body.len() == 1 && matches!(body[0].kind, StmtKind::Break) {
            Vec::new()
        } else {
            body
        }
    };
    let subject = prune_expr(subject);
    let cases = normalize_switch_cases(
        cases
            .into_iter()
            .map(|(patterns, body)| {
                (
                    patterns.into_iter().map(prune_expr).collect(),
                    trim_switch_noop_break(dce_block(body)),
                )
            })
            .collect(),
    );
    let mut cases = cases;
    while cases.last().is_some_and(|(_, body)| body.is_empty()) {
        cases.pop();
    }
    let default = normalize_optional_block(default.map(dce_block));

    if cases.iter().all(|(_, body)| body.is_empty()) && default.is_none() {
        return expr_to_effect_stmt(subject);
    }

    if cases.is_empty() {
        let mut stmts = expr_to_effect_stmt(subject);
        if let Some(default_body) = default {
            stmts.extend(default_body);
        }
        return stmts;
    }

    vec![Stmt::new(
        StmtKind::Switch {
            subject,
            cases,
            default,
        },
        span,
    )]
}

fn dce_switch_stmt_with_tail(
    subject: Expr,
    cases: Vec<(Vec<Expr>, Vec<Stmt>)>,
    default: Option<Vec<Stmt>>,
    tail: Vec<Stmt>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let trim_switch_noop_break = |body: Vec<Stmt>| {
        if body.len() == 1 && matches!(body[0].kind, StmtKind::Break) {
            Vec::new()
        } else {
            body
        }
    };
    let subject = prune_expr(subject);
    let tail = dce_block(tail);
    let mut cases = normalize_switch_cases(
        cases
            .into_iter()
            .map(|(patterns, body)| {
                (
                    patterns.into_iter().map(prune_expr).collect(),
                    trim_switch_noop_break(dce_block(body)),
                )
            })
            .collect(),
    );
    while cases.last().is_some_and(|(_, body)| body.is_empty()) {
        cases.pop();
    }
    let mut default = normalize_optional_block(default.map(dce_block));

    if tail.is_empty() {
        return dce_switch_stmt(subject, cases, default, span);
    }

    let has_break_exit = cases
        .iter()
        .any(|(_, body)| matches!(block_terminal_effect(body), TerminalEffect::Breaks))
        || default
            .as_ref()
            .is_some_and(|body| matches!(block_terminal_effect(body), TerminalEffect::Breaks));
    if has_break_exit {
        let mut stmts = dce_switch_stmt(subject, cases, default, span);
        stmts.extend(tail);
        return stmts;
    }

    let mut suffix_reaches_after_switch = false;
    if let Some(body) = default.as_mut() {
        match block_terminal_effect(body) {
            TerminalEffect::FallsThrough => {
                *body = sink_tail_into_terminal_path(
                    std::mem::take(body),
                    tail.clone(),
                    TailSinkTarget::FallsThrough,
                );
                suffix_reaches_after_switch = true;
            }
            TerminalEffect::Breaks | TerminalEffect::ExitsCurrentBlock | TerminalEffect::TerminatesMixed => {}
        }
    }

    let last_case_index = cases.len().checked_sub(1);
    for (index, (_, body)) in cases.iter_mut().enumerate().rev() {
        match block_terminal_effect(body) {
            TerminalEffect::FallsThrough if suffix_reaches_after_switch || Some(index) == last_case_index => {
                if !suffix_reaches_after_switch {
                    *body = sink_tail_into_terminal_path(
                        std::mem::take(body),
                        tail.clone(),
                        TailSinkTarget::FallsThrough,
                    );
                }
                suffix_reaches_after_switch = true;
            }
            TerminalEffect::FallsThrough => {}
            TerminalEffect::Breaks | TerminalEffect::ExitsCurrentBlock | TerminalEffect::TerminatesMixed => {
                suffix_reaches_after_switch = false;
            }
        }
    }

    dce_switch_stmt(subject, cases, default, span)
}

fn dce_try_stmt(
    try_body: Vec<Stmt>,
    catches: Vec<crate::parser::ast::CatchClause>,
    finally_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Vec<Stmt> {
    let try_body = dce_block(try_body);
    let catches: Vec<_> = catches
        .into_iter()
        .map(|catch| crate::parser::ast::CatchClause {
            exception_types: catch.exception_types,
            variable: catch.variable,
            body: dce_block(catch.body),
        })
        .collect();
    let catches = if block_may_throw(&try_body) {
        normalize_catch_clauses(catches)
    } else {
        Vec::new()
    };
    let finally_body = normalize_optional_block(finally_body.map(dce_block));

    if try_body.is_empty() {
        return finally_body.unwrap_or_default();
    }

    if catches.is_empty() && finally_body.is_none() {
        return try_body;
    }

    if catches.is_empty() {
        if let Some(finally_body) = finally_body {
            if !block_may_throw(&try_body)
                && matches!(block_terminal_effect(&try_body), TerminalEffect::FallsThrough)
            {
                let mut stmts = try_body;
                stmts.extend(finally_body);
                return stmts;
            }

            return vec![Stmt::new(
                StmtKind::Try {
                    try_body,
                    catches: Vec::new(),
                    finally_body: Some(finally_body),
                },
                span,
            )];
        }
    }

    vec![Stmt::new(
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        },
        span,
    )]
}

fn dce_stmt_with_tail(stmt: Stmt, tail: Vec<Stmt>) -> Vec<Stmt> {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => {
            let then_body = append_tail_to_fallthrough_path(then_body, tail.clone());
            let elseif_clauses: Vec<_> = elseif_clauses
                .into_iter()
                .map(|(condition, body)| (condition, append_tail_to_fallthrough_path(body, tail.clone())))
                .collect();
            let else_body = Some(match else_body {
                Some(body) => append_tail_to_fallthrough_path(body, tail),
                None => tail,
            });
            dce_if_stmt(condition, then_body, elseif_clauses, else_body, span)
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => dce_switch_stmt_with_tail(subject, cases, default, tail, span),
        _ => {
            let mut stmts = dce_stmt(stmt);
            if stmts
                .last()
                .is_some_and(|stmt| matches!(stmt_terminal_effect(stmt), TerminalEffect::FallsThrough))
            {
                stmts.extend(dce_block(tail));
            }
            stmts
        }
    }
}

pub(crate) fn dce_stmt(stmt: Stmt) -> Vec<Stmt> {
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
        } => dce_if_stmt(condition, then_body, elseif_clauses, else_body, span),
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let then_body = dce_block(then_body);
            let else_body = normalize_optional_block(else_body.map(dce_block));
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
                body: dce_block(body),
            },
            span,
        }],
        StmtKind::DoWhile { body, condition } => vec![Stmt {
            kind: StmtKind::DoWhile {
                body: dce_block(body),
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
                body: dce_block(body),
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
                body: dce_block(body),
            },
            span,
        }],
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => dce_switch_stmt(subject, cases, default, span),
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => dce_try_stmt(try_body, catches, finally_body, span),
        StmtKind::NamespaceBlock { name, body } => vec![Stmt {
            kind: StmtKind::NamespaceBlock {
                name,
                body: dce_block(body),
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
                body: dce_block(body),
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

pub(crate) fn dce_method(method: ClassMethod, class_name: &str, parent_name: Option<&str>) -> ClassMethod {
    let context = ClassEffectContext {
        class_name: class_name.to_string(),
        parent_name: parent_name.map(str::to_string),
    };
    ClassMethod {
        body: with_class_effect_context(Some(context), || dce_block(method.body)),
        ..method
    }
}

pub(crate) fn dce_method_without_context(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        body: with_class_effect_context(None, || dce_block(method.body)),
        ..method
    }
}
