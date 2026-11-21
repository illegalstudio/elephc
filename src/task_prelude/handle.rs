//! Purpose:
//! Builds the unified parent-owned task result and worker failure facade.
//!
//! Called from:
//! - `super::facade_declarations()` before frontend checking.
//!
//! Key details:
//! - Settlement clears executor/scope references before terminal results escape run.
//! - Legacy engine handles are implementation details during the namespace migration.

use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds the parent-side failure wrapper used for isolated worker errors.
pub(super) fn failure_declaration() -> Stmt {
    class("TaskFailure").final_().extends("\\RuntimeException")
        .method(method("__construct").param("source", t_class("\\Throwable"))
            .body(vec![
                s_prop_assign(e_this(), "message", e_method_call(e_var("source"), "getMessage", vec![])),
                s_prop_assign(e_this(), "code", e_method_call(e_var("source"), "getCode", vec![])),
                s_prop_assign(e_this(), "previous", e_var("source")),
            ])).build()
}

/// Builds a task handle whose cached terminal outcome is independent of its closed scope.
pub(super) fn task_declaration() -> Stmt {
    class("Task").final_()
        .private_prop("scope", t_nullable(t_class("\\Elephc\\Async\\TaskGroup")), Some(e_null()))
        .private_prop("local", t_nullable(t_class("\\Elephc\\Async\\Awaitable")), Some(e_null()))
        .private_prop("remote", t_nullable(t_class("\\Elephc\\Parallel\\Future")), Some(e_null()))
        .private_prop("completed", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("observed", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("value", t_mixed(), Some(e_null()))
        .private_prop("error", t_nullable(t_class("\\Throwable")), Some(e_null()))
        .method(method("__construct")
            .param("scope", t_class("\\Elephc\\Async\\TaskGroup"))
            .param("local", t_nullable(t_class("\\Elephc\\Async\\Awaitable")))
            .param("remote", t_nullable(t_class("\\Elephc\\Parallel\\Future")))
            .body(vec![
                s_prop_assign(e_this(), "scope", e_var("scope")),
                s_prop_assign(e_this(), "local", e_var("local")),
                s_prop_assign(e_this(), "remote", e_var("remote")),
            ]))
        .method(method("await").returns(t_mixed()).body(vec![
            s_expr(e_method_call(e_this(), "__settle", vec![])),
            s_prop_assign(e_this(), "observed", e_bool(true)),
            s_assign("error", e_this_prop("error")),
            s_if(e_instance_of(e_var("error"), "\\Throwable"),
                vec![s_throw(e_var("error"))], vec![], None),
            s_return(e_this_prop("value")),
        ]))
        .method(method("isComplete").returns(TypeExpr::Bool).body(vec![
            s_if(e_this_prop("completed"), vec![s_return(e_bool(true))], vec![], None),
            s_assign("local", e_this_prop("local")),
            s_if(e_binop(e_var("local"), BinOp::StrictNotEq, e_null()),
                vec![s_return(e_method_call(e_var("local"), "isComplete", vec![]))], vec![], None),
            s_assign("remote", e_this_prop("remote")),
            s_if(e_binop(e_var("remote"), BinOp::StrictNotEq, e_null()),
                vec![s_return(e_method_call(e_var("remote"), "isComplete", vec![]))], vec![], None),
            s_return(e_bool(false)),
        ]))
        .method(method("__settle").returns(TypeExpr::Void).body(vec![
            s_if(e_this_prop("completed"), vec![s_return_void()], vec![], None),
            s_assign("local", e_this_prop("local")),
            s_assign("remote", e_this_prop("remote")),
            s_assign("scope", e_this_prop("scope")),
            s_try(vec![
                s_if(e_binop(e_var("local"), BinOp::StrictNotEq, e_null()),
                    vec![s_prop_assign(e_this(), "value", e_method_call(e_var("local"), "await", vec![]))],
                    vec![], Some(vec![
                        s_if(e_binop(e_var("remote"), BinOp::StrictEq, e_null()),
                            vec![s_throw(e_new_fq("Error", vec![e_str("Task has no execution handle")]))], vec![], None),
                        s_if(e_binop(e_var("scope"), BinOp::StrictEq, e_null()),
                            vec![s_throw(e_new_fq("Error", vec![e_str("Task has no execution scope")]))], vec![], None),
                        s_prop_assign(e_this(), "value", e_method_call(e_var("remote"),
                            "__awaitWithScheduler", vec![e_var("scope")])),
                    ])),
            ], vec![(vec!["\\Throwable"], Some("caught"), vec![
                s_if(e_instance_of(e_var("caught"), "\\Elephc\\Parallel\\TaskFailure"),
                    vec![s_prop_assign(e_this(), "error", e_new("TaskFailure", vec![e_var("caught")]))],
                    vec![], Some(vec![s_prop_assign(e_this(), "error", e_var("caught"))])),
            ])], Some(vec![
                s_prop_assign(e_this(), "completed", e_bool(true)),
                s_prop_assign(e_this(), "local", e_null()),
                s_prop_assign(e_this(), "remote", e_null()),
                s_prop_assign(e_this(), "scope", e_null()),
            ])),
        ]))
        .method(method("__unobservedFailure").returns(t_nullable(t_class("\\Throwable"))).body(vec![
            s_assign("error", e_this_prop("error")),
            s_if(e_binop(e_this_prop("observed"), BinOp::Or,
                e_instance_of(e_var("error"), "\\Elephc\\Async\\CancelledException")),
                vec![s_return(e_null())], vec![], None),
            s_return(e_var("error")),
        ]))
        .build()
}
