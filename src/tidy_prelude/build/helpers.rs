//! Purpose:
//! Internal helpers of the tidy prelude: diagnostics, configuration/encoding coercion,
//! document creation, parse/repair plumbing, and argument validation used by the public
//! `tidy_*` functions.
//!
//! Called from:
//! - The public tidy functions built in `crate::tidy_prelude::build::functions`; the
//!   helpers reach them as ordinary PHP function calls in the built AST.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form kept in `crate::tidy_prelude::fragments`; the oracle
//!   test compares both node by node.
//! - Local variables are `$_`-prefixed so method/function locals cannot collide with
//!   user globals in the checker's name-keyed analyses. Parameter names stay public.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `__elephc_tidy_raise` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_raise() -> Stmt {
    function("__elephc_tidy_raise")
        .param("message", TypeExpr::Str)
        .param("throw", TypeExpr::Bool)
        .param_default("notice", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Void)
        .body(vec![
            // php-src's EH_THROW window converts warnings only; a configuration notice is
            // displayed and the constructor continues. Warnings become base `Exception`.
            s_if(
                e_var("notice"),
                vec![
                    s_expr(e_call("trigger_error", vec![e_var("message"), e_const("E_USER_NOTICE")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("throw"),
                vec![s_throw(e_new("Exception", vec![e_var("message")]))],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_builtin_warning", vec![e_var("message")])),
        ])
        .build()
}

/// `__elephc_tidy_numeric_prefix` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_numeric_prefix() -> Stmt {
    function("__elephc_tidy_numeric_prefix")
        .param("text", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float, TypeExpr::False]))
        .body(vec![
            s_assign("_length", e_call("strlen", vec![e_var("text")])),
            s_assign("_index", e_int(0)),
            s_while(e_binop(e_binop(e_var("_index"), BinOp::Lt, e_var("_length")), BinOp::And, e_binop(e_call("strpos", vec![e_str(" \t\n\r\u{b}\u{c}"), e_index(e_var("text"), e_var("_index"))]), BinOp::StrictNotEq, e_bool(false))), vec![
                s_expr(e_post_inc("_index")),
            ]),
            s_if(
                e_binop(e_binop(e_var("_index"), BinOp::Lt, e_var("_length")), BinOp::And, e_binop(e_binop(e_index(e_var("text"), e_var("_index")), BinOp::StrictEq, e_str("+")), BinOp::Or, e_binop(e_index(e_var("text"), e_var("_index")), BinOp::StrictEq, e_str("-")))),
                vec![
                    s_expr(e_post_inc("_index")),
                ],
                vec![],
                None,
            ),
            s_assign("_digits", e_int(0)),
            s_while(e_binop(e_binop(e_binop(e_var("_index"), BinOp::Lt, e_var("_length")), BinOp::And, e_binop(e_index(e_var("text"), e_var("_index")), BinOp::GtEq, e_str("0"))), BinOp::And, e_binop(e_index(e_var("text"), e_var("_index")), BinOp::LtEq, e_str("9"))), vec![
                s_expr(e_post_inc("_index")),
                s_expr(e_post_inc("_digits")),
            ]),
            s_assign("_is_float", e_bool(false)),
            s_if(
                e_binop(e_binop(e_var("_index"), BinOp::Lt, e_var("_length")), BinOp::And, e_binop(e_index(e_var("text"), e_var("_index")), BinOp::StrictEq, e_str("."))),
                vec![
                    s_assign("_is_float", e_bool(true)),
                    s_expr(e_post_inc("_index")),
                    s_while(e_binop(e_binop(e_binop(e_var("_index"), BinOp::Lt, e_var("_length")), BinOp::And, e_binop(e_index(e_var("text"), e_var("_index")), BinOp::GtEq, e_str("0"))), BinOp::And, e_binop(e_index(e_var("text"), e_var("_index")), BinOp::LtEq, e_str("9"))), vec![
                        s_expr(e_post_inc("_index")),
                        s_expr(e_post_inc("_digits")),
                    ]),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_digits"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("_index"), BinOp::Lt, e_var("_length")), BinOp::And, e_binop(e_binop(e_index(e_var("text"), e_var("_index")), BinOp::StrictEq, e_str("e")), BinOp::Or, e_binop(e_index(e_var("text"), e_var("_index")), BinOp::StrictEq, e_str("E")))),
                vec![
                    s_assign("_exponent", e_binop(e_var("_index"), BinOp::Add, e_int(1))),
                    s_if(
                        e_binop(e_binop(e_var("_exponent"), BinOp::Lt, e_var("_length")), BinOp::And, e_binop(e_binop(e_index(e_var("text"), e_var("_exponent")), BinOp::StrictEq, e_str("+")), BinOp::Or, e_binop(e_index(e_var("text"), e_var("_exponent")), BinOp::StrictEq, e_str("-")))),
                        vec![
                            s_expr(e_post_inc("_exponent")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_exponent_digits", e_int(0)),
                    s_while(e_binop(e_binop(e_binop(e_var("_exponent"), BinOp::Lt, e_var("_length")), BinOp::And, e_binop(e_index(e_var("text"), e_var("_exponent")), BinOp::GtEq, e_str("0"))), BinOp::And, e_binop(e_index(e_var("text"), e_var("_exponent")), BinOp::LtEq, e_str("9"))), vec![
                        s_expr(e_post_inc("_exponent")),
                        s_expr(e_post_inc("_exponent_digits")),
                    ]),
                    s_if(
                        e_binop(e_var("_exponent_digits"), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("_is_float", e_bool(true)),
                            s_assign("_index", e_var("_exponent")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("_prefix", e_call("substr", vec![e_var("text"), e_int(0), e_var("_index")])),
            s_return(e_ternary(e_var("_is_float"), e_cast(CastType::Float, e_var("_prefix")), e_cast(CastType::Int, e_var("_prefix")))),
        ])
        .build()
}

/// `__elephc_tidy_config_argument` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_config_argument() -> Stmt {
    function("__elephc_tidy_config_argument")
        .param("config", t_mixed())
        .param("label", TypeExpr::Str)
        .returns(t_union(vec![t_array(), TypeExpr::Str, TypeExpr::Void]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_var("config"), BinOp::StrictEq, e_null()), BinOp::Or, e_call("is_array", vec![e_var("config")])), BinOp::Or, e_call("is_string", vec![e_var("config")])),
                vec![
                    s_return(e_var("config")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("is_int", vec![e_var("config")]), BinOp::Or, e_call("is_float", vec![e_var("config")])), BinOp::Or, e_call("is_bool", vec![e_var("config")])),
                vec![
                    s_return(e_cast(CastType::String, e_var("config"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_object", vec![e_var("config")]), BinOp::And, e_call("is_callable", vec![e_array(vec![e_var("config"), e_str("__toString")])])),
                vec![
                    s_return(e_cast(CastType::String, e_var("config"))),
                ],
                vec![],
                None,
            ),
            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #2 ($config) must be of type array|string|null, ")), BinOp::Concat, e_call("__elephc_tidy_type_name", vec![e_var("config")])), BinOp::Concat, e_str(" given"))])),
        ])
        .build()
}

/// `__elephc_tidy_encoding_argument` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_encoding_argument() -> Stmt {
    function("__elephc_tidy_encoding_argument")
        .param("encoding", t_mixed())
        .param("label", TypeExpr::Str)
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_if(
                e_binop(e_var("encoding"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("encoding")]),
                vec![
                    s_return(e_var("encoding")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("is_int", vec![e_var("encoding")]), BinOp::Or, e_call("is_float", vec![e_var("encoding")])), BinOp::Or, e_call("is_bool", vec![e_var("encoding")])),
                vec![
                    s_return(e_cast(CastType::String, e_var("encoding"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_object", vec![e_var("encoding")]), BinOp::And, e_call("is_callable", vec![e_array(vec![e_var("encoding"), e_str("__toString")])])),
                vec![
                    s_return(e_cast(CastType::String, e_var("encoding"))),
                ],
                vec![],
                None,
            ),
            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #3 ($encoding) must be of type ?string, ")), BinOp::Concat, e_call("__elephc_tidy_type_name", vec![e_var("encoding")])), BinOp::Concat, e_str(" given"))])),
        ])
        .build()
}

/// `__elephc_tidy_open_basedir_message` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_open_basedir_message() -> Stmt {
    function("__elephc_tidy_open_basedir_message")
        .param("path", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("_open", e_call("__elephc_tidy_ini_open_basedir", vec![])),
            s_if(
                e_binop(e_var("_open"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_resolved", e_call("realpath", vec![e_var("path")])),
            s_if(
                e_binop(e_var("_resolved"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("path"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_index(e_var("path"), e_int(0)), BinOp::StrictEq, e_str("/"))),
                        vec![
                            s_assign("_resolved", e_var("path")),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("_cwd", e_call("getcwd", vec![])),
                        s_assign("_resolved", e_ternary(e_binop(e_var("_cwd"), BinOp::StrictEq, e_bool(false)), e_var("path"), e_binop(e_binop(e_var("_cwd"), BinOp::Concat, e_str("/")), BinOp::Concat, e_var("path")))),
                    ]),
                    ),
                    s_assign("_parts", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str("/"), e_var("_resolved")]), None, "_part", vec![
                        s_if(
                            e_binop(e_binop(e_var("_part"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("_part"), BinOp::StrictEq, e_str("."))),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_part"), BinOp::StrictEq, e_str("..")),
                            vec![
                                s_expr(e_call("array_pop", vec![e_var("_parts")])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_array_push("_parts", e_var("_part")),
                    ]),
                    s_assign("_resolved", e_binop(e_str("/"), BinOp::Concat, e_call("implode", vec![e_str("/"), e_var("_parts")]))),
                ],
                vec![],
                None,
            ),
            s_foreach(e_call("explode", vec![e_str(":"), e_var("_open")]), None, "_root", vec![
                s_if(
                    e_binop(e_var("_root"), BinOp::StrictEq, e_str("")),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_root_resolved", e_call("realpath", vec![e_var("_root")])),
                s_if(
                    e_binop(e_var("_root_resolved"), BinOp::StrictEq, e_bool(false)),
                    vec![
                        s_assign("_root_resolved", e_var("_root")),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_prefix", e_binop(e_call("rtrim", vec![e_var("_root_resolved"), e_str("/")]), BinOp::Concat, e_str("/"))),
                s_if(
                    e_binop(e_binop(e_var("_resolved"), BinOp::StrictEq, e_var("_root_resolved")), BinOp::Or, e_binop(e_call("strpos", vec![e_var("_resolved"), e_var("_prefix")]), BinOp::StrictEq, e_int(0))),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_binop(e_binop(e_binop(e_binop(e_str("open_basedir restriction in effect. File("), BinOp::Concat, e_var("path")), BinOp::Concat, e_str(") is not within the allowed path(s): (")), BinOp::Concat, e_var("_open")), BinOp::Concat, e_str(")"))),
        ])
        .build()
}

/// `__elephc_tidy_load_config_file` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_load_config_file() -> Stmt {
    function("__elephc_tidy_load_config_file")
        .param("doc", TypeExpr::Int)
        .param("path", TypeExpr::Str)
        .param("label", TypeExpr::Str)
        .param("throw", TypeExpr::Bool)
        .param_default("check_basedir", TypeExpr::Bool, e_bool(true))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_var("check_basedir"),
                vec![
                    s_assign("_basedir", e_call("__elephc_tidy_open_basedir_message", vec![e_var("path")])),
                    s_if(
                        e_binop(e_var("_basedir"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_expr(e_call("__elephc_tidy_raise", vec![e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("_basedir")), e_var("throw")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("_status", e_call("elephc_tidy_load_config", vec![e_var("doc"), e_var("path")])),
            s_if(
                e_binop(e_var("_status"), BinOp::Lt, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_tidy_raise", vec![e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Could not load the Tidy configuration file \"")), BinOp::Concat, e_var("path")), BinOp::Concat, e_str("\"")), e_var("throw")])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_status"), BinOp::Gt, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_tidy_raise", vec![e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": There were errors while parsing the Tidy configuration file \"")), BinOp::Concat, e_var("path")), BinOp::Concat, e_str("\"")), e_var("throw"), e_bool(true)])),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_tidy_caller_label` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_caller_label() -> Stmt {
    function("__elephc_tidy_caller_label")
        .returns(TypeExpr::Str)
        .body(vec![
            s_foreach(e_call("debug_backtrace", vec![e_const("DEBUG_BACKTRACE_IGNORE_ARGS"), e_int(6)]), None, "_frame", vec![
                s_assign("_function", e_null_coalesce(e_index(e_var("_frame"), e_str("function")), e_str(""))),
                s_assign("_class", e_null_coalesce(e_index(e_var("_frame"), e_str("class")), e_str(""))),
                s_if(
                    e_binop(e_binop(e_var("_function"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("_function"), BinOp::StrictEq, e_str("{main}"))),
                    vec![
                        s_return(e_str("main()")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("strpos", vec![e_var("_function"), e_str("__elephc_tidy")]), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_var("_class"), BinOp::StrictEq, e_str("tidy")), BinOp::And, e_binop(e_binop(e_var("_function"), BinOp::StrictEq, e_str("__construct")), BinOp::Or, e_binop(e_var("_function"), BinOp::StrictEq, e_str("__elephc_wrap")))),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("_class"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_return(e_binop(e_binop(e_binop(e_var("_class"), BinOp::Concat, e_str("::")), BinOp::Concat, e_var("_function")), BinOp::Concat, e_str("()"))),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_binop(e_var("_function"), BinOp::Concat, e_str("()"))),
            ]),
            s_return(e_str("main()")),
        ])
        .build()
}

/// `__elephc_tidy_load_default_config` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_load_default_config() -> Stmt {
    function("__elephc_tidy_load_default_config")
        .param("doc", TypeExpr::Int)
        .param_default("label", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_path", e_call("__elephc_tidy_ini_default_config", vec![])),
            s_if(
                e_binop(e_var("_path"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("label"), BinOp::StrictEq, e_null()),
                vec![
                    s_assign("label", e_call("__elephc_tidy_caller_label", vec![])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_tidy_load_config_file", vec![e_var("doc"), e_var("_path"), e_cast(CastType::String, e_var("label")), e_bool(false), e_bool(false)])),
        ])
        .build()
}

/// `__elephc_tidy_new_doc` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_new_doc() -> Stmt {
    function("__elephc_tidy_new_doc")
        .param("label", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_doc", e_call("elephc_tidy_create", vec![])),
            s_if(
                e_binop(e_var("_doc"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_throw(e_new("Exception", vec![e_str("Could not create tidy document")])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_tidy_load_default_config", vec![e_var("_doc"), e_var("label")])),
            s_return(e_var("_doc")),
        ])
        .build()
}

/// `__elephc_tidy_apply_config` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_apply_config() -> Stmt {
    function("__elephc_tidy_apply_config")
        .param("doc", TypeExpr::Int)
        .param("config", t_mixed())
        .param("label", TypeExpr::Str)
        .param("throw", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("config", e_call("__elephc_tidy_config_argument", vec![e_var("config"), e_var("label")])),
            s_if(
                e_call("is_array", vec![e_var("config")]),
                vec![
                    s_if(
                        e_binop(e_var("config"), BinOp::StrictEq, e_array(vec![])),
                        vec![
                            s_throw(e_new("TypeError", vec![e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #2 ($config) must be of type array with keys as string"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_failures", e_bool(false)),
                    s_foreach(e_var("config"), Some("_key"), "_value", vec![
                        s_if(
                            e_not(e_call("is_string", vec![e_var("_key")])),
                            vec![
                                s_throw(e_new("TypeError", vec![e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #2 ($config) must be of type array with keys as string"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_index", e_call("elephc_tidy_option_index", vec![e_var("doc"), e_var("_key")])),
                        s_if(
                            e_binop(e_var("_index"), BinOp::Lt, e_int(0)),
                            vec![
                                s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #2 ($config) Unknown Tidy configuration option \"")), BinOp::Concat, e_var("_key")), BinOp::Concat, e_str("\""))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_call("elephc_tidy_option_internal", vec![e_var("doc"), e_var("_index")]), BinOp::StrictNotEq, e_int(0)),
                            vec![
                                s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #2 ($config) Attempting to set read-only option \"")), BinOp::Concat, e_var("_key")), BinOp::Concat, e_str("\""))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_type", e_call("elephc_tidy_option_type", vec![e_var("doc"), e_var("_index")])),
                        s_if(
                            e_binop(e_var("_type"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_assign("_text", e_cast(CastType::String, e_var("_value"))),
                                s_if(
                                    e_binop(e_call("elephc_tidy_set_option_str", vec![e_var("doc"), e_var("_key"), e_var("_text")]), BinOp::StrictEq, e_int(0)),
                                    vec![
                                        s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #2 ($config) option \"")), BinOp::Concat, e_var("_key")), BinOp::Concat, e_str("\" does not accept \"")), BinOp::Concat, e_var("_text")), BinOp::Concat, e_str("\" as a value"))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![
                            (e_binop(e_var("_type"), BinOp::StrictEq, e_int(1)), vec![
                                s_if(
                                    e_binop(e_call("is_string", vec![e_var("_value")]), BinOp::Or, e_call("is_object", vec![e_var("_value")])),
                                    vec![
                                        s_assign("_text", e_cast(CastType::String, e_var("_value"))),
                                        s_assign("_number", e_call("__elephc_tidy_numeric_prefix", vec![e_var("_text")])),
                                        s_if(
                                            e_binop(e_var("_number"), BinOp::StrictNotEq, e_bool(false)),
                                            vec![
                                                s_if(
                                                    e_binop(e_call("elephc_tidy_set_option_int", vec![e_var("doc"), e_var("_key"), e_cast(CastType::Int, e_var("_number"))]), BinOp::StrictEq, e_int(0)),
                                                    vec![
                                                        s_assign("_failures", e_bool(true)),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                            ],
                                            vec![
                                            (e_binop(e_call("elephc_tidy_set_option_str", vec![e_var("doc"), e_var("_key"), e_var("_text")]), BinOp::StrictEq, e_int(0)), vec![
                                                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #2 ($config) option \"")), BinOp::Concat, e_var("_key")), BinOp::Concat, e_str("\" does not accept \"")), BinOp::Concat, e_var("_text")), BinOp::Concat, e_str("\" as a value"))])),
                                            ]),
                                        ],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_if(
                                        e_binop(e_call("is_float", vec![e_var("_value")]), BinOp::And, e_binop(e_call("is_nan", vec![e_var("_value")]), BinOp::Or, e_call("is_infinite", vec![e_var("_value")]))),
                                        vec![
                                            s_if(
                                                e_call("is_nan", vec![e_var("_value")]),
                                                vec![
                                                    s_assign("_float_name", e_str("NAN")),
                                                ],
                                                vec![
                                                (e_binop(e_var("_value"), BinOp::Gt, e_int(0)), vec![
                                                    s_assign("_float_name", e_str("INF")),
                                                ]),
                                            ],
                                                Some(vec![
                                                s_assign("_float_name", e_str("-INF")),
                                            ]),
                                            ),
                                            s_expr(e_call("__elephc_builtin_warning", vec![e_binop(e_binop(e_str("The float "), BinOp::Concat, e_var("_float_name")), BinOp::Concat, e_str(" is not representable as an int, cast occurred"))])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_call("elephc_tidy_set_option_int", vec![e_var("doc"), e_var("_key"), e_cast(CastType::Int, e_var("_value"))]), BinOp::StrictEq, e_int(0)),
                                        vec![
                                            s_assign("_failures", e_bool(true)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                                ),
                            ]),
                        ],
                            Some(vec![
                            s_if(
                                e_binop(e_call("elephc_tidy_set_option_bool", vec![e_var("doc"), e_var("_key"), e_cast(CastType::Int, e_var("_value"))]), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_assign("_failures", e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                        ),
                    ]),
                    s_return(e_not(e_var("_failures"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("config")]),
                vec![
                    s_return(e_call("__elephc_tidy_load_config_file", vec![e_var("doc"), e_var("config"), e_var("label"), e_var("throw")])),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_tidy_set_encoding` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_set_encoding() -> Stmt {
    function("__elephc_tidy_set_encoding")
        .param("doc", TypeExpr::Int)
        .param("encoding", t_mixed())
        .param("label", TypeExpr::Str)
        .param("throw", TypeExpr::Bool)
        .param_default("skip_empty", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("encoding", e_call("__elephc_tidy_encoding_argument", vec![e_var("encoding"), e_var("label")])),
            s_if(
                e_binop(e_binop(e_var("encoding"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("skip_empty"), BinOp::And, e_binop(e_var("encoding"), BinOp::StrictEq, e_str("")))),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_tidy_set_encoding", vec![e_var("doc"), e_var("encoding")]), BinOp::Lt, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_tidy_raise", vec![e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Could not set encoding \"")), BinOp::Concat, e_var("encoding")), BinOp::Concat, e_str("\"")), e_var("throw")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_tidy_parse_buffer` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_parse_buffer() -> Stmt {
    function("__elephc_tidy_parse_buffer")
        .param("doc", TypeExpr::Int)
        .param("data", TypeExpr::Str)
        .param("label", TypeExpr::Str)
        .param("throw", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("strlen", vec![e_var("data")]), BinOp::Gt, e_int(4294967295)),
                vec![
                    s_throw(e_new("ValueError", vec![e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #1 ($string) is too long"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_tidy_parse", vec![e_var("doc"), e_var("data"), e_call("strlen", vec![e_var("data")])]), BinOp::StrictEq, e_int(0)),
                vec![
                    s_assign("_error", e_call("elephc_tidy_error_buffer", vec![e_var("doc")])),
                    s_if(
                        e_binop(e_var("_error"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_expr(e_call("__elephc_tidy_raise", vec![e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("_error")), e_var("throw")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_tidy_parse_into` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_parse_into() -> Stmt {
    function("__elephc_tidy_parse_into")
        .param("doc", TypeExpr::Int)
        .param("data", TypeExpr::Str)
        .param("encoding", t_mixed())
        .param("label", TypeExpr::Str)
        .param("throw", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("encoding", e_call("__elephc_tidy_encoding_argument", vec![e_var("encoding"), e_var("label")])),
            s_if(
                e_not(e_call("__elephc_tidy_set_encoding", vec![e_var("doc"), e_var("encoding"), e_var("label"), e_var("throw")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_tidy_parse_buffer", vec![e_var("doc"), e_var("data"), e_var("label"), e_var("throw")])),
        ])
        .build()
}

/// `__elephc_tidy_repair_parse_into` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_repair_parse_into() -> Stmt {
    function("__elephc_tidy_repair_parse_into")
        .param("doc", TypeExpr::Int)
        .param("data", TypeExpr::Str)
        .param("encoding", t_mixed())
        .param("label", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_expr(e_call("__elephc_tidy_set_encoding", vec![e_var("doc"), e_var("encoding"), e_var("label"), e_bool(false), e_bool(true)])),
            s_return(e_call("__elephc_tidy_parse_buffer", vec![e_var("doc"), e_var("data"), e_var("label"), e_bool(false)])),
        ])
        .build()
}

/// `__elephc_tidy_read_file` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_read_file() -> Stmt {
    function("__elephc_tidy_read_file")
        .param("filename", TypeExpr::Str)
        .param("use_include_path", TypeExpr::Bool)
        .param("label", TypeExpr::Str)
        .param("mode", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_var("filename"), BinOp::StrictEq, e_str("")),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("Path must not be empty")])),
                ],
                vec![],
                None,
            ),
            s_assign("_suffix", e_ternary(e_var("use_include_path"), e_str(" (using include path)"), e_str(""))),
            s_assign("_message", e_binop(e_binop(e_binop(e_str("Cannot load \""), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("\" into memory")), BinOp::Concat, e_var("_suffix"))),
            s_assign("_basedir", e_call("__elephc_tidy_open_basedir_message", vec![e_var("filename")])),
            s_if(
                e_binop(e_var("_basedir"), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_expr(e_call("__elephc_builtin_warning", vec![e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("_basedir"))])),
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_str("construct")),
                        vec![
                            s_throw(e_new("Exception", vec![e_var("_message")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictNotEq, e_str("repair")),
                        vec![
                            s_expr(e_call("__elephc_builtin_warning", vec![e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("_message"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_contents", e_error_suppress(e_call("file_get_contents", vec![e_var("filename"), e_var("use_include_path")]))),
            s_if(
                e_binop(e_var("_contents"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_str("construct")),
                        vec![
                            s_throw(e_new("Exception", vec![e_var("_message")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_str("repair")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_builtin_warning", vec![e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("_message"))])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("strlen", vec![e_var("_contents")]), BinOp::Gt, e_int(4294967295)),
                vec![
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_str("repair")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #1 ($filename) File content is too long"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("ValueError", vec![e_str("File content is too long")])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("_contents")),
        ])
        .build()
}

/// `__elephc_tidy_sync_properties` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_sync_properties() -> Stmt {
    function("__elephc_tidy_sync_properties")
        .param("tidy", t_class("tidy"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("_doc", e_method_call(e_var("tidy"), "__elephc_doc_handle", vec![])),
            s_if(
                e_binop(e_call("elephc_tidy_output_present", vec![e_var("_doc")]), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_prop_assign(e_var("tidy"), "value", e_call("elephc_tidy_output", vec![e_var("_doc")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_tidy_error_buffer_present", vec![e_var("_doc")]), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_prop_assign(e_var("tidy"), "errorBuffer", e_call("elephc_tidy_error_buffer", vec![e_var("_doc")])),
                ],
                vec![],
                None,
            ),
        ])
        .build()
}

/// `__elephc_tidy_get_option` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_get_option() -> Stmt {
    function("__elephc_tidy_get_option")
        .param("doc", TypeExpr::Int)
        .param("option", TypeExpr::Str)
        .param("label", TypeExpr::Str)
        .param("argument", TypeExpr::Int)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Int, TypeExpr::Bool]))
        .body(vec![
            s_assign("_index", e_call("elephc_tidy_option_index", vec![e_var("doc"), e_var("option")])),
            s_if(
                e_binop(e_var("_index"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #")), BinOp::Concat, e_var("argument")), BinOp::Concat, e_str(" ($option) is an invalid configuration option, \"")), BinOp::Concat, e_var("option")), BinOp::Concat, e_str("\" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_type", e_call("elephc_tidy_option_type", vec![e_var("doc"), e_var("_index")])),
            s_if(
                e_binop(e_var("_type"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_call("elephc_tidy_option_value_str", vec![e_var("doc"), e_var("_index")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_type"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_return(e_call("elephc_tidy_option_value_int", vec![e_var("doc"), e_var("_index")])),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_call("elephc_tidy_option_value_bool", vec![e_var("doc"), e_var("_index")]), BinOp::StrictNotEq, e_int(0))),
        ])
        .build()
}

/// `__elephc_tidy_get_option_doc` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_get_option_doc() -> Stmt {
    function("__elephc_tidy_get_option_doc")
        .param("doc", TypeExpr::Int)
        .param("option", TypeExpr::Str)
        .param("label", TypeExpr::Str)
        .param("argument", TypeExpr::Int)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("_index", e_call("elephc_tidy_option_index", vec![e_var("doc"), e_var("option")])),
            s_if(
                e_binop(e_var("_index"), BinOp::Lt, e_int(0)),
                vec![
                    s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #")), BinOp::Concat, e_var("argument")), BinOp::Concat, e_str(" ($option) is an invalid configuration option, \"")), BinOp::Concat, e_var("option")), BinOp::Concat, e_str("\" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("elephc_tidy_option_doc_present", vec![e_var("doc"), e_var("_index")]), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_tidy_option_doc", vec![e_var("doc"), e_var("_index")])),
        ])
        .build()
}

/// `__elephc_tidy_type_name` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_type_name() -> Stmt {
    function("__elephc_tidy_type_name")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
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
                e_call("is_bool", vec![e_var("value")]),
                vec![
                    s_return(e_str("bool")),
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
            s_if(
                e_call("is_null", vec![e_var("value")]),
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
                e_call("is_resource", vec![e_var("value")]),
                vec![
                    s_return(e_str("resource")),
                ],
                vec![],
                None,
            ),
            s_return(e_call("gettype", vec![e_var("value")])),
        ])
        .build()
}

/// `__elephc_tidy_document` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_document() -> Stmt {
    function("__elephc_tidy_document")
        .param("value", t_mixed())
        .param("label", TypeExpr::Str)
        .returns(t_class("tidy"))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("value"), "tidy")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #1 ($tidy) must be of type tidy, ")), BinOp::Concat, e_call("__elephc_tidy_type_name", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("value")),
        ])
        .build()
}

/// `__elephc_tidy_repair_string` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_repair_string() -> Stmt {
    function("__elephc_tidy_repair_string")
        .param("string", TypeExpr::Str)
        .param("config", t_mixed())
        .param("encoding", t_mixed())
        .param("label", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_call("strlen", vec![e_var("string")]), BinOp::Gt, e_int(4294967295)),
                vec![
                    s_throw(e_new("ValueError", vec![e_binop(e_var("label"), BinOp::Concat, e_str(": Argument #1 ($string) is too long"))])),
                ],
                vec![],
                None,
            ),
            s_assign("_doc", e_call("__elephc_tidy_new_doc", vec![e_var("label")])),
            s_try(vec![
                s_assign("_config_ok", e_call("__elephc_tidy_apply_config", vec![e_var("_doc"), e_var("config"), e_var("label"), e_bool(false)])),
                s_if(
                    e_not(e_call("__elephc_tidy_repair_parse_into", vec![e_var("_doc"), e_var("string"), e_ternary(e_var("_config_ok"), e_var("encoding"), e_null()), e_var("label")])),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("elephc_tidy_clean_repair", vec![e_var("_doc")]), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_call("elephc_tidy_output", vec![e_var("_doc")])),
            ], vec![
            ], Some(vec![
                s_expr(e_call("elephc_tidy_release", vec![e_var("_doc")])),
            ])),
        ])
        .build()
}

/// `__elephc_tidy_repair_file` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_tidy_repair_file() -> Stmt {
    function("__elephc_tidy_repair_file")
        .param("filename", TypeExpr::Str)
        .param("config", t_mixed())
        .param("encoding", t_mixed())
        .param("useIncludePath", TypeExpr::Bool)
        .param("label", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("_contents", e_call("__elephc_tidy_read_file", vec![e_var("filename"), e_var("useIncludePath"), e_var("label"), e_str("repair")])),
            s_if(
                e_binop(e_var("_contents"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_doc", e_call("__elephc_tidy_new_doc", vec![e_var("label")])),
            s_try(vec![
                s_assign("_config_ok", e_call("__elephc_tidy_apply_config", vec![e_var("_doc"), e_var("config"), e_var("label"), e_bool(false)])),
                s_if(
                    e_not(e_call("__elephc_tidy_repair_parse_into", vec![e_var("_doc"), e_cast(CastType::String, e_var("_contents")), e_ternary(e_var("_config_ok"), e_var("encoding"), e_null()), e_var("label")])),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("elephc_tidy_clean_repair", vec![e_var("_doc")]), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_call("elephc_tidy_output", vec![e_var("_doc")])),
            ], vec![
            ], Some(vec![
                s_expr(e_call("elephc_tidy_release", vec![e_var("_doc")])),
            ])),
        ])
        .build()
}
