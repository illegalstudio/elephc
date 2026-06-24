//! Purpose:
//! Implements constant propagation stmt support.
//! Tracks scalar facts through expressions, writes, simulations, and statement rewriting.
//!
//! Called from:
//! - `crate::optimize::propagate`
//!
//! Key details:
//! - Only immutable scalar facts are propagated; arrays, objects, references, and unknown calls force conservative invalidation.

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

/// Returns the input environment if no expression has side effects,
/// otherwise returns an empty environment to force conservative invalidation.
fn env_after_expr_side_effects(env: ConstantEnv, exprs: &[&Expr]) -> ConstantEnv {
    if exprs
        .iter()
        .any(|expr| expr_effect(expr).has_side_effects)
    {
        HashMap::new()
    } else {
        env
    }
}

/// Iterates through a block of statements, propagating constants and stopping early
/// when a terminal effect (return, throw, exit) is encountered.
pub(crate) fn propagate_block(body: Vec<Stmt>, mut env: ConstantEnv) -> (Vec<Stmt>, ConstantEnv) {
    // A `label:` is reachable through a `goto` even when the preceding statement terminates, so it
    // must survive. When the block contains a label, keep every statement (the `Label` arm of
    // `propagate_stmt` already clears the environment at each join point); labels are rare, so the
    // retained unreachable statements are an acceptable trade.
    let block_has_label = body.iter().any(|stmt| matches!(stmt.kind, StmtKind::Label(_)));
    let mut propagated = Vec::new();
    for stmt in body {
        let (stmt, next_env) = propagate_stmt(stmt, env);
        let stops_here =
            !block_has_label && !matches!(stmt_terminal_effect(&stmt), TerminalEffect::FallsThrough);
        propagated.push(stmt);
        env = next_env;
        if stops_here {
            break;
        }
    }
    (propagated, env)
}

