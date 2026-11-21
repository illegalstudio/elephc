//! Purpose:
//! Builds the unified TaskGroup submission and structured-drain facade.
//!
//! Called from:
//! - `super::facade_declarations()`.
//!
//! Key details:
//! - Parallel submission is replaced at the checked call site by dedicated EIR lowering.
//! - The source fallback never executes a Parallel task synchronously.
//! - Every returned Task is tracked and materialized before executor scopes close.

use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds the shared scope API while retaining the existing execution engines underneath.
pub(super) fn declaration() -> Stmt {
    let mut group = class("TaskGroup").final_()
        .private_prop("asyncScope", t_nullable(t_class("\\Elephc\\Async\\TaskGroup")), Some(e_null()))
        .private_prop("parallelScope", t_nullable(t_class("\\Elephc\\Parallel\\TaskGroup")), Some(e_null()))
        .private_prop("tasks", t_array(), Some(e_array(vec![])))
        .private_prop("controls", TypeExpr::Array(Box::new(t_class("PeriodicControl"))), Some(e_array(vec![])))
        .private_prop("closed", TypeExpr::Bool, Some(e_bool(false)))
        .method(method("__construct")
            .param("asyncScope", t_class("\\Elephc\\Async\\TaskGroup"))
            .param("parallelScope", t_class("\\Elephc\\Parallel\\TaskGroup"))
            .body(vec![
                s_prop_assign(e_this(), "asyncScope", e_var("asyncScope")),
                s_prop_assign(e_this(), "parallelScope", e_var("parallelScope")),
            ]))
        .method(method("__assertOpen").returns(TypeExpr::Void).body(vec![
            s_if(e_this_prop("closed"),
                vec![s_throw(e_new_fq("Error", vec![e_str("Elephc TaskGroup scope has ended")]))], vec![], None),
        ]))
        .method(method("__asyncScope").returns(t_class("\\Elephc\\Async\\TaskGroup")).body(vec![
            s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
            s_assign("scope", e_this_prop("asyncScope")),
            s_if(e_binop(e_var("scope"), BinOp::StrictEq, e_null()),
                vec![s_throw(e_new_fq("Error", vec![e_str("TaskGroup has no Async coordinator")]))], vec![], None),
            s_return(e_var("scope")),
        ]))
        .method(method("__parallelScope").returns(t_class("\\Elephc\\Parallel\\TaskGroup")).body(vec![
            s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
            s_assign("scope", e_this_prop("parallelScope")),
            s_if(e_binop(e_var("scope"), BinOp::StrictEq, e_null()),
                vec![s_throw(e_new_fq("Error", vec![e_str("TaskGroup has no Parallel executor")]))], vec![], None),
            s_return(e_var("scope")),
        ]))
        .method(method("__record").param("task", t_class("Task")).returns(TypeExpr::Void).body(vec![
            s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
            s_prop_array_push(e_this(), "tasks", e_var("task")),
        ]))
        .method(method("spawn").param("execution", t_class("Execution"))
            .param("task", t_class("callable")).variadic("args", Some(t_mixed()))
            .returns(t_class("Task")).body(vec![
                s_assign("scope", e_method_call(e_this(), "__asyncScope", vec![])),
                s_if(e_binop(e_var("execution"), BinOp::StrictNotEq, e_class_const("Execution", "Async")),
                    vec![s_throw(e_new_fq("Error", vec![e_str("Task Parallel submission was not lowered")]))], vec![], None),
                s_assign("local", e_method_call(e_var("scope"), "spawn", vec![
                    e_var("task"), crate::parser::ast::Expr::new(
                        crate::parser::ast::ExprKind::Spread(Box::new(e_var("args"))), crate::span::Span::dummy()),
                ])),
                s_assign("handle", e_new("Task", vec![e_var("scope"), e_var("local"), e_null()])),
                s_expr(e_method_call(e_this(), "__record", vec![e_var("handle")])),
                s_return(e_var("handle")),
            ]))
        .method(method("cancel").param_default("reason", t_nullable(t_class("\\Throwable")), e_null())
            .returns(TypeExpr::Void).body(vec![
                s_foreach(e_this_prop("controls"), None, "control", vec![
                    s_expr(e_method_call(e_var("control"), "stop", vec![])),
                ]),
                s_expr(e_method_call(e_method_call(e_this(), "__asyncScope", vec![]), "cancel", vec![e_var("reason")])),
                s_expr(e_method_call(e_method_call(e_this(), "__parallelScope", vec![]), "cancel", vec![e_var("reason")])),
            ]))
        .method(method("__drain").returns(TypeExpr::Void).body(vec![
            s_assign("failure", e_null()),
            s_assign("index", e_int(0)),
            // Awaiting a child may append descendants. Read the live collection,
            // not a foreach snapshot, before closing result and scope authority.
            s_while(e_binop(e_var("index"), BinOp::Lt,
                e_call("\\count", vec![e_this_prop("tasks")])), vec![
                s_assign("task", e_index(e_this_prop("tasks"), e_var("index"))),
                s_expr(e_method_call(e_var("task"), "__settle", vec![])),
                s_assign("error", e_method_call(e_var("task"), "__unobservedFailure", vec![])),
                s_if(e_binop(e_var("failure"), BinOp::StrictEq, e_null()),
                    vec![s_assign("failure", e_var("error"))], vec![], None),
                s_assign("index", e_binop(e_var("index"), BinOp::Add, e_int(1))),
            ]),
            s_prop_assign(e_this(), "tasks", e_array(vec![])),
            s_foreach(e_this_prop("controls"), None, "control", vec![
                s_expr(e_method_call(e_var("control"), "__close", vec![])),
            ]),
            s_prop_assign(e_this(), "controls", e_array(vec![])),
            s_prop_assign(e_this(), "asyncScope", e_null()),
            s_prop_assign(e_this(), "parallelScope", e_null()),
            s_prop_assign(e_this(), "closed", e_bool(true)),
            s_if(e_instance_of(e_var("failure"), "\\Throwable"),
                vec![s_throw(e_var("failure"))], vec![], None),
        ]));
    for name in ["reschedule", "sleep"] {
        let mut declaration = method(name).returns(TypeExpr::Void);
        let args = if name == "sleep" {
            declaration = declaration.param("seconds", TypeExpr::Float);
            vec![e_var("seconds")]
        } else { vec![] };
        group = group.method(declaration.body(vec![s_expr(e_method_call(
            e_method_call(e_this(), "__asyncScope", vec![]), name, args))]));
    }
    group.method(super::periodic::every_method())
        .method(super::periodic::create_control_method())
        .method(super::periodic::parallel_every_method()).build()
}
