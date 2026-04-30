use super::*;

mod control;
mod declarations;
mod env;

use control::{
    propagate_do_while_stmt,
    propagate_for_stmt,
    propagate_foreach_stmt,
    propagate_if_stmt,
    propagate_ifdef_stmt,
    propagate_switch_stmt,
    propagate_try_stmt,
    propagate_while_stmt,
};
pub(crate) use declarations::propagate_params;
use declarations::{propagate_enum_case, propagate_method, propagate_property};
use env::{env_after_list_unpack, env_after_scalar_assign};

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
        StmtKind::Synthetic(stmts) => {
            let (stmts, next_env) = propagate_block(stmts, env);
            (Stmt::new(StmtKind::Synthetic(stmts), span), next_env)
        }
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
        } => propagate_ifdef_stmt(symbol, then_body, else_body, span, env),
        StmtKind::While { condition, body } => propagate_while_stmt(condition, body, span, env),
        StmtKind::DoWhile { body, condition } => propagate_do_while_stmt(body, condition, span, env),
        StmtKind::For {
            init,
            condition,
            update,
            body,
        } => propagate_for_stmt(init, condition, update, body, span, env),
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
        } => propagate_foreach_stmt(array, key_var, value_var, body, span, env),
        StmtKind::Switch {
            subject,
            cases,
            default,
        } => propagate_switch_stmt(subject, cases, default, span, env),
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
        } => propagate_try_stmt(try_body, catches, finally_body, span, env),
        StmtKind::Break(levels) => (Stmt::new(StmtKind::Break(levels), span), env),
        StmtKind::Continue(levels) => (Stmt::new(StmtKind::Continue(levels), span), env),
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
            is_final,
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
                    is_final,
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
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => (
            Stmt::new(
                StmtKind::StaticPropertyAssign {
                    receiver,
                    property,
                    value: propagate_expr(value, &env),
                },
                span,
            ),
            env,
        ),
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => (
            Stmt::new(
                StmtKind::StaticPropertyArrayPush {
                    receiver,
                    property,
                    value: propagate_expr(value, &env),
                },
                span,
            ),
            env,
        ),
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => (
            Stmt::new(
                StmtKind::StaticPropertyArrayAssign {
                    receiver,
                    property,
                    index: propagate_expr(index, &env),
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
