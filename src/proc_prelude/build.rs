//! Purpose:
//! Builds the `__elephc_proc_*` helpers of the proc prelude: php-src's argument
//! diagnostics and the `proc_get_status`/`proc_close`/`proc_terminate`/`proc_nice` twins.
//!
//! Called from:
//! - `crate::proc_prelude::declarations`.
//!
//! Key details:
//! - Transcribed from PHP with `crate::synthetic_class::transcribe`; the spawning half
//!   (descriptor walk, `__elephc_proc_open`, pipes array) lives in `open`.
//! - `proc_open`/`proc_terminate`/`proc_nice` take a trailing compiler-owned `bool $strict`
//!   (the calling file's `strict_types`): `__elephc_proc_int` uses it to pick php-src's int
//!   coercion, and `__elephc_proc_open` refuses scalar `$command`/`$cwd` conversions with it.
//! - Field selectors 0..6 of `elephc_proc_status_field` are `crates/elephc-proc`'s `FIELD_*`.
//! - A process resource is kind 11 with its bridge id as payload; a closed one keeps the
//!   kind and carries the negative closed sentinel, which php-src reports as "not a valid
//!   process resource".

mod open;

use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array_assoc, e_binop, e_bool, e_call, e_cast, e_concat_all, e_float, e_int, e_ternary, e_neg, e_new_fq, e_null, e_str,
    e_var, function, internal_declarations, s_assign, s_expr, s_if, s_return, s_throw,
    t_array, t_mixed,
};