/// Dispatches constant propagation for a single statement, applying expression-level
/// propagation and computing the output environment for each statement variant.
/// Returns the rewritten statement and the constant environment after the statement.
pub(crate) fn propagate_stmt(stmt: Stmt, env: ConstantEnv) -> (Stmt, ConstantEnv) {
    let span = stmt.span;
    match stmt.kind {
        StmtKind::Synthetic(stmts) => {
            let (stmts, next_env) = propagate_block(stmts, env);
            (Stmt::new(StmtKind::Synthetic(stmts), span), next_env)
        }
        StmtKind::IncludeOnceMark { label } => (
            Stmt::new(StmtKind::IncludeOnceMark { label }, span),
            HashMap::new(),
        ),
        StmtKind::IncludeOnceGuard { label, body } => {
            let (body, _) = propagate_block(body, HashMap::new());
            (
                Stmt::new(StmtKind::IncludeOnceGuard { label, body }, span),
                HashMap::new(),
            )
        }
        StmtKind::Echo(expr) => {
            let expr = propagate_expr(expr, &env);
            let next_env = env_after_expr_side_effects(env, &[&expr]);
            (Stmt::new(StmtKind::Echo(expr), span), next_env)
        }
        StmtKind::Assign { name, value } => {
            let value = propagate_expr(value, &env);
            let mut next_env = env_after_scalar_assign(env, &name, &value);
            (Stmt::new(StmtKind::Assign { name, value }, span), std::mem::take(&mut next_env))
        }
        StmtKind::RefAssign { target, source } => {
            // `$target =& $source` binds both names to one storage; a write through either changes
            // the other, so neither may be propagated as a known constant again.
            crate::optimize::mark_ref_escaped_var(&target);
            crate::optimize::mark_ref_escaped_var(&source);
            (
                Stmt::new(StmtKind::RefAssign { target, source }, span),
                HashMap::new(),
            )
        }
        StmtKind::RefAssignTarget { target, source } => {
            let target = propagate_expr(target, &env);
            // The source is aliased into an array element or object property; a write through that
            // container can change it invisibly, so suppress constant propagation for it from here.
            crate::optimize::mark_ref_escaped_var(&source);
            (
                Stmt::new(StmtKind::RefAssignTarget { target, source }, span),
                HashMap::new(),
            )
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
            let mut next_env = env_after_expr_side_effects(env, &[&index, &value]);
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
        StmtKind::NestedArrayAssign { target, value } => {
            let target = propagate_expr(target, &env);
            let value = propagate_expr(value, &env);
            (
                Stmt::new(StmtKind::NestedArrayAssign { target, value }, span),
                HashMap::new(),
            )
        }
        StmtKind::ArrayPush { array, value } => {
            let value = propagate_expr(value, &env);
            let mut next_env = env_after_expr_side_effects(env, &[&value]);
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
            value_by_ref,
            body,
        } => propagate_foreach_stmt(array, key_var, value_var, value_by_ref, body, span, env),
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
        // `goto` transfers control unconditionally; the fall-through environment it yields is never
        // observed (the next statement is unreachable until a label), so pass it through unchanged.
        StmtKind::Goto(label) => (Stmt::new(StmtKind::Goto(label), span), env),
        // A label is a join point: a `goto` elsewhere may reach it with a different variable state
        // than the straight-line predecessor, so no constant known above the label may be assumed
        // below it. Clear the environment, mirroring `IncludeOnceMark`.
        StmtKind::Label(label) => (Stmt::new(StmtKind::Label(label), span), HashMap::new()),
        StmtKind::ExprStmt(expr) => {
            let expr = propagate_expr(expr, &env);
            let next_env = if let Some(names) = unset_target_names(&expr) {
                let mut next_env = env;
                for name in names {
                    next_env.remove(&name);
                }
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
            variadic_type,
            return_type,
            body,
        } => (
            Stmt::new(
                StmtKind::FunctionDecl {
                    name,
                    params: propagate_params(params),
                    variadic,
                    variadic_type,
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
        constants,
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
                constants,
                },
                span,
            ),
            env,
        ),
        StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
            implements,
            methods,
            constants,
        } => (
            Stmt::new(
                StmtKind::EnumDecl {
                    name,
                    backing_type,
                    implements,
                    methods: methods.into_iter().map(propagate_method).collect(),
                    constants,
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
            properties,
            methods,
        constants,
        } => (
            Stmt::new(
                StmtKind::InterfaceDecl {
                    name,
                    extends,
                    properties: properties.into_iter().map(propagate_property).collect(),
                    methods: methods.into_iter().map(propagate_method).collect(),
                constants,
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
        constants,
        } => (
            Stmt::new(
                StmtKind::TraitDecl {
                    name,
                    trait_uses,
                    properties: properties.into_iter().map(propagate_property).collect(),
                    methods: methods.into_iter().map(propagate_method).collect(),
                constants,
                },
                span,
            ),
            env,
        ),
        StmtKind::PropertyAssign {
            object,
            property,
            value,
        } => {
            let object = propagate_expr(*object, &env);
            let value = propagate_expr(value, &env);
            let next_env = env_after_expr_side_effects(env, &[&object, &value]);
            (
                Stmt::new(
                    StmtKind::PropertyAssign {
                        object: Box::new(object),
                        property,
                        value,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::StaticPropertyAssign {
            receiver,
            property,
            value,
        } => {
            let value = propagate_expr(value, &env);
            let next_env = env_after_expr_side_effects(env, &[&value]);
            (
                Stmt::new(
                    StmtKind::StaticPropertyAssign {
                        receiver,
                        property,
                        value,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::StaticPropertyArrayPush {
            receiver,
            property,
            value,
        } => {
            let value = propagate_expr(value, &env);
            let next_env = env_after_expr_side_effects(env, &[&value]);
            (
                Stmt::new(
                    StmtKind::StaticPropertyArrayPush {
                        receiver,
                        property,
                        value,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::StaticPropertyArrayAssign {
            receiver,
            property,
            index,
            value,
        } => {
            let index = propagate_expr(index, &env);
            let value = propagate_expr(value, &env);
            let next_env = env_after_expr_side_effects(env, &[&index, &value]);
            (
                Stmt::new(
                    StmtKind::StaticPropertyArrayAssign {
                        receiver,
                        property,
                        index,
                        value,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::PropertyArrayPush {
            object,
            property,
            value,
        } => {
            let object = propagate_expr(*object, &env);
            let value = propagate_expr(value, &env);
            let next_env = env_after_expr_side_effects(env, &[&object, &value]);
            (
                Stmt::new(
                    StmtKind::PropertyArrayPush {
                        object: Box::new(object),
                        property,
                        value,
                    },
                    span,
                ),
                next_env,
            )
        }
        StmtKind::PropertyArrayAssign {
            object,
            property,
            index,
            value,
        } => {
            let object = propagate_expr(*object, &env);
            let index = propagate_expr(index, &env);
            let value = propagate_expr(value, &env);
            let next_env = env_after_expr_side_effects(env, &[&object, &index, &value]);
            (
                Stmt::new(
                    StmtKind::PropertyArrayAssign {
                        object: Box::new(object),
                        property,
                        index,
                        value,
                    },
                    span,
                ),
                next_env,
            )
        }
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
        StmtKind::FunctionVariantGroup { name, variants } => (
            Stmt::new(StmtKind::FunctionVariantGroup { name, variants }, span),
            env,
        ),
        StmtKind::FunctionVariantMark { name, variant } => (
            Stmt::new(StmtKind::FunctionVariantMark { name, variant }, span),
            HashMap::new(),
        ),
    }
}
