//! Purpose:
//! Builds the PHP 8.6 `fread()` / `stream_get_contents()` error-aware wrappers.
//!
//! Called from:
//! - `crate::stream_errors_prelude::declarations()`.
//!
//! Key details:
//! - The runtime records `UserspaceInvalidReturn` events in three program globals; the
//!   wrappers turn them into `StreamError` objects and apply the stream context's
//!   mode/store/handler policy after the underlying read.
//! - Nested reads save and restore the tracking globals, so an inner operation cannot
//!   consume the outer operation's pending errors.
//! - The helpers are ordinary PHP functions, so every supported target shares one
//!   implementation and the checker validates the whole surface.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_class_const, e_concat, e_concat_all, e_const, e_int,
    e_instance_of, e_new, e_not, e_null, e_null_coalesce, e_post_inc, e_prop, e_str, e_var,
    function, s_array_push, s_assign, s_expr, s_foreach, s_global, s_if, s_return, s_return_void,
    s_static_prop_assign, s_throw, s_while, t_mixed,
};

/// Builds the read-time error wrappers.
pub(super) fn declarations() -> Program {
    vec![
        stream_read_flush_helper(),
        fread_wrapper(),
        stream_get_contents_wrapper(),
    ]
}

/// Builds the shared UserspaceInvalidReturn dispatcher used by the read wrappers.
fn stream_read_flush_helper() -> Stmt {
    let count = e_var("count");
    let excess = e_var("excess");
    let class = e_var("class");
    let errors = e_var("errors");
    let mode = e_var("mode");
    let store = e_var("store");
    let stream = e_var("stream");
    let handler = e_var("handler");
    let message = e_concat_all(vec![
        class,
        e_str("::stream_read - read "),
        excess.clone(),
        e_str(" bytes more data than requested ("),
        e_binop(e_int(8192), BinOp::Add, excess),
        e_str(" read, 8192 max) - excess data will be lost"),
    ]);
    function("__elephc_stream_read_flush")
        .returns(TypeExpr::Void)
        .body(vec![
            s_global(tracking_globals()),
            s_assign(
                "count",
                e_null_coalesce(e_var("__elephc_stream_read_count"), e_int(0)),
            ),
            s_if(
                e_binop(
                    e_binop(
                        e_not(e_call("is_int", vec![count.clone()])),
                        BinOp::Or,
                        e_binop(count.clone(), BinOp::LtEq, e_int(0)),
                    ),
                    BinOp::Or,
                    e_bool(false),
                ),
                vec![s_return_void()],
                Vec::new(),
                None,
            ),
            s_assign("excess", e_var("__elephc_stream_read_excess")),
            s_assign("class", e_var("__elephc_stream_read_class")),
            s_assign("errors", e_array(vec![])),
            s_assign("i", e_int(0)),
            s_while(
                e_binop(e_var("i"), BinOp::Lt, count),
                vec![
                    s_assign("message", message),
                    s_array_push(
                        "errors",
                        e_new(
                            "StreamError",
                            vec![
                                e_class_const("StreamErrorCode", "UserspaceInvalidReturn"),
                                e_var("message"),
                                e_str("user-space"),
                                e_const("E_WARNING"),
                                e_bool(false),
                            ],
                        ),
                    ),
                    s_expr(e_post_inc("i")),
                ],
            ),
            s_assign(
                "options",
                e_call(
                    "stream_context_get_options",
                    vec![e_call("stream_context_get_default", vec![])],
                ),
            ),
            s_assign(
                "stream",
                e_null_coalesce(e_index_of("options"), e_array(vec![])),
            ),
            s_assign(
                "mode",
                e_null_coalesce(
                    e_index_key(stream.clone(), "error_mode"),
                    e_class_const("StreamErrorMode", "Error"),
                ),
            ),
            s_if(
                e_not(e_instance_of(mode.clone(), "StreamErrorMode")),
                vec![s_throw(e_new(
                    "TypeError",
                    vec![e_str(
                        "stream context option 'error_mode' must be of type StreamErrorMode",
                    )],
                ))],
                Vec::new(),
                None,
            ),
            s_assign(
                "store",
                e_null_coalesce(
                    e_index_key(stream.clone(), "error_store"),
                    e_class_const("StreamErrorStore", "Auto"),
                ),
            ),
            s_if(
                e_not(e_instance_of(store.clone(), "StreamErrorStore")),
                vec![s_throw(e_new(
                    "TypeError",
                    vec![e_str(
                        "stream context option 'error_store' must be of type StreamErrorStore",
                    )],
                ))],
                Vec::new(),
                None,
            ),
            s_assign("keep", e_bool(false)),
            s_if(
                e_binop(
                    store.clone(),
                    BinOp::StrictEq,
                    e_class_const("StreamErrorStore", "All"),
                ),
                vec![s_assign("keep", e_bool(true))],
                vec![
                    (
                        e_binop(
                            store.clone(),
                            BinOp::StrictEq,
                            e_class_const("StreamErrorStore", "NonTerminating"),
                        ),
                        vec![s_assign("keep", e_bool(true))],
                    ),
                    (
                        e_binop(
                            store.clone(),
                            BinOp::StrictEq,
                            e_class_const("StreamErrorStore", "Auto"),
                        ),
                        vec![s_if(
                            e_binop(
                                e_binop(
                                    mode.clone(),
                                    BinOp::StrictEq,
                                    e_class_const("StreamErrorMode", "Silent"),
                                ),
                                BinOp::Or,
                                e_binop(
                                    mode.clone(),
                                    BinOp::StrictEq,
                                    e_class_const("StreamErrorMode", "Exception"),
                                ),
                            ),
                            vec![s_assign("keep", e_bool(true))],
                            Vec::new(),
                            None,
                        )],
                    ),
                ],
                None,
            ),
            s_if(
                e_var("keep"),
                vec![s_static_prop_assign(
                    "__ElephcStreamErrors",
                    "errors",
                    errors.clone(),
                )],
                Vec::new(),
                Some(vec![s_static_prop_assign(
                    "__ElephcStreamErrors",
                    "errors",
                    e_array(vec![]),
                )]),
            ),
            s_assign(
                "handler",
                e_null_coalesce(e_index_key(stream, "error_handler"), e_null()),
            ),
            s_if(
                e_binop(handler.clone(), BinOp::StrictNotEq, e_null()),
                vec![s_expr(e_call(
                    "call_user_func",
                    vec![handler, errors.clone()],
                ))],
                Vec::new(),
                None,
            ),
            s_if(
                e_binop(
                    mode.clone(),
                    BinOp::StrictEq,
                    e_class_const("StreamErrorMode", "Error"),
                ),
                vec![s_foreach(
                    errors.clone(),
                    None,
                    "entry",
                    vec![s_expr(e_call(
                        "trigger_error",
                        vec![
                            e_concat(e_str("fread(): "), e_prop(e_var("entry"), "message")),
                            e_const("E_USER_WARNING"),
                        ],
                    ))],
                )],
                Vec::new(),
                None,
            ),
            s_if(
                e_binop(
                    mode,
                    BinOp::StrictEq,
                    e_class_const("StreamErrorMode", "Exception"),
                ),
                vec![s_throw(e_new(
                    "StreamException",
                    vec![
                        e_prop(e_index_key(errors.clone(), "0"), "message"),
                        e_int(0),
                        e_null(),
                        errors,
                    ],
                ))],
                Vec::new(),
                None,
            ),
            s_return_void(),
        ])
        .build()
}

