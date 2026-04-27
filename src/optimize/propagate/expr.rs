use super::*;

pub(crate) fn captured_constant_env(captures: &[String], env: &ConstantEnv) -> ConstantEnv {
    captures
        .iter()
        .filter_map(|name| env.get(name).cloned().map(|value| (name.clone(), value)))
        .collect()
}

pub(crate) fn propagate_expr(expr: Expr, env: &ConstantEnv) -> Expr {
    let span = expr.span;
    let kind = match expr.kind {
        ExprKind::StringLiteral(value) => ExprKind::StringLiteral(value),
        ExprKind::IntLiteral(value) => ExprKind::IntLiteral(value),
        ExprKind::FloatLiteral(value) => ExprKind::FloatLiteral(value),
        ExprKind::Variable(name) => match env.get(&name) {
            Some(value) => value.clone().into_expr_kind(),
            None => ExprKind::Variable(name),
        },
        ExprKind::BinaryOp { left, op, right } => ExprKind::BinaryOp {
            left: Box::new(propagate_expr(*left, env)),
            op,
            right: Box::new(propagate_expr(*right, env)),
        },
        ExprKind::InstanceOf { value, target } => ExprKind::InstanceOf {
            value: Box::new(propagate_expr(*value, env)),
            target,
        },
        ExprKind::BoolLiteral(value) => ExprKind::BoolLiteral(value),
        ExprKind::Null => ExprKind::Null,
        ExprKind::Negate(inner) => ExprKind::Negate(Box::new(propagate_expr(*inner, env))),
        ExprKind::Not(inner) => ExprKind::Not(Box::new(propagate_expr(*inner, env))),
        ExprKind::BitNot(inner) => ExprKind::BitNot(Box::new(propagate_expr(*inner, env))),
        ExprKind::Throw(inner) => ExprKind::Throw(Box::new(propagate_expr(*inner, env))),
        ExprKind::NullCoalesce { value, default } => ExprKind::NullCoalesce {
            value: Box::new(propagate_expr(*value, env)),
            default: Box::new(propagate_expr(*default, env)),
        },
        ExprKind::PreIncrement(name) => ExprKind::PreIncrement(name),
        ExprKind::PostIncrement(name) => ExprKind::PostIncrement(name),
        ExprKind::PreDecrement(name) => ExprKind::PreDecrement(name),
        ExprKind::PostDecrement(name) => ExprKind::PostDecrement(name),
        ExprKind::FunctionCall { name, args } => {
            let arg_env = (!function_call_effect(name.as_str()).has_side_effects).then_some(env);
            ExprKind::FunctionCall {
                name,
                args: propagate_args(args, arg_env),
            }
        }
        ExprKind::ArrayLiteral(items) => {
            ExprKind::ArrayLiteral(items.into_iter().map(|item| propagate_expr(item, env)).collect())
        }
        ExprKind::ArrayLiteralAssoc(items) => ExprKind::ArrayLiteralAssoc(
            items.into_iter()
                .map(|(key, value)| (propagate_expr(key, env), propagate_expr(value, env)))
                .collect(),
        ),
        ExprKind::Match {
            subject,
            arms,
            default,
        } => ExprKind::Match {
            subject: Box::new(propagate_expr(*subject, env)),
            arms: arms
                .into_iter()
                .map(|(patterns, value)| {
                    (
                        patterns
                            .into_iter()
                            .map(|pattern| propagate_expr(pattern, env))
                            .collect(),
                        propagate_expr(value, env),
                    )
                })
                .collect(),
            default: default.map(|expr| Box::new(propagate_expr(*expr, env))),
        },
        ExprKind::ArrayAccess { array, index } => ExprKind::ArrayAccess {
            array: Box::new(propagate_expr(*array, env)),
            index: Box::new(propagate_expr(*index, env)),
        },
        ExprKind::Ternary {
            condition,
            then_expr,
            else_expr,
        } => ExprKind::Ternary {
            condition: Box::new(propagate_expr(*condition, env)),
            then_expr: Box::new(propagate_expr(*then_expr, env)),
            else_expr: Box::new(propagate_expr(*else_expr, env)),
        },
        ExprKind::ShortTernary { value, default } => ExprKind::ShortTernary {
            value: Box::new(propagate_expr(*value, env)),
            default: Box::new(propagate_expr(*default, env)),
        },
        ExprKind::Cast { target, expr } => ExprKind::Cast {
            target,
            expr: Box::new(propagate_expr(*expr, env)),
        },
        ExprKind::Closure {
            params,
            variadic,
            body,
            is_arrow,
            captures,
        } => ExprKind::Closure {
            params: propagate_params(params),
            variadic,
            body: propagate_block(body, captured_constant_env(&captures, env)).0,
            is_arrow,
            captures,
        },
        ExprKind::NamedArg { name, value } => ExprKind::NamedArg {
            name,
            value: Box::new(propagate_expr(*value, env)),
        },
        ExprKind::Spread(inner) => ExprKind::Spread(Box::new(propagate_expr(*inner, env))),
        ExprKind::ClosureCall { var, args } => {
            let arg_env = (!callable_alias_effect(&var).has_side_effects).then_some(env);
            ExprKind::ClosureCall {
                var,
                args: propagate_args(args, arg_env),
            }
        }
        ExprKind::ExprCall { callee, args } => {
            let callee = propagate_expr(*callee, env);
            let arg_env = (!expr_call_effect(&callee).has_side_effects).then_some(env);
            ExprKind::ExprCall {
                callee: Box::new(callee),
                args: propagate_args(args, arg_env),
            }
        }
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
            args: propagate_args(args, None),
        },
        ExprKind::PropertyAccess { object, property } => ExprKind::PropertyAccess {
            object: Box::new(propagate_expr(*object, env)),
            property,
        },
        ExprKind::StaticPropertyAccess { receiver, property } => {
            ExprKind::StaticPropertyAccess { receiver, property }
        }
        ExprKind::MethodCall {
            object,
            method,
            args,
        } => {
            let object = propagate_expr(*object, env);
            let arg_env =
                (!private_instance_method_call_effect(&object, &method).has_side_effects)
                    .then_some(env);
            ExprKind::MethodCall {
                object: Box::new(object),
                method,
                args: propagate_args(args, arg_env),
            }
        }
        ExprKind::StaticMethodCall {
            receiver,
            method,
            args,
        } => {
            let arg_env =
                (!static_method_call_effect(&receiver, &method).has_side_effects).then_some(env);
            ExprKind::StaticMethodCall {
                receiver,
                method,
                args: propagate_args(args, arg_env),
            }
        }
        ExprKind::FirstClassCallable(target) => {
            ExprKind::FirstClassCallable(propagate_callable_target(target, env))
        }
        ExprKind::This => ExprKind::This,
        ExprKind::PtrCast { target_type, expr } => ExprKind::PtrCast {
            target_type,
            expr: Box::new(propagate_expr(*expr, env)),
        },
        ExprKind::BufferNew { element_type, len } => ExprKind::BufferNew {
            element_type,
            len: Box::new(propagate_expr(*len, env)),
        },
        ExprKind::MagicConstant(_) => {
            unreachable!("MagicConstant must be lowered before optimizer passes")
        }
    };

    fold_expr(Expr { kind, span })
}

