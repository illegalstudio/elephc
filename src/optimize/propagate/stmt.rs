use super::*;

pub(crate) fn propagate_block(body: Vec<Stmt>, mut env: ConstantEnv) -> (Vec<Stmt>, ConstantEnv) {
    let mut propagated = Vec::new();
    for stmt in body {
        let (stmt, next_env) = propagate_stmt(stmt, env);
        let stops_here = !matches!(stmt_terminal_effect(&stmt), TerminalEffect::FallsThrough);
        propagated.push(stmt);
        env = next_env;
        if stops_here {
            break;
        }
    }
    (propagated, env)
}

pub(crate) fn propagate_stmt(stmt: Stmt, env: ConstantEnv) -> (Stmt, ConstantEnv) {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Echo(expr) => {
            let expr = propagate_expr(expr, &env);
            (Stmt::new(StmtKind::Echo(expr), span), env)
        }
        StmtKind::Assign { name, value } => {
            let value = propagate_expr(value, &env);
            let mut next_env = env_after_scalar_assign(env, &name, &value);
            (Stmt::new(StmtKind::Assign { name, value }, span), std::mem::take(&mut next_env))
        }
        StmtKind::TypedAssign {
            type_expr,
            name,
            value,
        } => {
            let value = propagate_expr(value, &env);
            let mut next_env = env_after_scalar_assign(env, &name, &value);
            (
                Stmt::new(
                    StmtKind::TypedAssign {
                        type_expr,
                        name,
                        value,
                    },
                    span,
                ),
                std::mem::take(&mut next_env),
            )
        }
        StmtKind::If {
            condition,
            then_body,
            elseif_clauses,
            else_body,
        } => propagate_if_stmt(condition, then_body, elseif_clauses, else_body, span, env),
        StmtKind::IfDef {
            symbol,
            then_body,
            else_body,
        } => {
            let (then_body, then_env) = propagate_block(then_body, env.clone());
            let (else_body, next_env) = match else_body {
                Some(body) => {
                    let (body, else_env) = propagate_block(body, env);
                    (Some(body), merge_constant_env_paths(vec![then_env, else_env]))
                }
                None => (None, merge_constant_env_paths(vec![then_env, env])),
            };
            (
                Stmt::new(
                    StmtKind::IfDef {
                        symbol,
                        then_body,
                        else_body,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::While { condition, body } => {
            let loop_env = safe_loop_env(&env, std::slice::from_ref(&condition), &body, None);
            let condition = propagate_expr(condition, &loop_env);
            let (body, _) = propagate_block(body, loop_env.clone());
            (
                Stmt::new(StmtKind::While { condition, body }, span),
                loop_env,
            )
        }
        StmtKind::DoWhile { body, condition } => {
            let loop_env = safe_loop_env(&env, std::slice::from_ref(&condition), &body, None);
            let (body, _) = propagate_block(body, loop_env.clone());
            let condition = propagate_expr(condition, &loop_env);
            (
                Stmt::new(StmtKind::DoWhile { body, condition }, span),
                loop_env,
            )
        }
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => {
            let (init, init_env) = match init {
                Some(stmt) => {
                    let (stmt, next_env) = propagate_stmt(*stmt, env);
                    (Some(Box::new(stmt)), next_env)
                }
                None => (None, env),
            };
            let condition_exprs = condition.iter().cloned().collect::<Vec<_>>();
            let update_stmt = update.as_deref();
            let loop_env = safe_loop_env(&init_env, &condition_exprs, &body, update_stmt);
            let condition = condition.map(|expr| propagate_expr(expr, &loop_env));
            let update = update.map(|stmt| Box::new(propagate_stmt(*stmt, loop_env.clone()).0));
            let (body, _) = propagate_block(body, loop_env.clone());
            (
                Stmt::new(
                    StmtKind::For {
                        init,
                        condition,
                        update,
                        body,
                    },
                    span,
                ),
                loop_env,
            )
        }
        StmtKind::ArrayAssign {
            array,
            index,
            value,
        } => {
            let index = propagate_expr(index, &env);
            let value = propagate_expr(value, &env);
            let mut next_env = env;
            next_env.remove(&array);
            (
                Stmt::new(
                    StmtKind::ArrayAssign {
                        array,
                        index,
                        value,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::ArrayPush { array, value } => {
            let value = propagate_expr(value, &env);
            let mut next_env = env;
            next_env.remove(&array);
            (
                Stmt::new(StmtKind::ArrayPush { array, value }, span),
                next_env,
            )
        }
        StmtKind::Foreach {
            array,
            key_var,
            value_var,
            body,
        } => {
            let loop_env = safe_foreach_env(&env, &array, key_var.as_deref(), &value_var, &body);
            let array = propagate_expr(array, &env);
            let (body, _) = propagate_block(body, loop_env.clone());
            (
                Stmt::new(
                    StmtKind::Foreach {
                        array,
                        key_var,
                        value_var,
                        body,
                    },
                    span,
                ),
                loop_env,
            )
        }
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => {
            let subject = propagate_expr(subject, &env);
            let base_env = if expr_effect(&subject).has_side_effects {
                HashMap::new()
            } else {
                env
            };
            let cases: Vec<_> = cases
                .into_iter()
                .map(|(patterns, body)| {
                    let patterns = patterns
                        .into_iter()
                        .map(|pattern| propagate_expr(pattern, &base_env))
                        .collect();
                    let (body, _) = propagate_block(body, base_env.clone());
                    (patterns, body)
                })
                .collect();
            let default = default.map(|body| propagate_block(body, base_env.clone()).0);
            let next_env =
                merge_switch_constant_env_paths(&subject, &cases, default.as_deref(), &base_env);
            (
                Stmt::new(
                    StmtKind::Switch {
                        subject,
                        cases,
                        default,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::Include {
            path,
            once,
            required,
        } => (
            Stmt::new(
                StmtKind::Include {
                    path,
                    once,
                    required,
                },
                span,
            ),
            HashMap::new(),
        ),
        StmtKind::Throw(expr) => {
            let expr = propagate_expr(expr, &env);
            (Stmt::new(StmtKind::Throw(expr), span), HashMap::new())
        }
        StmtKind::Try {
            try_body,
            catches,
            finally_body,
        } => {
            let (try_body, _) = propagate_block(try_body, env.clone());
            let catches: Vec<_> = catches
                .into_iter()
                .map(|catch| crate::parser::ast::CatchClause {
                    exception_types: catch.exception_types,
                    variable: catch.variable,
                    body: propagate_block(catch.body, env.clone()).0,
                })
                .collect();
            let finally_body = finally_body.map(|body| propagate_block(body, HashMap::new()).0);
            let next_env =
                merge_try_constant_env_paths(&try_body, &catches, finally_body.as_deref(), &env);
            (
                Stmt::new(
                    StmtKind::Try {
                        try_body,
                        catches,
                        finally_body,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::Break => (Stmt::new(StmtKind::Break, span), env),
        StmtKind::Continue => (Stmt::new(StmtKind::Continue, span), env),
        StmtKind::ExprStmt(expr) => {
            let expr = propagate_expr(expr, &env);
            let next_env = if let Some(name) = unset_target_name(&expr) {
                let mut next_env = env;
                next_env.remove(&name);
                next_env
            } else if expr_effect(&expr).has_side_effects {
                HashMap::new()
            } else {
                env
            };
            (Stmt::new(StmtKind::ExprStmt(expr), span), next_env)
        }
        StmtKind::NamespaceDecl { name } => (Stmt::new(StmtKind::NamespaceDecl { name }, span), env),
        StmtKind::NamespaceBlock { name, body } => {
            let (body, _) = propagate_block(body, HashMap::new());
            (
                Stmt::new(StmtKind::NamespaceBlock { name, body }, span),
                env,
            )
        }
        StmtKind::UseDecl { imports } => (Stmt::new(StmtKind::UseDecl { imports }, span), env),
        StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
        } => (
            Stmt::new(
                StmtKind::FunctionDecl {
                    name,
                    params: propagate_params(params),
                    variadic,
                    return_type,
                    body: propagate_block(body, HashMap::new()).0,
                },
                span,
            ),
            env,
        ),
        StmtKind::Return(expr) => {
            let expr = expr.map(|expr| propagate_expr(expr, &env));
            (Stmt::new(StmtKind::Return(expr), span), env)
        }
        StmtKind::ConstDecl { name, value } => {
            let value = propagate_expr(value, &env);
            (Stmt::new(StmtKind::ConstDecl { name, value }, span), env)
        }
        StmtKind::ListUnpack { vars, value } => {
            let value = propagate_expr(value, &env);
            let next_env = env_after_list_unpack(env, &vars, &value);
            (
                Stmt::new(StmtKind::ListUnpack { vars, value }, span),
                next_env,
            )
        }
        StmtKind::Global { vars } => {
            let mut next_env = env;
            for var in &vars {
                next_env.remove(var);
            }
            (Stmt::new(StmtKind::Global { vars }, span), next_env)
        }
        StmtKind::StaticVar { name, init } => {
            let init = propagate_expr(init, &env);
            let mut next_env = env;
            next_env.remove(&name);
            (
                Stmt::new(StmtKind::StaticVar { name, init }, span),
                next_env,
            )
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
        } => (
            Stmt::new(
                StmtKind::ClassDecl {
                    name,
                    extends,
                    implements,
                    is_abstract,
                    is_readonly_class,
                    trait_uses,
                    properties: properties.into_iter().map(propagate_property).collect(),
                    methods: methods.into_iter().map(propagate_method).collect(),
                },
                span,
            ),
            env,
        ),
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } => (
            Stmt::new(
                StmtKind::EnumDecl {
                    name,
                    backing_type,
                    cases: cases.into_iter().map(propagate_enum_case).collect(),
                },
                span,
            ),
            env,
        ),
        StmtKind::PackedClassDecl { name, fields } => {
            (Stmt::new(StmtKind::PackedClassDecl { name, fields }, span), env)
        }
        StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } => (
            Stmt::new(
                StmtKind::InterfaceDecl {
                    name,
                    extends,
                    methods: methods.into_iter().map(propagate_method).collect(),
                },
                span,
            ),
            env,
        ),
        StmtKind::TraitDecl {
            name,
            trait_uses,
            properties,
            methods,
        } => (
            Stmt::new(
                StmtKind::TraitDecl {
                    name,
                    trait_uses,
                    properties: properties.into_iter().map(propagate_property).collect(),
                    methods: methods.into_iter().map(propagate_method).collect(),
                },
                span,
            ),
            env,
        ),
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => (
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(propagate_expr(*object, &env)),
                    property,
                    value: propagate_expr(value, &env),
                },
                span,
            ),
            env,
        ),
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => (
            Stmt::new(
                StmtKind::PropertyArrayPush {
                    object: Box::new(propagate_expr(*object, &env)),
                    property,
                    value: propagate_expr(value, &env),
                },
                span,
            ),
            env,
        ),
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => (
            Stmt::new(
                StmtKind::PropertyArrayAssign {
                    object: Box::new(propagate_expr(*object, &env)),
                    property,
                    index: propagate_expr(index, &env),
                    value: propagate_expr(value, &env),
                },
                span,
            ),
            env,
        ),
        StmtKind::ExternFunctionDecl {
            name,
            params,
            return_type,
            library,
        } => (
            Stmt::new(
                StmtKind::ExternFunctionDecl {
                    name,
                    params,
                    return_type,
                    library,
                },
                span,
            ),
            env,
        ),
        StmtKind::ExternClassDecl { name, fields } => (
            Stmt::new(StmtKind::ExternClassDecl { name, fields }, span),
            env,
        ),
        StmtKind::ExternGlobalDecl { name, c_type } => (
            Stmt::new(StmtKind::ExternGlobalDecl { name, c_type }, span),
            env,
        ),
    }
}

pub(crate) fn propagate_if_stmt(
    condition: Expr,
    then_body: Vec<Stmt>,
    elseif_clauses: Vec<(Expr, Vec<Stmt>)>,
    else_body: Option<Vec<Stmt>>,
    span: crate::span::Span,
    env: ConstantEnv,
) -> (Stmt, ConstantEnv) {
    let condition = propagate_expr(condition, &env);
    let base_env = if expr_effect(&condition).has_side_effects {
        HashMap::new()
    } else {
        env
    };

    let (then_body, then_env) = propagate_block(then_body, base_env.clone());
    let mut propagated_elseifs = Vec::new();
    let mut elseif_envs = Vec::new();
    for (condition, body) in elseif_clauses {
        let condition = propagate_expr(condition, &base_env);
        let branch_env = if expr_effect(&condition).has_side_effects {
            HashMap::new()
        } else {
            base_env.clone()
        };
        let (body, env_after_body) = propagate_block(body, branch_env);
        if matches!(block_terminal_effect(&body), TerminalEffect::FallsThrough) {
            elseif_envs.push(env_after_body.clone());
        }
        propagated_elseifs.push((condition, body));
    }

    let (else_body, else_env) = match else_body {
        Some(body) => {
            let (body, env_after_body) = propagate_block(body, base_env.clone());
            (Some(body), Some(env_after_body))
        }
        None => (None, Some(base_env.clone())),
    };

    let next_env = match scalar_value(&condition) {
        Some(value) if value.truthy() => then_env,
        Some(_) => else_env.unwrap_or_default(),
        None => {
            let mut paths = Vec::new();
            if matches!(block_terminal_effect(&then_body), TerminalEffect::FallsThrough) {
                paths.push(then_env);
            }
            paths.extend(elseif_envs);
            if let Some(else_env) = else_env {
                if else_body
                    .as_ref()
                    .is_none_or(|body| matches!(block_terminal_effect(body), TerminalEffect::FallsThrough))
                {
                    paths.push(else_env);
                }
            }
            merge_constant_env_paths(paths)
        }
    };

    (
        Stmt::new(
            StmtKind::If {
                condition,
                then_body,
                elseif_clauses: propagated_elseifs,
                else_body,
            },
            span,
        ),
        next_env,
    )
}

pub(crate) fn env_after_scalar_assign(mut env: ConstantEnv, name: &str, value: &Expr) -> ConstantEnv {
    if expr_effect(value).has_side_effects {
        env.clear();
    }
    if let Some(value) = assigned_scalar_value(value) {
        env.insert(name.to_string(), value);
    } else {
        env.remove(name);
    }
    env
}

pub(crate) fn env_after_list_unpack(mut env: ConstantEnv, vars: &[String], value: &Expr) -> ConstantEnv {
    if expr_effect(value).has_side_effects {
        env.clear();
    }

    for var in vars {
        env.remove(var);
    }

    if let ExprKind::ArrayLiteral(items) = &value.kind {
        for (var, item) in vars.iter().zip(items.iter()) {
            if let Some(value) = assigned_scalar_value(item) {
                env.insert(var.clone(), value);
            }
        }
    }

    env
}

pub(crate) fn propagate_params(
    params: Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)>,
) -> Vec<(String, Option<crate::parser::ast::TypeExpr>, Option<Expr>, bool)> {
    params
        .into_iter()
        .map(|(name, type_expr, default, is_ref)| {
            (
                name,
                type_expr,
                default.map(|expr| propagate_expr(expr, &HashMap::new())),
                is_ref,
            )
        })
        .collect()
}

pub(crate) fn propagate_property(property: ClassProperty) -> ClassProperty {
    ClassProperty {
        name: property.name,
        visibility: property.visibility,
        readonly: property.readonly,
        default: property
            .default
            .map(|expr| propagate_expr(expr, &HashMap::new())),
        span: property.span,
    }
}

pub(crate) fn propagate_method(method: ClassMethod) -> ClassMethod {
    ClassMethod {
        params: propagate_params(method.params),
        body: propagate_block(method.body, HashMap::new()).0,
        ..method
    }
}

pub(crate) fn propagate_enum_case(case: EnumCaseDecl) -> EnumCaseDecl {
    EnumCaseDecl {
        name: case.name,
        value: case
            .value
            .map(|expr| propagate_expr(expr, &HashMap::new())),
        span: case.span,
    }
}
