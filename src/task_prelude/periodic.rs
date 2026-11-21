//! Purpose:
//! Builds periodic control and Async pump declarations directly as compiler-owned AST.
//!
//! Called from:
//! - `super::facade_declarations()` and `super::group::declaration()`.
//!
//! Key details:
//! - Registration returns immediately; its pump remains a structured child until stopped.
//! - Native cadence state contains no PHP pointer and wakes interval/stop changes through poll.
//! - Scope closure, not callback completion, retires public control authority.
//! - Parallel every lowering and opaque control hardening remain separate implementation gates.

use crate::parser::ast::{BinOp, CType, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Declares native periodic operations in the global extern namespace.
pub(super) fn extern_declarations() -> Program {
    let mut declarations = vec![extern_fn("elephc_task_periodic_create", "elephc_parallel")
        .param("seconds", CType::Float).returns(CType::Int).build()];
    for name in ["fd", "drain", "remaining", "claim", "stop", "release"] {
        declarations.push(extern_fn(&format!("elephc_task_periodic_{name}"), "elephc_parallel")
            .param("id", CType::Int).returns(if name == "remaining" { CType::Float } else { CType::Int }).build());
    }
    declarations.push(extern_fn("elephc_task_periodic_set_interval", "elephc_parallel")
        .param("id", CType::Int).param("seconds", CType::Float).returns(CType::Int).build());
    declarations
}

/// Builds explicit per-registration control with scope-owned native identity.
pub(super) fn control_declaration() -> Stmt {
    let mut control = class("PeriodicControl").final_()
        .private_prop("id", TypeExpr::Int, Some(e_int(0)))
        .method(method("__construct").private().param("seconds", TypeExpr::Float)
            .param_default("nativeId", TypeExpr::Int, e_int(0)).body(vec![
            s_if(e_binop(e_var("nativeId"), BinOp::Gt, e_int(0)), vec![
                s_prop_assign(e_this(), "id", e_var("nativeId")), s_return_void(),
            ], vec![], None),
            s_assign("id", e_call("\\elephc_task_periodic_create", vec![e_var("seconds")])),
            s_if(e_binop(e_var("id"), BinOp::StrictEq, e_int(-2)),
                vec![s_throw(e_new_fq("ValueError", vec![e_str("Periodic interval must be finite positive seconds representable in nanoseconds")]))], vec![], None),
            s_if(e_binop(e_var("id"), BinOp::LtEq, e_int(0)),
                vec![s_throw(e_new_fq("Error", vec![e_str("Could not create periodic registration")]))], vec![], None),
            s_prop_assign(e_this(), "id", e_var("id")),
        ]))
        .method(method("__clone").private().returns(TypeExpr::Void).body(vec![]))
        .method(method("__serialize").returns(t_array()).body(vec![
            s_throw(e_new_fq("Error", vec![e_str("Elephc periodic control cannot be serialized")])),
        ]))
        .method(method("__unserialize").param("data", t_array()).returns(TypeExpr::Void).body(vec![
            s_expr(e_call("unset", vec![e_var("data")])),
            s_throw(e_new_fq("Error", vec![e_str("Elephc periodic control cannot be unserialized")])),
        ]))
        .method(method("__fromNativeId").private().static_().param("id", TypeExpr::Int)
            .returns(t_class("PeriodicControl")).body(vec![
                s_return(e_new("PeriodicControl", vec![e_float(0.0), e_var("id")])),
            ]))
        .method(method("__nativeId").private().returns(TypeExpr::Int).body(vec![
            s_expr(e_method_call(e_this(), "__assertOpen", vec![])), s_return(e_this_prop("id")),
        ]))
        .method(method("__assertOpen").private().returns(TypeExpr::Void).body(vec![
            s_if(e_binop(e_this_prop("id"), BinOp::LtEq, e_int(0)),
                vec![s_throw(e_new_fq("Error", vec![e_str("Elephc periodic control scope has ended")]))], vec![], None),
        ]))
        .method(method("stop").returns(TypeExpr::Void).body(vec![
            s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
            s_assign("status", e_call("\\elephc_task_periodic_stop", vec![e_this_prop("id")])),
            s_if(e_binop(e_var("status"), BinOp::StrictNotEq, e_int(1)),
                vec![s_throw(e_new_fq("Error", vec![e_str("Elephc periodic control scope has ended")]))], vec![], None),
        ]))
        .method(method("setInterval").param("seconds", TypeExpr::Float).returns(TypeExpr::Void).body(vec![
            s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
            s_assign("status", e_call("\\elephc_task_periodic_set_interval", vec![e_this_prop("id"), e_var("seconds")])),
            s_if(e_binop(e_var("status"), BinOp::StrictEq, e_int(-2)),
                vec![s_throw(e_new_fq("ValueError", vec![e_str("Periodic interval must be finite positive seconds representable in nanoseconds")]))], vec![], None),
            s_if(e_binop(e_var("status"), BinOp::Lt, e_int(0)),
                vec![s_throw(e_new_fq("Error", vec![e_str("Periodic control is stopped or unavailable")]))], vec![], None),
        ]))
        .method(method("__close").private().returns(TypeExpr::Void).body(vec![
            s_if(e_binop(e_this_prop("id"), BinOp::StrictEq, e_int(0)), vec![s_return_void()], vec![], None),
            s_assign("status", e_call("\\elephc_task_periodic_release", vec![e_this_prop("id")])),
            s_if(e_binop(e_var("status"), BinOp::Lt, e_int(0)),
                vec![s_throw(e_new_fq("Error", vec![e_str("Periodic registration has not drained")]))], vec![], None),
            s_prop_assign(e_this(), "id", e_int(0)),
        ]));
    for (method_name, native_name, result) in [
        ("__fd", "fd", TypeExpr::Int), ("__drainWake", "drain", TypeExpr::Int),
        ("__remaining", "remaining", TypeExpr::Float), ("__claim", "claim", TypeExpr::Int),
    ] {
        control = control.method(method(method_name).private().returns(result).body(vec![
            s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
            s_return(e_call(&format!("\\elephc_task_periodic_{native_name}"), vec![e_this_prop("id")])),
        ]));
    }
    control.method(method("__complete").private().returns(TypeExpr::Void).body(vec![
        s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
        s_assign("status", e_call("\\elephc_task_periodic_complete", vec![e_this_prop("id")])),
        s_if(e_binop(e_var("status"), BinOp::StrictNotEq, e_int(1)),
            vec![s_throw(e_new_fq("Error", vec![e_str("Periodic invocation completion is invalid")]))], vec![], None),
    ])).build()
}

/// Registers an Async pump without calling the user's action from the registering task.
pub(super) fn every_method() -> crate::synthetic_class::MethodBuilder {
    let pump = closure().static_()
        .param("scope", t_class("\\Elephc\\Async\\TaskGroup"))
        .param("control", t_class("PeriodicControl"))
        .param("task", t_class("callable")).param("args", t_mixed())
        .returns(TypeExpr::Void).body(vec![
            s_try(vec![s_while(e_bool(true), vec![
                s_assign("remaining", e_method_call(e_var("control"), "__remaining", vec![])),
                s_if(e_binop(e_var("remaining"), BinOp::Lt, e_float(0.0)), vec![s_break(1)], vec![], None),
                s_if(e_binop(e_var("remaining"), BinOp::Gt, e_float(0.0)), vec![
                    s_expr(e_method_call(e_var("scope"), "awaitReadable", vec![
                        e_method_call(e_var("control"), "__fd", vec![]), e_var("remaining")])),
                    s_assign("wake", e_method_call(e_var("control"), "__drainWake", vec![])),
                    s_if(e_binop(e_var("wake"), BinOp::Lt, e_int(0)),
                        vec![s_throw(e_new_fq("Error", vec![e_str("Periodic wake transport failed")]))], vec![], None),
                    s_continue(1),
                ], vec![], None),
                s_assign("claimed", e_method_call(e_var("control"), "__claim", vec![])),
                s_if(e_binop(e_var("claimed"), BinOp::Lt, e_int(0)),
                    vec![s_throw(e_new_fq("Error", vec![e_str("Periodic registration is unavailable")]))], vec![], None),
                s_if(e_binop(e_var("claimed"), BinOp::StrictEq, e_int(2)), vec![s_break(1)], vec![], None),
                s_if(e_binop(e_var("claimed"), BinOp::StrictNotEq, e_int(1)), vec![s_continue(1)], vec![], None),
                s_try(vec![s_expr(e_closure_call("task", vec![e_var("control"), e_spread(e_var("args"))]))],
                    vec![], Some(vec![s_expr(e_method_call(e_var("control"), "__complete", vec![]))])),
            ])], vec![], Some(vec![s_expr(e_method_call(e_var("control"), "stop", vec![]))])),
        ]).build();
    method("every").param("execution", t_class("Execution")).param("seconds", TypeExpr::Float)
        .param("task", t_class("callable")).variadic("args", Some(t_mixed()))
        .returns(t_class("PeriodicControl")).body(vec![
            s_assign("scope", e_method_call(e_this(), "__asyncScope", vec![])),
            s_if(e_binop(e_var("execution"), BinOp::StrictNotEq, e_class_const("Execution", "Async")),
                vec![s_throw(e_new_fq("Error", vec![e_str("Task Parallel periodic submission was not lowered")]))], vec![], None),
            s_assign("control", e_new("PeriodicControl", vec![e_var("seconds")])),
            s_prop_array_push(e_this(), "controls", e_var("control")),
            s_expr(e_method_call(e_this(), "spawn", vec![e_class_const("Execution", "Async"),
                pump, e_var("scope"), e_var("control"), e_var("task"), e_var("args")])),
            s_return(e_var("control")),
        ])
}

/// Owns the cooperative repeated-worker pump while the frozen factory returns a fresh job per claim.
pub(super) fn parallel_every_method() -> crate::synthetic_class::MethodBuilder {
    let pump = closure().static_()
        .param("scope", t_class("\\Elephc\\Async\\TaskGroup"))
        .param("parallel", t_class("\\Elephc\\Parallel\\TaskGroup"))
        .param("control", t_class("PeriodicControl"))
        .param("factory", t_class("callable")).returns(TypeExpr::Void).body(vec![
            s_try(vec![s_while(e_bool(true), vec![
                s_assign("remaining", e_method_call(e_var("control"), "__remaining", vec![])),
                s_if(e_binop(e_var("remaining"), BinOp::Lt, e_float(0.0)), vec![s_break(1)], vec![], None),
                s_if(e_binop(e_var("remaining"), BinOp::Gt, e_float(0.0)), vec![
                    s_expr(e_method_call(e_var("scope"), "awaitReadable", vec![
                        e_method_call(e_var("control"), "__fd", vec![]), e_var("remaining")])),
                    s_assign("wake", e_method_call(e_var("control"), "__drainWake", vec![])),
                    s_if(e_binop(e_var("wake"), BinOp::Lt, e_int(0)),
                        vec![s_throw(e_new_fq("Error", vec![e_str("Periodic wake transport failed")]))], vec![], None),
                    s_continue(1),
                ], vec![], None),
                s_assign("claimed", e_method_call(e_var("control"), "__claim", vec![])),
                s_if(e_binop(e_var("claimed"), BinOp::Lt, e_int(0)),
                    vec![s_throw(e_new_fq("Error", vec![e_str("Periodic registration is unavailable")]))], vec![], None),
                s_if(e_binop(e_var("claimed"), BinOp::StrictEq, e_int(2)), vec![s_break(1)], vec![], None),
                s_if(e_binop(e_var("claimed"), BinOp::StrictNotEq, e_int(1)), vec![s_continue(1)], vec![], None),
                s_assign("future", e_null()),
                s_try(vec![
                    s_assign("future", e_method_call(e_var("parallel"), "__spawnPeriodic", vec![
                        e_var("factory"), e_method_call(e_var("control"), "__nativeId", vec![])])),
                    s_expr(e_method_call(e_var("future"), "__awaitWithScheduler", vec![e_var("scope")])),
                ], vec![], Some(vec![
                    s_if(e_instance_of(e_var("future"), "\\Elephc\\Parallel\\Future"), vec![
                        s_expr(e_method_call(e_var("parallel"), "__retirePeriodicFuture", vec![e_var("future")])),
                    ], vec![], Some(vec![s_expr(e_method_call(e_var("control"), "__complete", vec![]))])),
                    s_expr(e_call("unset", vec![e_var("future")])),
                ])),
            ])], vec![], Some(vec![s_expr(e_method_call(e_var("control"), "stop", vec![]))])),
        ]).build();
    method("__registerParallelEvery").param("control", t_class("PeriodicControl"))
        .param("factory", t_class("callable")).returns(TypeExpr::Void).body(vec![
            s_assign("scope", e_method_call(e_this(), "__asyncScope", vec![])),
            s_assign("parallel", e_method_call(e_this(), "__parallelScope", vec![])),
            s_expr(e_method_call(e_this(), "spawn", vec![e_class_const("Execution", "Async"),
                pump, e_var("scope"), e_var("parallel"), e_var("control"), e_var("factory")])),
        ])
}

/// Creates and records a registration after all user input expressions have been evaluated.
pub(super) fn create_control_method() -> crate::synthetic_class::MethodBuilder {
    method("__createPeriodicControl").param("seconds", TypeExpr::Float)
        .returns(t_class("PeriodicControl")).body(vec![
            s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
            s_assign("control", e_new("PeriodicControl", vec![e_var("seconds")])),
            s_prop_array_push(e_this(), "controls", e_var("control")),
            s_return(e_var("control")),
        ])
}
