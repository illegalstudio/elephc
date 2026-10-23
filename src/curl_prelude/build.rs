//! Purpose:
//! Builds the curl standard-library surface as AST — the `CurlHandle`/`CurlMultiHandle`/
//! `CurlShareHandle`/`CurlSharePersistentHandle` classes and every `curl_*` function —
//! replacing the PHP source `crate::curl_prelude::CURL_PRELUDE_SRC` used to carry.
//!
//! Called from:
//! - `crate::curl_prelude::inject_if_used_for_version`, after include resolution and before
//!   name resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration here was generated from the parse of the
//!   PHP it replaces (`synthetic_class::transcribe`), and the migration oracle
//!   (`ELEPHC_ORACLE_PHP` / `ELEPHC_ORACLE_WHICH=curl`) compares the built AST against that
//!   parse node by node. `CURL_PRELUDE_SRC` stays the migration oracle.
//! - The PHP-version fences become the `php_version >= PhpVersion::Php85` conditional in
//!   `curl_declarations`: `curl_multi_get_handles`, `CurlSharePersistentHandle` and
//!   `curl_share_init_persistent` exist only from 8.5.

#![allow(dead_code)]
#![allow(clippy::too_many_lines)]

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::php_version::PhpVersion;
use crate::synthetic_class::*;

/// `CurlHandle` — transcribed from the PHP form.
fn decl_class_curlhandle() -> Stmt {
    class("CurlHandle")
        .final_()
        .prop("__elephc_push_user", t_mixed(), Some(e_null()))
        .static_prop("__elephc_pushed", t_array(), Some(e_array(vec![])))
        .prop("__elephc_handle", t_mixed(), Some(e_null()))
        .prop("__elephc_return_transfer", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_private", t_mixed(), Some(e_bool(false)))
        .prop("__elephc_callbacks", t_array(), Some(e_array(vec![])))
        .prop("__elephc_write_user", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_file", t_mixed(), Some(e_null()))
        .prop("__elephc_writeheader", t_mixed(), Some(e_null()))
        .prop("__elephc_infile", t_mixed(), Some(e_null()))
        .prop("__elephc_stderr", t_mixed(), Some(e_null()))
        .prop("__elephc_read_user", t_mixed(), Some(e_null()))
        .prop("__elephc_prereq_user", t_mixed(), Some(e_null()))
        .prop("__elephc_debug_user", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_debug_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_header_out", t_mixed(), Some(e_null()))
        .prop("__elephc_header_out_enabled", TypeExpr::Bool, Some(e_bool(false)))
        .private_static_prop("__elephc_minting", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .body(vec![
                    s_if(
                        e_not(e_self_static_prop("__elephc_minting")),
                        vec![
                            s_throw(e_new_fq("Error", vec![e_str("Cannot directly construct CurlHandle, use curl_init() instead")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_wrap")
                .static_()
                .param("raw", t_mixed())
                .returns(t_class("CurlHandle"))
                .body(vec![
                    s_self_static_prop_assign("__elephc_minting", e_bool(true)),
                    s_assign("h", e_new_self(vec![])),
                    s_self_static_prop_assign("__elephc_minting", e_bool(false)),
                    s_prop_assign(e_var("h"), "__elephc_handle", e_var("raw")),
                    s_return(e_var("h")),
                ]),
        )
        .method(
            method("__debugInfo")
                .returns(t_array())
                .body(vec![
                    s_return(e_array(vec![])),
                ]),
        )
        .method(
            method("__serialize")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new_fq("Exception", vec![e_str("Serialization of 'CurlHandle' is not allowed")])),
                ]),
        )
        .build()
}

/// `curl_init` — transcribed from the PHP form.
fn decl_fn_curl_init() -> Stmt {
    function("curl_init")
        .param_default("url", t_nullable(TypeExpr::Str), e_null())
        .returns(t_class("CurlHandle"))
        .body(vec![
            s_assign("raw", e_call("__elephc_curl_easy_init", vec![])),
            s_if(
                e_binop(e_var("raw"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new_fq("RuntimeException", vec![e_str("curl_init(): libcurl could not allocate an easy handle")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("url"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_assign("handle", e_static_call("CurlHandle", "__elephc_wrap", vec![e_var("raw")])),
                    s_expr(e_call("curl_setopt", vec![e_var("handle"), e_const("CURLOPT_URL"), e_var("url")])),
                    s_return(e_var("handle")),
                ],
                vec![],
                None,
            ),
            s_return(e_static_call("CurlHandle", "__elephc_wrap", vec![e_var("raw")])),
        ])
        .build()
}

/// `__elephc_curl_slist_option_name` — transcribed from the PHP form.
fn decl_fn_elephc_curl_slist_option_name() -> Stmt {
    function("__elephc_curl_slist_option_name")
        .param("option", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10023)),
                vec![
                    s_return(e_str("CURLOPT_HTTPHEADER")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10104)),
                vec![
                    s_return(e_str("CURLOPT_HTTP200ALIASES")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10243)),
                vec![
                    s_return(e_str("CURLOPT_CONNECT_TO")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10187)),
                vec![
                    s_return(e_str("CURLOPT_MAIL_RCPT")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10039)),
                vec![
                    s_return(e_str("CURLOPT_POSTQUOTE")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10093)),
                vec![
                    s_return(e_str("CURLOPT_PREQUOTE")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10228)),
                vec![
                    s_return(e_str("CURLOPT_PROXYHEADER")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10028)),
                vec![
                    s_return(e_str("CURLOPT_QUOTE")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10203)),
                vec![
                    s_return(e_str("CURLOPT_RESOLVE")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(10070)),
                vec![
                    s_return(e_str("CURLOPT_TELNETOPTIONS")),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_str("CURLOPT_"), BinOp::Concat, e_var("option"))),
        ])
        .build()
}

/// `__elephc_curl_nullable_string_option` — transcribed from the PHP form.
fn decl_fn_elephc_curl_nullable_string_option() -> Stmt {
    function("__elephc_curl_nullable_string_option")
        .param("option", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_int(10036)), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10017))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10007))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10134))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10190))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10102))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10221))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10222))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10223))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10220))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10231))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10279))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10300))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10063))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10005))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10173))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10174))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10328)))),
        ])
        .build()
}

/// `__elephc_curl_given_type` — transcribed from the PHP form.
fn decl_fn_elephc_curl_given_type() -> Stmt {
    function("__elephc_curl_given_type")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_return(e_str("string")),
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
                e_call("is_bool", vec![e_var("value")]),
                vec![
                    s_return(e_str("bool")),
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
                e_call("is_object", vec![e_var("value")]),
                vec![
                    s_return(e_call("get_class", vec![e_var("value")])),
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
            s_return(e_call("gettype", vec![e_var("value")])),
        ])
        .build()
}

/// `__elephc_curl_open_basedir` — transcribed from the PHP form.
fn decl_fn_elephc_curl_open_basedir() -> Stmt {
    function("__elephc_curl_open_basedir")
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("value", e_call("ini_get", vec![e_str("open_basedir")])),
            s_return(e_ternary(e_call("is_string", vec![e_var("value")]), e_var("value"), e_str(""))),
        ])
        .build()
}

/// `__elephc_curl_open_basedir_is_set` — transcribed from the PHP form.
fn decl_fn_elephc_curl_open_basedir_is_set() -> Stmt {
    function("__elephc_curl_open_basedir_is_set")
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_call("__elephc_curl_open_basedir", vec![]), BinOp::StrictNotEq, e_str(""))),
        ])
        .build()
}

/// `__elephc_curl_open_basedir_warn` — transcribed from the PHP form.
fn decl_fn_elephc_curl_open_basedir_warn() -> Stmt {
    function("__elephc_curl_open_basedir_warn")
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_call("trigger_error", vec![e_var("message"), e_int(512)])),
        ])
        .build()
}

/// `__elephc_curl_path_within_open_basedir` — transcribed from the PHP form.
fn decl_fn_elephc_curl_path_within_open_basedir() -> Stmt {
    function("__elephc_curl_path_within_open_basedir")
        .param("path", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("allowed", e_call("__elephc_curl_open_basedir", vec![])),
            s_if(
                e_binop(e_var("allowed"), BinOp::StrictEq, e_str("")),
                vec![s_return(e_bool(true))],
                vec![],
                None,
            ),
            // Judge the FILE THE PATH NAMES, not its text: php-src's
            // `php_check_open_basedir` runs the path through the realpath expansion, so
            // `/srv/app/../private/x` and a symlink inside the allowed tree are judged by
            // what they resolve to.
            s_assign("resolved", e_call("realpath", vec![e_var("path")])),
            s_if(
                e_binop(e_var("resolved"), BinOp::StrictEq, e_bool(false)),
                vec![
                    // A file that does not exist yet (a cookie jar) is judged by its
                    // parent directory.
                    s_assign("parent", e_call("realpath", vec![e_call("dirname", vec![e_var("path")])])),
                    s_if(
                        e_binop(e_var("parent"), BinOp::StrictEq, e_bool(false)),
                        vec![s_return(e_bool(false))],
                        vec![],
                        None,
                    ),
                    s_assign(
                        "resolved",
                        e_binop(
                            e_binop(e_var("parent"), BinOp::Concat, e_str("/")),
                            BinOp::Concat,
                            e_call("basename", vec![e_var("path")]),
                        ),
                    ),
                ],
                vec![],
                None,
            ),
            // Both supported platforms use ':' as PATH_SEPARATOR, and the directive lists
            // its roots separated by it (`PATH_SEPARATOR` is not in the constant catalog).
            s_foreach(
                e_call("explode", vec![e_str(":"), e_var("allowed")]),
                None,
                "root",
                vec![
                    s_assign("real", e_call("realpath", vec![e_var("root")])),
                    s_if(
                        e_binop(e_var("real"), BinOp::StrictEq, e_bool(false)),
                        vec![s_assign("real", e_var("root"))],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("real"), BinOp::StrictEq, e_str("/")),
                        vec![s_return(e_bool(true))],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(
                            e_binop(e_call("strlen", vec![e_var("real")]), BinOp::Gt, e_int(1)),
                            BinOp::And,
                            e_binop(
                                e_call("substr", vec![e_var("real"), e_binop(e_call("strlen", vec![e_var("real")]), BinOp::Sub, e_int(1))]),
                                BinOp::StrictEq,
                                e_str("/"),
                            ),
                        ),
                        vec![s_assign("real", e_call("substr", vec![e_var("real"), e_int(0), e_binop(e_call("strlen", vec![e_var("real")]), BinOp::Sub, e_int(1))]))],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(
                            e_binop(e_var("resolved"), BinOp::StrictEq, e_var("real")),
                            BinOp::Or,
                            e_call("str_starts_with", vec![e_var("resolved"), e_binop(e_var("real"), BinOp::Concat, e_str("/"))]),
                        ),
                        vec![s_return(e_bool(true))],
                        vec![],
                        None,
                    ),
                ],
            ),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `__elephc_curl_strip_file_protocol` — transcribed from the PHP form.
fn decl_fn_elephc_curl_strip_file_protocol() -> Stmt {
    function("__elephc_curl_strip_file_protocol")
        .param("raw", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_not(e_call("__elephc_curl_open_basedir_is_set", vec![])),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_curl_easy_setopt_long", vec![e_var("raw"), e_int(181), e_binop(e_const("CURLPROTO_ALL"), BinOp::Sub, e_const("CURLPROTO_FILE"))])),
        ])
        .build()
}

