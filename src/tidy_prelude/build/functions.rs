//! Purpose:
//! The 24 public `tidy_*` functions and the `ob_tidyhandler` output callback of the tidy
//! prelude.
//!
//! Called from:
//! - `crate::tidy_prelude::build::tidy_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form kept in `crate::tidy_prelude::fragments`; the oracle
//!   test compares both node by node.
//! - Parameter names stay public; locals are `$_`-prefixed to avoid checker name
//!   collisions with user globals.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `tidy_parse_string` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_parse_string() -> Stmt {
    function("tidy_parse_string")
        .param("string", TypeExpr::Str)
        .param_default("config", t_union(vec![t_array(), TypeExpr::Str, TypeExpr::Void]), e_null())
        .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_call("strlen", vec![e_var("string")]), BinOp::Gt, e_int(4294967295)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("tidy_parse_string(): Argument #1 ($string) is too long")])),
                ],
                vec![],
                None,
            ),
            s_assign("_doc", e_call("__elephc_tidy_new_doc", vec![e_str("tidy_parse_string()")])),
            s_assign("_tidy", e_static_call("tidy", "__elephc_wrap", vec![e_var("_doc")])),
            s_if(
                e_not(e_call("__elephc_tidy_apply_config", vec![e_var("_doc"), e_var("config"), e_str("tidy_parse_string()"), e_bool(false)])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("__elephc_tidy_parse_into", vec![e_var("_doc"), e_var("string"), e_var("encoding"), e_str("tidy_parse_string()"), e_bool(false)])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_tidy_sync_properties", vec![e_var("_tidy")])),
            s_return(e_var("_tidy")),
        ])
        .build()
}

/// `tidy_get_error_buffer` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_error_buffer() -> Stmt {
    function("tidy_get_error_buffer")
        .param("tidy", t_mixed())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("_doc", e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_error_buffer()")]), "__elephc_doc_handle", vec![])),
            s_if(
                e_binop(e_call("elephc_tidy_error_buffer_present", vec![e_var("_doc")]), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_tidy_error_buffer", vec![e_var("_doc")])),
        ])
        .build()
}

/// `tidy_get_output` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_output() -> Stmt {
    function("tidy_get_output")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_return(e_call("elephc_tidy_output", vec![e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_output()")]), "__elephc_doc_handle", vec![])])),
        ])
        .build()
}

/// `tidy_parse_file` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_parse_file() -> Stmt {
    function("tidy_parse_file")
        .param("filename", TypeExpr::Str)
        .param_default("config", t_union(vec![t_array(), TypeExpr::Str, TypeExpr::Void]), e_null())
        .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
        .param_default("useIncludePath", TypeExpr::Bool, e_bool(false))
        .returns(t_mixed())
        .body(vec![
            s_assign("_contents", e_call("__elephc_tidy_read_file", vec![e_var("filename"), e_var("useIncludePath"), e_str("tidy_parse_file()"), e_str("parse")])),
            s_if(
                e_binop(e_var("_contents"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_doc", e_call("__elephc_tidy_new_doc", vec![e_str("tidy_parse_file()")])),
            s_assign("_tidy", e_static_call("tidy", "__elephc_wrap", vec![e_var("_doc")])),
            s_if(
                e_not(e_call("__elephc_tidy_apply_config", vec![e_var("_doc"), e_var("config"), e_str("tidy_parse_file()"), e_bool(false)])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("__elephc_tidy_parse_into", vec![e_var("_doc"), e_cast(CastType::String, e_var("_contents")), e_var("encoding"), e_str("tidy_parse_file()"), e_bool(false)])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_tidy_sync_properties", vec![e_var("_tidy")])),
            s_return(e_var("_tidy")),
        ])
        .build()
}

/// `tidy_clean_repair` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_clean_repair() -> Stmt {
    function("tidy_clean_repair")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_clean_repair()")]), "cleanRepair", vec![])),
        ])
        .build()
}

/// `tidy_repair_string` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_repair_string() -> Stmt {
    function("tidy_repair_string")
        .param("string", TypeExpr::Str)
        .param_default("config", t_union(vec![t_array(), TypeExpr::Str, TypeExpr::Void]), e_null())
        .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_return(e_call("__elephc_tidy_repair_string", vec![e_var("string"), e_var("config"), e_var("encoding"), e_str("tidy_repair_string()")])),
        ])
        .build()
}

/// `tidy_repair_file` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_repair_file() -> Stmt {
    function("tidy_repair_file")
        .param("filename", TypeExpr::Str)
        .param_default("config", t_union(vec![t_array(), TypeExpr::Str, TypeExpr::Void]), e_null())
        .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
        .param_default("useIncludePath", TypeExpr::Bool, e_bool(false))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_return(e_call("__elephc_tidy_repair_file", vec![e_var("filename"), e_var("config"), e_var("encoding"), e_var("useIncludePath"), e_str("tidy_repair_file()")])),
        ])
        .build()
}

/// `tidy_diagnose` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_diagnose() -> Stmt {
    function("tidy_diagnose")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_diagnose()")]), "diagnose", vec![])),
        ])
        .build()
}

/// `tidy_get_release` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_release() -> Stmt {
    function("tidy_get_release")
        .returns(TypeExpr::Str)
        .body(vec![
            s_return(e_call("elephc_tidy_release_date", vec![])),
        ])
        .build()
}