/// `__elephc_proc_type_name` — transcribed from the PHP form.
fn decl_fn_elephc_proc_type_name() -> Stmt {
    function("__elephc_proc_type_name")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_bool(true)),
                vec![
                    s_return(e_str("true")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_str("false")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_str("null")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![
                    s_return(e_call("get_class", vec![e_var("value")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![
                    s_return(e_str("int")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("value")]),
                vec![
                    s_return(e_str("float")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_return(e_str("string")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_return(e_str("array")),
                ],
                vec![],
                None,
            ),
            s_return(e_str("resource")),
        ])
        .build()
}

/// `__elephc_proc_handle` — transcribed from the PHP form.
fn decl_fn_elephc_proc_handle() -> Stmt {
    function("__elephc_proc_handle")
        .param("process", t_mixed())
        .param("function", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("kind", e_call("__elephc_resource_kind", vec![e_var("process")])),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_neg(e_int(1))),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument #1 ($process) must be of type resource, ")), BinOp::Concat, e_call("__elephc_proc_type_name", vec![e_var("process")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("id", e_call("__elephc_resource_payload", vec![e_var("process")])),
            s_if(
                e_binop(e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(11)), BinOp::Or, e_binop(e_var("id"), BinOp::Lt, e_int(0))),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_var("function"), BinOp::Concat, e_str("(): supplied resource is not a valid process resource"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("id")),
        ])
        .build()
}

/// `__elephc_proc_int` — php-src's `int` parameter coercion for the caller's mode.
///
/// Strict mode (`$strict`, the compiler-owned flag of the calling file) admits only an int.
/// Coercive mode also admits a bool, a finite in-range float and a numeric string, with
/// PHP's deprecations for `null` and for a float (or float-string) that loses precision.
fn decl_fn_elephc_proc_int() -> Stmt {
    let type_error = e_new_fq("TypeError", vec![e_concat_all(vec![
        e_var("function"),
        e_str("(): Argument "),
        e_var("parameter"),
        e_str(" must be of type int, "),
        e_call("__elephc_proc_type_name", vec![e_var("value")]),
        e_str(" given"),
    ])]);
    let in_range = e_binop(
        e_binop(e_call("is_finite", vec![e_var("number")]), BinOp::And,
            e_binop(e_var("number"), BinOp::GtEq, e_neg(e_float(9223372036854775808.0)))),
        BinOp::And,
        e_binop(e_var("number"), BinOp::Lt, e_float(9223372036854775808.0)),
    );
    function("__elephc_proc_int")
        .param("value", t_mixed())
        .param("strict", TypeExpr::Bool)
        .param("function", TypeExpr::Str)
        .param("parameter", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            s_if(
                e_var("strict"),
                vec![s_throw(type_error.clone())],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                vec![
                    s_expr(e_call("__elephc_builtin_deprecated", vec![e_concat_all(vec![
                        e_var("function"),
                        e_str("(): Passing null to parameter "),
                        e_var("parameter"),
                        e_str(" of type int is deprecated"),
                    ])])),
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![s_return(e_cast(CastType::Int, e_var("value")))],
                vec![],
                None,
            ),
            s_assign("number", e_var("value")),
            s_assign("described", e_str("")),
            s_if(
                e_binop(e_call("is_string", vec![e_var("value")]), BinOp::And, e_call("is_numeric", vec![e_var("value")])),
                vec![
                    s_assign("number", e_binop(e_var("value"), BinOp::Add, e_int(0))),
                    s_if(
                        e_call("is_int", vec![e_var("number")]),
                        vec![s_return(e_var("number"))],
                        vec![],
                        None,
                    ),
                    s_assign("described", e_concat_all(vec![
                        e_str("float-string \""),
                        e_var("value"),
                        e_str("\""),
                    ])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_float", vec![e_var("number")]), BinOp::And, in_range),
                vec![
                    s_assign("integer", e_cast(CastType::Int, e_var("number"))),
                    s_if(
                        e_binop(e_cast(CastType::Float, e_var("integer")), BinOp::StrictNotEq, e_var("number")),
                        vec![s_expr(e_call("__elephc_builtin_deprecated", vec![e_concat_all(vec![
                            e_str("Implicit conversion from "),
                            e_ternary(
                                e_binop(e_var("described"), BinOp::StrictEq, e_str("")),
                                e_binop(e_str("float "), BinOp::Concat, e_cast(CastType::String, e_var("number"))),
                                e_var("described"),
                            ),
                            e_str(" to int loses precision"),
                        ])]))],
                        vec![],
                        None,
                    ),
                    s_return(e_var("integer")),
                ],
                vec![],
                None,
            ),
            s_throw(type_error),
        ])
        .build()
}

/// `__elephc_proc_close` — transcribed from the PHP form.
fn decl_fn_elephc_proc_close() -> Stmt {
    function("__elephc_proc_close")
        .param("process", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("id", e_call("__elephc_proc_handle", vec![e_var("process"), e_str("proc_close")])),
            s_assign("status", e_call("elephc_proc_close", vec![e_var("id")])),
            s_expr(e_call("__elephc_resource_close", vec![e_var("process")])),
            s_return(e_var("status")),
        ])
        .build()
}

/// `__elephc_proc_get_status` — transcribed from the PHP form.
fn decl_fn_elephc_proc_get_status() -> Stmt {
    function("__elephc_proc_get_status")
        .param("process", t_mixed())
        .returns(t_array())
        .body(vec![
            s_assign("id", e_call("__elephc_proc_handle", vec![e_var("process"), e_str("proc_get_status")])),
            s_expr(e_call("elephc_proc_poll", vec![e_var("id")])),
            s_return(e_array_assoc(vec![(e_str("command"), e_call("elephc_proc_command", vec![e_var("id")])), (e_str("pid"), e_call("elephc_proc_pid", vec![e_var("id")])), (e_str("cached"), e_binop(e_call("elephc_proc_status_field", vec![e_var("id"), e_int(6)]), BinOp::StrictEq, e_int(1))), (e_str("running"), e_binop(e_call("elephc_proc_status_field", vec![e_var("id"), e_int(0)]), BinOp::StrictEq, e_int(1))), (e_str("signaled"), e_binop(e_call("elephc_proc_status_field", vec![e_var("id"), e_int(1)]), BinOp::StrictEq, e_int(1))), (e_str("stopped"), e_binop(e_call("elephc_proc_status_field", vec![e_var("id"), e_int(2)]), BinOp::StrictEq, e_int(1))), (e_str("exitcode"), e_call("elephc_proc_status_field", vec![e_var("id"), e_int(3)])), (e_str("termsig"), e_call("elephc_proc_status_field", vec![e_var("id"), e_int(4)])), (e_str("stopsig"), e_call("elephc_proc_status_field", vec![e_var("id"), e_int(5)]))])),
        ])
        .build()
}

/// `__elephc_proc_terminate` — transcribed from the PHP form.
fn decl_fn_elephc_proc_terminate() -> Stmt {
    function("__elephc_proc_terminate")
        .param("process", t_mixed())
        .param("signal", t_mixed())
        .param("strict", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("id", e_call("__elephc_proc_handle", vec![e_var("process"), e_str("proc_terminate")])),
            s_return(e_binop(e_call("elephc_proc_terminate", vec![e_var("id"), e_call("__elephc_proc_int", vec![e_var("signal"), e_var("strict"), e_str("proc_terminate"), e_str("#2 ($signal)")])]), BinOp::StrictEq, e_int(1))),
        ])
        .build()
}

/// `__elephc_proc_nice` — transcribed from the PHP form.
fn decl_fn_elephc_proc_nice() -> Stmt {
    function("__elephc_proc_nice")
        .param("priority", t_mixed())
        .param("strict", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("elephc_proc_nice", vec![e_call("__elephc_proc_int", vec![e_var("priority"), e_var("strict"), e_str("proc_nice"), e_str("#1 ($priority)")])]), BinOp::StrictEq, e_int(1)),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_builtin_warning", vec![e_str("proc_nice(): Only a super user may attempt to increase the priority of a process")])),
            s_return(e_bool(false)),
        ])
        .build()
}

/// Builds the whole helper surface, in dependency order.
pub(super) fn declarations() -> Vec<Stmt> {
    internal_declarations(|| {
        vec![
            decl_fn_elephc_proc_type_name(),
            decl_fn_elephc_proc_handle(),
            decl_fn_elephc_proc_int(),
            open::decl_fn_elephc_proc_stream_fd(),
            open::decl_fn_elephc_proc_spec_string(),
            open::decl_fn_elephc_proc_descriptor(),
            open::decl_fn_elephc_proc_open(),
            open::decl_fn_elephc_proc_open_pipes(),
            decl_fn_elephc_proc_close(),
            decl_fn_elephc_proc_get_status(),
            decl_fn_elephc_proc_terminate(),
            decl_fn_elephc_proc_nice(),
        ]
    })
}
