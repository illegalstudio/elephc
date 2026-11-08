//! Purpose:
//! Builds the spawning half of the proc prelude: the descriptor-spec walk,
//! `__elephc_proc_open`, and `__elephc_proc_open_pipes`.
//!
//! Called from:
//! - `super::declarations` (the proc prelude helper aggregator).
//!
//! Key details:
//! - Transcribed from PHP with `crate::synthetic_class::transcribe`.
//! - Descriptors are staged in spec order, so php-src's first failing entry wins; a
//!   staging answer of 1 is success, 0 a bridge failure whose php-src warning text
//!   `elephc_proc_last_error()` returns, and -1 a failure already warned about here.
//! - A stream spec must be a kind 0/1/3 resource with a real descriptor payload
//!   (below the synthetic-wrapper base `0x40000000`).

use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_index, e_int, e_neg, e_new_fq, e_not,
    e_null, e_post_inc, e_str, e_var, function, s_array_assign, s_assign, s_continue,
    s_expr, s_for, s_foreach, s_if, s_return, s_throw, s_try, t_array, t_mixed,
};

/// `__elephc_proc_stream_fd` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_proc_stream_fd() -> Stmt {
    function("__elephc_proc_stream_fd")
        .param("stream", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("kind", e_call("__elephc_resource_kind", vec![e_var("stream")])),
            s_assign("fd", e_call("__elephc_resource_payload", vec![e_var("stream")])),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(1))), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(3))), BinOp::And, e_binop(e_var("fd"), BinOp::GtEq, e_int(0))), BinOp::And, e_binop(e_var("fd"), BinOp::Lt, e_int(1073741824))),
                vec![
                    s_return(e_var("fd")),
                ],
                vec![],
                None,
            ),
            s_throw(e_new_fq("TypeError", vec![e_str("proc_open(): supplied resource is not a valid stream resource")])),
        ])
        .build()
}

/// `__elephc_proc_spec_string` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_proc_spec_string() -> Stmt {
    function("__elephc_proc_spec_string")
        .param("spec", t_array())
        .param("position", TypeExpr::Int)
        .param("missing", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_call("array_key_exists", vec![e_var("position"), e_var("spec")])),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_binop(e_str("Missing "), BinOp::Concat, e_var("missing"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_cast(CastType::String, e_index(e_var("spec"), e_var("position")))),
        ])
        .build()
}