/// `tidy_get_opt_doc` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_opt_doc() -> Stmt {
    function("tidy_get_opt_doc")
        .param("tidy", t_mixed())
        .param("option", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_return(e_call("__elephc_tidy_get_option_doc", vec![e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_opt_doc()")]), "__elephc_doc_handle", vec![]), e_var("option"), e_str("tidy_get_opt_doc()"), e_int(2)])),
        ])
        .build()
}

/// `tidy_get_config` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_config() -> Stmt {
    function("tidy_get_config")
        .param("tidy", t_mixed())
        .returns(t_array())
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_config()")]), "getConfig", vec![])),
        ])
        .build()
}

/// `tidy_get_status` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_status() -> Stmt {
    function("tidy_get_status")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_tidy_status", vec![e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_status()")]), "__elephc_doc_handle", vec![])])),
        ])
        .build()
}

/// `tidy_get_html_ver` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_html_ver() -> Stmt {
    function("tidy_get_html_ver")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_html_ver()")]), "getHtmlVer", vec![])),
        ])
        .build()
}

/// `tidy_is_xhtml` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_is_xhtml() -> Stmt {
    function("tidy_is_xhtml")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_is_xhtml()")]), "isXhtml", vec![])),
        ])
        .build()
}

/// `tidy_is_xml` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_is_xml() -> Stmt {
    function("tidy_is_xml")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_is_xml()")]), "isXml", vec![])),
        ])
        .build()
}

/// `tidy_error_count` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_error_count() -> Stmt {
    function("tidy_error_count")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_tidy_error_count", vec![e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_error_count()")]), "__elephc_doc_handle", vec![])])),
        ])
        .build()
}

/// `tidy_warning_count` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_warning_count() -> Stmt {
    function("tidy_warning_count")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_tidy_warning_count", vec![e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_warning_count()")]), "__elephc_doc_handle", vec![])])),
        ])
        .build()
}

/// `tidy_access_count` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_access_count() -> Stmt {
    function("tidy_access_count")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_tidy_access_count", vec![e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_access_count()")]), "__elephc_doc_handle", vec![])])),
        ])
        .build()
}

/// `tidy_config_count` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_config_count() -> Stmt {
    function("tidy_config_count")
        .param("tidy", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_call("elephc_tidy_config_count", vec![e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_config_count()")]), "__elephc_doc_handle", vec![])])),
        ])
        .build()
}

/// `tidy_getopt` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_getopt() -> Stmt {
    function("tidy_getopt")
        .param("tidy", t_mixed())
        .param("option", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Int, TypeExpr::Bool]))
        .body(vec![
            s_return(e_call("__elephc_tidy_get_option", vec![e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_getopt()")]), "__elephc_doc_handle", vec![]), e_var("option"), e_str("tidy_getopt()"), e_int(2)])),
        ])
        .build()
}

/// `tidy_get_root` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_root() -> Stmt {
    function("tidy_get_root")
        .param("tidy", t_mixed())
        .returns(t_nullable(t_class("tidyNode")))
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_root()")]), "root", vec![])),
        ])
        .build()
}

/// `tidy_get_html` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_html() -> Stmt {
    function("tidy_get_html")
        .param("tidy", t_mixed())
        .returns(t_nullable(t_class("tidyNode")))
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_html()")]), "html", vec![])),
        ])
        .build()
}

/// `tidy_get_head` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_head() -> Stmt {
    function("tidy_get_head")
        .param("tidy", t_mixed())
        .returns(t_nullable(t_class("tidyNode")))
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_head()")]), "head", vec![])),
        ])
        .build()
}

/// `tidy_get_body` — transcribed from the PHP form.
pub(super) fn decl_fn_tidy_get_body() -> Stmt {
    function("tidy_get_body")
        .param("tidy", t_mixed())
        .returns(t_nullable(t_class("tidyNode")))
        .body(vec![
            s_return(e_method_call(e_call("__elephc_tidy_document", vec![e_var("tidy"), e_str("tidy_get_body()")]), "body", vec![])),
        ])
        .build()
}

/// `ob_tidyhandler` — transcribed from the PHP form.
pub(super) fn decl_fn_ob_tidyhandler() -> Stmt {
    function("ob_tidyhandler")
        .param("buffer", TypeExpr::Str)
        .param_default("phase", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_var("phase"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_binop(e_var("phase"), BinOp::BitAnd, e_const("PHP_OUTPUT_HANDLER_START")), BinOp::StrictEq, e_int(0))), BinOp::Or, e_binop(e_binop(e_var("phase"), BinOp::BitAnd, e_const("PHP_OUTPUT_HANDLER_FINAL")), BinOp::StrictEq, e_int(0))),
                vec![
                    s_return(e_var("buffer")),
                ],
                vec![],
                None,
            ),
            s_assign("_doc", e_call("__elephc_tidy_new_doc", vec![e_str("ob_tidyhandler()")])),
            s_try(vec![
                s_if(
                    e_binop(e_call("elephc_tidy_parse", vec![e_var("_doc"), e_var("buffer"), e_call("strlen", vec![e_var("buffer")])]), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_return(e_var("buffer")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("elephc_tidy_clean_repair", vec![e_var("_doc")]), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_return(e_var("buffer")),
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