/// Builds the `fread()` wrapper that reports `UserspaceInvalidReturn` errors.
fn fread_wrapper() -> Stmt {
    function("__elephc_fread")
        .param("stream", t_mixed())
        .param("length", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_global(tracking_globals()),
            s_assign("prevCount", e_var("__elephc_stream_read_count")),
            s_assign("prevExcess", e_var("__elephc_stream_read_excess")),
            s_assign("prevClass", e_var("__elephc_stream_read_class")),
            s_assign("__elephc_stream_read_count", e_int(0)),
            s_assign(
                "data",
                e_call("fread", vec![e_var("stream"), e_var("length")]),
            ),
            s_expr(e_call("__elephc_stream_read_flush", vec![])),
            s_assign("__elephc_stream_read_count", e_var("prevCount")),
            s_assign("__elephc_stream_read_excess", e_var("prevExcess")),
            s_assign("__elephc_stream_read_class", e_var("prevClass")),
            s_return(e_var("data")),
        ])
        .build()
}

/// Builds the `stream_get_contents()` wrapper for the same error surface.
fn stream_get_contents_wrapper() -> Stmt {
    function("__elephc_stream_get_contents")
        .param("stream", t_mixed())
        .param_default("length", TypeExpr::Int, e_int(-1))
        .param_default("offset", TypeExpr::Int, e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_global(tracking_globals()),
            s_assign("prevCount", e_var("__elephc_stream_read_count")),
            s_assign("prevExcess", e_var("__elephc_stream_read_excess")),
            s_assign("prevClass", e_var("__elephc_stream_read_class")),
            s_assign("__elephc_stream_read_count", e_int(0)),
            s_if(
                e_binop(
                    e_call("func_num_args", vec![]),
                    BinOp::StrictEq,
                    e_int(1),
                ),
                vec![s_assign(
                    "data",
                    e_call("stream_get_contents", vec![e_var("stream")]),
                )],
                vec![(
                    e_binop(
                        e_call("func_num_args", vec![]),
                        BinOp::StrictEq,
                        e_int(2),
                    ),
                    vec![s_assign(
                        "data",
                        e_call(
                            "stream_get_contents",
                            vec![e_var("stream"), e_var("length")],
                        ),
                    )],
                )],
                Some(vec![s_assign(
                    "data",
                    e_call(
                        "stream_get_contents",
                        vec![e_var("stream"), e_var("length"), e_var("offset")],
                    ),
                )]),
            ),
            s_expr(e_call("__elephc_stream_read_flush", vec![])),
            s_assign("__elephc_stream_read_count", e_var("prevCount")),
            s_assign("__elephc_stream_read_excess", e_var("prevExcess")),
            s_assign("__elephc_stream_read_class", e_var("prevClass")),
            s_return(e_var("data")),
        ])
        .build()
}

/// Returns the three program globals the runtime uses to publish excess reads.
fn tracking_globals() -> Vec<String> {
    vec![
        "__elephc_stream_read_count".to_string(),
        "__elephc_stream_read_excess".to_string(),
        "__elephc_stream_read_class".to_string(),
    ]
}

/// Builds `$options['stream']` for the fixed key the dispatcher reads.
fn e_index_of(name: &str) -> Expr {
    e_index_key(e_var(name), "stream")
}

/// Builds `$value[$key]` for a literal string key.
fn e_index_key(value: Expr, key: &str) -> Expr {
    crate::synthetic_class::e_index(value, e_str(key))
}