pub(crate) fn propagate_callable_target(target: CallableTarget, env: &ConstantEnv) -> CallableTarget {
    match target {
        CallableTarget::Function(name) => CallableTarget::Function(name),
        CallableTarget::StaticMethod { receiver, method } => {
            CallableTarget::StaticMethod { receiver, method }
        }
        CallableTarget::Method { object, method } => CallableTarget::Method {
            object: Box::new(propagate_expr(*object, env)),
            method,
        },
    }
}

pub(crate) fn propagate_args(args: Vec<Expr>, env: Option<&ConstantEnv>) -> Vec<Expr> {
    match env {
        Some(env) => args.into_iter().map(|arg| propagate_expr(arg, env)).collect(),
        None => {
            let empty_env = HashMap::new();
            args.into_iter()
                .map(|arg| propagate_expr(arg, &empty_env))
                .collect()
        }
    }
}

pub(crate) fn build_if_stmt(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
) -> Stmt {
    if elseif_clauses.is_empty() {
        if let Some(else_body_ref) = else_body.as_ref() {
            if else_body_ref.len() == 1 {
                if let StmtKind::If {
                    condition: inner_condition,
                    then_body: inner_then_body,
                    elseif_clauses: inner_elseifs,
                    else_body: inner_else,
                } = &else_body_ref[0].kind
                {
                    if inner_elseifs.is_empty() && *inner_then_body == then_body {
                        return build_if_stmt(
                            combine_if_chain_conditions(condition, inner_condition.clone()),
                            then_body,
                            Vec::new(),
                            inner_else.clone(),
                            span,
                        );
                    }

                    if inner_elseifs.is_empty() && inner_else.as_ref() == Some(&then_body) {
                        return build_if_stmt(
                            combine_if_conditions(
                                invert_condition(condition),
                                inner_condition.clone(),
                            ),
                            inner_then_body.clone(),
                            Vec::new(),
                            Some(then_body),
                            span,
                        );
                    }
                }
            }
        }

        if else_body.is_none() && then_body.len() == 1 {
            if let StmtKind::If {
                condition: inner_condition,
                then_body: inner_then_body,
                elseif_clauses: inner_elseifs,
                else_body: inner_else,
            } = &then_body[0].kind
            {
                if inner_elseifs.is_empty() && inner_else.is_none() {
                    return Stmt {
                        kind: StmtKind::If {
                            condition: combine_if_conditions(condition, inner_condition.clone()),
                            then_body: inner_then_body.clone(),
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        span,
                    };
                }
            }
        }
    }

    Stmt {
        kind: StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        },
        span,
    }
}
