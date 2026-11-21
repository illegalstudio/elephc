//! Purpose:
//! Builds the single public root boundary owning both execution engines.
//!
//! Called from:
//! - `super::facade_declarations()`.
//!
//! Key details:
//! - Executor scopes stay live until the unified group has materialized every outcome.
//! - Closure captures here are compiler machinery, not required client API syntax.

use crate::parser::ast::{BinOp, Stmt};
use crate::synthetic_class::*;

/// Builds Task::run with explicit drain and cancellation on root failure.
pub(super) fn declaration() -> Stmt {
    let inner = closure().static_().param("asyncScope", t_class("\\Elephc\\Async\\TaskGroup"))
        .captures("parallelScope").captures("body").returns(t_mixed()).body(vec![
            s_assign("group", e_new("TaskGroup", vec![e_var("asyncScope"), e_var("parallelScope")])),
            s_assign("failure", e_null()),
            s_assign("result", e_null()),
            s_try(vec![s_assign("result", e_closure_call("body", vec![e_var("group")]))],
                vec![(vec!["\\Throwable"], Some("caught"), vec![
                    s_assign("failure", e_var("caught")),
                    s_expr(e_method_call(e_var("group"), "cancel", vec![e_var("caught")])),
                ])], None),
            s_try(vec![s_expr(e_method_call(e_var("group"), "__drain", vec![]))],
                vec![(vec!["\\Throwable"], Some("drainFailure"), vec![
                    s_if(e_binop(e_var("failure"), BinOp::StrictEq, e_null()),
                        vec![s_assign("failure", e_var("drainFailure"))], vec![], None),
                ])], None),
            s_if(e_instance_of(e_var("failure"), "\\Throwable"),
                vec![s_throw(e_var("failure"))], vec![], None),
            s_return(e_var("result")),
        ]).build();
    let outer = closure().static_().param("parallelScope", t_class("\\Elephc\\Parallel\\TaskGroup"))
        .captures("body").returns(t_mixed())
        .body(vec![s_return(e_call("\\Elephc\\Async\\run", vec![inner]))]).build();
    function("run").param("body", t_class("callable")).returns(t_mixed())
        .body(vec![s_return(e_call("\\Elephc\\Parallel\\run", vec![outer]))]).build()
}

