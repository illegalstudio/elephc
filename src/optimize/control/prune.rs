use super::*;

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
        }],
        StmtKind::Assign { name, value } => vec![Stmt {
            kind: StmtKind::Assign {
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
        }],
        StmtKind::Return(expr) => vec![Stmt {
            kind: StmtKind::Return(expr.map(prune_expr)),
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
                .map(|method| prune_method(method, &name, parent_name.as_deref()))
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
                    .map(prune_method_without_context)
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
                    .map(prune_method_without_context)
                    .collect(),
            },
            span,
        }],
        kind => vec![Stmt { kind, span }],
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

fn block_contains_loop_exit(body: &[Stmt]) -> bool {
    body.iter().any(stmt_contains_loop_exit)
}

fn stmt_contains_loop_exit(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Break | StmtKind::Continue => true,
        StmtKind::If {
            then_body,
            elseif_clauses,
            else_body,
            ..
        } => {
            block_contains_loop_exit(then_body)
                || elseif_clauses
                    .iter()
                    .any(|(_, body)| block_contains_loop_exit(body))
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_contains_loop_exit(body))
        }
        StmtKind::IfDef {
            then_body, else_body, ..
        } => {
            block_contains_loop_exit(then_body)
                || else_body
                    .as_ref()
                    .is_some_and(|body| block_contains_loop_exit(body))
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            block_contains_loop_exit(try_body)
                || catches
                    .iter()
                    .any(|catch| block_contains_loop_exit(&catch.body))
                || finally_body
                    .as_ref()
                    .is_some_and(|body| block_contains_loop_exit(body))
        }
        StmtKind::Switch { cases, default, .. } => {
            cases
                .iter()
                .any(|(_, body)| block_contains_loop_exit(body))
                || default
                    .as_ref()
                    .is_some_and(|body| block_contains_loop_exit(body))
        }
        _ => false,
    }
}

pub(crate) fn prune_for_clause(stmt: Option<Box<Stmt>>) -> Option<Box<Stmt>> {
    let stmt = stmt?;
    prune_stmt(*stmt).into_iter().next().map(Box::new)
}

pub(crate) fn prune_expr(expr: Expr) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => ExprKind::Variable(name),
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(prune_expr(*left)),
            op,
            right: Box::new(prune_expr(*right)),
        },
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(prune_expr(*inner))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(prune_expr(*inner))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(prune_expr(*inner))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(prune_expr(*inner))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(prune_expr(*value)),
            default: Box::new(prune_expr(*default)),
        },
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => ExprKind::FunctionCall {
            name,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(prune_expr).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items.into_iter()
                .map(|(key, value)| (prune_expr(key), prune_expr(value)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => {
            let subject = prune_expr(*subject);
            let arms: Vec<(Vec<Expr>, Expr)> = arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns.into_iter().map(prune_expr).collect(),
                        prune_expr(value),
                    )
                })
                .collect();
            let default = default.map(|expr| Box::new(prune_expr(*expr)));
            try_prune_match_expr(subject, arms, default)
        }
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(prune_expr(*array)),
            index: Box::new(prune_expr(*index)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(prune_expr(*condition)),
            then_expr: Box::new(prune_expr(*then_expr)),
            else_expr: Box::new(prune_expr(*else_expr)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(prune_expr(*expr)),
        },
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => ExprKind::Closure {
            params,
            variadic,
            body: prune_block(body),
            is_arrow,
            captures,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(prune_expr(*value)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(prune_expr(*inner))),
        ExprKind::ClosureCall { var, args } => ExprKind::ClosureCall {
            var,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ExprCall { callee, args } => ExprKind::ExprCall {
            callee: Box::new(prune_expr(*callee)),
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::ConstRef(name) => ExprKind::ConstRef(name),
        ExprKind::EnumCase {
            enum_name,
            case_name,
        } => ExprKind::EnumCase {
            enum_name,
            case_name,
        },
        ExprKind::NewObject { class_name, args } => ExprKind::NewObject {
            class_name,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(prune_expr(*object)),
            property,
        },
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => ExprKind::MethodCall {
            object: Box::new(prune_expr(*object)),
            method,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => ExprKind::StaticMethodCall {
            receiver,
            method,
            args: args.into_iter().map(prune_expr).collect(),
        },
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(prune_callable_target(target))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(prune_expr(*expr)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(prune_expr(*len)),
        },
    };
    let kind = prune_unused_pure_subexpressions(kind);
    Expr { kind, span }
}

pub(crate) fn expr_has_side_effects(expr: &Expr) -> bool {
    expr_effect(expr).has_side_effects
}

pub(crate) fn callable_target_effect(target: &CallableTarget) -> Effect {
    match target {
        CallableTarget::Function(_) | CallableTarget::StaticMethod { .. } => Effect::PURE,
        CallableTarget::Method { object, .. } => expr_effect(object),
    }
}

pub(crate) fn prune_unused_pure_subexpressions(kind: ExprKind) -> ExprKind {
    match kind {
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => match scalar_value(&condition) {
            Some(value) if value.truthy() && !expr_has_side_effects(&else_expr) => then_expr.kind,
            Some(value) if !value.truthy() && !expr_has_side_effects(&then_expr) => else_expr.kind,
            _ => ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            },
        },
        ExprKind::NullCoalesce { value, default } => match scalar_value(&value) {
            Some(ScalarValue::Null) => default.kind,
            Some(_) if !expr_has_side_effects(&default) => value.kind,
            _ => ExprKind::NullCoalesce { value, default },
        },
        ExprKind::BinaryOp { left, op, right } => match op {
            BinOp::And => match scalar_value(&left) {
                Some(value) if !value.truthy() && !expr_has_side_effects(&right) => {
                    ExprKind::BoolLiteral(false)
                }
                _ => ExprKind::BinaryOp { left, op, right },
            },
            BinOp::Or => match scalar_value(&left) {
                Some(value) if value.truthy() && !expr_has_side_effects(&right) => {
                    ExprKind::BoolLiteral(true)
                }
                _ => ExprKind::BinaryOp { left, op, right },
            },
            _ => ExprKind::BinaryOp { left, op, right },
        },
        other => other,
    }
}

pub(crate) fn prune_callable_target(target: CallableTarget) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(prune_expr(*object)),
            method,
        },
    }
}