/// `__elephc_proc_descriptor` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_proc_descriptor() -> Stmt {
    function("__elephc_proc_descriptor")
        .param("builder", TypeExpr::Int)
        .param("index", TypeExpr::Int)
        .param("spec", t_mixed())
        .param("raw_streams", TypeExpr::Bool)
        .returns(TypeExpr::Int)
        .body(vec![
            // A spec array whose static element type is `resource` holds raw native
            // descriptors rather than boxed resource cells (`[STDIN, STDOUT, STDERR]`).
            s_if(
                e_binop(e_var("raw_streams"), BinOp::And, e_call("is_int", vec![e_var("spec")])),
                vec![
                    s_return(e_call("elephc_proc_builder_fd", vec![e_var("builder"), e_var("index"), e_var("spec")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_resource", vec![e_var("spec")]), BinOp::Or, e_binop(e_call("__elephc_resource_kind", vec![e_var("spec")]), BinOp::StrictNotEq, e_neg(e_int(1)))),
                vec![
                    s_return(e_call("elephc_proc_builder_fd", vec![e_var("builder"), e_var("index"), e_call("__elephc_proc_stream_fd", vec![e_var("spec")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("is_array", vec![e_var("spec")])),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("proc_open(): Argument #2 ($descriptor_spec) must only contain arrays and streams")])),
                ],
                vec![],
                None,
            ),
            s_assign("spec", e_cast(CastType::Array, e_var("spec"))),
            s_assign("type", e_call("__elephc_proc_spec_string", vec![e_cast(CastType::Array, e_var("spec")), e_int(0), e_str("handle qualifier")])),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("pipe")),
                vec![
                    s_return(e_call("elephc_proc_builder_pipe", vec![e_var("builder"), e_var("index"), e_call("__elephc_proc_spec_string", vec![e_cast(CastType::Array, e_var("spec")), e_int(1), e_str("mode parameter for 'pipe'")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("file")),
                vec![
                    s_assign("path", e_call("__elephc_proc_spec_string", vec![e_cast(CastType::Array, e_var("spec")), e_int(1), e_str("file name parameter for 'file'")])),
                    s_return(e_call("elephc_proc_builder_file", vec![e_var("builder"), e_var("index"), e_var("path"), e_call("__elephc_proc_spec_string", vec![e_cast(CastType::Array, e_var("spec")), e_int(2), e_str("mode parameter for 'file'")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("redirect")),
                vec![
                    s_if(
                        e_not(e_call("array_key_exists", vec![e_int(1), e_cast(CastType::Array, e_var("spec"))])),
                        vec![
                            s_throw(e_new_fq("ValueError", vec![e_str("Missing redirection target")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("target", e_index(e_var("spec"), e_int(1))),
                    s_if(
                        e_not(e_call("is_int", vec![e_var("target")])),
                        vec![
                            s_throw(e_new_fq("ValueError", vec![e_binop(e_binop(e_str("Redirection target must be of type int, "), BinOp::Concat, e_call("__elephc_proc_type_name", vec![e_var("target")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("elephc_proc_builder_redirect", vec![e_var("builder"), e_var("index"), e_var("target")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("null")),
                vec![
                    s_return(e_call("elephc_proc_builder_null", vec![e_var("builder"), e_var("index")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("pty")),
                vec![
                    s_expr(e_call("__elephc_builtin_warning", vec![e_str("proc_open(): PTY (pseudoterminal) not supported on this system")])),
                    s_return(e_neg(e_int(1))),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_builtin_warning", vec![e_binop(e_binop(e_str("proc_open(): "), BinOp::Concat, e_var("type")), BinOp::Concat, e_str(" is not a valid descriptor spec/mode"))])),
            s_return(e_neg(e_int(1))),
        ])
        .build()
}

/// `__elephc_proc_open` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_proc_open() -> Stmt {
    function("__elephc_proc_open")
        .param("command", t_mixed())
        .param("descriptor_spec", t_mixed())
        .param("cwd", t_mixed())
        .param("env_vars", t_mixed())
        .param("options", t_mixed())
        .param("raw_streams", TypeExpr::Bool)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_array", vec![e_var("command")]),
                vec![
                    s_if(
                        e_binop(e_call("count", vec![e_var("command")]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new_fq("ValueError", vec![e_str("proc_open(): Argument #1 ($command) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![
                (e_binop(e_binop(e_call("is_int", vec![e_var("command")]), BinOp::Or, e_call("is_float", vec![e_var("command")])), BinOp::Or, e_call("is_bool", vec![e_var("command")])), vec![
                    s_assign("command", e_cast(CastType::String, e_var("command"))),
                ]),
                (e_not(e_call("is_string", vec![e_var("command")])), vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("proc_open(): Argument #1 ($command) must be of type array|string, "), BinOp::Concat, e_call("__elephc_proc_type_name", vec![e_var("command")])), BinOp::Concat, e_str(" given"))])),
                ]),
            ],
                None,
            ),
            s_if(
                e_not(e_call("is_array", vec![e_var("descriptor_spec")])),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("proc_open(): Argument #2 ($descriptor_spec) must be of type array, "), BinOp::Concat, e_call("__elephc_proc_type_name", vec![e_var("descriptor_spec")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("is_int", vec![e_var("cwd")]), BinOp::Or, e_call("is_float", vec![e_var("cwd")])), BinOp::Or, e_call("is_bool", vec![e_var("cwd")])),
                vec![
                    s_assign("cwd", e_cast(CastType::String, e_var("cwd"))),
                ],
                vec![
                (e_binop(e_binop(e_var("cwd"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_string", vec![e_var("cwd")]))), vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("proc_open(): Argument #4 ($cwd) must be of type ?string, "), BinOp::Concat, e_call("__elephc_proc_type_name", vec![e_var("cwd")])), BinOp::Concat, e_str(" given"))])),
                ]),
            ],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("env_vars"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_array", vec![e_var("env_vars")]))),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("proc_open(): Argument #5 ($env_vars) must be of type ?array, "), BinOp::Concat, e_call("__elephc_proc_type_name", vec![e_var("env_vars")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("options"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_array", vec![e_var("options")]))),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("proc_open(): Argument #6 ($options) must be of type ?array, "), BinOp::Concat, e_call("__elephc_proc_type_name", vec![e_var("options")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("builder", e_call("elephc_proc_builder_new", vec![])),
            s_if(
                e_call("is_array", vec![e_var("command")]),
                vec![
                    s_assign("position", e_int(0)),
                    s_foreach(e_var("command"), None, "argument", vec![
                        s_assign("position", e_binop(e_var("position"), BinOp::Add, e_int(1))),
                        s_assign("text", e_cast(CastType::String, e_var("argument"))),
                        s_if(
                            e_call("str_contains", vec![e_var("text"), e_str("\0")]),
                            vec![
                                s_expr(e_call("elephc_proc_builder_free", vec![e_var("builder")])),
                                s_throw(e_new_fq("ValueError", vec![e_binop(e_binop(e_str("Command array element "), BinOp::Concat, e_var("position")), BinOp::Concat, e_str(" contains a null byte"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_call("elephc_proc_builder_arg", vec![e_var("builder"), e_var("text")])),
                    ]),
                ],
                vec![],
                Some(vec![
                s_expr(e_call("elephc_proc_builder_shell", vec![e_var("builder"), e_var("command")])),
            ]),
            ),
            s_if(
                e_binop(e_var("env_vars"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_proc_builder_env_reset", vec![e_var("builder")])),
                    s_foreach(e_var("env_vars"), Some("name"), "value", vec![
                        s_assign("entry", e_cast(CastType::String, e_var("value"))),
                        s_if(
                            e_binop(e_var("entry"), BinOp::StrictEq, e_str("")),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_call("is_string", vec![e_var("name")]), BinOp::And, e_binop(e_var("name"), BinOp::StrictNotEq, e_str(""))),
                            vec![
                                s_assign("entry", e_binop(e_binop(e_var("name"), BinOp::Concat, e_str("=")), BinOp::Concat, e_var("entry"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_call("elephc_proc_builder_env", vec![e_var("builder"), e_var("entry")])),
                    ]),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("descriptor_spec"), Some("index"), "spec", vec![
                s_if(
                    e_not(e_call("is_int", vec![e_var("index")])),
                    vec![
                        s_expr(e_call("elephc_proc_builder_free", vec![e_var("builder")])),
                        s_throw(e_new_fq("ValueError", vec![e_str("proc_open(): Argument #2 ($descriptor_spec) must be an integer indexed array")])),
                    ],
                    vec![],
                    None,
                ),
                s_try(vec![
                    s_assign("staged", e_call("__elephc_proc_descriptor", vec![e_var("builder"), e_var("index"), e_var("spec"), e_var("raw_streams")])),
                ], vec![
                    (vec!["\\Throwable"], Some("error"), vec![
                        s_expr(e_call("elephc_proc_builder_free", vec![e_var("builder")])),
                        s_throw(e_var("error")),
                    ]),
                ], None),
                s_if(
                    e_binop(e_var("staged"), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_expr(e_call("__elephc_builtin_warning", vec![e_call("elephc_proc_last_error", vec![])])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("staged"), BinOp::StrictNotEq, e_int(1)),
                    vec![
                        s_expr(e_call("elephc_proc_builder_free", vec![e_var("builder")])),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_if(
                e_binop(e_var("cwd"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("elephc_proc_builder_cwd", vec![e_var("builder"), e_var("cwd")])),
                ],
                vec![],
                None,
            ),
            s_assign("process", e_call("elephc_proc_spawn", vec![e_var("builder")])),
            s_if(
                e_binop(e_var("process"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_builtin_warning", vec![e_call("elephc_proc_last_error", vec![])])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_proc_resource", vec![e_var("process")])),
        ])
        .build()
}

/// `__elephc_proc_open_pipes` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_proc_open_pipes() -> Stmt {
    function("__elephc_proc_open_pipes")
        .param("process", t_mixed())
        .param("previous", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_var("process"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_var("previous")),
                ],
                vec![],
                None,
            ),
            s_assign("id", e_call("__elephc_resource_payload", vec![e_var("process")])),
            s_assign("pipes", e_array(vec![])),
            s_assign("count", e_call("elephc_proc_pipe_count", vec![e_var("id")])),
            s_for(Some(s_assign("position", e_int(0))), Some(e_binop(e_var("position"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("position"))), vec![
                s_array_assign("pipes", e_call("elephc_proc_pipe_index", vec![e_var("id"), e_var("position")]), e_call("__elephc_fd_stream", vec![e_call("elephc_proc_pipe_fd", vec![e_var("id"), e_var("position")])])),
            ]),
            s_return(e_var("pipes")),
        ])
        .build()
}
