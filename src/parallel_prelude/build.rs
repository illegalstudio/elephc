//! Purpose:
//! Generated AST builders for the Parallel extern contract and public PHP surface.
//!
//! Called from:
//! - `crate::parallel_prelude::inject_if_used()`.
//!
//! Key details:
//! - Generated from `source.php` through `synthetic_class::transcribe_split`.
//! - Keep this file structurally identical to the parse-parity oracle.

use super::*;

/// `elephc_parallel_worker_active` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_worker_active() -> Stmt {
    extern_fn("elephc_parallel_worker_active", "elephc_parallel")
        .returns(CType::Int)
        .build()
}

/// Declares the native operation entering a parent Parallel root scope.
fn decl_extern_elephc_parallel_parent_scope_enter() -> Stmt {
    extern_fn("elephc_parallel_parent_scope_enter", "elephc_parallel")
        .returns(CType::Int)
        .build()
}

/// Declares the native operation leaving a parent Parallel root scope.
fn decl_extern_elephc_parallel_parent_scope_leave() -> Stmt {
    extern_fn("elephc_parallel_parent_scope_leave", "elephc_parallel")
        .returns(CType::Int)
        .build()
}

/// Declares the native probe used to detect an already-active parent Parallel scope.
fn decl_extern_elephc_parallel_parent_scope_active() -> Stmt {
    extern_fn("elephc_parallel_parent_scope_active", "elephc_parallel")
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_create_php_serialized` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_create_php_serialized() -> Stmt {
    extern_fn("elephc_parallel_job_create_php_serialized", "elephc_parallel")
        .param("source", CType::Ptr)
        .param("sourceLen", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_input_php_prepare` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_input_php_prepare() -> Stmt {
    extern_fn("elephc_parallel_job_input_php_prepare", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_complete_php_serialized` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_complete_php_serialized() -> Stmt {
    extern_fn("elephc_parallel_job_complete_php_serialized", "elephc_parallel")
        .param("jobId", CType::Int)
        .param("source", CType::Ptr)
        .param("sourceLen", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_fail_php_serialized` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_fail_php_serialized() -> Stmt {
    extern_fn("elephc_parallel_job_fail_php_serialized", "elephc_parallel")
        .param("jobId", CType::Int)
        .param("source", CType::Ptr)
        .param("sourceLen", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_result_php_prepare` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_result_php_prepare() -> Stmt {
    extern_fn("elephc_parallel_job_result_php_prepare", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_failure_php_prepare` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_failure_php_prepare() -> Stmt {
    extern_fn("elephc_parallel_job_failure_php_prepare", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_observe_failure` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_observe_failure() -> Stmt {
    extern_fn("elephc_parallel_job_observe_failure", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_failure_observed` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_failure_observed() -> Stmt {
    extern_fn("elephc_parallel_job_failure_observed", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_php_blob_ptr` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_php_blob_ptr() -> Stmt {
    extern_fn("elephc_parallel_php_blob_ptr", "elephc_parallel")
        .returns(CType::Ptr)
        .build()
}

/// `elephc_parallel_php_blob_len` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_php_blob_len() -> Stmt {
    extern_fn("elephc_parallel_php_blob_len", "elephc_parallel")
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_php_blob_release` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_php_blob_release() -> Stmt {
    extern_fn("elephc_parallel_php_blob_release", "elephc_parallel")
        .returns(CType::Void)
        .build()
}

/// `elephc_parallel_php_buffer_alloc` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_php_buffer_alloc() -> Stmt {
    extern_fn("elephc_parallel_php_buffer_alloc", "elephc_parallel")
        .param("length", CType::Int)
        .returns(CType::Ptr)
        .build()
}

/// `elephc_parallel_php_buffer_free` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_php_buffer_free() -> Stmt {
    extern_fn("elephc_parallel_php_buffer_free", "elephc_parallel")
        .param("pointer", CType::Ptr)
        .param("length", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_parallel_job_phase` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_phase() -> Stmt {
    extern_fn("elephc_parallel_job_phase", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_wait` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_wait() -> Stmt {
    extern_fn("elephc_parallel_job_wait", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_cancel` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_cancel() -> Stmt {
    extern_fn("elephc_parallel_job_cancel", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_cancellation_requested` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_cancellation_requested() -> Stmt {
    extern_fn("elephc_parallel_job_cancellation_requested", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_job_release` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_job_release() -> Stmt {
    extern_fn("elephc_parallel_job_release", "elephc_parallel")
        .param("jobId", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_completion_generation` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_completion_generation() -> Stmt {
    extern_fn("elephc_parallel_completion_generation", "elephc_parallel")
        .returns(CType::Int)
        .build()
}

/// `elephc_parallel_completion_wait` — transcribed from the PHP form.
fn decl_extern_elephc_parallel_completion_wait() -> Stmt {
    extern_fn("elephc_parallel_completion_wait", "elephc_parallel")
        .param("observed", CType::Int)
        .returns(CType::Int)
        .build()
}

/// Builds case-insensitive detection of Fiber::suspend callable pairs for worker guards.
fn method_is_fiber_suspend_callable_array() -> MethodBuilder {
    let is_fiber_class_name = e_binop(
        e_binop(
            e_call(
                "strcasecmp",
                vec![e_index(e_var("callback"), e_int(0)), e_str("Fiber")],
            ),
            BinOp::StrictEq,
            e_int(0),
        ),
        BinOp::Or,
        e_binop(
            e_call(
                "strcasecmp",
                vec![e_index(e_var("callback"), e_int(0)), e_str("\\Fiber")],
            ),
            BinOp::StrictEq,
            e_int(0),
        ),
    );
    let is_fiber_class = e_binop(
        e_binop(
            e_call(
                "is_string",
                vec![e_index(e_var("callback"), e_int(0))],
            ),
            BinOp::And,
            is_fiber_class_name,
        ),
        BinOp::Or,
        e_instance_of(e_index(e_var("callback"), e_int(0)), "\\Fiber"),
    );
    let is_suspend_method = e_binop(
        e_call(
            "strcasecmp",
            vec![e_index(e_var("callback"), e_int(1)), e_str("suspend")],
        ),
        BinOp::StrictEq,
        e_int(0),
    );
    let is_callable_pair = e_binop(
        e_binop(
            e_binop(
                e_binop(
                    e_call("is_array", vec![e_var("callback")]),
                    BinOp::And,
                    e_binop(
                        e_call("count", vec![e_var("callback")]),
                        BinOp::StrictEq,
                        e_int(2),
                    ),
                ),
                BinOp::And,
                e_call(
                    "is_string",
                    vec![e_index(e_var("callback"), e_int(1))],
                ),
            ),
            BinOp::And,
            is_fiber_class,
        ),
        BinOp::And,
        is_suspend_method,
    );
    method("__isFiberSuspendCallableArray")
        .private()
        .static_()
        .param("callback", t_array())
        .returns(TypeExpr::Bool)
        .body(vec![s_return(is_callable_pair)])
}

/// Builds the fail-closed placeholder that Parallel callable-guard lowering must replace.
fn method_assert_fiber_suspend_callable_guard() -> MethodBuilder {
    method("__assertFiberSuspendCallableGuard")
        .private()
        .static_()
        .param("callback", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_call("unset", vec![e_var("callback")])),
            s_throw(e_new_fq(
                "Error",
                vec![e_str("Elephc Parallel callable guard was not lowered")],
            )),
        ])
}

/// Builds a serialization refusal for runtime-owned Parallel scope, outcome or failure handles.
fn method_reject_serialization(class_name: &str) -> MethodBuilder {
    method("__serialize")
        .returns(t_array())
        .body(vec![s_throw(e_new_fq(
            "Error",
            vec![e_str(&format!("Elephc Parallel {class_name} cannot be serialized"))],
        ))])
}

/// Builds an unserialization refusal rather than reconstructing a detached runtime handle.
fn method_reject_unserialization(class_name: &str) -> MethodBuilder {
    method("__unserialize")
        .param("data", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_call("unset", vec![e_var("data")])),
            s_throw(e_new_fq(
                "Error",
                vec![e_str(&format!("Elephc Parallel {class_name} cannot be unserialized"))],
            )),
        ])
}

/// Builds the Parallel namespace containing failures, futures, groups and the structured run root.
fn decl_stmt_bootstrap_1() -> Stmt {
    s_namespace("Elephc\\Parallel", vec![
        enum_decl("TaskFailureKind")
            .backed(TypeExpr::Int)
            .case_value("PhpThrowable", e_int(1))
            .case_value("PhpFatal", e_int(2))
            .case_value("WorkerPanic", e_int(3))
            .case_value("ContextUnavailable", e_int(4))
            .case_value("ArenaExhausted", e_int(5))
            .case_value("TransferEncode", e_int(6))
            .case_value("TransferDecode", e_int(7))
            .case_value("Infrastructure", e_int(8))
            .build(),
        class("TaskFailure")
            .final_()
            .extends("\\RuntimeException")
            .private_prop("kind", t_class("TaskFailureKind"), None)
            .private_prop("remoteClass", t_nullable(TypeExpr::Str), None)
            .private_prop("remoteFile", t_nullable(TypeExpr::Str), None)
            .private_prop("remoteLine", TypeExpr::Int, None)
            .private_prop("remoteFrames", t_array(), None)
            .method(
                method("__construct")
                    .param("kind", t_class("TaskFailureKind"))
                    .param("message", TypeExpr::Str)
                    .param_default("code", TypeExpr::Int, e_int(0))
                    .param_default("remoteClass", t_nullable(TypeExpr::Str), e_null())
                    .param_default("remoteFile", t_nullable(TypeExpr::Str), e_null())
                    .param_default("remoteLine", TypeExpr::Int, e_int(0))
                    .param_default("remoteFrames", t_array(), e_array(vec![]))
                    .body(vec![
                        s_prop_assign(e_this(), "kind", e_var("kind")),
                        s_prop_assign(e_this(), "message", e_var("message")),
                        s_prop_assign(e_this(), "code", e_var("code")),
                        s_prop_assign(e_this(), "remoteClass", e_var("remoteClass")),
                        s_prop_assign(e_this(), "remoteFile", e_var("remoteFile")),
                        s_prop_assign(e_this(), "remoteLine", e_var("remoteLine")),
                        s_prop_assign(e_this(), "remoteFrames", e_var("remoteFrames")),
                    ]),
            )
            .method(
                method("kind")
                    .returns(t_class("TaskFailureKind"))
                    .body(vec![
                        s_return(e_this_prop("kind")),
                    ]),
            )
            .method(
                method("remoteClass")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_this_prop("remoteClass")),
                    ]),
            )
            .method(
                method("remoteFile")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_this_prop("remoteFile")),
                    ]),
            )
            .method(
                method("remoteLine")
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_return(e_this_prop("remoteLine")),
                    ]),
            )
            .method(
                method("remoteFrames")
                    .returns(t_array())
                    .body(vec![
                        s_return(e_this_prop("remoteFrames")),
                    ]),
            )
            .build(),
        class("TaskGroupFailure")
            .final_()
            .extends("\\RuntimeException")
            .private_prop("taskFailures", t_array(), None)
            .private_prop("rootFailure", t_nullable(t_class("\\Throwable")), None)
            .method(
                method("__construct")
                    .param("failures", t_array())
                    .param_default("rootFailure", t_nullable(t_class("\\Throwable")), e_null())
                    .body(vec![
                        s_prop_assign(e_this(), "message", e_str("One or more Elephc Parallel tasks failed")),
                        s_prop_assign(e_this(), "code", e_int(0)),
                        s_prop_assign(e_this(), "taskFailures", e_var("failures")),
                        s_prop_assign(e_this(), "rootFailure", e_var("rootFailure")),
                    ]),
            )
            .method(
                method("failures")
                    .returns(t_array())
                    .body(vec![
                        s_return(e_this_prop("taskFailures")),
                    ]),
            )
            .method(
                method("rootFailure")
                    .returns(t_nullable(t_class("\\Throwable")))
                    .body(vec![
                        s_return(e_this_prop("rootFailure")),
                    ]),
            )
            .build(),
        class("Future")
            .final_()
            .private_prop("jobId", TypeExpr::Int, None)
            .private_prop("settled", TypeExpr::Bool, Some(e_bool(false)))
            .private_prop("failed", TypeExpr::Bool, Some(e_bool(false)))
            .private_prop("observed", TypeExpr::Bool, Some(e_bool(false)))
            .private_prop("cancelled", TypeExpr::Bool, Some(e_bool(false)))
            .private_prop("resultBytes", TypeExpr::Str, None)
            .private_prop("failure", t_class("TaskFailure"), None)
            .private_prop("scope", t_nullable(t_class("TaskGroup")), Some(e_null()))
            .method(
                method("__construct")
                    .private()
                    .param("jobId", TypeExpr::Int)
                    .body(vec![
                        s_prop_assign(e_this(), "jobId", e_var("jobId")),
                    ]),
            )
            .method(method("__clone").private().returns(TypeExpr::Void).body(vec![]))
            .method(
                method("__takePreparedBytes")
                    .private()
                    .static_()
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_assign("length", e_call("\\elephc_parallel_php_blob_len", vec![])),
                        s_assign("bytes", e_call("\\__elephc_ptr_read_string", vec![e_call("\\elephc_parallel_php_blob_ptr", vec![]), e_var("length")])),
                        s_expr(e_call("\\elephc_parallel_php_blob_release", vec![])),
                        s_return(e_var("bytes")),
                    ]),
            )
            .method(
                method("__takePreparedValue")
                    .private()
                    .static_()
                    .returns(t_mixed())
                    .body(vec![
                        s_assign("bytes", e_self_call("__takePreparedBytes", vec![])),
                        s_assign("value", e_call("unserialize", vec![e_var("bytes")])),
                        s_expr(e_call("unset", vec![e_var("bytes")])),
                        s_return(e_var("value")),
                    ]),
            )
            .method(method_reject_serialization("Future"))
            .method(method_reject_unserialization("Future"))
            .method(
                method("__destruct")
                    .body(vec![
                        s_if(
                            e_not(e_call("isset", vec![e_this_prop("jobId")])),
                            vec![s_return_void()],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(
                                e_binop(e_this_prop("settled"), BinOp::And, e_not(e_this_prop("failed"))),
                                BinOp::And,
                                e_not(e_this_prop("cancelled")),
                            ),
                            vec![
                                s_expr(e_call("unset", vec![e_this_prop("resultBytes")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_this_prop("failed"),
                            vec![
                                s_expr(e_call("unset", vec![e_this_prop("failure")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_this_prop("jobId"), BinOp::StrictNotEq, e_int(0)),
                            vec![
                                s_expr(e_call("\\elephc_parallel_job_release", vec![e_this_prop("jobId")])),
                                s_prop_assign(e_this(), "jobId", e_int(0)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("join")
                    .returns(t_mixed())
                    .body(vec![
                        s_expr(e_method_call(e_this(), "settle", vec![])),
                        s_if(
                            e_this_prop("cancelled"),
                            vec![
                                s_throw(e_new_fq("Elephc\\Async\\CancelledException", vec![e_str("Elephc Parallel task cancelled")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_this_prop("failed"),
                            vec![
                                s_assign("scope", e_this_prop("scope")),
                                s_if(
                                    e_binop(e_var("scope"), BinOp::StrictNotEq, e_null()),
                                    vec![s_expr(e_method_call(e_var("scope"), "cancel", vec![]))],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_call("unset", vec![e_var("scope")])),
                                s_prop_assign(e_this(), "observed", e_bool(true)),
                                s_if(
                                    e_binop(e_this_prop("jobId"), BinOp::StrictNotEq, e_int(0)),
                                    vec![
                                        s_expr(e_call("\\elephc_parallel_job_observe_failure", vec![e_this_prop("jobId")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_throw(e_this_prop("failure")),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("result", e_call("unserialize", vec![e_this_prop("resultBytes")])),
                        s_return(e_var("result")),
                    ]),
            )
            .method(
                method("isComplete")
                    .returns(TypeExpr::Bool)
                    .body(vec![
                        s_if(
                            e_binop(
                                e_this_prop("settled"),
                                BinOp::Or,
                                e_binop(e_this_prop("jobId"), BinOp::StrictEq, e_int(0)),
                            ),
                            vec![
                                s_return(e_bool(true)),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_binop(e_call("\\elephc_parallel_job_phase", vec![e_this_prop("jobId")]), BinOp::GtEq, e_int(3))),
                    ]),
            )
            .method(
                method("__scopeFailure")
                    .private()
                    .returns(t_nullable(t_class("TaskFailure")))
                    .body(vec![
                        s_expr(e_method_call(e_this(), "settle", vec![])),
                        s_if(
                            e_binop(e_this_prop("failed"), BinOp::And, e_binop(e_this_prop("observed"), BinOp::Or, e_binop(e_call("\\elephc_parallel_job_failure_observed", vec![e_this_prop("jobId")]), BinOp::StrictEq, e_int(1)))),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_ternary(e_this_prop("failed"), e_this_prop("failure"), e_null())),
                    ]),
            )
            .method(
                method("__phase")
                    .private()
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_if(
                            e_this_prop("settled"),
                            vec![s_return(e_ternary(e_this_prop("failed"), e_int(4), e_ternary(e_this_prop("cancelled"), e_int(5), e_int(3))))],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_this_prop("jobId"), BinOp::StrictEq, e_int(0)),
                            vec![s_return(e_int(4))],
                            vec![],
                            None,
                        ),
                        s_return(e_call(
                            "\\elephc_parallel_job_phase",
                            vec![e_this_prop("jobId")],
                        )),
                    ]),
            )
            .method(
                method("__cancel")
                    .private()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_binop(
                                e_not(e_this_prop("settled")),
                                BinOp::And,
                                e_binop(e_this_prop("jobId"), BinOp::StrictNotEq, e_int(0)),
                            ),
                            vec![
                                s_expr(e_call("\\elephc_parallel_job_cancel", vec![e_this_prop("jobId")])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("__finishScope")
                    .private()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_expr(e_method_call(e_this(), "settle", vec![])),
                        s_prop_assign(e_this(), "scope", e_null()),
                        s_expr(e_call("\\elephc_parallel_job_release", vec![e_this_prop("jobId")])),
                        s_prop_assign(e_this(), "jobId", e_int(0)),
                    ]),
            )
            .method(
                method("__attachScope")
                    .private()
                    .param("scope", t_class("TaskGroup"))
                    .returns(TypeExpr::Void)
                    .body(vec![s_prop_assign(e_this(), "scope", e_var("scope"))]),
            )
            .method(
                method("settle")
                    .private()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_this_prop("settled"),
                            vec![
                                s_return_void(),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("phase", e_call("\\elephc_parallel_job_wait", vec![e_this_prop("jobId")])),
                        s_if(
                            e_binop(e_var("phase"), BinOp::StrictEq, e_int(3)),
                            vec![
                                s_assign("status", e_call("\\elephc_parallel_job_result_php_prepare", vec![e_this_prop("jobId")])),
                                s_if(
                                    e_binop(e_var("status"), BinOp::StrictEq, e_int(0)),
                                    vec![
                                        s_assign("length", e_call("\\elephc_parallel_php_blob_len", vec![])),
                                        s_prop_assign(e_this(), "resultBytes", e_call("\\__elephc_ptr_read_string", vec![e_call("\\elephc_parallel_php_blob_ptr", vec![]), e_var("length")])),
                                        s_expr(e_call("\\elephc_parallel_php_blob_release", vec![])),
                                    ],
                                    vec![],
                                    Some(vec![
                                        s_prop_assign(e_this(), "failed", e_bool(true)),
                                        s_prop_assign(e_this(), "failure", e_new("TaskFailure", vec![e_class_const("TaskFailureKind", "TransferDecode"), e_str("Elephc Parallel could not decode the worker result")])),
                                    ]),
                                ),
                            ],
                            vec![
                            (e_binop(e_var("phase"), BinOp::StrictEq, e_int(4)), vec![
                                s_assign("status", e_call("\\elephc_parallel_job_failure_php_prepare", vec![e_this_prop("jobId")])),
                                s_if(
                                    e_binop(e_var("status"), BinOp::StrictEq, e_int(0)),
                                    vec![
                                        s_assign("fields", e_self_call("__takePreparedValue", vec![])),
                                        s_assign("remoteFrames", e_array(vec![])),
                                        s_foreach(e_index(e_var("fields"), e_int(6)), None, "remoteFrame", vec![
                                            s_array_push("remoteFrames", e_var("remoteFrame")),
                                        ]),
                                        s_expr(e_call("unset", vec![e_var("remoteFrame")])),
                                        s_prop_assign(e_this(), "failed", e_bool(true)),
                                        s_prop_assign(e_this(), "failure", e_new("TaskFailure", vec![e_static_call("TaskFailureKind", "from", vec![e_index(e_var("fields"), e_int(0))]), e_index(e_var("fields"), e_int(2)), e_index(e_var("fields"), e_int(3)), e_index(e_var("fields"), e_int(1)), e_index(e_var("fields"), e_int(4)), e_index(e_var("fields"), e_int(5)), e_var("remoteFrames")])),
                                        s_expr(e_call("unset", vec![e_var("fields")])),
                                        s_expr(e_call("unset", vec![e_var("remoteFrames")])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_prop_assign(e_this(), "failed", e_bool(true)),
                                    s_prop_assign(e_this(), "failure", e_new("TaskFailure", vec![e_class_const("TaskFailureKind", "TransferDecode"), e_str("Elephc Parallel could not decode the worker failure")])),
                                ]),
                                ),
                            ]),
                            (e_binop(e_var("phase"), BinOp::StrictEq, e_int(5)), vec![
                                s_prop_assign(e_this(), "cancelled", e_bool(true)),
                            ]),
                        ],
                            Some(vec![
                            s_prop_assign(e_this(), "failed", e_bool(true)),
                            s_prop_assign(e_this(), "failure", e_new("TaskFailure", vec![e_class_const("TaskFailureKind", "Infrastructure"), e_str("Elephc Parallel worker failed")])),
                        ]),
                        ),
                        s_prop_assign(e_this(), "settled", e_bool(true)),
                    ]),
            )
            .build(),
        class("TaskGroup")
            .final_()
            .private_prop("state", t_class("\\Elephc\\Async\\__CancellationState"), None)
            .private_prop("cancellation", t_class("\\Elephc\\Async\\Cancellation"), None)
            .private_prop("futures", t_array(), Some(e_array(vec![])))
            .private_prop("closed", TypeExpr::Bool, Some(e_bool(false)))
            .private_prop("ownsParentRoot", TypeExpr::Bool, Some(e_bool(false)))
            .method(
                method("__construct")
                    .private()
                    .param("state", t_class("\\Elephc\\Async\\__CancellationState"))
                    .body(vec![
                        s_prop_assign(e_this(), "state", e_var("state")),
                        s_prop_assign(e_this(), "cancellation", e_new_fq("Elephc\\Async\\Cancellation", vec![e_var("state")])),
                        s_if(
                            e_binop(
                                e_call("\\elephc_parallel_parent_scope_enter", vec![]),
                                BinOp::StrictNotEq,
                                e_int(1),
                            ),
                            vec![s_throw(e_new_fq(
                                "Error",
                                vec![e_str("Elephc\\Parallel\\run(): nested Parallel scopes are not supported in a parent root in v1")],
                            ))],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "ownsParentRoot", e_bool(true)),
                    ]),
            )
            .method(method("__clone").private().returns(TypeExpr::Void).body(vec![]))
            .method(
                method("__completeJobValue")
                    .private()
                    .static_()
                    .param("jobId", TypeExpr::Int)
                    .param("value", t_mixed())
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_assign("serialized", e_call("serialize", vec![e_var("value")])),
                        s_assign("length", e_call("strlen", vec![e_var("serialized")])),
                        s_assign("buffer", e_call("\\elephc_parallel_php_buffer_alloc", vec![e_var("length")])),
                        s_if(
                            e_binop(e_var("buffer"), BinOp::StrictEq, e_null()),
                            vec![s_throw(e_new_fq("Error", vec![e_str("Elephc Parallel worker transfer buffer allocation failed")]))],
                            vec![],
                            None,
                        ),
                        s_expr(e_call("\\__elephc_ptr_write_string", vec![e_var("buffer"), e_var("serialized")])),
                        s_assign("status", e_call("\\elephc_parallel_job_complete_php_serialized", vec![e_var("jobId"), e_var("buffer"), e_var("length")])),
                        s_expr(e_call("\\elephc_parallel_php_buffer_free", vec![e_var("buffer"), e_var("length")])),
                        s_return(e_var("status")),
                    ]),
            )
            .method(
                method("__failJobValue")
                    .private()
                    .static_()
                    .param("jobId", TypeExpr::Int)
                    .param("failure", t_mixed())
                    .returns(TypeExpr::Int)
                    .body(vec![
                        s_assign("remoteFrames", e_array(vec![])),
                        s_foreach(e_method_call(e_var("failure"), "getTrace", vec![]), None, "remoteFrame", vec![
                            s_assign("remoteFunction", e_ternary(e_call("isset", vec![e_index(e_var("remoteFrame"), e_str("function"))]), e_index(e_var("remoteFrame"), e_str("function")), e_str(""))),
                            s_assign("remoteFile", e_ternary(e_call("isset", vec![e_index(e_var("remoteFrame"), e_str("file"))]), e_index(e_var("remoteFrame"), e_str("file")), e_null())),
                            s_assign("remoteLine", e_ternary(e_call("isset", vec![e_index(e_var("remoteFrame"), e_str("line"))]), e_index(e_var("remoteFrame"), e_str("line")), e_int(0))),
                            s_array_push("remoteFrames", e_array(vec![e_var("remoteFunction"), e_var("remoteFile"), e_var("remoteLine")])),
                        ]),
                        s_assign("fields", e_array(vec![e_int(1), e_call("get_class", vec![e_var("failure")]), e_method_call(e_var("failure"), "getMessage", vec![]), e_method_call(e_var("failure"), "getCode", vec![]), e_method_call(e_var("failure"), "getFile", vec![]), e_method_call(e_var("failure"), "getLine", vec![]), e_var("remoteFrames")])),
                        s_assign("serialized", e_call("serialize", vec![e_var("fields")])),
                        s_assign("length", e_call("strlen", vec![e_var("serialized")])),
                        s_assign("buffer", e_call("\\elephc_parallel_php_buffer_alloc", vec![e_var("length")])),
                        s_if(
                            e_binop(e_var("buffer"), BinOp::StrictEq, e_null()),
                            vec![s_throw(e_new_fq("Error", vec![e_str("Elephc Parallel worker transfer buffer allocation failed")]))],
                            vec![],
                            None,
                        ),
                        s_expr(e_call("\\__elephc_ptr_write_string", vec![e_var("buffer"), e_var("serialized")])),
                        s_assign("status", e_call("\\elephc_parallel_job_fail_php_serialized", vec![e_var("jobId"), e_var("buffer"), e_var("length")])),
                        s_expr(e_call("\\elephc_parallel_php_buffer_free", vec![e_var("buffer"), e_var("length")])),
                        s_expr(e_call("unset", vec![e_var("serialized")])),
                        s_expr(e_call("unset", vec![e_var("fields")])),
                        s_expr(e_call("unset", vec![e_var("remoteLine")])),
                        s_expr(e_call("unset", vec![e_var("remoteFile")])),
                        s_expr(e_call("unset", vec![e_var("remoteFunction")])),
                        s_expr(e_call("unset", vec![e_var("remoteFrame")])),
                        s_expr(e_call("unset", vec![e_var("remoteFrames")])),
                        s_return(e_var("status")),
                    ]),
            )
            .method(method_reject_serialization("TaskGroup"))
            .method(method_reject_unserialization("TaskGroup"))
            .method(
                method("__destruct")
                    .body(vec![
                        s_expr(e_method_call(e_this(), "__cleanupOnExit", vec![])),
                    ]),
            )
            .method(
                method("__cleanupOnExit")
                    .private()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_this_prop("closed"),
                            vec![
                                s_expr(e_method_call(e_this(), "__releaseParentRoot", vec![])),
                                s_return_void(),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_not(e_call("isset", vec![e_this_prop("state")])),
                            vec![s_return_void()],
                            vec![],
                            None,
                        ),
                        s_try(
                            vec![
                                s_expr(e_method_call(e_this(), "__cancelAll", vec![])),
                                s_assign("failures", e_method_call(e_this(), "__drain", vec![])),
                                s_expr(e_call("unset", vec![e_var("failures")])),
                            ],
                            vec![(vec!["\\Throwable"], Some("cleanupFailure"), vec![
                                s_expr(e_call("unset", vec![e_var("cleanupFailure")])),
                            ])],
                            Some(vec![
                                s_try(
                                    vec![s_expr(e_method_call(e_this(), "__close", vec![]))],
                                    vec![],
                                    Some(vec![s_expr(e_method_call(e_this(), "__releaseParentRoot", vec![]))]),
                                ),
                            ]),
                        ),
                    ]),
            )
            .method(
                method("spawn")
                    .param("task", t_class("\\Closure"))
                    .variadic("args", Some(t_mixed()))
                    .returns(t_class("Future"))
                    .body(vec![
                        s_expr(e_method_call(e_this(), "__assertSpawnable", vec![])),
                        s_expr(e_call("unset", vec![e_var("task")])),
                        s_expr(e_call("unset", vec![e_var("args")])),
                        s_throw(e_new_fq("Error", vec![e_str("Elephc Parallel TaskGroup::spawn() was not lowered by the compiler")])),
                    ]),
            )
            .method(
                method("cancellation")
                    .returns(t_class("\\Elephc\\Async\\Cancellation"))
                    .body(vec![
                        s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
                        s_return(e_this_prop("cancellation")),
                    ]),
            )
            .method(
                method("cancel")
                    .param_default("reason", t_nullable(t_class("\\Throwable")), e_null())
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
                        s_expr(e_method_call(e_this(), "__cancelAll", vec![e_var("reason")])),
                    ]),
            )
            .method(
                method("__assertOpen")
                    .private()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_binop(e_call("\\elephc_parallel_worker_active", vec![]), BinOp::StrictEq, e_int(1)),
                            vec![s_throw(e_new_fq(
                                "Error",
                                vec![e_str("Elephc Parallel TaskGroup cannot be used from a Parallel worker")],
                            ))],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_this_prop("closed"),
                            vec![s_throw(e_new_fq(
                                "Error",
                                vec![e_str("Elephc Parallel TaskGroup cannot be used after its Parallel\\run() scope has closed")],
                            ))],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("__assertSpawnable")
                    .private()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_expr(e_method_call(e_this(), "__assertOpen", vec![])),
                        s_if(
                            e_method_call(e_this_prop("state"), "isRequested", vec![]),
                            vec![s_throw(e_new_fq(
                                "Error",
                                vec![e_str("Elephc Parallel TaskGroup cannot spawn after cancellation has been requested")],
                            ))],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("__recordFuture")
                    .private()
                    .param("future", t_class("Future"))
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_expr(e_method_call(e_var("future"), "__attachScope", vec![e_this()])),
                        s_prop_array_push(e_this(), "futures", e_var("future")),
                    ]),
            )
            .method(
                method("__drain")
                    .private()
                    .returns(t_array())
                    .body(vec![
                        s_assign("generation", e_call("\\elephc_parallel_completion_generation", vec![])),
                        s_while(e_bool(true), vec![
                            s_assign("allComplete", e_bool(true)),
                            s_assign("hasFailure", e_bool(false)),
                            s_foreach(e_this_prop("futures"), None, "future", vec![
                                s_assign("phase", e_method_call(e_var("future"), "__phase", vec![])),
                                s_if(
                                    e_binop(e_var("phase"), BinOp::Lt, e_int(3)),
                                    vec![
                                        s_assign("allComplete", e_bool(false)),
                                    ],
                                    vec![
                                    (e_binop(e_var("phase"), BinOp::StrictEq, e_int(4)), vec![
                                        s_assign("hasFailure", e_bool(true)),
                                    ]),
                                ],
                                    None,
                                ),
                            ]),
                            s_expr(e_call("unset", vec![e_var("future")])),
                            s_if(
                                e_var("hasFailure"),
                                vec![
                                    s_expr(e_method_call(e_this(), "__cancelAll", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_var("allComplete"),
                                vec![
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("generation", e_call("\\elephc_parallel_completion_wait", vec![e_var("generation")])),
                        ]),
                        s_assign("failures", e_array(vec![])),
                        s_foreach(e_this_prop("futures"), None, "future", vec![
                            s_assign("failure", e_method_call(e_var("future"), "__scopeFailure", vec![])),
                            s_if(
                                e_binop(e_var("failure"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_array_push("failures", e_var("failure")),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_call("unset", vec![e_var("failure")])),
                            s_expr(e_method_call(e_var("future"), "__finishScope", vec![])),
                            s_expr(e_call("unset", vec![e_var("future")])),
                        ]),
                        s_expr(e_call("unset", vec![e_var("future")])),
                        s_expr(e_call("unset", vec![e_this_prop("futures")])),
                        s_return(e_var("failures")),
                    ]),
            )
            .method(
                method("__cancelAll")
                    .private()
                    .param_default("reason", t_nullable(t_class("\\Throwable")), e_null())
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_expr(e_method_call(e_this_prop("state"), "request", vec![e_var("reason")])),
                        s_foreach(e_this_prop("futures"), None, "future", vec![
                            s_expr(e_method_call(e_var("future"), "__cancel", vec![])),
                            s_expr(e_call("unset", vec![e_var("future")])),
                        ]),
                        s_expr(e_call("unset", vec![e_var("future")])),
                    ]),
            )
            .method(
                method("__close")
                    .private()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_this_prop("closed"),
                            vec![s_return_void()],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "closed", e_bool(true)),
                        s_expr(e_call("unset", vec![e_this_prop("futures")])),
                        s_expr(e_call("unset", vec![e_this_prop("cancellation")])),
                        s_expr(e_method_call(e_this_prop("state"), "__clearReason", vec![])),
                        s_expr(e_call("unset", vec![e_this_prop("state")])),
                    ]),
            )
            .method(
                method("__releaseParentRoot")
                    .private()
                    .returns(TypeExpr::Void)
                    .body(vec![s_if(
                        e_this_prop("ownsParentRoot"),
                        vec![
                            s_expr(e_call("\\elephc_parallel_parent_scope_leave", vec![])),
                            s_expr(e_static_call("TaskGroup", "__assertFiberSuspendAllowed", vec![])),
                            s_prop_assign(e_this(), "ownsParentRoot", e_bool(false)),
                        ],
                        vec![],
                        None,
                    )]),
            )
            .method(
                method("__assertFiberSuspendAllowed")
                    .private()
                    .static_()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_binop(
                                e_binop(
                                    e_call("\\elephc_parallel_parent_scope_active", vec![]),
                                    BinOp::StrictEq,
                                    e_int(1),
                                ),
                                BinOp::Or,
                                e_binop(
                                    e_call("\\elephc_parallel_worker_active", vec![]),
                                    BinOp::StrictEq,
                                    e_int(1),
                                ),
                            ),
                            vec![s_throw(e_new_fq(
                                "Error",
                                vec![e_str("Fiber::suspend() is not supported while an Elephc\\Parallel scope or worker is active in v1")],
                            ))],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("__assertFiberSuspendCallableAllowed")
                    .private()
                    .static_()
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_binop(
                                e_binop(
                                    e_call("\\elephc_parallel_parent_scope_active", vec![]),
                                    BinOp::StrictEq,
                                    e_int(1),
                                ),
                                BinOp::Or,
                                e_binop(
                                    e_call("\\elephc_parallel_worker_active", vec![]),
                                    BinOp::StrictEq,
                                    e_int(1),
                                ),
                            ),
                            vec![s_expr(e_static_call(
                                "TaskGroup",
                                "__assertFiberSuspendAllowed",
                                vec![],
                            ))],
                            vec![],
                            None,
                        ),
                        s_expr(e_static_call(
                            "\\Elephc\\Async\\__Scheduler",
                            "__assertFiberSuspendAllowed",
                            vec![],
                        )),
                    ]),
            )
            .method(method_is_fiber_suspend_callable_array())
            .method(method_assert_fiber_suspend_callable_guard())
            .build(),
        function("run")
            .param("body", t_class("\\Closure"))
            .returns(t_mixed())
            .body(vec![
                s_if(
                    e_binop(
                        e_call("\\elephc_parallel_worker_active", vec![]),
                        BinOp::StrictEq,
                        e_int(1),
                    ),
                    vec![s_throw(e_new_fq(
                        "Error",
                        vec![e_str(
                            "Elephc\\Parallel\\run(): nested Parallel scopes are not supported inside a Parallel worker in v1",
                        )],
                    ))],
                    vec![],
                    None,
                ),
                s_assign("state", e_new_fq("Elephc\\Async\\__CancellationState", vec![])),
                s_assign("tasks", e_new("TaskGroup", vec![e_var("state")])),
                s_expr(e_call("unset", vec![e_var("state")])),
                s_try(vec![
                s_assign("resultBox", e_array(vec![])),
                s_assign("rootFailed", e_bool(false)),
                s_assign("rootFailure", e_null()),
                s_try(vec![
                    s_array_push("resultBox", e_closure_call("body", vec![e_var("tasks")])),
                ], vec![
                    (vec!["\\Throwable"], Some("caught"), vec![
                        s_assign("rootFailed", e_bool(true)),
                        s_assign("rootFailure", e_var("caught")),
                        s_expr(e_method_call(e_var("tasks"), "__cancelAll", vec![e_var("caught")])),
                        s_expr(e_call("unset", vec![e_var("caught")])),
                    ]),
                ], None),
                s_assign("failures", e_method_call(e_var("tasks"), "__drain", vec![])),
                s_expr(e_method_call(e_var("tasks"), "__close", vec![])),
                s_expr(e_call("unset", vec![e_var("body")])),
                s_expr(e_call("\\__elephc_async_gc_collect", vec![])),
                s_if(
                    e_binop(
                        e_var("rootFailed"),
                        BinOp::And,
                        e_binop(e_call("count", vec![e_var("failures")]), BinOp::StrictNotEq, e_int(0)),
                    ),
                    vec![
                        s_expr(e_call("unset", vec![e_var("resultBox")])),
                        s_assign("groupFailure", e_new("TaskGroupFailure", vec![e_var("failures"), e_var("rootFailure")])),
                        s_expr(e_call("unset", vec![e_var("failures")])),
                        s_throw(e_var("groupFailure")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_var("rootFailed"),
                    vec![
                        s_expr(e_call("unset", vec![e_var("resultBox")])),
                        s_expr(e_call("unset", vec![e_var("failures")])),
                        s_throw(e_var("rootFailure")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("count", vec![e_var("failures")]), BinOp::StrictNotEq, e_int(0)),
                    vec![
                        s_expr(e_call("unset", vec![e_var("resultBox")])),
                        s_assign("groupFailure", e_new("TaskGroupFailure", vec![e_var("failures")])),
                        s_expr(e_call("unset", vec![e_var("failures")])),
                        s_throw(e_var("groupFailure")),
                    ],
                    vec![],
                    None,
                ),
                s_assign("result", e_index(e_var("resultBox"), e_int(0))),
                s_expr(e_call("unset", vec![e_var("resultBox")])),
                s_expr(e_call("unset", vec![e_var("failures")])),
                s_return(e_var("result")),
                ], vec![], Some(vec![
                    s_expr(e_method_call(e_var("tasks"), "__cleanupOnExit", vec![])),
                ])),
            ])
            .build()
    ])
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn parallel_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_extern_elephc_parallel_worker_active(),
            decl_extern_elephc_parallel_parent_scope_enter(),
            decl_extern_elephc_parallel_parent_scope_leave(),
            decl_extern_elephc_parallel_parent_scope_active(),
            decl_extern_elephc_parallel_job_create_php_serialized(),
            decl_extern_elephc_parallel_job_input_php_prepare(),
            decl_extern_elephc_parallel_job_complete_php_serialized(),
            decl_extern_elephc_parallel_job_fail_php_serialized(),
            decl_extern_elephc_parallel_job_result_php_prepare(),
            decl_extern_elephc_parallel_job_failure_php_prepare(),
            decl_extern_elephc_parallel_job_observe_failure(),
            decl_extern_elephc_parallel_job_failure_observed(),
            decl_extern_elephc_parallel_php_blob_ptr(),
            decl_extern_elephc_parallel_php_blob_len(),
            decl_extern_elephc_parallel_php_blob_release(),
            decl_extern_elephc_parallel_php_buffer_alloc(),
            decl_extern_elephc_parallel_php_buffer_free(),
            decl_extern_elephc_parallel_job_phase(),
            decl_extern_elephc_parallel_job_wait(),
            decl_extern_elephc_parallel_job_cancel(),
            decl_extern_elephc_parallel_job_cancellation_requested(),
            decl_extern_elephc_parallel_job_release(),
            decl_extern_elephc_parallel_completion_generation(),
            decl_extern_elephc_parallel_completion_wait(),
            decl_stmt_bootstrap_1(),
        ]
    })
}