/// `curl_setopt` — transcribed from the PHP form.
fn decl_fn_curl_setopt() -> Stmt {
    function("curl_setopt")
        .param("handle", t_mixed())
        .param("option", t_mixed())
        .param("value", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("handle"), "CurlHandle")),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): Argument #1 ($handle) must be of type CurlHandle, "), BinOp::Concat, e_call("__elephc_curl_given_type", vec![e_var("handle")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("is_int", vec![e_var("option")])),
                vec![
                    s_if(
                        e_call("is_numeric", vec![e_var("option")]),
                        vec![
                            s_assign("option", e_cast(CastType::Int, e_var("option"))),
                        ],
                        vec![],
                        Some(vec![
                        s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): Argument #2 ($option) must be of type int, "), BinOp::Concat, e_call("__elephc_curl_given_type", vec![e_var("option")])), BinOp::Concat, e_str(" given"))])),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("option", e_cast(CastType::Int, e_var("option"))),
            s_if(
                e_call("__elephc_curl_open_basedir_is_set", vec![]),
                vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_int(181)), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(182))), BinOp::And, e_binop(e_binop(e_cast(CastType::Int, e_var("value")), BinOp::BitAnd, e_const("CURLPROTO_FILE")), BinOp::StrictNotEq, e_int(0))),
                        vec![
                            s_expr(e_call("__elephc_curl_open_basedir_warn", vec![e_str("curl_setopt(): CURLPROTO_FILE cannot be activated when an open_basedir is set")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_int(10318)), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10319))), BinOp::And, e_binop(e_binop(e_call("is_string", vec![e_var("value")]), BinOp::Or, e_call("is_int", vec![e_var("value")])), BinOp::Or, e_call("is_float", vec![e_var("value")]))),
                        vec![
                            s_assign("protocols", e_cast(CastType::String, e_var("value"))),
                            s_if(
                                e_binop(e_binop(e_call("stripos", vec![e_var("protocols"), e_str("file")]), BinOp::StrictNotEq, e_bool(false)), BinOp::Or, e_binop(e_call("stripos", vec![e_var("protocols"), e_str("all")]), BinOp::StrictNotEq, e_bool(false))),
                                vec![
                                    s_expr(e_call("__elephc_curl_open_basedir_warn", vec![e_str("curl_setopt(): The FILE protocol cannot be activated when an open_basedir is set")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("fileBasedirOptions", e_array(vec![e_int(10031), e_int(10082), e_int(10076), e_int(10025), e_int(10118), e_int(10152), e_int(10153), e_int(10183), e_int(10169), e_int(10170)])),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_int(10031)), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10082))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10076))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10025))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10118))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10152))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10153))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10183))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10169))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(10170))), BinOp::And, e_call("is_string", vec![e_var("value")])), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str(""))), BinOp::And, e_not(e_call("__elephc_curl_path_within_open_basedir", vec![e_var("value")]))),
                        vec![
                            s_expr(e_call("__elephc_curl_open_basedir_warn", vec![e_binop(e_binop(e_binop(e_binop(e_str("curl_setopt(): open_basedir restriction in effect. File("), BinOp::Concat, e_var("value")), BinOp::Concat, e_str(") is not within the allowed path(s): (")), BinOp::Concat, e_call("__elephc_curl_open_basedir", vec![])), BinOp::Concat, e_str(")"))])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_assign("kind", e_call("__elephc_curl_option_kind", vec![e_var("option")])),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("curl_setopt(): Argument #2 ($option) is not a valid cURL option")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_if(
                        e_prop(e_var("handle"), "__elephc_debug_installed"),
                        vec![
                            s_throw(e_new_fq("ValueError", vec![e_str("CURLINFO_HEADER_OUT option must not be set when the CURLOPT_DEBUGFUNCTION option is set")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_cast(CastType::Bool, e_var("value")),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_header_out_enabled", e_bool(true)),
                            s_prop_assign(e_var("handle"), "__elephc_header_out", e_null()),
                            s_expr(e_call("__elephc_curl_easy_setopt_long", vec![e_var("raw"), e_int(41), e_int(1)])),
                            s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(4), closure()
                                .param("ch", t_class("CurlHandle"))
                                .param("type", TypeExpr::Int)
                                .param("data", TypeExpr::Str)
                                .returns(TypeExpr::Int)
                                .body(vec![
                                    s_if(
                                        e_binop(e_var("type"), BinOp::StrictEq, e_int(2)),
                                        vec![
                                            s_prop_assign(e_var("ch"), "__elephc_header_out", e_var("data")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_int(0)),
                                ])
                                .build()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_var("handle"), "__elephc_header_out_enabled", e_bool(false)),
                    s_prop_assign(e_var("handle"), "__elephc_header_out", e_null()),
                    s_expr(e_call("__elephc_curl_easy_setopt_long", vec![e_var("raw"), e_int(41), e_int(0)])),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                vec![
                    s_if(
                        e_not(e_call("is_array", vec![e_var("value")])),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): The "), BinOp::Concat, e_call("__elephc_curl_slist_option_name", vec![e_var("option")])), BinOp::Concat, e_str(" option must have an array value"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("blob", e_str("")),
                    s_foreach(e_var("value"), None, "item", vec![
                        s_if(
                            e_binop(e_binop(e_call("is_array", vec![e_var("item")]), BinOp::Or, e_call("is_object", vec![e_var("item")])), BinOp::Or, e_call("is_null", vec![e_var("item")])),
                            vec![
                                s_throw(e_new_fq("TypeError", vec![e_str("curl_setopt(): Argument #3 ($value) must be an array of strings for this option")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("blob", e_binop(e_var("blob"), BinOp::Concat, e_binop(e_cast(CastType::String, e_var("item")), BinOp::Concat, e_str("\0")))),
                    ]),
                    s_return(e_call("__elephc_curl_easy_setopt_slist", vec![e_var("raw"), e_var("option"), e_var("blob")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_int(10015)), BinOp::And, e_call("is_array", vec![e_var("value")])),
                vec![
                    s_if(
                        e_binop(e_call("count", vec![e_var("value")]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_return(e_call("__elephc_curl_easy_setopt_str", vec![e_var("raw"), e_var("option"), e_str("")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_curl_build_multipart", vec![e_var("raw"), e_var("value")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)),
                vec![
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(19913)),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_return_transfer", e_cast(CastType::Bool, e_var("value"))),
                            s_prop_assign(e_var("handle"), "__elephc_write_user", e_bool(false)),
                            s_return(e_call("__elephc_curl_easy_setopt_long", vec![e_var("raw"), e_var("option"), e_ternary(e_var("value"), e_int(1), e_int(0))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(10103)),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_private", e_var("value")),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_neg(e_int(1))),
                        vec![
                            s_if(
                                e_not(e_var("value")),
                                vec![
                                    s_throw(e_new_fq("ValueError", vec![e_str("curl_setopt(): Disabling safe uploads is no longer supported")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(7)),
                vec![
                    s_assign("isShare", e_binop(e_instance_of(e_var("value"), "CurlShareHandle"), BinOp::Or, e_binop(e_call("is_object", vec![e_var("value")]), BinOp::And, e_binop(e_call("get_class", vec![e_var("value")]), BinOp::StrictEq, e_str("CurlSharePersistentHandle"))))),
                    s_if(
                        e_not(e_var("isShare")),
                        vec![
                            s_assign("given", e_ternary(e_call("is_object", vec![e_var("value")]), e_call("get_class", vec![e_var("value")]), e_ternary(e_call("is_null", vec![e_var("value")]), e_str("null"), e_call("gettype", vec![e_var("value")])))),
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): Argument #3 ($value) must be of type CurlShareHandle, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("shareRaw", e_prop(e_var("value"), "__elephc_handle")),
                    s_return(e_call("__elephc_curl_easy_set_share", vec![e_var("raw"), e_var("shareRaw")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(8)),
                vec![
                    s_assign("slot", e_int(5)),
                    s_assign("optionName", e_str("CURLOPT_XFERINFOFUNCTION")),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(20011)),
                        vec![
                            s_assign("slot", e_int(0)),
                            s_assign("optionName", e_str("CURLOPT_WRITEFUNCTION")),
                        ],
                        vec![
                        (e_binop(e_var("option"), BinOp::StrictEq, e_int(20079)), vec![
                            s_assign("slot", e_int(1)),
                            s_assign("optionName", e_str("CURLOPT_HEADERFUNCTION")),
                        ]),
                        (e_binop(e_var("option"), BinOp::StrictEq, e_int(20012)), vec![
                            s_assign("slot", e_int(2)),
                            s_assign("optionName", e_str("CURLOPT_READFUNCTION")),
                        ]),
                        (e_binop(e_var("option"), BinOp::StrictEq, e_int(20056)), vec![
                            s_assign("slot", e_int(3)),
                            s_assign("optionName", e_str("CURLOPT_PROGRESSFUNCTION")),
                        ]),
                        (e_binop(e_var("option"), BinOp::StrictEq, e_int(20094)), vec![
                            s_assign("slot", e_int(4)),
                            s_assign("optionName", e_str("CURLOPT_DEBUGFUNCTION")),
                        ]),
                        (e_binop(e_var("option"), BinOp::StrictEq, e_int(20312)), vec![
                            s_assign("slot", e_int(6)),
                            s_assign("optionName", e_str("CURLOPT_PREREQFUNCTION")),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("slot"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_debug_user", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("is_null", vec![e_var("value")]),
                        vec![
                            s_if(
                                e_binop(e_var("slot"), BinOp::StrictEq, e_int(2)),
                                vec![
                                    s_prop_assign(e_var("handle"), "__elephc_read_user", e_null()),
                                    s_return(e_call("__elephc_curl_sync_read_slot", vec![e_var("handle")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("slot"), BinOp::StrictEq, e_int(4)),
                                vec![
                                    s_prop_assign(e_var("handle"), "__elephc_debug_installed", e_bool(false)),
                                    s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(4), closure()
                                        .param("_ch", t_class("CurlHandle"))
                                        .param("_type", TypeExpr::Int)
                                        .param("_data", TypeExpr::Str)
                                        .returns(TypeExpr::Int)
                                        .body(vec![
                                            s_return(e_int(0)),
                                        ])
                                        .build()])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("slot"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("slot"), BinOp::StrictEq, e_int(5))),
                                vec![
                                    s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_var("slot"), closure()
                                        .param("_ch", t_class("CurlHandle"))
                                        .param("_dlTotal", TypeExpr::Int)
                                        .param("_dlNow", TypeExpr::Int)
                                        .param("_ulTotal", TypeExpr::Int)
                                        .param("_ulNow", TypeExpr::Int)
                                        .returns(TypeExpr::Int)
                                        .body(vec![
                                            s_return(e_int(0)),
                                        ])
                                        .build()])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("callbacks", e_prop(e_var("handle"), "__elephc_callbacks")),
                            s_array_assign("callbacks", e_var("slot"), e_null()),
                            s_prop_assign(e_var("handle"), "__elephc_callbacks", e_var("callbacks")),
                            s_if(
                                e_binop(e_var("slot"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_prop_assign(e_var("handle"), "__elephc_return_transfer", e_bool(false)),
                                    s_prop_assign(e_var("handle"), "__elephc_write_user", e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("slot"), BinOp::StrictEq, e_int(6)),
                                vec![
                                    s_prop_assign(e_var("handle"), "__elephc_prereq_user", e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("__elephc_curl_easy_set_callback", vec![e_var("raw"), e_var("slot"), e_int(0), e_var("handle"), e_int(0)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_call("is_array", vec![e_var("value")]), BinOp::And, e_binop(e_call("count", vec![e_var("value")]), BinOp::StrictEq, e_int(2))), BinOp::And, e_call("is_object", vec![e_index(e_var("value"), e_int(0))])), BinOp::And, e_call("is_string", vec![e_index(e_var("value"), e_int(1))])),
                        vec![
                            s_assign("receiver", e_index(e_var("value"), e_int(0))),
                            s_assign("method", e_index(e_var("value"), e_int(1))),
                            s_if(
                                e_binop(e_not(e_call("in_array", vec![e_var("method"), e_call("get_class_methods", vec![e_var("receiver")])])), BinOp::And, e_call("is_callable", vec![e_array(vec![e_var("receiver"), e_str("__call")])])),
                                vec![
                                    s_assign("value", closure()
                                        .captures("receiver")
                                        .captures("method")
                                        .body(vec![
                                            s_return(e_method_call(e_var("receiver"), "__call", vec![e_var("method"), e_call("func_get_args", vec![])])),
                                        ])
                                        .build()),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("is_callable", vec![e_var("value")])),
                        vec![
                            s_if(
                                e_call("is_string", vec![e_var("value")]),
                                vec![
                                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_str("curl_setopt(): Argument #3 ($value) must be a valid callback for option "), BinOp::Concat, e_var("optionName")), BinOp::Concat, e_str(", function \"")), BinOp::Concat, e_var("value")), BinOp::Concat, e_str("\" not found or invalid function name"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): Argument #3 ($value) must be a valid callback for option "), BinOp::Concat, e_var("optionName")), BinOp::Concat, e_str(", no array or string given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("slot"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_assign("user", e_var("value")),
                            s_assign("value", closure()
                                .param("ch", t_class("CurlHandle"))
                                .param("type", TypeExpr::Int)
                                .param("data", TypeExpr::Str)
                                .captures("user")
                                .body(vec![
                                    s_if(
                                        e_binop(e_var("type"), BinOp::StrictEq, e_int(2)),
                                        vec![
                                            s_prop_assign(e_var("ch"), "__elephc_header_out", e_var("data")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_call("call_user_func", vec![e_var("user"), e_var("ch"), e_var("type"), e_var("data")])),
                                ])
                                .build()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("slot"), BinOp::StrictEq, e_int(6)),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_prereq_user", e_var("value")),
                            s_assign("value", closure()
                                .param("ch", t_class("CurlHandle"))
                                .param("primaryIp", TypeExpr::Str)
                                .param("localIp", TypeExpr::Str)
                                .param("primaryPort", TypeExpr::Int)
                                .param("localPort", TypeExpr::Int)
                                .body(vec![
                                    s_assign("ret", e_call("call_user_func", vec![e_prop(e_var("ch"), "__elephc_prereq_user"), e_var("ch"), e_var("primaryIp"), e_var("localIp"), e_var("primaryPort"), e_var("localPort")])),
                                    s_if(
                                        e_call("is_int", vec![e_var("ret")]),
                                        vec![
                                            s_if(
                                                e_binop(e_binop(e_var("ret"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("ret"), BinOp::StrictEq, e_int(1))),
                                                vec![
                                                    s_return(e_var("ret")),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                            s_throw(e_new_fq("ValueError", vec![e_str("The CURLOPT_PREREQFUNCTION callback must return either CURL_PREREQFUNC_OK or CURL_PREREQFUNC_ABORT")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_throw(e_new_fq("TypeError", vec![e_str("The CURLOPT_PREREQFUNCTION callback must return either CURL_PREREQFUNC_OK or CURL_PREREQFUNC_ABORT")])),
                                ])
                                .build()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("normalized", e_call("__elephc_normalize_callable", vec![e_var("value")])),
                    s_if(
                        e_binop(e_var("slot"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_read_user", e_var("normalized")),
                            s_return(e_call("__elephc_curl_sync_read_slot", vec![e_var("handle")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("descriptor", e_call("__elephc_callable_ptr", vec![e_var("normalized")])),
                    s_assign("adapter", e_call("__elephc_curl_adapter_addr", vec![])),
                    s_if(
                        e_not(e_call("__elephc_curl_easy_set_callback", vec![e_var("raw"), e_var("slot"), e_var("descriptor"), e_var("handle"), e_var("adapter")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("callbacks", e_prop(e_var("handle"), "__elephc_callbacks")),
                    s_array_assign("callbacks", e_var("slot"), e_var("normalized")),
                    s_prop_assign(e_var("handle"), "__elephc_callbacks", e_var("callbacks")),
                    s_if(
                        e_binop(e_var("slot"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_debug_installed", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("slot"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_return_transfer", e_bool(false)),
                            s_prop_assign(e_var("handle"), "__elephc_write_user", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(9)),
                vec![
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(10001)),
                        vec![
                            s_if(
                                e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_prop_assign(e_var("handle"), "__elephc_file", e_null()),
                                    s_prop_assign(e_var("handle"), "__elephc_return_transfer", e_bool(false)),
                                    s_prop_assign(e_var("handle"), "__elephc_write_user", e_bool(false)),
                                    s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(0), e_null()])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_call("__elephc_curl_check_stream_option", vec![e_var("value"), e_bool(true)])),
                            s_prop_assign(e_var("handle"), "__elephc_file", e_var("value")),
                            s_prop_assign(e_var("handle"), "__elephc_return_transfer", e_bool(false)),
                            s_prop_assign(e_var("handle"), "__elephc_write_user", e_bool(true)),
                            s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(0), closure()
                                .param("ch", t_class("CurlHandle"))
                                .param("data", TypeExpr::Str)
                                .returns(TypeExpr::Int)
                                .body(vec![
                                    s_assign("sink", e_prop(e_var("ch"), "__elephc_file")),
                                    s_if(
                                        e_not(e_call("is_resource", vec![e_var("sink")])),
                                        vec![
                                            s_if(
                                                e_prop(e_var("ch"), "__elephc_write_user"),
                                                vec![
                                                    s_expr(e_call("trigger_error", vec![e_str("curl_exec(): CURLOPT_FILE resource has gone away, resetting to default"), e_const("E_USER_WARNING")])),
                                                    s_prop_assign(e_var("ch"), "__elephc_file", e_null()),
                                                    s_prop_assign(e_var("ch"), "__elephc_return_transfer", e_bool(false)),
                                                    s_prop_assign(e_var("ch"), "__elephc_write_user", e_bool(false)),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                            s_echo(e_var("data")),
                                            s_return(e_call("strlen", vec![e_var("data")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("written", e_call("fwrite", vec![e_var("sink"), e_var("data")])),
                                    s_if(
                                        e_not(e_call("is_int", vec![e_var("written")])),
                                        vec![
                                            s_return(e_int(0)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_var("written")),
                                ])
                                .build()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(10029)),
                        vec![
                            s_if(
                                e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_prop_assign(e_var("handle"), "__elephc_writeheader", e_null()),
                                    s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(1), e_null()])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_call("__elephc_curl_check_stream_option", vec![e_var("value"), e_bool(true)])),
                            s_prop_assign(e_var("handle"), "__elephc_writeheader", e_var("value")),
                            s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(1), closure()
                                .param("ch", t_class("CurlHandle"))
                                .param("data", TypeExpr::Str)
                                .returns(TypeExpr::Int)
                                .body(vec![
                                    s_assign("sink", e_prop(e_var("ch"), "__elephc_writeheader")),
                                    s_if(
                                        e_not(e_call("is_resource", vec![e_var("sink")])),
                                        vec![
                                            s_expr(e_call("trigger_error", vec![e_str("curl_exec(): CURLOPT_WRITEHEADER resource has gone away, resetting to default"), e_const("E_USER_WARNING")])),
                                            s_prop_assign(e_var("ch"), "__elephc_writeheader", e_null()),
                                            s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("ch"), e_int(1), e_null()])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("written", e_call("fwrite", vec![e_var("sink"), e_var("data")])),
                                    s_if(
                                        e_not(e_call("is_int", vec![e_var("written")])),
                                        vec![
                                            s_return(e_int(0)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_var("written")),
                                ])
                                .build()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(10009)),
                        vec![
                            s_if(
                                e_binop(e_var("value"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_expr(e_call("__elephc_curl_check_stream_option", vec![e_var("value"), e_bool(false)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_var("handle"), "__elephc_infile", e_var("value")),
                            s_return(e_call("__elephc_curl_sync_read_slot", vec![e_var("handle")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_stderr", e_null()),
                            s_if(
                                e_prop(e_var("handle"), "__elephc_debug_user"),
                                vec![
                                    s_return(e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(4), e_null()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_curl_check_stream_option", vec![e_var("value"), e_bool(true)])),
                    s_prop_assign(e_var("handle"), "__elephc_stderr", e_var("value")),
                    s_if(
                        e_prop(e_var("handle"), "__elephc_debug_user"),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(4), closure()
                        .param("ch", t_class("CurlHandle"))
                        .param("type", TypeExpr::Int)
                        .param("data", TypeExpr::Str)
                        .returns(TypeExpr::Int)
                        .body(vec![
                            s_assign("sink", e_prop(e_var("ch"), "__elephc_stderr")),
                            s_if(
                                e_not(e_call("is_resource", vec![e_var("sink")])),
                                vec![
                                    s_return(e_int(0)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("prefix", e_str("")),
                            s_if(
                                e_binop(e_var("type"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_assign("prefix", e_str("* ")),
                                ],
                                vec![
                                (e_binop(e_var("type"), BinOp::StrictEq, e_int(1)), vec![
                                    s_assign("prefix", e_str("< ")),
                                ]),
                                (e_binop(e_var("type"), BinOp::StrictEq, e_int(2)), vec![
                                    s_assign("prefix", e_str("> ")),
                                ]),
                            ],
                                Some(vec![
                                s_return(e_int(0)),
                            ]),
                            ),
                            s_expr(e_call("fwrite", vec![e_var("sink"), e_var("prefix")])),
                            s_expr(e_call("fwrite", vec![e_var("sink"), e_var("data")])),
                            s_return(e_int(0)),
                        ])
                        .build()])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(20316)),
                vec![
                    s_if(
                        e_call("is_null", vec![e_var("value")]),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("is_callable", vec![e_var("value")])),
                        vec![
                            s_if(
                                e_call("is_string", vec![e_var("value")]),
                                vec![
                                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): Argument #3 ($value) must be a valid callback for option CURLOPT_SSH_HOSTKEYFUNCTION, function \""), BinOp::Concat, e_var("value")), BinOp::Concat, e_str("\" not found or invalid function name"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new_fq("TypeError", vec![e_str("curl_setopt(): Argument #3 ($value) must be a valid callback for option CURLOPT_SSH_HOSTKEYFUNCTION, no array or string given")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(20200)),
                vec![
                    s_assign("optionName", e_str("CURLOPT_FNMATCH_FUNCTION")),
                    s_if(
                        e_call("is_null", vec![e_var("value")]),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("is_callable", vec![e_var("value")])),
                        vec![
                            s_if(
                                e_call("is_string", vec![e_var("value")]),
                                vec![
                                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_str("curl_setopt(): Argument #3 ($value) must be a valid callback for option "), BinOp::Concat, e_var("optionName")), BinOp::Concat, e_str(", function \"")), BinOp::Concat, e_var("value")), BinOp::Concat, e_str("\" not found or invalid function name"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): Argument #3 ($value) must be a valid callback for option "), BinOp::Concat, e_var("optionName")), BinOp::Concat, e_str(", no array or string given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_curl_setopt_unsupported_warning", vec![e_var("option")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)),
                vec![
                    s_expr(e_call("__elephc_curl_setopt_unsupported_warning", vec![e_var("option")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_null", vec![e_var("value")]),
                vec![
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_if(
                                e_call("__elephc_curl_nullable_string_option", vec![e_var("option")]),
                                vec![
                                    s_return(e_call("__elephc_curl_easy_setopt_null", vec![e_var("raw"), e_var("option")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("__elephc_curl_easy_setopt_str", vec![e_var("raw"), e_var("option"), e_str("")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4))),
                        vec![
                            s_return(e_call("__elephc_curl_easy_setopt_long", vec![e_var("raw"), e_var("option"), e_int(0)])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)), BinOp::And, e_call("is_object", vec![e_var("value")])),
                vec![
                    s_throw(e_new_fq("Error", vec![e_binop(e_binop(e_str("Object of class "), BinOp::Concat, e_call("get_class", vec![e_var("value")])), BinOp::Concat, e_str(" could not be converted to string"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_not(e_call("is_int", vec![e_var("value")])), BinOp::And, e_not(e_call("is_bool", vec![e_var("value")]))), BinOp::And, e_not(e_call("is_float", vec![e_var("value")]))), BinOp::And, e_not(e_call("is_string", vec![e_var("value")]))),
                vec![
                    s_assign("given", e_ternary(e_call("is_array", vec![e_var("value")]), e_str("array"), e_ternary(e_call("is_object", vec![e_var("value")]), e_call("get_class", vec![e_var("value")]), e_ternary(e_call("is_null", vec![e_var("value")]), e_str("null"), e_call("gettype", vec![e_var("value")]))))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): Argument #3 ($value) must be of type string|int|float|bool, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictNotEq, e_int(10015)), BinOp::And, e_call("is_string", vec![e_var("value")])), BinOp::And, e_call("str_contains", vec![e_var("value"), e_str("\0")])),
                        vec![
                            s_throw(e_new_fq("ValueError", vec![e_str("curl_setopt(): cURL option must not contain any null bytes")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("set", e_call("__elephc_curl_easy_setopt_str", vec![e_var("raw"), e_var("option"), e_cast(CastType::String, e_var("value"))])),
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_int(10002)), BinOp::And, e_var("set")),
                        vec![
                            s_expr(e_call("__elephc_curl_strip_file_protocol", vec![e_var("raw")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("set")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4))),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_int(81)), BinOp::And, e_binop(e_cast(CastType::Int, e_var("value")), BinOp::StrictEq, e_int(1))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("curl_setopt(): CURLOPT_SSL_VERIFYHOST no longer accepts the value 1, value 2 will be used instead"), e_const("E_USER_NOTICE")])),
                            s_return(e_call("__elephc_curl_easy_setopt_long", vec![e_var("raw"), e_var("option"), e_int(2)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_curl_easy_setopt_long", vec![e_var("raw"), e_var("option"), e_cast(CastType::Int, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_curl_setopt_unsupported_warning", vec![e_var("option")])),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `__elephc_curl_check_stream_option` — transcribed from the PHP form.
fn decl_fn_elephc_curl_check_stream_option() -> Stmt {
    function("__elephc_curl_check_stream_option")
        .param("value", t_mixed())
        .param("mustBeWritable", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_not(e_call("is_resource", vec![e_var("value")])),
                vec![
                    s_throw(e_new_fq("TypeError", vec![e_str("curl_setopt(): supplied argument is not a valid File-Handle resource")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("mustBeWritable"),
                vec![
                    s_assign("mode", e_str("")),
                    s_assign("meta", e_call("stream_get_meta_data", vec![e_var("value")])),
                    s_if(
                        e_call("array_key_exists", vec![e_str("mode"), e_var("meta")]),
                        vec![
                            s_assign("mode", e_cast(CastType::String, e_index(e_var("meta"), e_str("mode")))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("mode"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_not(e_call("str_contains", vec![e_var("mode"), e_str("+")]))), BinOp::And, e_binop(e_call("substr", vec![e_var("mode"), e_int(0), e_int(1)]), BinOp::StrictEq, e_str("r"))),
                        vec![
                            s_throw(e_new_fq("ValueError", vec![e_str("curl_setopt(): The provided file handle must be writable")])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
        ])
        .build()
}

/// `__elephc_curl_reset_gone_away` — transcribed from the PHP form.
fn decl_fn_elephc_curl_reset_gone_away() -> Stmt {
    function("__elephc_curl_reset_gone_away")
        .param("handle", t_class("CurlHandle"))
        .param("caller", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_binop(e_prop(e_var("handle"), "__elephc_stderr"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_resource", vec![e_prop(e_var("handle"), "__elephc_stderr")]))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_binop(e_var("caller"), BinOp::Concat, e_str("(): CURLOPT_STDERR resource has gone away, resetting to stderr")), e_const("E_USER_WARNING")])),
                    s_prop_assign(e_var("handle"), "__elephc_stderr", e_null()),
                    s_if(
                        e_not(e_prop(e_var("handle"), "__elephc_debug_user")),
                        vec![
                            s_expr(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(4), e_null()])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_prop(e_var("handle"), "__elephc_file"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_resource", vec![e_prop(e_var("handle"), "__elephc_file")]))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_binop(e_var("caller"), BinOp::Concat, e_str("(): CURLOPT_FILE resource has gone away, resetting to default")), e_const("E_USER_WARNING")])),
                    s_prop_assign(e_var("handle"), "__elephc_file", e_null()),
                    s_prop_assign(e_var("handle"), "__elephc_return_transfer", e_bool(false)),
                    s_prop_assign(e_var("handle"), "__elephc_write_user", e_bool(false)),
                    s_expr(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(0), e_null()])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_prop(e_var("handle"), "__elephc_writeheader"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_resource", vec![e_prop(e_var("handle"), "__elephc_writeheader")]))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_binop(e_var("caller"), BinOp::Concat, e_str("(): CURLOPT_WRITEHEADER resource has gone away, resetting to default")), e_const("E_USER_WARNING")])),
                    s_prop_assign(e_var("handle"), "__elephc_writeheader", e_null()),
                    s_expr(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(1), e_null()])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_prop(e_var("handle"), "__elephc_infile"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_resource", vec![e_prop(e_var("handle"), "__elephc_infile")]))),
                vec![
                    s_expr(e_call("trigger_error", vec![e_binop(e_var("caller"), BinOp::Concat, e_str("(): CURLOPT_INFILE resource has gone away, resetting to default")), e_const("E_USER_WARNING")])),
                    s_prop_assign(e_var("handle"), "__elephc_infile", e_null()),
                    s_expr(e_call("__elephc_curl_sync_read_slot", vec![e_var("handle")])),
                ],
                vec![],
                None,
            ),
        ])
        .build()
}

/// `__elephc_curl_install_internal_callback` — transcribed from the PHP form.
fn decl_fn_elephc_curl_install_internal_callback() -> Stmt {
    function("__elephc_curl_install_internal_callback")
        .param("handle", t_class("CurlHandle"))
        .param("slot", TypeExpr::Int)
        .param("closure", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_if(
                e_binop(e_var("closure"), BinOp::StrictEq, e_null()),
                vec![
                    s_prop_array_assign(e_var("handle"), "__elephc_callbacks", e_var("slot"), e_null()),
                    s_return(e_call("__elephc_curl_easy_set_callback", vec![e_var("raw"), e_var("slot"), e_int(0), e_var("handle"), e_int(0)])),
                ],
                vec![],
                None,
            ),
            s_assign("normalized", e_call("__elephc_normalize_callable", vec![e_var("closure")])),
            s_assign("descriptor", e_call("__elephc_callable_ptr", vec![e_var("normalized")])),
            s_assign("adapter", e_call("__elephc_curl_adapter_addr", vec![])),
            s_if(
                e_not(e_call("__elephc_curl_easy_set_callback", vec![e_var("raw"), e_var("slot"), e_var("descriptor"), e_var("handle"), e_var("adapter")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_prop_array_assign(e_var("handle"), "__elephc_callbacks", e_var("slot"), e_var("normalized")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_curl_sync_read_slot` — transcribed from the PHP form.
fn decl_fn_elephc_curl_sync_read_slot() -> Stmt {
    function("__elephc_curl_sync_read_slot")
        .param("handle", t_class("CurlHandle"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_binop(e_prop(e_var("handle"), "__elephc_read_user"), BinOp::StrictEq, e_null()), BinOp::And, e_binop(e_prop(e_var("handle"), "__elephc_infile"), BinOp::StrictEq, e_null())),
                vec![
                    s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(2), e_null()])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_curl_install_internal_callback", vec![e_var("handle"), e_int(2), closure()
                .param("ch", t_class("CurlHandle"))
                .param("_fd", t_mixed())
                .param("length", TypeExpr::Int)
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::Int]))
                .body(vec![
                    s_assign("user", e_prop(e_var("ch"), "__elephc_read_user")),
                    s_assign("source", e_prop(e_var("ch"), "__elephc_infile")),
                    s_if(
                        e_binop(e_var("user"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("produced", e_call("call_user_func", vec![e_var("user"), e_var("ch"), e_var("source"), e_var("length")])),
                            s_if(
                                e_call("is_int", vec![e_var("produced")]),
                                vec![
                                    s_return(e_var("produced")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_not(e_call("is_string", vec![e_var("produced")])),
                                vec![
                                    s_return(e_str("")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("produced")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("is_resource", vec![e_var("source")])),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("chunk", e_call("fread", vec![e_var("source"), e_var("length")])),
                    s_if(
                        e_not(e_call("is_string", vec![e_var("chunk")])),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("chunk")),
                ])
                .build()])),
        ])
        .build()
}

/// `CURLFile` — transcribed from the PHP form.
fn decl_class_curlfile() -> Stmt {
    class("CURLFile")
        .prop("name", TypeExpr::Str, Some(e_str("")))
        .prop("mime", TypeExpr::Str, Some(e_str("")))
        .prop("postname", TypeExpr::Str, Some(e_str("")))
        .method(
            method("__construct")
                .param("filename", TypeExpr::Str)
                .param_default("mimeType", t_nullable(TypeExpr::Str), e_null())
                .param_default("postFilename", t_nullable(TypeExpr::Str), e_null())
                .body(vec![
                    s_prop_assign(e_this(), "name", e_var("filename")),
                    s_prop_assign(e_this(), "mime", e_null_coalesce(e_var("mimeType"), e_str(""))),
                    s_prop_assign(e_this(), "postname", e_null_coalesce(e_var("postFilename"), e_str(""))),
                ]),
        )
        .method(
            method("getFilename")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("name")),
                ]),
        )
        .method(
            method("getMimeType")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("mime")),
                ]),
        )
        .method(
            method("getPostFilename")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("postname")),
                ]),
        )
        .method(
            method("setMimeType")
                .param("mime", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "mime", e_var("mime")),
                ]),
        )
        .method(
            method("setPostFilename")
                .param("postname", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "postname", e_var("postname")),
                ]),
        )
        .build()
}

/// `curl_file_create` — transcribed from the PHP form.
fn decl_fn_curl_file_create() -> Stmt {
    function("curl_file_create")
        .param("filename", TypeExpr::Str)
        .param_default("mime_type", t_nullable(TypeExpr::Str), e_null())
        .param_default("posted_filename", t_nullable(TypeExpr::Str), e_null())
        .returns(t_class("CURLFile"))
        .body(vec![
            s_return(e_new("CURLFile", vec![e_var("filename"), e_var("mime_type"), e_var("posted_filename")])),
        ])
        .build()
}

/// `CURLStringFile` — transcribed from the PHP form.
fn decl_class_curlstringfile() -> Stmt {
    class("CURLStringFile")
        .prop("data", TypeExpr::Str, Some(e_str("")))
        .prop("postname", TypeExpr::Str, Some(e_str("")))
        .prop("mime", TypeExpr::Str, Some(e_str("application/octet-stream")))
        .method(
            method("__construct")
                .param("data", TypeExpr::Str)
                .param("postname", TypeExpr::Str)
                .param_default("mime", TypeExpr::Str, e_str("application/octet-stream"))
                .body(vec![
                    s_prop_assign(e_this(), "data", e_var("data")),
                    s_prop_assign(e_this(), "postname", e_var("postname")),
                    s_prop_assign(e_this(), "mime", e_var("mime")),
                ]),
        )
        .build()
}

/// `__elephc_curl_read_file_url` — transcribed from the PHP form.
fn decl_fn_elephc_curl_read_file_url() -> Stmt {
    function("__elephc_curl_read_file_url")
        .param("path", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_call("str_starts_with", vec![e_var("path"), e_str("data://")])),
                vec![
                    s_return(e_call("file_get_contents", vec![e_var("path")])),
                ],
                vec![],
                None,
            ),
            s_assign("rest", e_call("substr", vec![e_var("path"), e_int(7)])),
            s_assign("comma", e_call("strpos", vec![e_var("rest"), e_str(",")])),
            s_if(
                e_binop(e_var("comma"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("meta", e_call("substr", vec![e_var("rest"), e_int(0), e_var("comma")])),
            s_assign("payload", e_call("substr", vec![e_var("rest"), e_binop(e_var("comma"), BinOp::Add, e_int(1))])),
            s_if(
                e_call("str_ends_with", vec![e_call("strtolower", vec![e_var("meta")]), e_str(";base64")]),
                vec![
                    s_return(e_call("base64_decode", vec![e_var("payload"), e_bool(false)])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("urldecode", vec![e_var("payload")])),
        ])
        .build()
}

/// `__elephc_curl_build_multipart` — transcribed from the PHP form.
fn decl_fn_elephc_curl_build_multipart() -> Stmt {
    function("__elephc_curl_build_multipart")
        .param("raw", t_mixed())
        .param("fields", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_call("__elephc_curl_mime_new", vec![e_var("raw")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("fields"), Some("name"), "value", vec![
                s_assign("nameRaw", e_binop(e_cast(CastType::String, e_var("name")), BinOp::Concat, e_str(""))),
                s_if(
                    e_call("is_array", vec![e_var("value")]),
                    vec![
                        s_foreach(e_var("value"), None, "inner", vec![
                            s_if(
                                e_binop(e_call("is_array", vec![e_var("inner")]), BinOp::Or, e_call("is_object", vec![e_var("inner")])),
                                vec![
                                    s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                    s_throw(e_new_fq("TypeError", vec![e_str("curl_setopt(): CURLOPT_POSTFIELDS nested array value must contain only scalars")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_not(e_call("__elephc_curl_mime_add_part", vec![e_var("raw")])),
                                vec![
                                    s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(0), e_var("nameRaw")])),
                                vec![
                                    s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("innerRaw", e_binop(e_cast(CastType::String, e_var("inner")), BinOp::Concat, e_str(""))),
                            s_if(
                                e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(1), e_var("innerRaw")])),
                                vec![
                                    s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_not(e_call("__elephc_curl_mime_add_part", vec![e_var("raw")])),
                    vec![
                        s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(0), e_var("nameRaw")])),
                    vec![
                        s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_instance_of(e_var("value"), "CURLFile"),
                    vec![
                        s_assign("path", e_binop(e_cast(CastType::String, e_prop(e_var("value"), "name")), BinOp::Concat, e_str(""))),
                        s_if(
                            e_call("str_contains", vec![e_var("path"), e_str("://")]),
                            vec![
                                s_assign("data", e_call("__elephc_curl_read_file_url", vec![e_var("path")])),
                                s_if(
                                    e_not(e_call("is_string", vec![e_var("data")])),
                                    vec![
                                        s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(1), e_var("data")])),
                                    vec![
                                        s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![
                            (e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(2), e_var("path")])), vec![
                                s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                s_return(e_bool(false)),
                            ]),
                        ],
                            None,
                        ),
                        s_assign("mime", e_binop(e_cast(CastType::String, e_prop(e_var("value"), "mime")), BinOp::Concat, e_str(""))),
                        s_assign("mimeType", e_ternary(e_binop(e_var("mime"), BinOp::StrictNotEq, e_str("")), e_var("mime"), e_str("application/octet-stream"))),
                        s_if(
                            e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(3), e_var("mimeType")])),
                            vec![
                                s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("postname", e_binop(e_cast(CastType::String, e_prop(e_var("value"), "postname")), BinOp::Concat, e_str(""))),
                        s_assign("filename", e_ternary(e_binop(e_var("postname"), BinOp::StrictNotEq, e_str("")), e_var("postname"), e_var("path"))),
                        s_if(
                            e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(4), e_var("filename")])),
                            vec![
                                s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_instance_of(e_var("value"), "CURLStringFile"),
                    vec![
                        s_assign("data", e_binop(e_cast(CastType::String, e_prop(e_var("value"), "data")), BinOp::Concat, e_str(""))),
                        s_if(
                            e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(1), e_var("data")])),
                            vec![
                                s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("postname", e_binop(e_cast(CastType::String, e_prop(e_var("value"), "postname")), BinOp::Concat, e_str(""))),
                        s_if(
                            e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(4), e_var("postname")])),
                            vec![
                                s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("mime", e_binop(e_cast(CastType::String, e_prop(e_var("value"), "mime")), BinOp::Concat, e_str(""))),
                        s_if(
                            e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(3), e_var("mime")])),
                            vec![
                                s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_call("is_object", vec![e_var("value")]),
                    vec![
                        s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                        s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_setopt(): CURLOPT_POSTFIELDS array value must be of type string|int|float|bool|CURLFile|CURLStringFile, "), BinOp::Concat, e_call("get_class", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("valueRaw", e_binop(e_cast(CastType::String, e_var("value")), BinOp::Concat, e_str(""))),
                s_if(
                    e_not(e_call("__elephc_curl_mime_part_field", vec![e_var("raw"), e_int(1), e_var("valueRaw")])),
                    vec![
                        s_expr(e_call("__elephc_curl_mime_abort", vec![e_var("raw")])),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_call("__elephc_curl_mime_post", vec![e_var("raw")])),
        ])
        .build()
}

/// `curl_setopt_array` — transcribed from the PHP form.
fn decl_fn_curl_setopt_array() -> Stmt {
    function("curl_setopt_array")
        .param("handle", t_class("CurlHandle"))
        .param("options", t_array())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_foreach(e_var("options"), Some("option"), "value", vec![
                s_try(vec![
                    s_if(
                        e_not(e_call("curl_setopt", vec![e_var("handle"), e_cast(CastType::Int, e_var("option")), e_var("value")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ], vec![
                    (vec!["\\TypeError"], Some("e"), vec![
                        s_throw(e_new_fq("TypeError", vec![e_call("str_replace", vec![e_str("curl_setopt(): Argument #3 ($value)"), e_str("curl_setopt_array(): Argument #2 ($options)"), e_method_call(e_var("e"), "getMessage", vec![])])])),
                    ]),
                    (vec!["\\ValueError"], Some("e"), vec![
                        s_throw(e_new_fq("ValueError", vec![e_call("str_replace", vec![e_str("curl_setopt(): Argument #3 ($value)"), e_str("curl_setopt_array(): Argument #2 ($options)"), e_method_call(e_var("e"), "getMessage", vec![])])])),
                    ]),
                ], None),
            ]),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `curl_exec` — transcribed from the PHP form.
fn decl_fn_curl_exec() -> Stmt {
    function("curl_exec")
        .param("handle", t_class("CurlHandle"))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
        .body(vec![
            s_expr(e_call("__elephc_curl_reset_gone_away", vec![e_var("handle"), e_str("curl_exec")])),
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_if(
                e_not(e_call("__elephc_curl_easy_perform", vec![e_var("raw")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_prop(e_var("handle"), "__elephc_return_transfer"),
                vec![
                    s_assign("body", e_call("__elephc_curl_easy_body", vec![e_var("raw")])),
                    s_return(e_var("body")),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `curl_errno` — transcribed from the PHP form.
fn decl_fn_curl_errno() -> Stmt {
    function("curl_errno")
        .param("handle", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("handle"), "CurlHandle")),
                vec![
                    s_assign("given", e_ternary(e_call("is_object", vec![e_var("handle")]), e_call("get_class", vec![e_var("handle")]), e_ternary(e_call("is_null", vec![e_var("handle")]), e_str("null"), e_call("gettype", vec![e_var("handle")])))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_errno(): Argument #1 ($handle) must be of type CurlHandle, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_return(e_call("__elephc_curl_easy_errno", vec![e_var("raw")])),
        ])
        .build()
}

/// `curl_error` — transcribed from the PHP form.
fn decl_fn_curl_error() -> Stmt {
    function("curl_error")
        .param("handle", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("handle"), "CurlHandle")),
                vec![
                    s_assign("given", e_ternary(e_call("is_object", vec![e_var("handle")]), e_call("get_class", vec![e_var("handle")]), e_ternary(e_call("is_null", vec![e_var("handle")]), e_str("null"), e_call("gettype", vec![e_var("handle")])))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_error(): Argument #1 ($handle) must be of type CurlHandle, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_assign("message", e_call("__elephc_curl_easy_error", vec![e_var("raw")])),
            s_return(e_var("message")),
        ])
        .build()
}

/// `curl_close` — transcribed from the PHP form.
fn decl_fn_curl_close(php_version: PhpVersion) -> Stmt {
    let body = if php_version >= PhpVersion::Php85 {
        vec![
            s_if(
                e_binop(e_prop(e_var("handle"), "__elephc_handle"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("Function curl_close() is deprecated since 8.5, as it has no effect since PHP 8.0"), e_const("E_USER_DEPRECATED")])),
                ],
                vec![],
                None,
            ),
        ]
    } else {
        vec![]
    };
    function("curl_close")
        .param("handle", t_class("CurlHandle"))
        .keep_unread_params()
        .returns(TypeExpr::Void)
        .body(body)
        .build()
}

/// `curl_getinfo` — transcribed from the PHP form.
fn decl_fn_curl_getinfo() -> Stmt {
    function("curl_getinfo")
        .param("handle", t_class("CurlHandle"))
        .param_default("option", t_nullable(TypeExpr::Int), e_null())
        .returns(t_mixed())
        .body(vec![
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_null()),
                vec![
                    s_assign("json", e_call("__elephc_curl_easy_str_op", vec![e_var("raw"), e_int(5), e_str(""), e_int(0)])),
                    s_if(
                        e_not(e_call("is_string", vec![e_var("json")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("decoded", e_call("json_decode", vec![e_var("json"), e_bool(true)])),
                    s_if(
                        e_not(e_call("is_array", vec![e_var("decoded")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_prop(e_var("handle"), "__elephc_header_out"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_array_assign("decoded", e_str("request_header"), e_prop(e_var("handle"), "__elephc_header_out")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("decoded")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(1048597)),
                vec![
                    s_return(e_prop(e_var("handle"), "__elephc_private")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_assign("header", e_call("__elephc_curl_easy_str_op", vec![e_var("raw"), e_int(7), e_str(""), e_int(0)])),
                    s_if(
                        e_not(e_call("is_string", vec![e_var("header")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("header")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("option"), BinOp::StrictEq, e_int(4194338)),
                vec![
                    s_assign("json", e_call("__elephc_curl_easy_str_op", vec![e_var("raw"), e_int(6), e_str(""), e_var("option")])),
                    s_if(
                        e_not(e_call("is_string", vec![e_var("json")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("decoded", e_call("json_decode", vec![e_var("json"), e_bool(true)])),
                    s_if(
                        e_not(e_call("is_array", vec![e_var("decoded")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("decoded")),
                ],
                vec![],
                None,
            ),
            s_assign("mask", e_binop(e_var("option"), BinOp::BitAnd, e_int(15728640))),
            s_if(
                e_binop(e_binop(e_var("mask"), BinOp::StrictEq, e_int(2097152)), BinOp::Or, e_binop(e_var("mask"), BinOp::StrictEq, e_int(6291456))),
                vec![
                    s_return(e_call("__elephc_curl_easy_getinfo_long", vec![e_var("raw"), e_var("option")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("mask"), BinOp::StrictEq, e_int(3145728)),
                vec![
                    s_return(e_call("__elephc_curl_easy_getinfo_double", vec![e_var("raw"), e_var("option")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("mask"), BinOp::StrictEq, e_int(1048576)),
                vec![
                    s_return(e_call("__elephc_curl_easy_str_op", vec![e_var("raw"), e_int(3), e_str(""), e_var("option")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("mask"), BinOp::StrictEq, e_int(4194304)),
                vec![
                    s_assign("blob", e_call("__elephc_curl_easy_str_op", vec![e_var("raw"), e_int(4), e_str(""), e_var("option")])),
                    s_if(
                        e_not(e_call("is_string", vec![e_var("blob")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("text", e_binop(e_cast(CastType::String, e_var("blob")), BinOp::Concat, e_str(""))),
                    s_if(
                        e_binop(e_var("text"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return(e_array(vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("explode", vec![e_str("\0"), e_call("substr", vec![e_var("text"), e_int(0), e_binop(e_call("strlen", vec![e_var("text")]), BinOp::Sub, e_int(1))])])),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `curl_reset` — transcribed from the PHP form.
fn decl_fn_curl_reset() -> Stmt {
    function("curl_reset")
        .param("handle", t_class("CurlHandle"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_expr(e_call("__elephc_curl_easy_reset", vec![e_var("raw")])),
            s_prop_assign(e_var("handle"), "__elephc_return_transfer", e_bool(false)),
            s_prop_assign(e_var("handle"), "__elephc_write_user", e_bool(false)),
            s_prop_assign(e_var("handle"), "__elephc_private", e_bool(false)),
            s_prop_assign(e_var("handle"), "__elephc_callbacks", e_array(vec![])),
            s_prop_assign(e_var("handle"), "__elephc_file", e_null()),
            s_prop_assign(e_var("handle"), "__elephc_writeheader", e_null()),
            s_prop_assign(e_var("handle"), "__elephc_infile", e_null()),
            s_prop_assign(e_var("handle"), "__elephc_stderr", e_null()),
            s_prop_assign(e_var("handle"), "__elephc_read_user", e_null()),
            s_prop_assign(e_var("handle"), "__elephc_prereq_user", e_null()),
            s_prop_assign(e_var("handle"), "__elephc_debug_user", e_bool(false)),
            s_prop_assign(e_var("handle"), "__elephc_header_out", e_null()),
            s_prop_assign(e_var("handle"), "__elephc_header_out_enabled", e_bool(false)),
        ])
        .build()
}

/// `curl_copy_handle` — transcribed from the PHP form.
fn decl_fn_curl_copy_handle() -> Stmt {
    function("curl_copy_handle")
        .param("handle", t_class("CurlHandle"))
        .returns(t_class("CurlHandle"))
        .body(vec![
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_assign("copy", e_call("__elephc_curl_easy_copy", vec![e_var("raw")])),
            s_if(
                e_binop(e_var("copy"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new_fq("RuntimeException", vec![e_str("curl_copy_handle(): libcurl could not duplicate the easy handle")])),
                ],
                vec![],
                None,
            ),
            s_assign("new", e_static_call("CurlHandle", "__elephc_wrap", vec![e_var("copy")])),
            s_prop_assign(e_var("new"), "__elephc_return_transfer", e_prop(e_var("handle"), "__elephc_return_transfer")),
            s_prop_assign(e_var("new"), "__elephc_write_user", e_prop(e_var("handle"), "__elephc_write_user")),
            s_prop_assign(e_var("new"), "__elephc_private", e_prop(e_var("handle"), "__elephc_private")),
            s_prop_assign(e_var("new"), "__elephc_file", e_prop(e_var("handle"), "__elephc_file")),
            s_prop_assign(e_var("new"), "__elephc_writeheader", e_prop(e_var("handle"), "__elephc_writeheader")),
            s_prop_assign(e_var("new"), "__elephc_infile", e_prop(e_var("handle"), "__elephc_infile")),
            s_prop_assign(e_var("new"), "__elephc_stderr", e_prop(e_var("handle"), "__elephc_stderr")),
            s_prop_assign(e_var("new"), "__elephc_read_user", e_prop(e_var("handle"), "__elephc_read_user")),
            s_prop_assign(e_var("new"), "__elephc_prereq_user", e_prop(e_var("handle"), "__elephc_prereq_user")),
            s_prop_assign(e_var("new"), "__elephc_debug_user", e_prop(e_var("handle"), "__elephc_debug_user")),
            s_prop_assign(e_var("new"), "__elephc_debug_installed", e_prop(e_var("handle"), "__elephc_debug_installed")),
            s_prop_assign(e_var("new"), "__elephc_header_out_enabled", e_prop(e_var("handle"), "__elephc_header_out_enabled")),
            s_prop_assign(e_var("new"), "__elephc_header_out", e_prop(e_var("handle"), "__elephc_header_out")),
            s_assign("newRaw", e_prop(e_var("new"), "__elephc_handle")),
            s_assign("adapter", e_call("__elephc_curl_adapter_addr", vec![])),
            s_foreach(e_prop(e_var("handle"), "__elephc_callbacks"), Some("slot"), "callback", vec![
                s_if(
                    e_binop(e_var("callback"), BinOp::StrictEq, e_null()),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("slotIndex", e_cast(CastType::Int, e_var("slot"))),
                s_if(
                    e_binop(e_binop(e_var("slotIndex"), BinOp::StrictEq, e_int(0)), BinOp::And, e_not(e_prop(e_var("new"), "__elephc_write_user"))),
                    vec![
                        s_prop_array_assign(e_var("new"), "__elephc_callbacks", e_int(0), e_var("callback")),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("descriptor", e_call("__elephc_callable_ptr", vec![e_var("callback")])),
                s_expr(e_call("__elephc_curl_easy_set_callback", vec![e_var("newRaw"), e_var("slotIndex"), e_var("descriptor"), e_var("new"), e_var("adapter")])),
                s_prop_array_assign(e_var("new"), "__elephc_callbacks", e_var("slotIndex"), e_var("callback")),
            ]),
            s_return(e_var("new")),
        ])
        .build()
}

/// `curl_escape` — transcribed from the PHP form.
fn decl_fn_curl_escape() -> Stmt {
    function("curl_escape")
        .param("handle", t_class("CurlHandle"))
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_assign("escaped", e_call("__elephc_curl_easy_str_op", vec![e_var("raw"), e_int(1), e_var("string"), e_int(0)])),
            s_if(
                e_not(e_call("is_string", vec![e_var("escaped")])),
                vec![
                    s_throw(e_new_fq("RuntimeException", vec![e_str("curl_escape(): libcurl could not URL-encode the string")])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("escaped")),
        ])
        .build()
}

/// `curl_unescape` — transcribed from the PHP form.
fn decl_fn_curl_unescape() -> Stmt {
    function("curl_unescape")
        .param("handle", t_class("CurlHandle"))
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_assign("decoded", e_call("__elephc_curl_easy_str_op", vec![e_var("raw"), e_int(2), e_var("string"), e_int(0)])),
            s_if(
                e_not(e_call("is_string", vec![e_var("decoded")])),
                vec![
                    s_throw(e_new_fq("RuntimeException", vec![e_str("curl_unescape(): libcurl could not URL-decode the string")])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("decoded")),
        ])
        .build()
}

/// `curl_pause` — transcribed from the PHP form.
fn decl_fn_curl_pause() -> Stmt {
    function("curl_pause")
        .param("handle", t_class("CurlHandle"))
        .param("flags", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_return(e_call("__elephc_curl_easy_pause", vec![e_var("raw"), e_var("flags")])),
        ])
        .build()
}

/// `curl_upkeep` — transcribed from the PHP form.
fn decl_fn_curl_upkeep() -> Stmt {
    function("curl_upkeep")
        .param("handle", t_class("CurlHandle"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_return(e_call("__elephc_curl_easy_upkeep", vec![e_var("raw")])),
        ])
        .build()
}

/// `curl_strerror` — transcribed from the PHP form.
fn decl_fn_curl_strerror() -> Stmt {
    function("curl_strerror")
        .param("error_code", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("message", e_call("__elephc_curl_strerror", vec![e_var("error_code")])),
            s_return(e_var("message")),
        ])
        .build()
}

/// `curl_version` — transcribed from the PHP form.
fn decl_fn_curl_version() -> Stmt {
    function("curl_version")
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(vec![
            s_assign("json", e_call("__elephc_curl_version", vec![])),
            s_if(
                e_binop(e_var("json"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("decoded", e_call("json_decode", vec![e_var("json"), e_bool(true)])),
            s_if(
                e_not(e_call("is_array", vec![e_var("decoded")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_var("decoded")),
        ])
        .build()
}

/// `phpinfo` — transcribed from the PHP form.
fn decl_fn_phpinfo() -> Stmt {
    function("phpinfo")
        .param_default("flags", TypeExpr::Int, e_int(0))
        .keep_unread_params()
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("info", e_call("curl_version", vec![])),
            s_echo(e_str("phpinfo()\n\nphpinfo\n\n")),
            s_if(
                e_call("is_array", vec![e_var("info")]),
                vec![
                    s_assign("features", e_index(e_var("info"), e_str("feature_list"))),
                    s_if(
                        e_call("is_array", vec![e_var("features")]),
                        vec![
                            s_foreach(e_var("features"), Some("key"), "value", vec![
                                s_echo(e_binop(e_binop(e_binop(e_var("key"), BinOp::Concat, e_str(" => ")), BinOp::Concat, e_ternary(e_var("value"), e_str("Yes"), e_str("No"))), BinOp::Concat, e_str("\n"))),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `CurlMultiHandle` — transcribed from the PHP form.
fn decl_class_curlmultihandle() -> Stmt {
    class("CurlMultiHandle")
        .final_()
        .prop("__elephc_handle", t_mixed(), Some(e_null()))
        .prop("__elephc_ids", t_array(), Some(e_array(vec![])))
        .prop("__elephc_handles", t_array(), Some(e_array(vec![])))
        .prop("__elephc_push_user", t_mixed(), Some(e_null()))
        .private_static_prop("__elephc_minting", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .body(vec![
                    s_if(
                        e_not(e_self_static_prop("__elephc_minting")),
                        vec![
                            s_throw(e_new_fq("Error", vec![e_str("Cannot directly construct CurlMultiHandle, use curl_multi_init() instead")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_wrap")
                .static_()
                .param("raw", t_mixed())
                .returns(t_class("CurlMultiHandle"))
                .body(vec![
                    s_self_static_prop_assign("__elephc_minting", e_bool(true)),
                    s_assign("h", e_new_self(vec![])),
                    s_self_static_prop_assign("__elephc_minting", e_bool(false)),
                    s_prop_assign(e_var("h"), "__elephc_handle", e_var("raw")),
                    s_return(e_var("h")),
                ]),
        )
        .method(
            method("__elephc_attach")
                .param("id", TypeExpr::Int)
                .param("handle", t_class("CurlHandle"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_array_push(e_this(), "__elephc_ids", e_var("id")),
                    s_prop_array_push(e_this(), "__elephc_handles", e_var("handle")),
                ]),
        )
        .method(
            method("__elephc_detach")
                .param("id", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("ids", e_array(vec![])),
                    s_assign("handles", e_array(vec![])),
                    s_assign("count", e_call("count", vec![e_this_prop("__elephc_ids")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_index(e_this_prop("__elephc_ids"), e_var("i")), BinOp::StrictEq, e_var("id")),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_array_push("ids", e_index(e_this_prop("__elephc_ids"), e_var("i"))),
                        s_array_push("handles", e_index(e_this_prop("__elephc_handles"), e_var("i"))),
                    ]),
                    s_prop_assign(e_this(), "__elephc_ids", e_var("ids")),
                    s_prop_assign(e_this(), "__elephc_handles", e_var("handles")),
                ]),
        )
        .method(
            method("__elephc_lookup")
                .param("id", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_assign("count", e_call("count", vec![e_this_prop("__elephc_ids")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_index(e_this_prop("__elephc_ids"), e_var("i")), BinOp::StrictEq, e_var("id")),
                            vec![
                                s_return(e_index(e_this_prop("__elephc_handles"), e_var("i"))),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("__debugInfo")
                .returns(t_array())
                .body(vec![
                    s_return(e_array(vec![])),
                ]),
        )
        .method(
            method("__serialize")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new_fq("Exception", vec![e_str("Serialization of 'CurlMultiHandle' is not allowed")])),
                ]),
        )
        .build()
}

/// `curl_multi_init` — transcribed from the PHP form.
fn decl_fn_curl_multi_init() -> Stmt {
    function("curl_multi_init")
        .returns(t_class("CurlMultiHandle"))
        .body(vec![
            s_assign("raw", e_call("__elephc_curl_multi_init", vec![])),
            s_if(
                e_binop(e_var("raw"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new_fq("RuntimeException", vec![e_str("curl_multi_init(): libcurl could not allocate a multi handle")])),
                ],
                vec![],
                None,
            ),
            s_return(e_static_call("CurlMultiHandle", "__elephc_wrap", vec![e_var("raw")])),
        ])
        .build()
}

/// `curl_multi_add_handle` — transcribed from the PHP form.
fn decl_fn_curl_multi_add_handle() -> Stmt {
    function("curl_multi_add_handle")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .param("handle", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("handle"), "CurlHandle")),
                vec![
                    s_assign("given", e_ternary(e_call("is_object", vec![e_var("handle")]), e_call("get_class", vec![e_var("handle")]), e_ternary(e_call("is_null", vec![e_var("handle")]), e_str("null"), e_call("gettype", vec![e_var("handle")])))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_multi_add_handle(): Argument #2 ($handle) must be of type CurlHandle, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_curl_reset_gone_away", vec![e_var("handle"), e_str("curl_multi_add_handle")])),
            s_assign("multiRaw", e_prop(e_var("multi_handle"), "__elephc_handle")),
            s_assign("easyRaw", e_prop(e_var("handle"), "__elephc_handle")),
            s_assign("code", e_call("__elephc_curl_multi_add", vec![e_var("multiRaw"), e_var("easyRaw")])),
            s_if(
                e_binop(e_var("code"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_assign("id", e_call("__elephc_curl_easy_id", vec![e_var("easyRaw")])),
                    s_expr(e_method_call(e_var("multi_handle"), "__elephc_attach", vec![e_var("id"), e_var("handle")])),
                    // A push callback set before this add applies to the new parent handle too.
                    s_if(
                        e_binop(e_prop(e_var("multi_handle"), "__elephc_push_user"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("handle"), "__elephc_push_user", e_prop(e_var("multi_handle"), "__elephc_push_user")),
                            s_assign("normalized", e_call("__elephc_normalize_callable", vec![e_str("__elephc_curl_push_invoke")])),
                            s_assign("descriptor", e_call("__elephc_callable_ptr", vec![e_var("normalized")])),
                            s_assign("adapter", e_call("__elephc_curl_adapter_addr", vec![])),
                            s_expr(e_call("__elephc_curl_easy_set_callback", vec![
                                e_var("easyRaw"),
                                e_int(7),
                                e_var("descriptor"),
                                e_var("handle"),
                                e_var("adapter"),
                            ])),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_var("code")),
        ])
        .build()
}

/// `curl_multi_remove_handle` — transcribed from the PHP form.
fn decl_fn_curl_multi_remove_handle() -> Stmt {
    function("curl_multi_remove_handle")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .param("handle", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("handle"), "CurlHandle")),
                vec![
                    s_assign("given", e_ternary(e_call("is_object", vec![e_var("handle")]), e_call("get_class", vec![e_var("handle")]), e_ternary(e_call("is_null", vec![e_var("handle")]), e_str("null"), e_call("gettype", vec![e_var("handle")])))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_multi_remove_handle(): Argument #2 ($handle) must be of type CurlHandle, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("multiRaw", e_prop(e_var("multi_handle"), "__elephc_handle")),
            s_assign("easyRaw", e_prop(e_var("handle"), "__elephc_handle")),
            s_assign("code", e_call("__elephc_curl_multi_remove", vec![e_var("multiRaw"), e_var("easyRaw")])),
            s_if(
                e_binop(e_var("code"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_assign("id", e_call("__elephc_curl_easy_id", vec![e_var("easyRaw")])),
                    s_expr(e_method_call(e_var("multi_handle"), "__elephc_detach", vec![e_var("id")])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("code")),
        ])
        .build()
}

/// `curl_multi_exec` — transcribed from the PHP form.
fn decl_fn_curl_multi_exec() -> Stmt {
    function("curl_multi_exec")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .param_by_ref("still_running", None)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("raw", e_prop(e_var("multi_handle"), "__elephc_handle")),
            s_assign("packed", e_call("__elephc_curl_multi_exec", vec![e_var("raw")])),
            s_assign("still_running", e_binop(e_binop(e_var("packed"), BinOp::ShiftRight, e_int(32)), BinOp::BitAnd, e_int(4294967295))),
            s_assign("code", e_binop(e_var("packed"), BinOp::BitAnd, e_int(4294967295))),
            s_if(
                e_binop(e_var("code"), BinOp::GtEq, e_int(2147483648)),
                vec![
                    s_assign("code", e_binop(e_var("code"), BinOp::Sub, e_int(4294967296))),
                ],
                vec![],
                None,
            ),
            s_return(e_var("code")),
        ])
        .build()
}

/// `curl_multi_select` — transcribed from the PHP form.
fn decl_fn_curl_multi_select() -> Stmt {
    function("curl_multi_select")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .param_default("timeout", TypeExpr::Float, e_float(1.0))
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("timeout"), BinOp::Lt, e_float(0.0)), BinOp::Or, e_binop(e_var("timeout"), BinOp::Gt, e_float(2147483.647))),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("curl_multi_select(): Argument #2 ($timeout) must be between 0 and 2147483.647000")])),
                ],
                vec![],
                None,
            ),
            s_assign("raw", e_prop(e_var("multi_handle"), "__elephc_handle")),
            s_assign("milliseconds", e_cast(CastType::Int, e_binop(e_var("timeout"), BinOp::Mul, e_float(1000.0)))),
            s_return(e_call("__elephc_curl_multi_select", vec![e_var("raw"), e_var("milliseconds")])),
        ])
        .build()
}

/// `curl_multi_info_read` — transcribed from the PHP form.
fn decl_fn_curl_multi_info_read() -> Stmt {
    function("curl_multi_info_read")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .param_by_ref_default("queued_messages", None, e_null())
        .body(vec![
            s_assign("raw", e_prop(e_var("multi_handle"), "__elephc_handle")),
            s_if(
                e_binop(e_call("__elephc_curl_multi_info_read", vec![e_var("raw"), e_int(0)]), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("queued_messages", e_call("__elephc_curl_multi_info_read", vec![e_var("raw"), e_int(4)])),
            s_assign("msg", e_call("__elephc_curl_multi_info_read", vec![e_var("raw"), e_int(1)])),
            s_assign("result", e_call("__elephc_curl_multi_info_read", vec![e_var("raw"), e_int(2)])),
            s_assign("easyId", e_call("__elephc_curl_multi_info_read", vec![e_var("raw"), e_int(3)])),
            s_assign("handle", e_method_call(e_var("multi_handle"), "__elephc_lookup", vec![e_var("easyId")])),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictEq, e_null()),
                vec![
                    s_assign("pushed", e_static_prop("CurlHandle", "__elephc_pushed")),
                    s_if(
                        e_call("isset", vec![e_index(e_var("pushed"), e_var("easyId"))]),
                        vec![
                            s_assign("handle", e_index(e_var("pushed"), e_var("easyId"))),
                            s_assign("rest", e_array(vec![])),
                            s_foreach(
                                e_var("pushed"),
                                Some("pendingId"),
                                "pendingHandle",
                                vec![s_if(
                                    e_binop(e_var("pendingId"), BinOp::StrictNotEq, e_var("easyId")),
                                    vec![s_array_assign("rest", e_var("pendingId"), e_var("pendingHandle"))],
                                    vec![],
                                    None,
                                )],
                            ),
                            s_static_prop_assign("CurlHandle", "__elephc_pushed", e_var("rest")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_array_assoc(vec![(e_str("msg"), e_var("msg")), (e_str("result"), e_var("result"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_array_assoc(vec![(e_str("msg"), e_var("msg")), (e_str("result"), e_var("result")), (e_str("handle"), e_var("handle"))])),
        ])
        .build()
}

/// `curl_multi_close` — transcribed from the PHP form.
fn decl_fn_curl_multi_close() -> Stmt {
    function("curl_multi_close")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .keep_unread_params()
        .returns(TypeExpr::Void)
        .build()
}

/// `curl_multi_getcontent` — transcribed from the PHP form.
fn decl_fn_curl_multi_getcontent() -> Stmt {
    function("curl_multi_getcontent")
        .param("handle", t_mixed())
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("handle"), "CurlHandle")),
                vec![
                    s_assign("given", e_ternary(e_call("is_object", vec![e_var("handle")]), e_call("get_class", vec![e_var("handle")]), e_ternary(e_call("is_null", vec![e_var("handle")]), e_str("null"), e_call("gettype", vec![e_var("handle")])))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_multi_getcontent(): Argument #1 ($handle) must be of type CurlHandle, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_prop(e_var("handle"), "__elephc_return_transfer")),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_assign("raw", e_prop(e_var("handle"), "__elephc_handle")),
            s_assign("body", e_call("__elephc_curl_easy_body", vec![e_var("raw")])),
            s_return(e_var("body")),
        ])
        .build()
}

/// `__elephc_curl_push_invoke` — transcribed from the PHP form. The bridge's push
/// trampoline calls it through the parent handle's push slot; because it is a plain named
/// function, no closure capture crosses the ABI.
fn decl_fn_elephc_curl_push_invoke() -> Stmt {
    function("__elephc_curl_push_invoke")
        .param("parent", t_mixed())
        .param("pushed_id", t_mixed())
        .param("headers_json", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("user", e_prop(e_var("parent"), "__elephc_push_user")),
            s_assign(
                "pushed",
                e_static_call(
                    "CurlHandle",
                    "__elephc_wrap",
                    vec![e_call(
                        "__elephc_curl_easy_from_id",
                        vec![e_cast(CastType::Int, e_var("pushed_id"))],
                    )],
                ),
            ),
            s_prop_assign(e_var("pushed"), "__elephc_return_transfer", e_prop(e_var("parent"), "__elephc_return_transfer")),
            s_prop_assign(e_var("pushed"), "__elephc_write_user", e_bool(false)),
            s_assign("callbacks", e_prop(e_var("parent"), "__elephc_callbacks")),
            s_if(
                e_binop(
                    e_binop(
                        e_prop(e_var("parent"), "__elephc_write_user"),
                        BinOp::And,
                        e_call("isset", vec![e_index(e_var("callbacks"), e_int(0))]),
                    ),
                    BinOp::And,
                    e_binop(e_index(e_var("callbacks"), e_int(0)), BinOp::StrictNotEq, e_null()),
                ),
                vec![s_expr(e_call(
                    "curl_setopt",
                    vec![
                        e_var("pushed"),
                        e_const("CURLOPT_WRITEFUNCTION"),
                        e_index(e_var("callbacks"), e_int(0)),
                    ],
                ))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("isset", vec![e_index(e_var("callbacks"), e_int(1))]),
                    BinOp::And,
                    e_binop(e_index(e_var("callbacks"), e_int(1)), BinOp::StrictNotEq, e_null()),
                ),
                vec![s_expr(e_call(
                    "curl_setopt",
                    vec![
                        e_var("pushed"),
                        e_const("CURLOPT_HEADERFUNCTION"),
                        e_index(e_var("callbacks"), e_int(1)),
                    ],
                ))],
                vec![],
                None,
            ),
            s_assign(
                "headers",
                e_ternary(
                    e_binop(e_var("headers_json"), BinOp::StrictEq, e_str("")),
                    e_array(vec![]),
                    e_call(
                        "json_decode",
                        vec![e_cast(CastType::String, e_var("headers_json")), e_bool(true)],
                    ),
                ),
            ),
            s_assign(
                "answer",
                e_cast(
                    CastType::Int,
                    e_call(
                        "call_user_func",
                        vec![e_var("user"), e_var("parent"), e_var("pushed"), e_var("headers")],
                    ),
                ),
            ),
            s_if(
                e_binop(e_var("answer"), BinOp::StrictEq, e_const("CURL_PUSH_OK")),
                vec![s_static_prop_array_assign(
                    "CurlHandle",
                    "__elephc_pushed",
                    e_cast(CastType::Int, e_var("pushed_id")),
                    e_var("pushed"),
                )],
                vec![],
                None,
            ),
            s_return(e_var("answer")),
        ])
        .build()
}

/// `curl_multi_setopt` — transcribed from the PHP form.
fn decl_fn_curl_multi_setopt() -> Stmt {
    function("curl_multi_setopt")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .param("option", TypeExpr::Int)
        .param("value", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("raw", e_prop(e_var("multi_handle"), "__elephc_handle")),
            s_assign("carryable", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("option"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(6))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(7))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(8))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(13))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(16))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(30009))), BinOp::Or, e_binop(e_var("option"), BinOp::StrictEq, e_int(30010)))),
            s_if(
                e_not(e_var("carryable")),
                vec![
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(20014)),
                        vec![
s_if(
                                e_binop(
                                    e_binop(
                                        e_binop(
                                            e_call("is_array", vec![e_var("value")]),
                                            BinOp::And,
                                            e_binop(e_call("count", vec![e_var("value")]), BinOp::StrictEq, e_int(2)),
                                        ),
                                        BinOp::And,
                                        e_call("is_object", vec![e_index(e_var("value"), e_int(0))]),
                                    ),
                                    BinOp::And,
                                    e_call("is_string", vec![e_index(e_var("value"), e_int(1))]),
                                ),
                                vec![
                                    s_assign("receiver", e_index(e_var("value"), e_int(0))),
                                    s_assign("method", e_index(e_var("value"), e_int(1))),
                                    s_if(
                                        e_binop(
                                            e_not(e_call("in_array", vec![e_var("method"), e_call("get_class_methods", vec![e_var("receiver")])])),
                                            BinOp::And,
                                            e_call("is_callable", vec![e_array(vec![e_var("receiver"), e_str("__call")])]),
                                        ),
                                        vec![
                                            s_assign(
                                                "value",
                                                closure()
                                                    .captures("receiver")
                                                    .captures("method")
                                                    .body(vec![
                                                        s_return(e_method_call(
                                                            e_var("receiver"),
                                                            "__call",
                                                            vec![e_var("method"), e_call("func_get_args", vec![])],
                                                        )),
                                                    ])
                                                    .build(),
                                            ),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_not(e_call("is_callable", vec![e_var("value")])),
                                vec![
                                    s_if(
                                        e_call("is_string", vec![e_var("value")]),
                                        vec![
                                            s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_multi_setopt(): Argument #2 ($option) must be a valid callback for option CURLMOPT_PUSHFUNCTION, function \""), BinOp::Concat, e_var("value")), BinOp::Concat, e_str("\" not found or invalid function name"))])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_throw(e_new_fq("TypeError", vec![e_str("curl_multi_setopt(): Argument #2 ($option) must be a valid callback for option CURLMOPT_PUSHFUNCTION, no array or string given")])),
                                ],
                                vec![],
                                None,
                            ),

                            s_prop_assign(e_var("multi_handle"), "__elephc_push_user", e_var("value")),
                            s_assign("normalized", e_call("__elephc_normalize_callable", vec![e_str("__elephc_curl_push_invoke")])),
                            s_assign("descriptor", e_call("__elephc_callable_ptr", vec![e_var("normalized")])),
                            s_assign("adapter", e_call("__elephc_curl_adapter_addr", vec![])),
                            s_foreach(
                                e_prop(e_var("multi_handle"), "__elephc_handles"),
                                None,
                                "attached",
                                vec![
                                    s_prop_assign(e_var("attached"), "__elephc_push_user", e_var("value")),
                                    s_expr(e_call("__elephc_curl_easy_set_callback", vec![
                                        e_prop(e_var("attached"), "__elephc_handle"),
                                        e_int(7),
                                        e_var("descriptor"),
                                        e_var("attached"),
                                        e_var("adapter"),
                                    ])),
                                ],
                            ),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_curl_multi_setopt", vec![e_var("raw"), e_var("option"), e_int(0)])),
                    s_throw(e_new_fq("ValueError", vec![e_str("curl_multi_setopt(): Argument #2 ($option) is not a valid cURL multi option")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_not(e_call("is_int", vec![e_var("value")])), BinOp::And, e_not(e_call("is_bool", vec![e_var("value")]))), BinOp::And, e_not(e_call("is_float", vec![e_var("value")]))), BinOp::And, e_not(e_call("is_string", vec![e_var("value")]))),
                vec![
                    s_assign("given", e_ternary(e_call("is_array", vec![e_var("value")]), e_str("array"), e_ternary(e_call("is_object", vec![e_var("value")]), e_call("get_class", vec![e_var("value")]), e_ternary(e_call("is_null", vec![e_var("value")]), e_str("null"), e_call("gettype", vec![e_var("value")]))))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_multi_setopt(): Argument #3 ($value) must be of type string|int|float|bool, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("applied", e_call("__elephc_curl_multi_setopt", vec![e_var("raw"), e_var("option"), e_cast(CastType::Int, e_var("value"))])),
            s_if(
                e_binop(e_var("applied"), BinOp::StrictEq, e_neg(e_int(1))),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("curl_multi_setopt(): Argument #2 ($option) is not a valid cURL multi option")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("applied"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_curl_multi_setopt_unsupported_warning", vec![e_var("option")])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `curl_multi_errno` — transcribed from the PHP form.
fn decl_fn_curl_multi_errno() -> Stmt {
    function("curl_multi_errno")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("raw", e_prop(e_var("multi_handle"), "__elephc_handle")),
            s_return(e_call("__elephc_curl_multi_errno", vec![e_var("raw")])),
        ])
        .build()
}

/// `curl_multi_strerror` — transcribed from the PHP form.
fn decl_fn_curl_multi_strerror() -> Stmt {
    function("curl_multi_strerror")
        .param("error_code", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("message", e_call("__elephc_curl_multi_strerror", vec![e_var("error_code")])),
            s_return(e_var("message")),
        ])
        .build()
}

/// `CurlShareHandle` — transcribed from the PHP form.
fn decl_class_curlsharehandle() -> Stmt {
    class("CurlShareHandle")
        .final_()
        .prop("__elephc_handle", t_mixed(), Some(e_null()))
        .private_static_prop("__elephc_minting", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .body(vec![
                    s_if(
                        e_not(e_self_static_prop("__elephc_minting")),
                        vec![
                            s_throw(e_new_fq("Error", vec![e_str("Cannot directly construct CurlShareHandle, use curl_share_init() instead")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_wrap")
                .static_()
                .param("raw", t_mixed())
                .returns(t_class("CurlShareHandle"))
                .body(vec![
                    s_self_static_prop_assign("__elephc_minting", e_bool(true)),
                    s_assign("h", e_new_self(vec![])),
                    s_self_static_prop_assign("__elephc_minting", e_bool(false)),
                    s_prop_assign(e_var("h"), "__elephc_handle", e_var("raw")),
                    s_return(e_var("h")),
                ]),
        )
        .method(
            method("__debugInfo")
                .returns(t_array())
                .body(vec![
                    s_return(e_array(vec![])),
                ]),
        )
        .method(
            method("__serialize")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new_fq("Exception", vec![e_str("Serialization of 'CurlShareHandle' is not allowed")])),
                ]),
        )
        .build()
}

/// `curl_share_init` — transcribed from the PHP form.
fn decl_fn_curl_share_init() -> Stmt {
    function("curl_share_init")
        .returns(t_class("CurlShareHandle"))
        .body(vec![
            s_assign("raw", e_call("__elephc_curl_share_init", vec![])),
            s_if(
                e_binop(e_var("raw"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new_fq("RuntimeException", vec![e_str("curl_share_init(): libcurl could not allocate a share handle")])),
                ],
                vec![],
                None,
            ),
            s_return(e_static_call("CurlShareHandle", "__elephc_wrap", vec![e_var("raw")])),
        ])
        .build()
}

/// `curl_share_setopt` — transcribed from the PHP form.
fn decl_fn_curl_share_setopt() -> Stmt {
    function("curl_share_setopt")
        .param("share_handle", t_mixed())
        .param("option", TypeExpr::Int)
        .param("value", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_instance_of(e_var("share_handle"), "CurlShareHandle")),
                vec![
                    s_assign("given", e_ternary(e_call("is_object", vec![e_var("share_handle")]), e_call("get_class", vec![e_var("share_handle")]), e_ternary(e_call("is_null", vec![e_var("share_handle")]), e_str("null"), e_call("gettype", vec![e_var("share_handle")])))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_share_setopt(): Argument #1 ($share_handle) must be of type CurlShareHandle, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("raw", e_prop(e_var("share_handle"), "__elephc_handle")),
            s_if(
                e_binop(e_binop(e_binop(e_not(e_call("is_int", vec![e_var("value")])), BinOp::And, e_not(e_call("is_bool", vec![e_var("value")]))), BinOp::And, e_not(e_call("is_float", vec![e_var("value")]))), BinOp::And, e_not(e_call("is_string", vec![e_var("value")]))),
                vec![
                    s_assign("given", e_ternary(e_call("is_array", vec![e_var("value")]), e_str("array"), e_ternary(e_call("is_object", vec![e_var("value")]), e_call("get_class", vec![e_var("value")]), e_ternary(e_call("is_null", vec![e_var("value")]), e_str("null"), e_call("gettype", vec![e_var("value")]))))),
                    s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_share_setopt(): Argument #3 ($value) must be of type string|int|float|bool, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_assign("applied", e_call("__elephc_curl_share_setopt", vec![e_var("raw"), e_var("option"), e_cast(CastType::Int, e_var("value"))])),
            s_if(
                e_binop(e_var("applied"), BinOp::StrictEq, e_neg(e_int(1))),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("curl_share_setopt(): Argument #2 ($option) is not a valid cURL share option")])),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_var("applied"), BinOp::StrictEq, e_int(1))),
        ])
        .build()
}

/// `curl_share_close` — transcribed from the PHP form.
fn decl_fn_curl_share_close(php_version: PhpVersion) -> Stmt {
    let body = if php_version >= PhpVersion::Php85 {
        vec![
            s_if(
                e_binop(e_prop(e_var("share_handle"), "__elephc_handle"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_expr(e_call("trigger_error", vec![e_str("Function curl_share_close() is deprecated since 8.5, as it has no effect since PHP 8.0"), e_const("E_USER_DEPRECATED")])),
                ],
                vec![],
                None,
            ),
        ]
    } else {
        vec![]
    };
    function("curl_share_close")
        .param("share_handle", t_class("CurlShareHandle"))
        .keep_unread_params()
        .returns(TypeExpr::Void)
        .body(body)
        .build()
}

/// `curl_share_errno` — transcribed from the PHP form.
fn decl_fn_curl_share_errno() -> Stmt {
    function("curl_share_errno")
        .param("share_handle", t_class("CurlShareHandle"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("raw", e_prop(e_var("share_handle"), "__elephc_handle")),
            s_return(e_call("__elephc_curl_share_errno", vec![e_var("raw")])),
        ])
        .build()
}

/// `curl_share_strerror` — transcribed from the PHP form.
fn decl_fn_curl_share_strerror() -> Stmt {
    function("curl_share_strerror")
        .param("error_code", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("message", e_call("__elephc_curl_share_strerror", vec![e_var("error_code")])),
            s_return(e_var("message")),
        ])
        .build()
}

/// `curl_multi_get_handles` — transcribed from the PHP form.
fn decl_fn_curl_multi_get_handles() -> Stmt {
    function("curl_multi_get_handles")
        .param("multi_handle", t_class("CurlMultiHandle"))
        .returns(t_array())
        .body(vec![
            s_assign("handles", e_prop(e_var("multi_handle"), "__elephc_handles")),
            s_return(e_var("handles")),
        ])
        .build()
}

/// `CurlSharePersistentHandle` — transcribed from the PHP form.
fn decl_class_curlsharepersistenthandle() -> Stmt {
    class("CurlSharePersistentHandle")
        .final_()
        .prop("__elephc_handle", t_mixed(), Some(e_null()))
        .prop("options", t_array(), Some(e_array(vec![])))
        .private_static_prop("__elephc_minting", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .body(vec![
                    s_if(
                        e_not(e_self_static_prop("__elephc_minting")),
                        vec![
                            s_throw(e_new_fq("Error", vec![e_str("Cannot directly construct CurlSharePersistentHandle, use curl_share_init_persistent() instead")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_wrap")
                .static_()
                .param("raw", t_mixed())
                .returns(t_class("CurlSharePersistentHandle"))
                .body(vec![
                    s_self_static_prop_assign("__elephc_minting", e_bool(true)),
                    s_assign("h", e_new_self(vec![])),
                    s_self_static_prop_assign("__elephc_minting", e_bool(false)),
                    s_prop_assign(e_var("h"), "__elephc_handle", e_var("raw")),
                    s_return(e_var("h")),
                ]),
        )
        .method(
            method("__debugInfo")
                .returns(t_array())
                .body(vec![
                    s_return(e_array(vec![])),
                ]),
        )
        .method(
            method("__serialize")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new_fq("Exception", vec![e_str("Serialization of 'CurlSharePersistentHandle' is not allowed")])),
                ]),
        )
        .build()
}

/// `curl_share_init_persistent` — transcribed from the PHP form.
fn decl_fn_curl_share_init_persistent() -> Stmt {
    function("curl_share_init_persistent")
        .param("share_options", t_array())
        .returns(t_class("CurlSharePersistentHandle"))
        .body(vec![
            s_if(
                e_binop(e_call("count", vec![e_var("share_options")]), BinOp::StrictEq, e_int(0)),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("curl_share_init_persistent(): Argument #1 ($share_options) must not be empty")])),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("share_options"), None, "opt", vec![
                s_if(
                    e_not(e_call("is_int", vec![e_var("opt")])),
                    vec![
                        s_assign("given", e_ternary(e_call("is_array", vec![e_var("opt")]), e_str("array"), e_ternary(e_call("is_object", vec![e_var("opt")]), e_call("get_class", vec![e_var("opt")]), e_call("gettype", vec![e_var("opt")])))),
                        s_throw(e_new_fq("TypeError", vec![e_binop(e_binop(e_str("curl_share_init_persistent(): Argument #1 ($share_options) must contain only int values, "), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("opt"), BinOp::StrictEq, e_int(2)),
                    vec![
                        s_throw(e_new_fq("ValueError", vec![e_str("curl_share_init_persistent(): Argument #1 ($share_options) must not contain CURL_LOCK_DATA_COOKIE because sharing cookies across PHP requests is unsafe")])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_binop(e_var("opt"), BinOp::StrictNotEq, e_int(3)), BinOp::And, e_binop(e_var("opt"), BinOp::StrictNotEq, e_int(4))), BinOp::And, e_binop(e_var("opt"), BinOp::StrictNotEq, e_int(5))), BinOp::And, e_binop(e_var("opt"), BinOp::StrictNotEq, e_int(6))),
                    vec![
                        s_throw(e_new_fq("ValueError", vec![e_str("curl_share_init_persistent(): Argument #1 ($share_options) must contain only CURL_LOCK_DATA_* constants")])),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_assign("unique", e_array(vec![])),
            s_assign("csv", e_str("")),
            s_foreach(e_array(vec![e_int(3), e_int(4), e_int(5), e_int(6)]), None, "candidate", vec![
                s_assign("found", e_bool(false)),
                s_foreach(e_var("share_options"), None, "opt", vec![
                    s_if(
                        e_binop(e_var("opt"), BinOp::StrictEq, e_var("candidate")),
                        vec![
                            s_assign("found", e_bool(true)),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_var("found"),
                    vec![
                        s_array_push("unique", e_var("candidate")),
                        s_if(
                            e_binop(e_var("csv"), BinOp::StrictNotEq, e_str("")),
                            vec![
                                s_assign("csv", e_binop(e_var("csv"), BinOp::Concat, e_str(","))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("csv", e_binop(e_var("csv"), BinOp::Concat, e_cast(CastType::String, e_var("candidate")))),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_assign("raw", e_call("__elephc_curl_share_init_persistent", vec![e_var("csv")])),
            s_if(
                e_binop(e_var("raw"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_throw(e_new_fq("RuntimeException", vec![e_str("curl_share_init_persistent(): libcurl could not allocate a share handle")])),
                ],
                vec![],
                None,
            ),
            s_assign("handle", e_static_call("CurlSharePersistentHandle", "__elephc_wrap", vec![e_var("raw")])),
            s_prop_assign(e_var("handle"), "options", e_var("unique")),
            s_return(e_var("handle")),
        ])
        .build()
}

/// Builds the whole surface for one PHP compatibility profile.
///
/// The 8.5-only members are appended last, matching the order of the PHP source the oracle
/// compares against.
pub(crate) fn curl_declarations(php_version: PhpVersion) -> Program {
    internal_declarations(|| {
        let mut decls = vec![
            decl_class_curlhandle(),
            decl_fn_curl_init(),
            decl_fn_elephc_curl_slist_option_name(),
            decl_fn_elephc_curl_nullable_string_option(),
            decl_fn_elephc_curl_given_type(),
            decl_fn_elephc_curl_open_basedir(),
            decl_fn_elephc_curl_open_basedir_is_set(),
            decl_fn_elephc_curl_open_basedir_warn(),
            decl_fn_elephc_curl_path_within_open_basedir(),
            decl_fn_elephc_curl_strip_file_protocol(),
            decl_fn_curl_setopt(),
            decl_fn_elephc_curl_check_stream_option(),
            decl_fn_elephc_curl_reset_gone_away(),
            decl_fn_elephc_curl_install_internal_callback(),
            decl_fn_elephc_curl_sync_read_slot(),
            decl_class_curlfile(),
            decl_fn_curl_file_create(),
            decl_class_curlstringfile(),
            decl_fn_elephc_curl_read_file_url(),
            decl_fn_elephc_curl_build_multipart(),
            decl_fn_curl_setopt_array(),
            decl_fn_curl_exec(),
            decl_fn_curl_errno(),
            decl_fn_curl_error(),
            decl_fn_curl_close(php_version),
            decl_fn_curl_getinfo(),
            decl_fn_curl_reset(),
            decl_fn_curl_copy_handle(),
            decl_fn_curl_escape(),
            decl_fn_curl_unescape(),
            decl_fn_curl_pause(),
            decl_fn_curl_upkeep(),
            decl_fn_curl_strerror(),
            decl_fn_curl_version(),
            decl_fn_phpinfo(),
            decl_class_curlmultihandle(),
            decl_fn_curl_multi_init(),
            decl_fn_curl_multi_add_handle(),
            decl_fn_curl_multi_remove_handle(),
            decl_fn_curl_multi_exec(),
            decl_fn_curl_multi_select(),
            decl_fn_curl_multi_info_read(),
            decl_fn_curl_multi_close(),
            decl_fn_curl_multi_getcontent(),
            decl_fn_elephc_curl_push_invoke(),
            decl_fn_curl_multi_setopt(),
            decl_fn_curl_multi_errno(),
            decl_fn_curl_multi_strerror(),
            decl_class_curlsharehandle(),
            decl_fn_curl_share_init(),
            decl_fn_curl_share_setopt(),
            decl_fn_curl_share_close(php_version),
            decl_fn_curl_share_errno(),
            decl_fn_curl_share_strerror(),
        ];
        if php_version >= PhpVersion::Php85 {
            decls.push(decl_fn_curl_multi_get_handles());
            decls.push(decl_class_curlsharepersistenthandle());
            decls.push(decl_fn_curl_share_init_persistent());
        }
        decls
    })
}
