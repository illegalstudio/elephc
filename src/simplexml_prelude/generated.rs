//! Purpose:
//! Rust AST builders for the SimpleXML prelude surface and stream wrappers.
//!
//! Called from:
//! - `crate::simplexml_prelude::declarations()`.
//!
//! Key details:
//! - Generated once from the former PHP prelude with `synthetic_class::transcribe`; the
//!   resulting nodes are exactly the ones the parser produced, with no PHP text parsed here.
//! - One helper per declaration keeps each builder frame small; the aggregator preserves
//!   declaration order.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `elephc_xml_sx_load` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_load() -> Stmt {
    extern_fn("elephc_xml_sx_load", "elephc_xml")
        .param("data", CType::Str)
        .param("length", CType::Int)
        .param("options", CType::Int)
        .param("ns", CType::Str)
        .param("prefix", CType::Int)
        .param("url", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_load_dom` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_load_dom() -> Stmt {
    extern_fn("elephc_xml_sx_load_dom", "elephc_xml")
        .param("data", CType::Str)
        .param("length", CType::Int)
        .param("options", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_html_document` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_html_document() -> Stmt {
    extern_fn("elephc_xml_sx_html_document", "elephc_xml")
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_html_append` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_html_append() -> Stmt {
    extern_fn("elephc_xml_sx_html_append", "elephc_xml")
        .param("handle", CType::Int)
        .param("kind", CType::Int)
        .param("name", CType::Str)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_html_root` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_html_root() -> Stmt {
    extern_fn("elephc_xml_sx_html_root", "elephc_xml")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_load_file` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_load_file() -> Stmt {
    extern_fn("elephc_xml_sx_load_file", "elephc_xml")
        .param("filename", CType::Str)
        .param("options", CType::Int)
        .param("ns", CType::Str)
        .param("prefix", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_load_stream` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_load_stream() -> Stmt {
    extern_fn("elephc_xml_sx_load_stream", "elephc_xml")
        .param("filename", CType::Str)
        .param("data", CType::Str)
        .param("length", CType::Int)
        .param("options", CType::Int)
        .param("ns", CType::Str)
        .param("prefix", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_diagnostics` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_diagnostics() -> Stmt {
    extern_fn("elephc_xml_sx_diagnostics", "elephc_xml")
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_sx_file_error` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_file_error() -> Stmt {
    extern_fn("elephc_xml_sx_file_error", "elephc_xml")
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_sx_free` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_free() -> Stmt {
    extern_fn("elephc_xml_sx_free", "elephc_xml")
        .param("handle", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_xml_sx_clone` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_clone() -> Stmt {
    extern_fn("elephc_xml_sx_clone", "elephc_xml")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_alias` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_alias() -> Stmt {
    extern_fn("elephc_xml_sx_alias", "elephc_xml")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_compare` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_compare() -> Stmt {
    extern_fn("elephc_xml_sx_compare", "elephc_xml")
        .param("left", CType::Int)
        .param("right", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_select` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_select() -> Stmt {
    extern_fn("elephc_xml_sx_select", "elephc_xml")
        .param("handle", CType::Int)
        .param("kind", CType::Int)
        .param("name", CType::Str)
        .param("ns", CType::Str)
        .param("prefix", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_int` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_int() -> Stmt {
    extern_fn("elephc_xml_sx_int", "elephc_xml")
        .param("handle", CType::Int)
        .param("operation", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_projection_base` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_projection_base() -> Stmt {
    extern_fn("elephc_xml_sx_projection_base", "elephc_xml")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_string` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_string() -> Stmt {
    extern_fn("elephc_xml_sx_string", "elephc_xml")
        .param("handle", CType::Int)
        .param("operation", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_sx_dom_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_dom_attribute() -> Stmt {
    extern_fn("elephc_xml_sx_dom_attribute", "elephc_xml")
        .param("handle", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_sx_xml_hex` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_xml_hex() -> Stmt {
    extern_fn("elephc_xml_sx_xml_hex", "elephc_xml")
        .param("handle", CType::Int)
        .param("whole", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_sx_item` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_item() -> Stmt {
    extern_fn("elephc_xml_sx_item", "elephc_xml")
        .param("handle", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_iterator` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_iterator() -> Stmt {
    extern_fn("elephc_xml_sx_iterator", "elephc_xml")
        .param("handle", CType::Int)
        .param("operation", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_mutate` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_mutate() -> Stmt {
    extern_fn("elephc_xml_sx_mutate", "elephc_xml")
        .param("handle", CType::Int)
        .param("operation", CType::Int)
        .param("key", CType::Str)
        .param("value", CType::Str)
        .param("ns", CType::Str)
        .param("flags", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_register` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_register() -> Stmt {
    extern_fn("elephc_xml_sx_register", "elephc_xml")
        .param("handle", CType::Int)
        .param("prefix", CType::Str)
        .param("uri", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_xpath` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_xpath() -> Stmt {
    extern_fn("elephc_xml_sx_xpath", "elephc_xml")
        .param("handle", CType::Int)
        .param("expression", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_sx_namespace` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_namespace() -> Stmt {
    extern_fn("elephc_xml_sx_namespace", "elephc_xml")
        .param("handle", CType::Int)
        .param("declared", CType::Int)
        .param("recursive", CType::Int)
        .param("root", CType::Int)
        .param("index", CType::Int)
        .param("field", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_xml_sx_set_encoding` — transcribed from the PHP form.
fn decl_extern_elephc_xml_sx_set_encoding() -> Stmt {
    extern_fn("elephc_xml_sx_set_encoding", "elephc_xml")
        .param("handle", CType::Int)
        .param("encoding", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `__elephc_simplexml_warn` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_warn() -> Stmt {
    function("__elephc_simplexml_warn")
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_call("str_starts_with", vec![e_var("message"), e_str("Warning: ")]),
                vec![
                    s_expr(e_call("__elephc_builtin_warning", vec![e_call("rtrim", vec![e_call("substr", vec![e_var("message"), e_int(9)]), e_str("\n")])])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_simplexml_warning", vec![e_var("message")])),
        ])
        .build()
}

/// `__elephc_simplexml_wrap` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_wrap() -> Stmt {
    function("__elephc_simplexml_wrap")
        .param("handle", TypeExpr::Int)
        .param("class", TypeExpr::Str)
        .returns(t_class("SimpleXMLElement"))
        .body(vec![
            s_try(vec![
                s_assign("object", e_call("__elephc_simplexml_allocate", vec![e_var("class")])),
            ], vec![
                (vec!["Throwable"], Some("error"), vec![
                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("handle")])),
                    s_throw(e_var("error")),
                ]),
            ], None),
            s_if(
                e_instance_of(e_var("object"), "SimpleXMLElement"),
                vec![
                    s_expr(e_method_call(e_var("object"), "__elephc_attach", vec![e_var("handle")])),
                    s_return(e_var("object")),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("elephc_xml_sx_free", vec![e_var("handle")])),
            s_throw(e_new("TypeError", vec![e_str("SimpleXML class must extend SimpleXMLElement")])),
        ])
        .build()
}

/// `__elephc_simplexml_class` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_class() -> Stmt {
    function("__elephc_simplexml_class")
        .param("class", t_nullable(TypeExpr::Str))
        .param("caller", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("class"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_str("SimpleXMLElement")),
                ],
                vec![],
                None,
            ),
            s_assign("name", e_cast(CastType::String, e_var("class"))),
            s_if(
                e_not(e_call("is_a", vec![e_var("name"), e_str("SimpleXMLElement"), e_bool(true)])),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_var("caller"), BinOp::Concat, e_str("(): Argument #2 ($class_name) must be a class name derived from SimpleXMLElement or null, ")), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("name")),
        ])
        .build()
}

/// `__elephc_simplexml_assigned_value` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_assigned_value() -> Stmt {
    function("__elephc_simplexml_assigned_value")
        .param("value", t_mixed())
        .param("attribute", TypeExpr::Bool)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("value"), BinOp::StrictEq, e_null()), BinOp::Or, e_call("is_scalar", vec![e_var("value")])),
                vec![
                    s_return(e_cast(CastType::String, e_var("value"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_object", vec![e_var("value")]), BinOp::And, e_binop(e_call("get_class", vec![e_var("value")]), BinOp::StrictEq, e_str("SimpleXMLElement"))),
                vec![
                    s_return(e_cast(CastType::String, e_var("value"))),
                ],
                vec![],
                None,
            ),
            s_assign("target", e_ternary(e_var("attribute"), e_str("attributes"), e_str("properties"))),
            s_assign("type", e_call("gettype", vec![e_var("value")])),
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![
                    s_assign("type", e_cast(CastType::String, e_index(e_call("explode", vec![e_str("\0"), e_call("get_class", vec![e_var("value")])]), e_int(0)))),
                ],
                vec![
                (e_binop(e_call("is_resource", vec![e_var("value")]), BinOp::Or, e_call("str_starts_with", vec![e_var("type"), e_str("resource")])), vec![
                    s_assign("type", e_str("resource")),
                ]),
            ],
                None,
            ),
            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_str("It's not possible to assign a complex type to "), BinOp::Concat, e_var("target")), BinOp::Concat, e_str(", ")), BinOp::Concat, e_var("type")), BinOp::Concat, e_str(" given"))])),
        ])
        .build()
}

/// `__elephc_simplexml_numeric_value` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_numeric_value() -> Stmt {
    function("__elephc_simplexml_numeric_value")
        .param("value", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_instance_of(e_var("value"), "SimpleXMLElement"),
                vec![
                    s_return(e_method_call(e_var("value"), "__elephc_number", vec![])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("value")),
        ])
        .build()
}

/// `__elephc_simplexml_parse_number` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_parse_number() -> Stmt {
    function("__elephc_simplexml_parse_number")
        .param("text", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
        .body(vec![
            s_assign("text", e_call("ltrim", vec![e_var("text"), e_str(" \t\n\r\u{b}\u{c}")])),
            s_assign("length", e_call("strlen", vec![e_var("text")])),
            s_assign("i", e_int(0)),
            s_assign("negative", e_binop(e_binop(e_var("length"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_index(e_var("text"), e_int(0)), BinOp::StrictEq, e_str("-")))),
            s_if(
                e_binop(e_binop(e_var("length"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_binop(e_index(e_var("text"), e_int(0)), BinOp::StrictEq, e_str("-")), BinOp::Or, e_binop(e_index(e_var("text"), e_int(0)), BinOp::StrictEq, e_str("+")))),
                vec![
                    s_expr(e_post_inc("i")),
                ],
                vec![],
                None,
            ),
            s_assign("start", e_var("i")),
            s_while(e_binop(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("length")), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::GtEq, e_int(48))), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::LtEq, e_int(57))), vec![
                s_expr(e_post_inc("i")),
            ]),
            s_assign("whole", e_call("substr", vec![e_var("text"), e_var("start"), e_binop(e_var("i"), BinOp::Sub, e_var("start"))])),
            s_assign("digits", e_binop(e_var("i"), BinOp::Sub, e_var("start"))),
            s_assign("floating", e_bool(false)),
            s_if(
                e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("length")), BinOp::And, e_binop(e_index(e_var("text"), e_var("i")), BinOp::StrictEq, e_str("."))),
                vec![
                    s_assign("floating", e_bool(true)),
                    s_expr(e_post_inc("i")),
                    s_assign("start", e_var("i")),
                    s_while(e_binop(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("length")), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::GtEq, e_int(48))), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::LtEq, e_int(57))), vec![
                        s_expr(e_post_inc("i")),
                    ]),
                    s_assign("digits", e_binop(e_var("digits"), BinOp::Add, e_binop(e_var("i"), BinOp::Sub, e_var("start")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("digits"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("length")), BinOp::And, e_binop(e_binop(e_index(e_var("text"), e_var("i")), BinOp::StrictEq, e_str("e")), BinOp::Or, e_binop(e_index(e_var("text"), e_var("i")), BinOp::StrictEq, e_str("E")))),
                vec![
                    s_expr(e_post_inc("i")),
                    s_if(
                        e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("length")), BinOp::And, e_binop(e_binop(e_index(e_var("text"), e_var("i")), BinOp::StrictEq, e_str("-")), BinOp::Or, e_binop(e_index(e_var("text"), e_var("i")), BinOp::StrictEq, e_str("+")))),
                        vec![
                            s_expr(e_post_inc("i")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("length")), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::GtEq, e_int(48))), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::LtEq, e_int(57))),
                        vec![
                            s_assign("floating", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_var("floating")),
                vec![
                    s_assign("whole", e_call("ltrim", vec![e_var("whole"), e_str("0")])),
                    s_assign("limit", e_ternary(e_var("negative"), e_str("9223372036854775808"), e_str("9223372036854775807"))),
                    s_assign("size", e_call("strlen", vec![e_var("whole")])),
                    s_assign("floating", e_binop(e_binop(e_var("size"), BinOp::Gt, e_int(19)), BinOp::Or, e_binop(e_binop(e_var("size"), BinOp::StrictEq, e_int(19)), BinOp::And, e_binop(e_call("strcmp", vec![e_var("whole"), e_var("limit")]), BinOp::Gt, e_int(0))))),
                ],
                vec![],
                None,
            ),
            s_return(e_ternary(e_var("floating"), e_cast(CastType::Float, e_var("text")), e_cast(CastType::Int, e_var("text")))),
        ])
        .build()
}

/// `__elephc_simplexml_write_warning` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_write_warning() -> Stmt {
    function("__elephc_simplexml_write_warning")
        .param("message", TypeExpr::Str)
        .param_default("sourceCaller", TypeExpr::Str, e_str(""))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("caller", e_ternary(e_binop(e_var("sourceCaller"), BinOp::StrictEq, e_str("")), e_str("main()"), e_var("sourceCaller"))),
            s_foreach(e_ternary(e_binop(e_var("sourceCaller"), BinOp::StrictEq, e_str("")), e_call("debug_backtrace", vec![e_const("DEBUG_BACKTRACE_IGNORE_ARGS")]), e_array(vec![])), None, "frame", vec![
                s_assign("function", e_cast(CastType::String, e_index(e_var("frame"), e_str("function")))),
                s_if(
                    e_call("str_starts_with", vec![e_var("function"), e_str("__elephc_")]),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("function"), BinOp::StrictEq, e_str("eval")),
                    vec![
                        s_assign("caller", e_str("Unknown")),
                    ],
                    vec![
                    (e_call("isset", vec![e_index(e_var("frame"), e_str("class"))]), vec![
                        s_assign("caller", e_binop(e_binop(e_binop(e_cast(CastType::String, e_index(e_var("frame"), e_str("class"))), BinOp::Concat, e_str("::")), BinOp::Concat, e_var("function")), BinOp::Concat, e_str("()"))),
                    ]),
                ],
                    Some(vec![
                    s_assign("caller", e_binop(e_var("function"), BinOp::Concat, e_str("()"))),
                ]),
                ),
                s_break(1),
            ]),
            s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_binop(e_str("Warning: "), BinOp::Concat, e_var("caller")), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("message")), BinOp::Concat, e_str("\n"))])),
        ])
        .build()
}

/// `__elephc_simplexml_report_diagnostics` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_report_diagnostics() -> Stmt {
    function("__elephc_simplexml_report_diagnostics")
        .param("caller", TypeExpr::Str)
        .param_default("dom", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("text", e_call("elephc_xml_sx_diagnostics", vec![])),
            s_expr(e_call("__elephc_simplexml_report_text", vec![e_var("text"), e_var("caller"), e_var("dom")])),
        ])
        .build()
}

/// `__elephc_simplexml_report_text` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_report_text() -> Stmt {
    function("__elephc_simplexml_report_text")
        .param("text", TypeExpr::Str)
        .param("caller", TypeExpr::Str)
        .param_default("dom", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("text"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("lines", e_call("explode", vec![e_str("\n"), e_var("text")])),
            s_assign("length", e_call("count", vec![e_var("lines")])),
            s_if(
                e_binop(e_binop(e_var("length"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_index(e_var("lines"), e_binop(e_var("length"), BinOp::Sub, e_int(1))), BinOp::StrictEq, e_str(""))),
                vec![
                    s_assign("length", e_binop(e_var("length"), BinOp::Sub, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("line", e_index(e_var("lines"), e_var("i"))),
                s_assign("category", e_str("Warning: ")),
                s_if(
                    e_binop(e_var("dom"), BinOp::And, e_call("str_starts_with", vec![e_var("line"), e_str("Notice: ")])),
                    vec![
                        s_assign("category", e_str("Notice: ")),
                        s_assign("line", e_call("substr", vec![e_var("line"), e_int(8)])),
                    ],
                    vec![
                    (e_binop(e_var("dom"), BinOp::And, e_call("str_starts_with", vec![e_var("line"), e_str("Warning: ")])), vec![
                        s_assign("line", e_call("substr", vec![e_var("line"), e_int(9)])),
                    ]),
                ],
                    None,
                ),
                s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_binop(e_var("category"), BinOp::Concat, e_var("caller")), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("line")), BinOp::Concat, e_str("\n"))])),
            ]),
        ])
        .build()
}

/// `__ElephcSimpleXMLLease` — transcribed from the PHP form.
fn decl_class_elephcsimplexmllease() -> Stmt {
    class("__ElephcSimpleXMLLease")
        .final_()
        .private_prop("handle", TypeExpr::Int, None)
        .method(
            method("__construct")
                .param("handle", TypeExpr::Int)
                .body(vec![
                    s_prop_assign(e_this(), "handle", e_var("handle")),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_assign("handle", e_this_prop("handle")),
                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("handle")])),
                ]),
        )
        .build()
}

/// `SimpleXMLElement` — transcribed from the PHP form.
fn decl_class_simplexmlelement() -> Stmt {
    class("SimpleXMLElement")
        .implements("Stringable")
        .implements("Countable")
        .implements("RecursiveIterator")
        .private_prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .private_prop("__elephc_position", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("__elephc_recovered", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("__elephc_current", t_mixed(), Some(e_null()))
        .private_prop("__elephc_owner", t_mixed(), Some(e_null()))
        .private_prop("__elephc_write_tmp", t_mixed(), Some(e_null()))
        .private_prop("__elephc_projection_cache", t_mixed(), Some(e_null()))
        .method(
            method("__elephc_attach")
                .param("handle", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "__elephc_handle", e_var("handle")),
                    s_prop_assign(e_this(), "__elephc_owner", e_new("__ElephcSimpleXMLLease", vec![e_var("handle")])),
                ]),
        )
        .method(
            method("__elephc_handle_value")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("__elephc_handle")),
                ]),
        )
        .method(
            method("__elephc_initialized_handle")
                .private()
                .param_default("document", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("required", e_ternary(e_var("document"), e_int(1), e_int(2))),
                    s_if(
                        e_binop(e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(15)]), BinOp::BitAnd, e_var("required")), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("SimpleXMLElement is not properly initialized")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("handle")),
                ]),
        )
        .method(
            method("__elephc_set_document_encoding")
                .final_()
                .param("encoding", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![e_bool(true)])),
                    s_return(e_binop(e_call("elephc_xml_sx_set_encoding", vec![e_var("handle"), e_var("encoding")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("__construct")
                .param("data", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .param_default("dataIsURL", TypeExpr::Bool, e_bool(false))
                .param_default("namespaceOrPrefix", TypeExpr::Str, e_str(""))
                .param_default("isPrefix", TypeExpr::Bool, e_bool(false))
                .body(vec![
                    s_if(
                        e_binop(e_call("strlen", vec![e_var("data")]), BinOp::Gt, e_int(2147483647)),
                        vec![
                            s_throw(e_new("Exception", vec![e_str("SimpleXMLElement::__construct(): Argument #1 ($data) is too long")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("strlen", vec![e_var("namespaceOrPrefix")]), BinOp::Gt, e_int(2147483647)),
                        vec![
                            s_throw(e_new("Exception", vec![e_str("SimpleXMLElement::__construct(): Argument #4 ($namespaceOrPrefix) is too long")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("options"), BinOp::Lt, e_neg(e_int(2147483648))), BinOp::Or, e_binop(e_var("options"), BinOp::Gt, e_int(2147483647))),
                        vec![
                            s_throw(e_new("Exception", vec![e_str("SimpleXMLElement::__construct(): Argument #2 ($options) is invalid")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_try(vec![
                        s_if(
                            e_var("dataIsURL"),
                            vec![
                                s_assign("handle", e_call("__elephc_simplexml_file_handle", vec![e_var("data"), e_var("options"), e_var("namespaceOrPrefix"), e_var("isPrefix"), e_str("SimpleXMLElement::__construct()")])),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("handle", e_call("elephc_xml_sx_load", vec![e_var("data"), e_call("strlen", vec![e_var("data")]), e_var("options"), e_var("namespaceOrPrefix"), e_cast(CastType::Int, e_var("isPrefix")), e_str("")])),
                        ]),
                        ),
                    ], vec![
                        (vec!["Throwable"], Some("error"), vec![
                            s_throw(e_new("Exception", vec![e_str("String could not be parsed as XML"), e_int(0), e_var("error")])),
                        ]),
                    ], None),
                    s_if(
                        e_binop(e_var("handle"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_attach", vec![e_var("handle")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_try(vec![
                        s_expr(e_call("__elephc_simplexml_report_diagnostics", vec![e_str("SimpleXMLElement::__construct()")])),
                    ], vec![
                        (vec!["Throwable"], Some("error"), vec![
                            s_if(
                                e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_throw(e_new("Exception", vec![e_str("String could not be parsed as XML"), e_int(0), e_var("error")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_var("error")),
                        ]),
                    ], None),
                    s_if(
                        e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_if(
                                e_binop(e_binop(e_var("options"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0)),
                                vec![
                                    s_prop_assign(e_this(), "__elephc_recovered", e_bool(true)),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new("Exception", vec![e_str("String could not be parsed as XML")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__elephc_current", e_null()),
                    s_prop_assign(e_this(), "__elephc_position", e_neg(e_int(1))),
                ]),
        )
        .method(
            method("__elephc_clone_initialize")
                .final_()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_expr(e_method_call(e_this(), "__elephc_attach", vec![e_call("elephc_xml_sx_clone", vec![e_var("handle")])])),
                    s_prop_assign(e_this(), "__elephc_current", e_null()),
                    s_prop_assign(e_this(), "__elephc_position", e_neg(e_int(1))),
                    s_prop_assign(e_this(), "__elephc_write_tmp", e_null()),
                    s_prop_assign(e_this(), "__elephc_projection_cache", e_null()),
                ]),
        )
        .method(
            method("__toString")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(0)])),
                ]),
        )
        .method(
            method("__elephc_int")
                .final_()
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_cast(CastType::Int, e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(0)]))),
                ]),
        )
        .method(
            method("__elephc_float")
                .final_()
                .returns(TypeExpr::Float)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_cast(CastType::Float, e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(0)]))),
                ]),
        )
        .method(
            method("__elephc_bitnot")
                .final_()
                .returns(t_mixed())
                .body(vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_str("Cannot perform bitwise not on "), BinOp::Concat, e_call("get_class", vec![e_this()]))])),
                ]),
        )
        .method(
            method("__elephc_number")
                .final_()
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("text", e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(0)])),
                    s_return(e_call("__elephc_simplexml_parse_number", vec![e_var("text")])),
                ]),
        )
        .method(
            method("__elephc_add")
                .static_()
                .param("left", t_mixed())
                .param("right", t_mixed())
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .body(vec![
                    s_return(e_binop(e_call("__elephc_simplexml_numeric_value", vec![e_var("left")]), BinOp::Add, e_call("__elephc_simplexml_numeric_value", vec![e_var("right")]))),
                ]),
        )
        .method(
            method("__elephc_sub")
                .static_()
                .param("left", t_mixed())
                .param("right", t_mixed())
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .body(vec![
                    s_return(e_binop(e_call("__elephc_simplexml_numeric_value", vec![e_var("left")]), BinOp::Sub, e_call("__elephc_simplexml_numeric_value", vec![e_var("right")]))),
                ]),
        )
        .method(
            method("__elephc_mul")
                .static_()
                .param("left", t_mixed())
                .param("right", t_mixed())
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .body(vec![
                    s_return(e_binop(e_call("__elephc_simplexml_numeric_value", vec![e_var("left")]), BinOp::Mul, e_call("__elephc_simplexml_numeric_value", vec![e_var("right")]))),
                ]),
        )
        .method(
            method("__elephc_div")
                .static_()
                .param("left", t_mixed())
                .param("right", t_mixed())
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .body(vec![
                    s_return(e_binop(e_call("__elephc_simplexml_numeric_value", vec![e_var("left")]), BinOp::Div, e_call("__elephc_simplexml_numeric_value", vec![e_var("right")]))),
                ]),
        )
        .method(
            method("__elephc_mod")
                .static_()
                .param("left", t_mixed())
                .param("right", t_mixed())
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .body(vec![
                    s_return(e_binop(e_call("__elephc_simplexml_numeric_value", vec![e_var("left")]), BinOp::Mod, e_call("__elephc_simplexml_numeric_value", vec![e_var("right")]))),
                ]),
        )
        .method(
            method("__elephc_pow")
                .static_()
                .param("left", t_mixed())
                .param("right", t_mixed())
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::Float]))
                .body(vec![
                    s_return(e_binop(e_call("__elephc_simplexml_numeric_value", vec![e_var("left")]), BinOp::Pow, e_call("__elephc_simplexml_numeric_value", vec![e_var("right")]))),
                ]),
        )
        .method(
            method("__elephc_compare")
                .static_()
                .param("left", t_mixed())
                .param("right", t_mixed())
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_instance_of(e_var("left"), "SimpleXMLElement"), BinOp::And, e_instance_of(e_var("right"), "SimpleXMLElement")),
                        vec![
                            s_return(e_call("elephc_xml_sx_compare", vec![e_prop(e_var("left"), "__elephc_handle"), e_prop(e_var("right"), "__elephc_handle")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("is_bool", vec![e_var("left")]), BinOp::Or, e_call("is_bool", vec![e_var("right")])),
                        vec![
                            s_assign("a", e_ternary(e_instance_of(e_var("left"), "SimpleXMLElement"), e_method_call(e_var("left"), "__elephc_bool", vec![]), e_cast(CastType::Bool, e_var("left")))),
                            s_assign("b", e_ternary(e_instance_of(e_var("right"), "SimpleXMLElement"), e_method_call(e_var("right"), "__elephc_bool", vec![]), e_cast(CastType::Bool, e_var("right")))),
                            s_return(e_binop(e_cast(CastType::Int, e_var("a")), BinOp::Sub, e_cast(CastType::Int, e_var("b")))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("is_object", vec![e_var("left")]), BinOp::And, e_not(e_instance_of(e_var("left"), "SimpleXMLElement"))),
                        vec![
                            s_return(e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("is_object", vec![e_var("right")]), BinOp::And, e_not(e_instance_of(e_var("right"), "SimpleXMLElement"))),
                        vec![
                            s_return(e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("left"), BinOp::StrictEq, e_null()), BinOp::Or, e_call("is_array", vec![e_var("left")])), BinOp::Or, e_call("is_resource", vec![e_var("left")])), BinOp::Or, e_binop(e_call("gettype", vec![e_var("left")]), BinOp::StrictEq, e_str("resource (closed)"))),
                        vec![
                            s_return(e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("right"), BinOp::StrictEq, e_null()), BinOp::Or, e_call("is_array", vec![e_var("right")])), BinOp::Or, e_call("is_resource", vec![e_var("right")])), BinOp::Or, e_binop(e_call("gettype", vec![e_var("right")]), BinOp::StrictEq, e_str("resource (closed)"))),
                        vec![
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_call("is_float", vec![e_var("left")]), BinOp::And, e_call("is_nan", vec![e_var("left")])), BinOp::Or, e_binop(e_call("is_float", vec![e_var("right")]), BinOp::And, e_call("is_nan", vec![e_var("right")]))),
                        vec![
                            s_return(e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("left"), "SimpleXMLElement"),
                        vec![
                            s_if(
                                e_call("is_int", vec![e_var("right")]),
                                vec![
                                    s_return(e_binop(e_method_call(e_var("left"), "__elephc_int", vec![]), BinOp::Spaceship, e_var("right"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_call("is_float", vec![e_var("right")]),
                                vec![
                                    s_return(e_binop(e_method_call(e_var("left"), "__elephc_float", vec![]), BinOp::Spaceship, e_var("right"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_binop(e_cast(CastType::String, e_var("left")), BinOp::Spaceship, e_cast(CastType::String, e_var("right")))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("is_int", vec![e_var("left")]),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::Spaceship, e_method_call(e_var("right"), "__elephc_int", vec![]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("is_float", vec![e_var("left")]),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::Spaceship, e_method_call(e_var("right"), "__elephc_float", vec![]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_cast(CastType::String, e_var("left")), BinOp::Spaceship, e_cast(CastType::String, e_var("right")))),
                ]),
        )
        .method(
            method("__elephc_bool")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_return(e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(3)]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("__elephc_property_get")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(t_nullable(t_class("static")))
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("mode", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)])),
                    s_if(
                        e_binop(e_binop(e_var("mode"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(1)]), BinOp::StrictEq, e_int(0))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(2), e_var("name"), e_str(""), e_int(0), e_int(0)])),
                    s_if(
                        e_binop(e_binop(e_var("mode"), BinOp::GtEq, e_int(3)), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("selected"), e_int(1)]), BinOp::StrictEq, e_int(0))),
                        vec![
                            s_expr(e_call("elephc_xml_sx_free", vec![e_var("selected")])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_var("selected"), e_call("get_class", vec![e_this()])])),
                ]),
        )
        .method(
            method("__elephc_property_select")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(t_class("static"))
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(2), e_var("name"), e_str(""), e_int(0), e_int(0)])),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_var("selected"), e_call("get_class", vec![e_this()])])),
                ]),
        )
        .method(
            method("__elephc_property_write_fetch")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(t_class("static"))
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_property_write_select", vec![e_var("name"), e_bool(true)])),
                ]),
        )
        .method(
            method("__elephc_property_reference_fetch")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(t_class("static"))
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_property_write_select", vec![e_var("name"), e_bool(false)])),
                ]),
        )
        .method(
            method("__elephc_property_write_select")
                .final_()
                .param("name", TypeExpr::Str)
                .param("keepTemporary", TypeExpr::Bool)
                .returns(t_class("static"))
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("Cannot create element with an empty name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("view", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(2), e_var("name"), e_str(""), e_int(0), e_int(0)])),
                    s_assign("present", e_binop(e_call("elephc_xml_sx_int", vec![e_var("view"), e_int(1)]), BinOp::StrictNotEq, e_int(0))),
                    s_assign("persist", e_binop(e_not(e_var("present")), BinOp::Or, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)]), BinOp::GtEq, e_int(3)))),
                    s_if(
                        e_not(e_var("present")),
                        vec![
                            s_assign("created", e_call("elephc_xml_sx_mutate", vec![e_var("view"), e_int(23), e_str(""), e_str(""), e_str(""), e_int(0), e_int(0)])),
                            s_if(
                                e_binop(e_var("created"), BinOp::Gt, e_int(0)),
                                vec![
                                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("view")])),
                                    s_assign("view", e_var("created")),
                                ],
                                vec![
                                (e_binop(e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(1)]), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(4)]), BinOp::StrictNotEq, e_int(1))), vec![
                                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("view")])),
                                    s_throw(e_new("Error", vec![e_str("SimpleXMLElement is not properly initialized")])),
                                ]),
                            ],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("selected", e_call("__elephc_simplexml_wrap", vec![e_var("view"), e_call("get_class", vec![e_this()])])),
                    s_if(
                        e_binop(e_var("persist"), BinOp::And, e_var("keepTemporary")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_write_tmp", e_var("selected")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("selected")),
                ]),
        )
        .method(
            method("__elephc_property_set")
                .final_()
                .param("name", TypeExpr::Str)
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "__elephc_property_write", vec![e_var("name"), e_var("value"), e_str("")])),
                ]),
        )
        .method(
            method("__elephc_property_update_fetch")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(t_array())
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("Cannot create element with an empty name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("view", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(2), e_var("name"), e_str(""), e_int(0), e_int(0)])),
                    s_assign("present", e_binop(e_call("elephc_xml_sx_int", vec![e_var("view"), e_int(1)]), BinOp::StrictNotEq, e_int(0))),
                    s_assign("store", e_binop(e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)]), BinOp::Lt, e_int(3)), BinOp::And, e_var("present"))),
                    s_if(
                        e_not(e_var("present")),
                        vec![
                            s_assign("created", e_call("elephc_xml_sx_mutate", vec![e_var("view"), e_int(23), e_str(""), e_str(""), e_str(""), e_int(0), e_int(0)])),
                            s_if(
                                e_binop(e_var("created"), BinOp::Gt, e_int(0)),
                                vec![
                                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("view")])),
                                    s_assign("view", e_var("created")),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("selected", e_call("__elephc_simplexml_wrap", vec![e_var("view"), e_call("get_class", vec![e_this()])])),
                    s_if(
                        e_not(e_var("store")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_write_tmp", e_var("selected")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array(vec![e_var("selected"), e_var("store")])),
                ]),
        )
        .method(
            method("__elephc_property_update")
                .final_()
                .param("name", TypeExpr::Str)
                .param("right", t_mixed())
                .param("operation", TypeExpr::Int)
                .param_default("post", TypeExpr::Bool, e_bool(false))
                .returns(t_mixed())
                .body(vec![
                    s_assign("fetch", e_method_call(e_this(), "__elephc_property_update_fetch", vec![e_var("name")])),
                    s_assign("current", e_index(e_var("fetch"), e_int(0))),
                    s_assign("result", e_self_call("__elephc_apply_property_update", vec![e_var("current"), e_var("right"), e_var("operation")])),
                    s_if(
                        e_index(e_var("fetch"), e_int(1)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_property_set", vec![e_var("name"), e_var("result")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_ternary(e_var("post"), e_var("current"), e_var("result"))),
                ]),
        )
        .method(
            method("__elephc_apply_property_update")
                .private()
                .static_()
                .param("left", t_mixed())
                .param("right", t_mixed())
                .param("operation", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::Add, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::Sub, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::Mul, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::Div, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::Mod, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(5)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::Pow, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(6)),
                        vec![
                            s_return(e_binop(e_cast(CastType::String, e_var("left")), BinOp::Concat, e_cast(CastType::String, e_var("right")))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(7)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::BitAnd, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(8)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::BitOr, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(9)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::BitXor, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(10)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::ShiftLeft, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("operation"), BinOp::StrictEq, e_int(11)),
                        vec![
                            s_return(e_binop(e_var("left"), BinOp::ShiftRight, e_var("right"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("Error", vec![e_str("Unknown SimpleXML property update operation")])),
                ]),
        )
        .method(
            method("__elephc_eval_property_set")
                .final_()
                .param("name", TypeExpr::Str)
                .param("value", t_mixed())
                .param("caller", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "__elephc_property_write", vec![e_var("name"), e_var("value"), e_var("caller")])),
                ]),
        )
        .method(
            method("__elephc_property_write")
                .private()
                .param("name", TypeExpr::Str)
                .param("value", t_mixed())
                .param("caller", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("Cannot create element with an empty name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("mode", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)])),
                    s_assign("text", e_call("__elephc_simplexml_assigned_value", vec![e_var("value"), e_binop(e_binop(e_var("mode"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("mode"), BinOp::StrictEq, e_int(4)))])),
                    s_assign("status", e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(2), e_var("name"), e_var("text"), e_str(""), e_int(0), e_int(0)])),
                    s_if(
                        e_binop(e_var("status"), BinOp::StrictEq, e_neg(e_int(2))),
                        vec![
                            s_expr(e_call("__elephc_simplexml_write_warning", vec![e_str("Cannot assign to an array of nodes (duplicate subnodes or attr detected)"), e_var("caller")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_property_isset")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(2), e_var("name"), e_str(""), e_int(0), e_int(0)])),
                    s_assign("present", e_binop(e_call("elephc_xml_sx_int", vec![e_var("selected"), e_int(1)]), BinOp::StrictNotEq, e_int(0))),
                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("selected")])),
                    s_return(e_var("present")),
                ]),
        )
        .method(
            method("__elephc_property_presence")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(t_nullable(TypeExpr::Bool))
                .body(vec![
                    s_return(e_ternary(e_method_call(e_this(), "__elephc_property_isset", vec![e_var("name")]), e_bool(true), e_null())),
                ]),
        )
        .method(
            method("__elephc_property_probe")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_method_call(e_this(), "__elephc_property_presence", vec![e_var("name")]), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "__elephc_property_get", vec![e_var("name")])),
                ]),
        )
        .method(
            method("__elephc_property_empty")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(2), e_var("name"), e_str(""), e_int(0), e_int(0)])),
                    s_assign("empty", e_binop(e_call("elephc_xml_sx_int", vec![e_var("selected"), e_int(7)]), BinOp::StrictNotEq, e_int(0))),
                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("selected")])),
                    s_return(e_var("empty")),
                ]),
        )
        .method(
            method("__elephc_dimension_empty")
                .final_()
                .param("offset", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_if(
                        e_call("is_int", vec![e_var("offset")]),
                        vec![
                            s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(4), e_str(""), e_str(""), e_int(0), e_var("offset")])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(3), e_cast(CastType::String, e_var("offset")), e_str(""), e_int(0), e_int(0)])),
                    ]),
                    ),
                    s_assign("empty", e_binop(e_call("elephc_xml_sx_int", vec![e_var("selected"), e_int(7)]), BinOp::StrictNotEq, e_int(0))),
                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("selected")])),
                    s_return(e_var("empty")),
                ]),
        )
        .method(
            method("__elephc_property_unset")
                .final_()
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_expr(e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(12), e_var("name"), e_str(""), e_str(""), e_int(0), e_int(0)])),
                ]),
        )
        .method(
            method("__elephc_dimension_get")
                .final_()
                .param("offset", t_mixed())
                .returns(t_mixed())
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_if(
                        e_call("is_int", vec![e_var("offset")]),
                        vec![
                            s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(4), e_str(""), e_str(""), e_int(0), e_var("offset")])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(3), e_cast(CastType::String, e_var("offset")), e_str(""), e_int(0), e_int(0)])),
                    ]),
                    ),
                    s_if(
                        e_binop(e_call("elephc_xml_sx_int", vec![e_var("selected"), e_int(1)]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_xml_sx_free", vec![e_var("selected")])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_var("selected"), e_call("get_class", vec![e_this()])])),
                ]),
        )
        .method(
            method("__elephc_dimension_isset")
                .final_()
                .param("offset", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_if(
                        e_call("is_int", vec![e_var("offset")]),
                        vec![
                            s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(4), e_str(""), e_str(""), e_int(0), e_var("offset")])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(3), e_cast(CastType::String, e_var("offset")), e_str(""), e_int(0), e_int(0)])),
                    ]),
                    ),
                    s_assign("present", e_binop(e_call("elephc_xml_sx_int", vec![e_var("selected"), e_int(1)]), BinOp::StrictNotEq, e_int(0))),
                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("selected")])),
                    s_return(e_var("present")),
                ]),
        )
        .method(
            method("__elephc_eval_dimension_select")
                .final_()
                .param("offset", t_mixed())
                .param("caller", TypeExpr::Str)
                .param_default("throwOnFailure", TypeExpr::Bool, e_bool(true))
                .param_default("create", TypeExpr::Bool, e_bool(true))
                .param_default("numeric", TypeExpr::Bool, e_bool(true))
                .returns(t_mixed())
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_dimension_select", vec![e_var("offset"), e_var("throwOnFailure"), e_var("create"), e_var("numeric"), e_var("caller")])),
                ]),
        )
        .method(
            method("__elephc_dimension_select")
                .final_()
                .param("offset", t_mixed())
                .param_default("throwOnFailure", TypeExpr::Bool, e_bool(true))
                .param_default("create", TypeExpr::Bool, e_bool(true))
                .param_default("numeric", TypeExpr::Bool, e_bool(true))
                .param_default("caller", TypeExpr::Str, e_str(""))
                .returns(t_mixed())
                .body(vec![
                    s_assign("selected", e_method_call(e_this(), "__elephc_dimension_get", vec![e_var("offset")])),
                    s_if(
                        e_binop(e_var("selected"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_return(e_ternary(e_binop(e_binop(e_not(e_var("throwOnFailure")), BinOp::And, e_var("create")), BinOp::And, e_var("numeric")), e_method_call(e_var("selected"), "__elephc_number", vec![]), e_var("selected"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_not(e_var("create")), BinOp::Or, e_not(e_call("is_int", vec![e_var("offset")]))),
                        vec![
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_str("Notice: Indirect modification of overloaded element of "), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(" has no effect\n"))])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("mode", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)])),
                    s_assign("length", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(0)])),
                    s_if(
                        e_binop(e_binop(e_var("mode"), BinOp::StrictEq, e_int(2)), BinOp::And, e_binop(e_var("length"), BinOp::StrictEq, e_int(0))),
                        vec![
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_str("Notice: Indirect modification of overloaded element of "), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(" has no effect\n"))])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("mode"), BinOp::Lt, e_int(3)),
                        vec![
                            s_assign("length", e_ternary(e_binop(e_var("mode"), BinOp::StrictEq, e_int(0)), e_int(0), e_var("length"))),
                            s_if(
                                e_binop(e_binop(e_binop(e_var("offset"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("offset"), BinOp::Gt, e_var("length"))), BinOp::Or, e_binop(e_var("mode"), BinOp::StrictEq, e_int(0))),
                                vec![
                                    s_assign("name", e_call("elephc_xml_sx_string", vec![e_var("handle"), e_ternary(e_binop(e_var("mode"), BinOp::StrictEq, e_int(1)), e_int(5), e_int(1))])),
                                    s_expr(e_call("__elephc_simplexml_write_warning", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Cannot add element "), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(" number ")), BinOp::Concat, e_var("offset")), BinOp::Concat, e_str(" when only ")), BinOp::Concat, e_var("length")), BinOp::Concat, e_str(" such elements exist")), e_var("caller")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("offset"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_var("mode"), BinOp::StrictNotEq, e_int(0))),
                                vec![
                                    s_assign("added", e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(24), e_str(""), e_str(""), e_str(""), e_int(0), e_int(0)])),
                                    s_if(
                                        e_binop(e_var("added"), BinOp::Gt, e_int(0)),
                                        vec![
                                            s_assign("selected", e_call("__elephc_simplexml_wrap", vec![e_var("added"), e_call("get_class", vec![e_this()])])),
                                            s_return(e_ternary(e_binop(e_binop(e_not(e_var("throwOnFailure")), BinOp::And, e_var("create")), BinOp::And, e_var("numeric")), e_method_call(e_var("selected"), "__elephc_number", vec![]), e_var("selected"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_var("throwOnFailure"),
                                        vec![
                                            s_throw(e_new("Error", vec![e_str("SimpleXMLElement is not properly initialized")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_str("Notice: Indirect modification of overloaded element of "), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(" has no effect\n"))])),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("__elephc_dimension_set")
                .final_()
                .param("offset", t_mixed())
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "__elephc_dimension_write", vec![e_var("offset"), e_var("value"), e_bool(false)])),
                ]),
        )
        .method(
            method("__elephc_eval_dimension_set")
                .final_()
                .param("offset", t_mixed())
                .param("value", t_mixed())
                .param("caller", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "__elephc_dimension_write", vec![e_var("offset"), e_var("value"), e_bool(false), e_var("caller")])),
                ]),
        )
        .method(
            method("__elephc_dimension_write")
                .final_()
                .param("offset", t_mixed())
                .param("value", t_mixed())
                .param("append", TypeExpr::Bool)
                .param_default("caller", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("mode", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)])),
                    s_assign("attributes", e_binop(e_binop(e_var("mode"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("mode"), BinOp::StrictEq, e_int(4)))),
                    s_assign("kind", e_int(3)),
                    s_assign("index", e_int(0)),
                    s_assign("key", e_str("")),
                    s_if(
                        e_var("append"),
                        vec![
                            s_assign("kind", e_int(5)),
                        ],
                        vec![
                        (e_call("is_int", vec![e_var("offset")]), vec![
                            s_assign("kind", e_int(4)),
                            s_assign("index", e_var("offset")),
                        ]),
                    ],
                        Some(vec![
                        s_assign("key", e_ternary(e_call("is_string", vec![e_var("offset")]), e_var("offset"), e_call("trim", vec![e_cast(CastType::String, e_var("offset"))]))),
                    ]),
                    ),
                    s_if(
                        e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)), BinOp::And, e_var("attributes")),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Cannot append to an attribute list")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)), BinOp::And, e_binop(e_var("mode"), BinOp::StrictEq, e_int(0))), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(9)]), BinOp::StrictNotEq, e_int(0))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("Cannot append to an attribute list")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), BinOp::And, e_binop(e_var("key"), BinOp::StrictEq, e_str(""))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("Cannot create attribute with an empty name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("text", e_call("__elephc_simplexml_assigned_value", vec![e_var("value"), e_binop(e_var("attributes"), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)))])),
                    s_if(
                        e_binop(e_binop(e_not(e_var("attributes")), BinOp::And, e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)))), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(4)]), BinOp::StrictEq, e_int(2))),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Cannot create duplicate attribute")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)), BinOp::And, e_var("attributes")),
                        vec![
                            s_assign("length", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(0)])),
                            s_if(
                                e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("index"), BinOp::GtEq, e_var("length"))),
                                vec![
                                    s_assign("length", e_ternary(e_binop(e_var("index"), BinOp::Lt, e_int(0)), e_int(0), e_var("length"))),
                                    s_expr(e_call("__elephc_simplexml_write_warning", vec![e_binop(e_binop(e_binop(e_binop(e_str("Cannot change attribute number "), BinOp::Concat, e_var("index")), BinOp::Concat, e_str(" when only ")), BinOp::Concat, e_var("length")), BinOp::Concat, e_str(" attributes exist")), e_var("caller")])),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)), BinOp::And, e_binop(e_var("mode"), BinOp::StrictEq, e_int(0))), BinOp::And, e_binop(e_var("index"), BinOp::StrictNotEq, e_int(0))), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(1)]), BinOp::StrictNotEq, e_int(0))),
                        vec![
                            s_assign("name", e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(1)])),
                            s_expr(e_call("__elephc_simplexml_write_warning", vec![e_binop(e_binop(e_binop(e_binop(e_str("Cannot add element "), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(" number ")), BinOp::Concat, e_var("index")), BinOp::Concat, e_str(" when only 0 such elements exist")), e_var("caller")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)), BinOp::And, e_binop(e_var("mode"), BinOp::StrictEq, e_int(2))),
                        vec![
                            s_assign("length", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(0)])),
                            s_if(
                                e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_binop(e_var("length"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_var("index"), BinOp::Gt, e_var("length")))),
                                vec![
                                    s_assign("name", e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(1)])),
                                    s_expr(e_call("__elephc_simplexml_write_warning", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Cannot add element "), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(" number ")), BinOp::Concat, e_var("index")), BinOp::Concat, e_str(" when only ")), BinOp::Concat, e_var("length")), BinOp::Concat, e_str(" such elements exist")), e_var("caller")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("status", e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_var("kind"), e_var("key"), e_var("text"), e_str(""), e_int(0), e_var("index")])),
                    s_if(
                        e_binop(e_var("status"), BinOp::StrictEq, e_neg(e_int(2))),
                        vec![
                            s_expr(e_call("__elephc_simplexml_write_warning", vec![e_str("Cannot assign to an array of nodes (duplicate subnodes or attr detected)"), e_var("caller")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_append")
                .final_()
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "__elephc_dimension_write", vec![e_null(), e_var("value"), e_bool(true)])),
                ]),
        )
        .method(
            method("__elephc_simplexml_root_append_failure")
                .final_()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(9)]), BinOp::StrictEq, e_int(0))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_str("Notice: Indirect modification of overloaded element of "), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(" has no effect\n"))])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_dimension_unset")
                .final_()
                .param("offset", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_if(
                        e_call("is_int", vec![e_var("offset")]),
                        vec![
                            s_expr(e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(14), e_str(""), e_str(""), e_str(""), e_int(0), e_var("offset")])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(13), e_cast(CastType::String, e_var("offset")), e_str(""), e_str(""), e_int(0), e_int(0)])),
                    ]),
                    ),
                ]),
        )
        .method(
            method("addChild")
                .param("qualifiedName", TypeExpr::Str)
                .param_default("value", t_nullable(TypeExpr::Str), e_null())
                .param_default("namespace", t_nullable(TypeExpr::Str), e_null())
                .returns(t_nullable(t_class("SimpleXMLElement")))
                .body(vec![
                    s_if(
                        e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("SimpleXMLElement::addChild(): Argument #1 ($qualifiedName) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("flags", e_int(0)),
                    s_if(
                        e_binop(e_var("value"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("flags", e_binop(e_var("flags"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("namespace"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("flags", e_binop(e_var("flags"), BinOp::Add, e_int(2))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("child", e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(20), e_var("qualifiedName"), e_null_coalesce(e_var("value"), e_str("")), e_null_coalesce(e_var("namespace"), e_str("")), e_var("flags"), e_int(0)])),
                    s_if(
                        e_binop(e_var("child"), BinOp::LtEq, e_int(0)),
                        vec![
                            s_if(
                                e_binop(e_var("child"), BinOp::StrictEq, e_neg(e_int(10))),
                                vec![
                                    s_expr(e_call("__elephc_simplexml_warn", vec![e_str("Warning: SimpleXMLElement::addChild(): Cannot add element to attributes\n")])),
                                ],
                                vec![
                                (e_binop(e_var("child"), BinOp::StrictEq, e_neg(e_int(11))), vec![
                                    s_expr(e_call("__elephc_simplexml_warn", vec![e_str("Warning: SimpleXMLElement::addChild(): Cannot add child. Parent is not a permanent member of the XML tree\n")])),
                                ]),
                            ],
                                None,
                            ),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_var("child"), e_call("get_class", vec![e_this()])])),
                ]),
        )
        .method(
            method("addAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .param_default("namespace", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("SimpleXMLElement::addAttribute(): Argument #1 ($qualifiedName) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("flags", e_int(1)),
                    s_if(
                        e_binop(e_var("namespace"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("flags", e_int(3)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("attribute", e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(21), e_var("qualifiedName"), e_var("value"), e_null_coalesce(e_var("namespace"), e_str("")), e_var("flags"), e_int(0)])),
                    s_if(
                        e_binop(e_var("attribute"), BinOp::StrictEq, e_neg(e_int(12))),
                        vec![
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_str("Warning: SimpleXMLElement::addAttribute(): Unable to locate parent Element\n")])),
                        ],
                        vec![
                        (e_binop(e_var("attribute"), BinOp::StrictEq, e_neg(e_int(13))), vec![
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_str("Warning: SimpleXMLElement::addAttribute(): Attribute requires prefix for namespace\n")])),
                        ]),
                        (e_binop(e_var("attribute"), BinOp::StrictEq, e_neg(e_int(14))), vec![
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_str("Warning: SimpleXMLElement::addAttribute(): Attribute already exists\n")])),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("attribute"), BinOp::Gt, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_xml_sx_free", vec![e_var("attribute")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("asXML")
                .param_default("filename", t_nullable(TypeExpr::Str), e_null())
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_save_xml", vec![e_var("filename"), e_str("asXML")])),
                ]),
        )
        .method(
            method("saveXML")
                .param_default("filename", t_nullable(TypeExpr::Str), e_null())
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_save_xml", vec![e_var("filename"), e_str("saveXML")])),
                ]),
        )
        .method(
            method("__elephc_save_xml")
                .private()
                .param("filename", t_nullable(TypeExpr::Str))
                .param("method", TypeExpr::Str)
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("filename"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("strpos", vec![e_var("filename"), e_str("\0")]), BinOp::StrictNotEq, e_bool(false))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_binop(e_str("SimpleXMLElement::"), BinOp::Concat, e_var("method")), BinOp::Concat, e_str("(): Argument #1 ($filename) must not contain any null bytes"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_if(
                        e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(1)]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("xml", e_cast(CastType::String, e_call("hex2bin", vec![e_call("elephc_xml_sx_xml_hex", vec![e_var("handle"), e_int(1)])]))),
                    s_if(
                        e_binop(e_var("filename"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_var("xml")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_contains", vec![e_var("filename"), e_str("%00")]),
                        vec![
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_str("Warning: SimpleXMLElement::"), BinOp::Concat, e_var("method")), BinOp::Concat, e_str("(): URI must not contain percent-encoded NUL bytes")), BinOp::Concat, e_str("\n"))])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("filename"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("Path must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("written", e_call("file_put_contents", vec![e_var("filename"), e_var("xml")])),
                    s_if(
                        e_binop(e_binop(e_var("written"), BinOp::StrictEq, e_bool(false)), BinOp::Or, e_binop(e_var("written"), BinOp::Lt, e_int(0))),
                        vec![
                            s_assign("reason", e_call("elephc_xml_sx_file_error", vec![])),
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Warning: SimpleXMLElement::"), BinOp::Concat, e_var("method")), BinOp::Concat, e_str("(")), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("): Failed to open stream: ")), BinOp::Concat, e_var("reason")), BinOp::Concat, e_str("\n"))])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("attributes")
                .param_default("namespaceOrPrefix", t_nullable(TypeExpr::Str), e_null())
                .param_default("isPrefix", TypeExpr::Bool, e_bool(false))
                .returns(t_nullable(t_class("SimpleXMLElement")))
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_if(
                        e_binop(e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(1)]), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)]), BinOp::GtEq, e_int(3))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(1), e_str(""), e_null_coalesce(e_var("namespaceOrPrefix"), e_str("")), e_cast(CastType::Int, e_var("isPrefix")), e_int(0)])),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_var("selected"), e_call("get_class", vec![e_this()])])),
                ]),
        )
        .method(
            method("children")
                .param_default("namespaceOrPrefix", t_nullable(TypeExpr::Str), e_null())
                .param_default("isPrefix", TypeExpr::Bool, e_bool(false))
                .returns(t_nullable(t_class("SimpleXMLElement")))
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_if(
                        e_binop(e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(1)]), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)]), BinOp::GtEq, e_int(3))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("selected", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(0), e_str(""), e_null_coalesce(e_var("namespaceOrPrefix"), e_str("")), e_cast(CastType::Int, e_var("isPrefix")), e_int(0)])),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_var("selected"), e_call("get_class", vec![e_this()])])),
                ]),
        )
        .method(
            method("count")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_return(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(0)])),
                ]),
        )
        .method(
            method("getName")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_return(e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(1)])),
                ]),
        )
        .method(
            method("registerXPathNamespace")
                .param("prefix", TypeExpr::Str)
                .param("namespace", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![e_bool(true)])),
                    s_return(e_binop(e_call("elephc_xml_sx_register", vec![e_var("handle"), e_var("prefix"), e_var("namespace")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("xpath")
                .param("expression", TypeExpr::Str)
                .returns(t_union(vec![t_array(), TypeExpr::False, TypeExpr::Void]))
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("result", e_call("elephc_xml_sx_xpath", vec![e_var("handle"), e_var("expression")])),
                    s_if(
                        e_binop(e_var("result"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("result"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_call("__elephc_simplexml_report_diagnostics", vec![e_str("SimpleXMLElement::xpath()")])),
                            s_if(
                                e_binop(e_var("result"), BinOp::StrictNotEq, e_neg(e_int(1))),
                                vec![
                                    s_assign("type", e_str("undefined")),
                                    s_if(
                                        e_binop(e_var("result"), BinOp::StrictEq, e_neg(e_int(3))),
                                        vec![
                                            s_assign("type", e_str("bool")),
                                        ],
                                        vec![
                                        (e_binop(e_var("result"), BinOp::StrictEq, e_neg(e_int(4))), vec![
                                            s_assign("type", e_str("number")),
                                        ]),
                                        (e_binop(e_var("result"), BinOp::StrictEq, e_neg(e_int(5))), vec![
                                            s_assign("type", e_str("string")),
                                        ]),
                                    ],
                                        None,
                                    ),
                                    s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_str("Warning: SimpleXMLElement::xpath(): XPath expression must return a node set, "), BinOp::Concat, e_var("type")), BinOp::Concat, e_str(" returned\n"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("nodes", e_array(vec![])),
                    s_assign("length", e_call("elephc_xml_sx_int", vec![e_var("result"), e_int(2)])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("child", e_call("elephc_xml_sx_item", vec![e_var("result"), e_var("i")])),
                        s_array_push("nodes", e_call("__elephc_simplexml_wrap", vec![e_var("child"), e_call("get_class", vec![e_this()])])),
                    ]),
                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("result")])),
                    s_return(e_var("nodes")),
                ]),
        )
        .method(
            method("getNamespaces")
                .param_default("recursive", TypeExpr::Bool, e_bool(false))
                .returns(t_array())
                .body(vec![
                    s_if(
                        e_this_prop("__elephc_recovered"),
                        vec![
                            s_return(e_array(vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_return(e_method_call(e_this(), "__elephc_namespaces", vec![e_bool(false), e_var("recursive"), e_bool(false)])),
                ]),
        )
        .method(
            method("getDocNamespaces")
                .param_default("recursive", TypeExpr::Bool, e_bool(false))
                .param_default("fromRoot", TypeExpr::Bool, e_bool(true))
                .returns(t_union(vec![t_array(), TypeExpr::False]))
                .body(vec![
                    s_if(
                        e_this_prop("__elephc_recovered"),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![e_var("fromRoot")])),
                    s_if(
                        e_binop(e_var("fromRoot"), BinOp::And, e_binop(e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(15)]), BinOp::BitAnd, e_int(4)), BinOp::StrictEq, e_int(0))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "__elephc_namespaces", vec![e_bool(true), e_var("recursive"), e_var("fromRoot")])),
                ]),
        )
        .method(
            method("__elephc_namespaces")
                .private()
                .param("declared", TypeExpr::Bool)
                .param("recursive", TypeExpr::Bool)
                .param("root", TypeExpr::Bool)
                .returns(t_array())
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("packed", e_call("elephc_xml_sx_namespace", vec![e_var("handle"), e_cast(CastType::Int, e_var("declared")), e_cast(CastType::Int, e_var("recursive")), e_cast(CastType::Int, e_var("root")), e_neg(e_int(2)), e_int(0)])),
                    s_assign("parts", e_call("explode", vec![e_str("\0"), e_cast(CastType::String, e_call("hex2bin", vec![e_var("packed")]))])),
                    s_assign("length", e_call("count", vec![e_var("parts")])),
                    s_assign("namespaces", e_array(vec![])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_binop(e_var("i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("length"))), Some(s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(2)))), vec![
                        s_array_assign("namespaces", e_index(e_var("parts"), e_var("i")), e_index(e_var("parts"), e_binop(e_var("i"), BinOp::Add, e_int(1)))),
                    ]),
                    s_return(e_var("namespaces")),
                ]),
        )
        .method(
            method("__elephc_array")
                .returns(TypeExpr::AssocArray { key: Box::new(TypeExpr::Str), value: Box::new(t_mixed()) })
                .body(vec![
                    s_prop_assign(e_this(), "__elephc_projection_cache", e_null()),
                    s_assign("result", e_method_call(e_this(), "__elephc_projection", vec![e_bool(false)])),
                    s_prop_assign(e_this(), "__elephc_projection_cache", e_var("result")),
                    s_return(e_var("result")),
                ]),
        )
        .method(
            method("__elephc_projection")
                .private()
                .param("debug", TypeExpr::Bool)
                .returns(TypeExpr::AssocArray { key: Box::new(TypeExpr::Str), value: Box::new(t_mixed()) })
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_assign("mode", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)])),
                    s_assign("base", e_call("__elephc_simplexml_wrap", vec![e_call("elephc_xml_sx_projection_base", vec![e_var("handle")]), e_str("SimpleXMLElement")])),
                    s_assign("result", e_array_assoc(vec![(e_str("__elephc_projection_seed"), e_null())])),
                    s_expr(e_call("unset", vec![e_index(e_var("result"), e_str("__elephc_projection_seed"))])),
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_assign("values", e_method_call(e_this(), "__elephc_projection_values", vec![e_bool(true)])),
                            s_if(
                                e_binop(e_call("count", vec![e_var("values")]), BinOp::Gt, e_int(0)),
                                vec![
                                    s_array_assign("result", e_str("@attributes"), e_var("values")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("result")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("mode"), BinOp::StrictNotEq, e_int(3)), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(4)]), BinOp::StrictEq, e_int(2))),
                        vec![
                            s_array_assign("result", e_str("0"), e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(0)])),
                            s_return(e_var("result")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("base_handle", e_method_call(e_var("base"), "__elephc_handle_value", vec![])),
                    s_assign("attributes", e_call("__elephc_simplexml_wrap", vec![e_call("elephc_xml_sx_select", vec![e_var("base_handle"), e_int(8), e_str(""), e_str(""), e_int(0), e_int(0)]), e_str("SimpleXMLElement")])),
                    s_if(
                        e_binop(e_binop(e_var("debug"), BinOp::Or, e_binop(e_var("mode"), BinOp::StrictNotEq, e_int(1))), BinOp::And, e_instance_of(e_var("attributes"), "SimpleXMLElement")),
                        vec![
                            s_if(
                                e_binop(e_binop(e_call("elephc_xml_sx_int", vec![e_var("base_handle"), e_int(1)]), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_method_call(e_var("attributes"), "count", vec![]), BinOp::Gt, e_int(0))),
                                vec![
                                    s_assign("values", e_array_assoc(vec![(e_str("__elephc_projection_seed"), e_str(""))])),
                                    s_expr(e_call("unset", vec![e_index(e_var("values"), e_str("__elephc_projection_seed"))])),
                                    s_foreach(e_var("attributes"), Some("name"), "attribute", vec![
                                        s_array_assign("values", e_var("name"), e_call("elephc_xml_sx_string", vec![e_method_call(e_var("attribute"), "__elephc_handle_value", vec![]), e_int(0)])),
                                    ]),
                                    s_array_assign("result", e_str("@attributes"), e_var("values")),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_return(e_var("result")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("mode"), BinOp::StrictEq, e_int(2)), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(10)]), BinOp::StrictNotEq, e_int(0))),
                        vec![
                            s_assign("values", e_method_call(e_this(), "__elephc_projection_values", vec![e_bool(false)])),
                            s_foreach(e_var("values"), Some("index"), "value", vec![
                                s_array_assign("result", e_var("index"), e_var("value")),
                            ]),
                            s_return(e_var("result")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("child_kind", e_ternary(e_binop(e_var("mode"), BinOp::StrictEq, e_int(1)), e_int(10), e_int(9))),
                    s_assign("child_source", e_ternary(e_binop(e_var("mode"), BinOp::StrictEq, e_int(1)), e_var("handle"), e_var("base_handle"))),
                    s_assign("children", e_call("elephc_xml_sx_select", vec![e_var("child_source"), e_var("child_kind"), e_str(""), e_str(""), e_int(0), e_int(0)])),
                    s_assign("has_children", e_bool(false)),
                    s_try(vec![
                        s_expr(e_call("elephc_xml_sx_iterator", vec![e_var("children"), e_int(0)])),
                        s_while(e_binop(e_call("elephc_xml_sx_iterator", vec![e_var("children"), e_int(2)]), BinOp::StrictNotEq, e_int(0)), vec![
                            s_assign("has_children", e_bool(true)),
                            s_assign("raw", e_call("elephc_xml_sx_iterator", vec![e_var("children"), e_int(3)])),
                            s_try(vec![
                                s_assign("name", e_call("elephc_xml_sx_string", vec![e_var("raw"), e_int(1)])),
                                s_assign("value", e_ternary(e_binop(e_call("elephc_xml_sx_int", vec![e_var("raw"), e_int(6)]), BinOp::StrictNotEq, e_int(0)), e_call("elephc_xml_sx_string", vec![e_var("raw"), e_int(0)]), e_call("__elephc_simplexml_wrap", vec![e_call("elephc_xml_sx_alias", vec![e_var("raw")]), e_call("get_class", vec![e_this()])]))),
                                s_if(
                                    e_call("array_key_exists", vec![e_var("name"), e_var("result")]),
                                    vec![
                                        s_if(
                                            e_call("is_array", vec![e_index(e_var("result"), e_var("name"))]),
                                            vec![
                                                s_synthetic(vec![
                                                    s_assign("__elephc_napp_677_57_0#gen", e_index(e_var("result"), e_var("name"))),
                                                    s_array_push("__elephc_napp_677_57_0#gen", e_var("value")),
                                                    s_array_assign("result", e_var("name"), e_var("__elephc_napp_677_57_0#gen")),
                                                ]),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_array_assign("result", e_var("name"), e_array(vec![e_index(e_var("result"), e_var("name")), e_var("value")])),
                                        ]),
                                        ),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_array_assign("result", e_var("name"), e_var("value")),
                                ]),
                                ),
                            ], vec![
                            ], Some(vec![
                                s_expr(e_call("elephc_xml_sx_free", vec![e_var("raw")])),
                            ])),
                            s_expr(e_call("elephc_xml_sx_iterator", vec![e_var("children"), e_int(1)])),
                        ]),
                    ], vec![
                    ], Some(vec![
                        s_expr(e_call("elephc_xml_sx_free", vec![e_var("children")])),
                    ])),
                    s_if(
                        e_var("has_children"),
                        vec![
                            s_return(e_var("result")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("mode"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_call("elephc_xml_sx_int", vec![e_var("base_handle"), e_int(11)]), BinOp::StrictNotEq, e_int(0))),
                        vec![
                            s_array_assign("result", e_str("0"), e_call("elephc_xml_sx_string", vec![e_var("base_handle"), e_int(0)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("result")),
                ]),
        )
        .method(
            method("__elephc_projection_values")
                .private()
                .param("attributes", TypeExpr::Bool)
                .returns(TypeExpr::AssocArray { key: Box::new(TypeExpr::Str), value: Box::new(t_mixed()) })
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("selection", e_call("elephc_xml_sx_alias", vec![e_var("handle")])),
                    s_assign("values", e_array_assoc(vec![(e_str("__elephc_projection_seed"), e_null())])),
                    s_expr(e_call("unset", vec![e_index(e_var("values"), e_str("__elephc_projection_seed"))])),
                    s_try(vec![
                        s_expr(e_call("elephc_xml_sx_iterator", vec![e_var("selection"), e_int(0)])),
                        s_assign("index", e_int(0)),
                        s_while(e_binop(e_call("elephc_xml_sx_iterator", vec![e_var("selection"), e_int(2)]), BinOp::StrictNotEq, e_int(0)), vec![
                            s_assign("node", e_call("elephc_xml_sx_iterator", vec![e_var("selection"), e_int(3)])),
                            s_try(vec![
                                s_assign("key", e_ternary(e_var("attributes"), e_call("elephc_xml_sx_string", vec![e_var("node"), e_int(1)]), e_cast(CastType::String, e_var("index")))),
                                s_array_assign("values", e_var("key"), e_ternary(e_binop(e_var("attributes"), BinOp::Or, e_binop(e_call("elephc_xml_sx_int", vec![e_var("node"), e_int(6)]), BinOp::StrictNotEq, e_int(0))), e_call("elephc_xml_sx_string", vec![e_var("node"), e_int(0)]), e_call("__elephc_simplexml_wrap", vec![e_call("elephc_xml_sx_alias", vec![e_var("node")]), e_call("get_class", vec![e_this()])]))),
                            ], vec![
                            ], Some(vec![
                                s_expr(e_call("elephc_xml_sx_free", vec![e_var("node")])),
                            ])),
                            s_expr(e_post_inc("index")),
                            s_expr(e_call("elephc_xml_sx_iterator", vec![e_var("selection"), e_int(1)])),
                        ]),
                    ], vec![
                    ], Some(vec![
                        s_expr(e_call("elephc_xml_sx_free", vec![e_var("selection")])),
                    ])),
                    s_return(e_var("values")),
                ]),
        )
        .method(
            method("__elephc_json")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_cast(CastType::Object, e_method_call(e_this(), "__elephc_array", vec![]))),
                ]),
        )
        .method(
            method("__debugInfo")
                .returns(t_nullable(t_array()))
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_projection", vec![e_bool(true)])),
                ]),
        )
        .method(
            method("__elephc_debug_info")
                .returns(t_mixed())
                .body(vec![
                    s_try(vec![
                        s_return(e_method_call(e_this(), "__debugInfo", vec![])),
                    ], vec![
                        (vec!["Throwable"], Some("error"), vec![
                            s_expr(e_self_call("__elephc_debug_warning", vec![e_var("error"), e_call("get_class", vec![e_var("error")])])),
                            s_throw(e_var("error")),
                        ]),
                    ], None),
                ]),
        )
        .method(
            method("__elephc_debug_warning")
                .static_()
                .param("error", t_class("Throwable"))
                .param("class", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("message", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Warning: Uncaught "), BinOp::Concat, e_var("class")), BinOp::Concat, e_str(": ")), BinOp::Concat, e_method_call(e_var("error"), "getMessage", vec![])), BinOp::Concat, e_str(" in ")), BinOp::Concat, e_method_call(e_var("error"), "getFile", vec![])), BinOp::Concat, e_str(":")), BinOp::Concat, e_method_call(e_var("error"), "getLine", vec![])), BinOp::Concat, e_str("\nStack trace:\n")), BinOp::Concat, e_method_call(e_var("error"), "getTraceAsString", vec![])), BinOp::Concat, e_str("\n  thrown in ")), BinOp::Concat, e_method_call(e_var("error"), "getFile", vec![])), BinOp::Concat, e_str(" on line ")), BinOp::Concat, e_method_call(e_var("error"), "getLine", vec![])), BinOp::Concat, e_str("\n"))),
                    s_expr(e_call("set_error_handler", vec![e_null()])),
                    s_try(vec![
                        s_expr(e_call("__elephc_simplexml_warn", vec![e_var("message")])),
                    ], vec![
                    ], Some(vec![
                        s_expr(e_call("restore_error_handler", vec![])),
                    ])),
                ]),
        )
        .method(
            method("__elephc_debug_projection")
                .param("projection", t_mixed())
                .returns(TypeExpr::AssocArray { key: Box::new(TypeExpr::Str), value: Box::new(t_mixed()) })
                .body(vec![
                    s_return(e_self_call("__elephc_debug_project", vec![e_var("projection"), e_call("get_class", vec![e_this()])])),
                ]),
        )
        .method(
            method("__elephc_debug_project")
                .static_()
                .param("projection", t_mixed())
                .param("class", TypeExpr::Str)
                .returns(TypeExpr::AssocArray { key: Box::new(TypeExpr::Str), value: Box::new(t_mixed()) })
                .body(vec![
                    s_assign("result", e_array_assoc(vec![(e_str("__elephc_projection_seed"), e_null())])),
                    s_expr(e_call("unset", vec![e_index(e_var("result"), e_str("__elephc_projection_seed"))])),
                    s_if(
                        e_binop(e_var("projection"), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_str("Deprecated: Returning null from "), BinOp::Concat, e_var("class")), BinOp::Concat, e_str("::__debugInfo() is deprecated, return an empty array instead\n"))])),
                        ],
                        vec![],
                        Some(vec![
                        s_foreach(e_var("projection"), Some("key"), "value", vec![
                            s_array_assign("result", e_cast(CastType::String, e_var("key")), e_var("value")),
                        ]),
                    ]),
                    ),
                    s_return(e_var("result")),
                ]),
        )
        .method(
            method("__elephc_dom_text")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(3)])),
                ]),
        )
        .method(
            method("__elephc_dom_document_text")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(7)])),
                ]),
        )
        .method(
            method("__elephc_dom_xml")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_cast(CastType::String, e_call("hex2bin", vec![e_call("elephc_xml_sx_xml_hex", vec![e_var("handle"), e_int(0)])]))),
                ]),
        )
        .method(
            method("__elephc_dom_document_xml")
                .param_default("nodeArgument", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_cast(CastType::String, e_call("hex2bin", vec![e_call("elephc_xml_sx_xml_hex", vec![e_var("handle"), e_ternary(e_var("nodeArgument"), e_int(3), e_int(2))])]))),
                ]),
        )
        .method(
            method("__elephc_dom_set_text")
                .param("text", TypeExpr::Str)
                .param_default("entities", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_expr(e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_ternary(e_var("entities"), e_int(26), e_int(22)), e_str(""), e_var("text"), e_str(""), e_int(0), e_int(0)])),
                ]),
        )
        .method(
            method("__elephc_dom_attribute")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_sx_dom_attribute", vec![e_var("handle"), e_var("name")])),
                ]),
        )
        .method(
            method("__elephc_dom_local_name")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(1)])),
                ]),
        )
        .method(
            method("__elephc_dom_identity")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(12)])),
                ]),
        )
        .method(
            method("__elephc_dom_is_html")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(13)]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("__elephc_dom_relation")
                .param("document", t_mixed())
                .param("kind", TypeExpr::Int)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("relative", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_var("kind"), e_str(""), e_str(""), e_int(0), e_int(0)])),
                    s_if(
                        e_binop(e_call("elephc_xml_sx_int", vec![e_var("relative"), e_int(1)]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_call("elephc_xml_sx_free", vec![e_var("relative")])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("view", e_call("__elephc_simplexml_wrap", vec![e_var("relative"), e_str("SimpleXMLElement")])),
                    s_return(e_method_call(e_var("view"), "__elephc_dom_node", vec![e_var("document"), e_null()])),
                ]),
        )
        .method(
            method("__elephc_dom_attributes")
                .returns(t_array())
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("all", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(15), e_str(""), e_str(""), e_int(0), e_int(0)])),
                    s_assign("attributes", e_array(vec![])),
                    s_try(vec![
                        s_assign("length", e_call("elephc_xml_sx_int", vec![e_var("all"), e_int(2)])),
                        s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                            s_assign("attribute", e_call("elephc_xml_sx_item", vec![e_var("all"), e_var("i")])),
                            s_try(vec![
                                s_array_push("attributes", e_array(vec![e_call("elephc_xml_sx_string", vec![e_var("attribute"), e_int(6)]), e_call("elephc_xml_sx_string", vec![e_var("attribute"), e_int(3)])])),
                            ], vec![
                            ], Some(vec![
                                s_expr(e_call("elephc_xml_sx_free", vec![e_var("attribute")])),
                            ])),
                        ]),
                    ], vec![
                    ], Some(vec![
                        s_expr(e_call("elephc_xml_sx_free", vec![e_var("all")])),
                    ])),
                    s_return(e_var("attributes")),
                ]),
        )
        .method(
            method("__elephc_dom_set_attribute")
                .param("name", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_binop(e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(25), e_var("name"), e_var("value"), e_str(""), e_int(0), e_int(0)]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("__elephc_dom_append_child")
                .param("parent", t_mixed())
                .param("node", t_class("DOMNode"))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_if(
                        e_instance_of(e_var("node"), "DOMText"),
                        vec![
                            s_assign("appended", e_call("elephc_xml_sx_html_append", vec![e_var("handle"), e_int(3), e_str(""), e_cast(CastType::String, e_prop(e_var("node"), "nodeValue"))])),
                            s_if(
                                e_binop(e_var("appended"), BinOp::StrictNotEq, e_int(0)),
                                vec![
                                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("appended")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_var("node"), "parentNode", e_var("parent")),
                            s_prop_assign(e_var("node"), "ownerDocument", e_ternary(e_instance_of(e_var("parent"), "DOMDocument"), e_var("parent"), e_prop(e_var("parent"), "ownerDocument"))),
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("node"), "DOMElement"),
                        vec![
                            s_assign("child", e_method_call(e_this(), "addChild", vec![e_prop(e_var("node"), "nodeName")])),
                            s_if(
                                e_not(e_instance_of(e_var("child"), "SimpleXMLElement")),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("xmlns", e_prop(e_var("node"), "__elephc_dom_xmlns")),
                            s_if(
                                e_binop(e_var("xmlns"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_expr(e_method_call(e_var("child"), "addAttribute", vec![e_str("xmlns"), e_cast(CastType::String, e_var("xmlns"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("attributes", e_prop(e_var("node"), "__attrs")),
                            s_if(
                                e_call("is_array", vec![e_var("attributes")]),
                                vec![
                                    s_foreach(e_var("attributes"), None, "pair", vec![
                                        s_if(
                                            e_call("is_array", vec![e_var("pair")]),
                                            vec![
                                                s_expr(e_method_call(e_var("child"), "addAttribute", vec![e_cast(CastType::String, e_index(e_var("pair"), e_int(0))), e_cast(CastType::String, e_index(e_var("pair"), e_int(1)))])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ]),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("children", e_prop(e_var("node"), "childNodes")),
                            s_if(
                                e_instance_of(e_var("children"), "DOMNodeList"),
                                vec![
                                    s_foreach(e_var("children"), None, "grandchild", vec![
                                        s_if(
                                            e_instance_of(e_var("grandchild"), "DOMNode"),
                                            vec![
                                                s_expr(e_method_call(e_var("child"), "__elephc_dom_append_child", vec![e_var("child"), e_var("grandchild")])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ]),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_var("node"), "parentNode", e_var("parent")),
                            s_prop_assign(e_var("node"), "ownerDocument", e_ternary(e_instance_of(e_var("parent"), "DOMDocument"), e_var("parent"), e_prop(e_var("parent"), "ownerDocument"))),
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                ]),
        )
        .method(
            method("__elephc_dom_selections")
                .param_default("wholeDocument", TypeExpr::Bool, e_bool(false))
                .returns(t_array())
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("all", e_call("elephc_xml_sx_select", vec![e_var("handle"), e_ternary(e_var("wholeDocument"), e_int(11), e_int(6)), e_str(""), e_str(""), e_int(0), e_int(0)])),
                    s_assign("length", e_call("elephc_xml_sx_int", vec![e_var("all"), e_int(2)])),
                    s_assign("nodes", e_array(vec![])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("child", e_call("elephc_xml_sx_item", vec![e_var("all"), e_var("i")])),
                        s_array_push("nodes", e_call("__elephc_simplexml_wrap", vec![e_var("child"), e_str("SimpleXMLElement")])),
                    ]),
                    s_expr(e_call("elephc_xml_sx_free", vec![e_var("all")])),
                    s_return(e_var("nodes")),
                ]),
        )
        .method(
            method("__elephc_dom_children")
                .param("document", t_mixed())
                .param("parent", t_mixed())
                .param_default("wholeDocument", TypeExpr::Bool, e_bool(false))
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_assign("children", e_array(vec![])),
                    s_foreach(e_method_call(e_this(), "__elephc_dom_selections", vec![e_var("wholeDocument")]), None, "view", vec![
                        s_if(
                            e_instance_of(e_var("view"), "SimpleXMLElement"),
                            vec![
                                s_array_push("children", e_method_call(e_var("view"), "__elephc_dom_node", vec![e_var("document"), e_var("parent")])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_assign("length", e_call("count", vec![e_var("children")])),
                    s_for(Some(s_assign("i", e_int(1))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("left", e_index(e_var("children"), e_binop(e_var("i"), BinOp::Sub, e_int(1)))),
                        s_assign("right", e_index(e_var("children"), e_var("i"))),
                        s_if(
                            e_instance_of(e_var("left"), "DOMNode"),
                            vec![
                                s_if(
                                    e_instance_of(e_var("right"), "DOMNode"),
                                    vec![
                                        s_expr(e_call("__elephc_dom_link_siblings", vec![e_var("left"), e_var("right")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_new("DOMNodeList", vec![e_var("children")])),
                ]),
        )
        .method(
            method("__elephc_dom_node")
                .param("document", t_mixed())
                .param("parent", t_mixed())
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("kind", e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(4)])),
                    s_if(
                        e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(13))),
                        vec![
                            s_if(
                                e_instance_of(e_var("document"), "DOMDocument"),
                                vec![
                                    s_return(e_var("document")),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("cached", e_call("__elephc_dom_cached_node", vec![e_var("document"), e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(12)])])),
                    s_if(
                        e_instance_of(e_var("cached"), "DOMNode"),
                        vec![
                            s_return(e_var("cached")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_assign("node", e_new("DOMText", vec![])),
                            s_prop_assign(e_var("node"), "__elephc_raw_text", e_bool(true)),
                            s_return(e_method_call(e_this(), "__elephc_dom_bind_node", vec![e_var("node"), e_var("document"), e_var("parent"), e_str("#cdata-section")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_assign("node", e_new("DOMText", vec![])),
                            s_prop_assign(e_var("node"), "__elephc_raw_text", e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(14)]), BinOp::StrictNotEq, e_int(0))),
                            s_return(e_method_call(e_this(), "__elephc_dom_bind_node", vec![e_var("node"), e_var("document"), e_var("parent"), e_str("#text")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(8)),
                        vec![
                            s_return(e_method_call(e_this(), "__elephc_dom_bind_node", vec![e_new("DOMComment", vec![]), e_var("document"), e_var("parent"), e_str("#comment")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(7)),
                        vec![
                            s_return(e_method_call(e_this(), "__elephc_dom_bind_node", vec![e_new("DOMProcessingInstruction", vec![e_method_call(e_this(), "getName", vec![]), e_method_call(e_this(), "__elephc_dom_text", vec![])]), e_var("document"), e_var("parent"), e_method_call(e_this(), "getName", vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_return(e_method_call(e_this(), "__elephc_dom_bind_node", vec![e_new("DOMElement", vec![]), e_var("document"), e_var("parent"), e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(6)])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "__elephc_dom_bind_node", vec![e_new("DOMNode", vec![]), e_var("document"), e_var("parent"), e_method_call(e_this(), "getName", vec![])])),
                ]),
        )
        .method(
            method("__elephc_dom_bind_node")
                .private()
                .param("node", t_class("DOMNode"))
                .param("document", t_mixed())
                .param("parent", t_mixed())
                .param("name", TypeExpr::Str)
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_prop_assign(e_var("node"), "__elephc_simplexml", e_this()),
                    s_prop_assign(e_var("node"), "nodeName", e_var("name")),
                    s_prop_assign(e_var("node"), "ownerDocument", e_var("document")),
                    s_prop_assign(e_var("node"), "parentNode", e_var("parent")),
                    s_expr(e_call("__elephc_dom_remember_node", vec![e_var("document"), e_method_call(e_this(), "__elephc_dom_identity", vec![]), e_var("node")])),
                    s_return(e_var("node")),
                ]),
        )
        .method(
            method("rewind")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "__elephc_current", e_null()),
                    s_prop_assign(e_this(), "__elephc_position", e_int(0)),
                    s_assign("handle", e_method_call(e_this(), "__elephc_initialized_handle", vec![])),
                    s_expr(e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(0)])),
                    s_if(
                        e_binop(e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(2)]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_iterator_current", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("next")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "__elephc_current", e_null()),
                    s_prop_assign(e_this(), "__elephc_position", e_binop(e_this_prop("__elephc_position"), BinOp::Add, e_int(1))),
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_expr(e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(1)])),
                    s_if(
                        e_binop(e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(2)]), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_iterator_current", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("valid")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_return(e_binop(e_binop(e_this_prop("__elephc_position"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(2)]), BinOp::StrictNotEq, e_int(0)))),
                ]),
        )
        .method(
            method("__elephc_iterator_current")
                .private()
                .returns(t_class("SimpleXMLElement"))
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(2)]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Iterator not initialized or already consumed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("cached", e_this_prop("__elephc_current")),
                    s_if(
                        e_instance_of(e_var("cached"), "SimpleXMLElement"),
                        vec![
                            s_return(e_var("cached")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_assign("child", e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(3)])),
                    s_assign("current", e_call("__elephc_simplexml_wrap", vec![e_var("child"), e_call("get_class", vec![e_this()])])),
                    s_prop_assign(e_this(), "__elephc_current", e_var("current")),
                    s_return(e_var("current")),
                ]),
        )
        .method(
            method("current")
                .returns(t_class("SimpleXMLElement"))
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_iterator_current", vec![])),
                ]),
        )
        .method(
            method("key")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("current", e_method_call(e_this(), "__elephc_iterator_current", vec![])),
                    s_assign("handle", e_prop(e_var("current"), "__elephc_handle")),
                    s_return(e_call("elephc_xml_sx_string", vec![e_var("handle"), e_int(1)])),
                ]),
        )
        .method(
            method("hasChildren")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_binop(e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(2)]), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)]), BinOp::GtEq, e_int(3))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("current", e_method_call(e_this(), "__elephc_iterator_current", vec![])),
                    s_assign("child", e_prop(e_var("current"), "__elephc_handle")),
                    s_return(e_binop(e_call("elephc_xml_sx_int", vec![e_var("child"), e_int(8)]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("getChildren")
                .returns(t_nullable(t_class("SimpleXMLElement")))
                .body(vec![
                    s_assign("handle", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_binop(e_call("elephc_xml_sx_iterator", vec![e_var("handle"), e_int(2)]), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(5)]), BinOp::GtEq, e_int(3))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "__elephc_iterator_current", vec![])),
                ]),
        )
        .build()
}

/// `SimpleXMLIterator` — transcribed from the PHP form.
fn decl_class_simplexmliterator() -> Stmt {
    class("SimpleXMLIterator")
        .extends("SimpleXMLElement")
        .method(
            method("getChildren")
                .returns(t_nullable(t_class("SimpleXMLIterator")))
                .body(vec![
                    s_assign("value", e_parent_call("getChildren", vec![])),
                    s_return(e_ternary(e_instance_of(e_var("value"), "SimpleXMLIterator"), e_var("value"), e_null())),
                ]),
        )
        .build()
}

/// `simplexml_load_string` — transcribed from the PHP form.
fn decl_fn_simplexml_load_string() -> Stmt {
    function("simplexml_load_string")
        .param("data", TypeExpr::Str)
        .param_default("class_name", t_nullable(TypeExpr::Str), e_str("SimpleXMLElement"))
        .param_default("options", TypeExpr::Int, e_int(0))
        .param_default("namespace_or_prefix", TypeExpr::Str, e_str(""))
        .param_default("is_prefix", TypeExpr::Bool, e_bool(false))
        .returns(t_union(vec![t_class("SimpleXMLElement"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("options"), BinOp::Gt, e_int(2147483647)), BinOp::Or, e_binop(e_var("options"), BinOp::Lt, e_neg(e_int(2147483648)))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("simplexml_load_string(): Argument #3 ($options) is too large")])),
                ],
                vec![],
                None,
            ),
            s_assign("class", e_call("__elephc_simplexml_class", vec![e_var("class_name"), e_str("simplexml_load_string")])),
            s_if(
                e_binop(e_call("strlen", vec![e_var("data")]), BinOp::Gt, e_int(2147483647)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("simplexml_load_string(): Argument #1 ($data) is too long")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("strlen", vec![e_var("namespace_or_prefix")]), BinOp::Gt, e_int(2147483647)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("simplexml_load_string(): Argument #4 ($namespace_or_prefix) is too long")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("options"), BinOp::Lt, e_neg(e_int(2147483648))), BinOp::Or, e_binop(e_var("options"), BinOp::Gt, e_int(2147483647))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("simplexml_load_string(): Argument #3 ($options) is too large")])),
                ],
                vec![],
                None,
            ),
            s_assign("handle", e_call("elephc_xml_sx_load", vec![e_var("data"), e_call("strlen", vec![e_var("data")]), e_var("options"), e_var("namespace_or_prefix"), e_cast(CastType::Int, e_var("is_prefix")), e_str("")])),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_simplexml_report_diagnostics", vec![e_str("simplexml_load_string()")])),
                    s_if(
                        e_binop(e_binop(e_var("options"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_return(e_call("__elephc_simplexml_allocate", vec![e_var("class")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("object", e_call("__elephc_simplexml_wrap", vec![e_var("handle"), e_var("class")])),
            s_expr(e_call("__elephc_simplexml_report_diagnostics", vec![e_str("simplexml_load_string()")])),
            s_return(e_var("object")),
        ])
        .build()
}

/// `simplexml_load_file` — transcribed from the PHP form.
fn decl_fn_simplexml_load_file() -> Stmt {
    function("simplexml_load_file")
        .param("filename", TypeExpr::Str)
        .param_default("class_name", t_nullable(TypeExpr::Str), e_str("SimpleXMLElement"))
        .param_default("options", TypeExpr::Int, e_int(0))
        .param_default("namespace_or_prefix", TypeExpr::Str, e_str(""))
        .param_default("is_prefix", TypeExpr::Bool, e_bool(false))
        .returns(t_union(vec![t_class("SimpleXMLElement"), TypeExpr::False]))
        .body(vec![
            s_if(
                e_binop(e_call("strpos", vec![e_var("filename"), e_str("\0")]), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("simplexml_load_file(): Argument #1 ($filename) must not contain any null bytes")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("options"), BinOp::Gt, e_int(2147483647)), BinOp::Or, e_binop(e_var("options"), BinOp::Lt, e_neg(e_int(2147483648)))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("simplexml_load_file(): Argument #3 ($options) is too large")])),
                ],
                vec![],
                None,
            ),
            s_assign("class", e_call("__elephc_simplexml_class", vec![e_var("class_name"), e_str("simplexml_load_file")])),
            s_if(
                e_binop(e_binop(e_var("options"), BinOp::Lt, e_neg(e_int(2147483648))), BinOp::Or, e_binop(e_var("options"), BinOp::Gt, e_int(2147483647))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("simplexml_load_file(): Argument #3 ($options) is too large")])),
                ],
                vec![],
                None,
            ),
            s_assign("handle", e_call("__elephc_simplexml_file_handle", vec![e_var("filename"), e_var("options"), e_var("namespace_or_prefix"), e_var("is_prefix"), e_str("simplexml_load_file()")])),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_simplexml_report_diagnostics", vec![e_str("simplexml_load_file()")])),
                    s_if(
                        e_binop(e_binop(e_var("options"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_return(e_call("__elephc_simplexml_allocate", vec![e_var("class")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("object", e_call("__elephc_simplexml_wrap", vec![e_var("handle"), e_var("class")])),
            s_expr(e_call("__elephc_simplexml_report_diagnostics", vec![e_str("simplexml_load_file()")])),
            s_return(e_var("object")),
        ])
        .build()
}

/// `__elephc_simplexml_load_dom` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_load_dom() -> Stmt {
    function("__elephc_simplexml_load_dom")
        .param("document", t_class("DOMDocument"))
        .param("data", TypeExpr::Str)
        .param("options", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("handle", e_call("elephc_xml_sx_load_dom", vec![e_var("data"), e_call("strlen", vec![e_var("data")]), e_var("options")])),
            s_assign("diagnostics", e_call("elephc_xml_sx_diagnostics", vec![])),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_assign("object", e_call("__elephc_simplexml_wrap", vec![e_var("handle"), e_str("SimpleXMLElement")])),
                    s_prop_assign(e_var("document"), "__elephc_simplexml", e_var("object")),
                    s_prop_assign(e_var("document"), "childNodes", e_null()),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_simplexml_report_text", vec![e_var("diagnostics"), e_str("DOMDocument::loadXML()"), e_bool(true)])),
            s_return(e_binop(e_var("handle"), BinOp::StrictNotEq, e_int(0))),
        ])
        .build()
}

/// `__elephc_simplexml_bind_dom` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_bind_dom() -> Stmt {
    function("__elephc_simplexml_bind_dom")
        .param("node", t_class("DOMNode"))
        .param("view", t_class("SimpleXMLElement"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("children", e_prop(e_var("node"), "childNodes")),
            s_if(
                e_instance_of(e_var("node"), "DOMDocument"),
                vec![
                    s_prop_assign(e_var("node"), "__elephc_simplexml", e_var("view")),
                    s_assign("selections", e_method_call(e_var("view"), "__elephc_dom_selections", vec![e_bool(true)])),
                    s_assign("i", e_int(0)),
                    s_if(
                        e_instance_of(e_var("children"), "DOMNodeList"),
                        vec![
                            s_foreach(e_var("children"), None, "child", vec![
                                s_if(
                                    e_instance_of(e_var("child"), "DOMNode"),
                                    vec![
                                        s_if(
                                            e_call("isset", vec![e_index(e_var("selections"), e_var("i"))]),
                                            vec![
                                                s_assign("selected", e_index(e_var("selections"), e_var("i"))),
                                                s_if(
                                                    e_instance_of(e_var("selected"), "SimpleXMLElement"),
                                                    vec![
                                                        s_expr(e_call("__elephc_simplexml_bind_dom", vec![e_var("child"), e_var("selected")])),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_post_inc("i")),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_dom_drop_snapshot", vec![e_var("node")])),
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("selections", e_method_call(e_var("view"), "__elephc_dom_selections", vec![])),
            s_prop_assign(e_var("node"), "__elephc_simplexml", e_var("view")),
            s_expr(e_call("__elephc_dom_remember_node", vec![e_prop(e_var("node"), "ownerDocument"), e_method_call(e_var("view"), "__elephc_dom_identity", vec![]), e_var("node")])),
            s_assign("i", e_int(0)),
            s_if(
                e_instance_of(e_var("children"), "DOMNodeList"),
                vec![
                    s_foreach(e_var("children"), None, "child", vec![
                        s_if(
                            e_instance_of(e_var("child"), "DOMNode"),
                            vec![
                                s_if(
                                    e_call("isset", vec![e_index(e_var("selections"), e_var("i"))]),
                                    vec![
                                        s_assign("selected", e_index(e_var("selections"), e_var("i"))),
                                        s_if(
                                            e_instance_of(e_var("selected"), "SimpleXMLElement"),
                                            vec![
                                                s_expr(e_call("__elephc_simplexml_bind_dom", vec![e_var("child"), e_var("selected")])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_post_inc("i")),
                    ]),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_dom_drop_snapshot", vec![e_var("node")])),
        ])
        .build()
}

/// `simplexml_import_dom` — transcribed from the PHP form.
fn decl_fn_simplexml_import_dom() -> Stmt {
    function("simplexml_import_dom")
        .param("node", t_class("object"))
        .param_default("class_name", t_nullable(TypeExpr::Str), e_str("SimpleXMLElement"))
        .returns(t_nullable(t_class("SimpleXMLElement")))
        .body(vec![
            s_assign("class", e_call("__elephc_simplexml_class", vec![e_var("class_name"), e_str("simplexml_import_dom")])),
            s_if(
                e_instance_of(e_var("node"), "SimpleXMLElement"),
                vec![
                    s_assign("handle", e_method_call(e_var("node"), "__elephc_handle_value", vec![])),
                    s_if(
                        e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(1)]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("TypeError", vec![e_str("simplexml_import_dom(): Argument #1 ($node) must be a valid XML node")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("elephc_xml_sx_int", vec![e_var("handle"), e_int(4)]), BinOp::StrictNotEq, e_int(1)),
                        vec![
                            s_expr(e_call("__elephc_simplexml_warn", vec![e_str("Warning: simplexml_import_dom(): Invalid Nodetype to import\n")])),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_call("elephc_xml_sx_select", vec![e_var("handle"), e_int(16), e_str(""), e_str(""), e_int(0), e_int(0)]), e_var("class")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("node"), "DOMNode"),
                vec![
                    s_return(e_call("__elephc_simplexml_import_node", vec![e_var("node"), e_var("class")])),
                ],
                vec![],
                None,
            ),
            s_throw(e_new("TypeError", vec![e_str("simplexml_import_dom(): Argument #1 ($node) must be a valid XML node")])),
        ])
        .build()
}

/// `__elephc_simplexml_import_node` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_import_node() -> Stmt {
    function("__elephc_simplexml_import_node")
        .param("node", t_class("DOMNode"))
        .param("class", TypeExpr::Str)
        .returns(t_nullable(t_class("SimpleXMLElement")))
        .body(vec![
            s_if(
                e_binop(e_prop(e_var("node"), "nodeName"), BinOp::StrictEq, e_str("")),
                vec![
                    s_throw(e_new("TypeError", vec![e_str("simplexml_import_dom(): Argument #1 ($node) must be a valid XML node")])),
                ],
                vec![],
                None,
            ),
            s_assign("bound", e_prop(e_var("node"), "__elephc_simplexml")),
            s_if(
                e_binop(e_instance_of(e_var("node"), "DOMElement"), BinOp::And, e_instance_of(e_var("bound"), "SimpleXMLElement")),
                vec![
                    s_assign("handle", e_method_call(e_var("bound"), "__elephc_handle_value", vec![])),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_call("elephc_xml_sx_alias", vec![e_var("handle")]), e_var("class")])),
                ],
                vec![],
                None,
            ),
            s_assign("document", e_prop(e_var("node"), "ownerDocument")),
            s_if(
                e_instance_of(e_var("node"), "DOMDocument"),
                vec![
                    s_assign("document", e_var("node")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_instance_of(e_var("document"), "DOMDocument")),
                vec![
                    s_expr(e_call("__elephc_simplexml_warn", vec![e_str("Warning: simplexml_import_dom(): Imported Node must have associated Document\n")])),
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_not(e_instance_of(e_var("node"), "DOMDocument")), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMElement"))), BinOp::Or, e_binop(e_instance_of(e_var("node"), "DOMDocument"), BinOp::And, e_not(e_instance_of(e_prop(e_var("document"), "documentElement"), "DOMElement")))),
                vec![
                    s_expr(e_call("__elephc_simplexml_warn", vec![e_str("Warning: simplexml_import_dom(): Invalid Nodetype to import\n")])),
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("bound"), "SimpleXMLElement"),
                vec![
                    s_assign("handle", e_method_call(e_var("bound"), "__elephc_handle_value", vec![])),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_call("elephc_xml_sx_alias", vec![e_var("handle")]), e_var("class")])),
                ],
                vec![],
                None,
            ),
            s_assign("result", e_call("__elephc_simplexml_import_document", vec![e_var("document"), e_str("SimpleXMLElement")])),
            s_if(
                e_not(e_instance_of(e_var("result"), "SimpleXMLElement")),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_simplexml_bind_dom", vec![e_var("document"), e_var("result")])),
            s_assign("bound", e_prop(e_var("node"), "__elephc_simplexml")),
            s_if(
                e_instance_of(e_var("bound"), "SimpleXMLElement"),
                vec![
                    s_assign("handle", e_method_call(e_var("bound"), "__elephc_handle_value", vec![])),
                    s_return(e_call("__elephc_simplexml_wrap", vec![e_call("elephc_xml_sx_alias", vec![e_var("handle")]), e_var("class")])),
                ],
                vec![],
                None,
            ),
            s_return(e_null()),
        ])
        .build()
}

/// `__elephc_simplexml_import_document` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_import_document() -> Stmt {
    function("__elephc_simplexml_import_document")
        .param("document", t_class("DOMDocument"))
        .param("class", TypeExpr::Str)
        .returns(t_union(vec![t_class("SimpleXMLElement"), TypeExpr::False]))
        .body(vec![
            s_assign("builder", e_call("elephc_xml_sx_html_document", vec![])),
            s_if(
                e_binop(e_var("builder"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_try(vec![
                s_foreach(e_prop(e_var("document"), "childNodes"), None, "child", vec![
                    s_if(
                        e_instance_of(e_var("child"), "DOMNode"),
                        vec![
                            s_expr(e_call("__elephc_simplexml_import_child", vec![e_var("builder"), e_var("child")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_assign("root", e_call("elephc_xml_sx_html_root", vec![e_var("builder")])),
                s_if(
                    e_binop(e_var("root"), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_call("__elephc_simplexml_wrap", vec![e_var("root"), e_var("class")])),
            ], vec![
            ], Some(vec![
                s_expr(e_call("elephc_xml_sx_free", vec![e_var("builder")])),
            ])),
        ])
        .build()
}

/// `__elephc_simplexml_import_text_kind` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_import_text_kind() -> Stmt {
    function("__elephc_simplexml_import_text_kind")
        .param("node", t_class("DOMText"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_ternary(e_prop(e_var("node"), "__elephc_raw_text"), e_int(4), e_int(3))),
        ])
        .build()
}

/// `__elephc_simplexml_import_child` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_import_child() -> Stmt {
    function("__elephc_simplexml_import_child")
        .param("parent", TypeExpr::Int)
        .param("node", t_class("DOMNode"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("kind", e_int(0)),
            s_if(
                e_instance_of(e_var("node"), "DOMElement"),
                vec![
                    s_assign("kind", e_int(1)),
                ],
                vec![
                (e_instance_of(e_var("node"), "DOMText"), vec![
                    s_assign("kind", e_call("__elephc_simplexml_import_text_kind", vec![e_var("node")])),
                ]),
                (e_instance_of(e_var("node"), "DOMComment"), vec![
                    s_assign("kind", e_int(5)),
                ]),
                (e_instance_of(e_var("node"), "DOMProcessingInstruction"), vec![
                    s_assign("kind", e_int(6)),
                ]),
            ],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("value", e_ternary(e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)), e_str(""), e_cast(CastType::String, e_prop(e_var("node"), "nodeValue")))),
            s_assign("handle", e_call("elephc_xml_sx_html_append", vec![e_var("parent"), e_var("kind"), e_prop(e_var("node"), "nodeName"), e_var("value")])),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_throw(e_new("Error", vec![e_str("Could not import DOM node")])),
                ],
                vec![],
                None,
            ),
            s_try(vec![
                s_if(
                    e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                    vec![
                        s_assign("xmlns", e_prop(e_var("node"), "__elephc_dom_xmlns")),
                        s_if(
                            e_binop(e_binop(e_var("xmlns"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("elephc_xml_sx_mutate", vec![e_var("handle"), e_int(25), e_str("xmlns"), e_cast(CastType::String, e_var("xmlns")), e_str(""), e_int(0), e_int(0)]), BinOp::StrictEq, e_int(0))),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Could not import DOM namespace declaration")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_foreach(e_prop(e_var("node"), "__attrs"), None, "pair", vec![
                            s_assign("attribute", e_call("elephc_xml_sx_html_append", vec![e_var("handle"), e_int(2), e_cast(CastType::String, e_index(e_var("pair"), e_int(0))), e_cast(CastType::String, e_index(e_var("pair"), e_int(1)))])),
                            s_if(
                                e_binop(e_var("attribute"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_throw(e_new("Error", vec![e_str("Could not import DOM attribute")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_call("elephc_xml_sx_free", vec![e_var("attribute")])),
                        ]),
                        s_foreach(e_prop(e_var("node"), "childNodes"), None, "child", vec![
                            s_if(
                                e_instance_of(e_var("child"), "DOMNode"),
                                vec![
                                    s_expr(e_call("__elephc_simplexml_import_child", vec![e_var("handle"), e_var("child")])),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                    ],
                    vec![],
                    None,
                ),
            ], vec![
            ], Some(vec![
                s_expr(e_call("elephc_xml_sx_free", vec![e_var("handle")])),
            ])),
        ])
        .build()
}

/// `elephc_runtime_simplexml_stream_close_mode` — transcribed from the PHP form.
fn decl_extern_elephc_runtime_simplexml_stream_close_mode() -> Stmt {
    extern_fn("elephc_runtime_simplexml_stream_close_mode", "System")
        .param("mode", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `__elephc_simplexml_file_handle` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_file_handle() -> Stmt {
    function("__elephc_simplexml_file_handle")
        .param("filename", TypeExpr::Str)
        .param("options", TypeExpr::Int)
        .param("ns", TypeExpr::Str)
        .param("prefix", TypeExpr::Bool)
        .param("caller", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("nul", e_call("strpos", vec![e_var("filename"), e_str("\0")])),
            s_if(
                e_binop(e_var("nul"), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_assign("filename", e_call("substr", vec![e_var("filename"), e_int(0), e_call("intval", vec![e_var("nul")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("str_contains", vec![e_var("filename"), e_str("%00")]),
                vec![
                    s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_str("Warning: "), BinOp::Concat, e_var("caller")), BinOp::Concat, e_str(": URI must not contain percent-encoded NUL bytes")), BinOp::Concat, e_str("\n"))])),
                    s_return(e_call("elephc_xml_sx_load_stream", vec![e_var("filename"), e_str(""), e_neg(e_int(1)), e_var("options"), e_var("ns"), e_cast(CastType::Int, e_var("prefix"))])),
                ],
                vec![],
                None,
            ),
            s_assign("separator", e_call("strpos", vec![e_var("filename"), e_str("://")])),
            s_if(
                e_binop(e_binop(e_var("separator"), BinOp::StrictEq, e_bool(false)), BinOp::Or, e_call("str_starts_with", vec![e_call("strtolower", vec![e_var("filename")]), e_str("file://")])),
                vec![
                    s_return(e_call("elephc_xml_sx_load_file", vec![e_var("filename"), e_var("options"), e_var("ns"), e_cast(CastType::Int, e_var("prefix"))])),
                ],
                vec![],
                None,
            ),
            s_assign("data", e_call("__elephc_simplexml_stream_bytes", vec![e_var("filename"), e_var("caller")])),
            s_if(
                e_binop(e_var("data"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_call("elephc_xml_sx_load_stream", vec![e_var("filename"), e_str(""), e_neg(e_int(1)), e_var("options"), e_var("ns"), e_cast(CastType::Int, e_var("prefix"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("elephc_xml_sx_load_stream", vec![e_var("filename"), e_var("data"), e_call("strlen", vec![e_var("data")]), e_var("options"), e_var("ns"), e_cast(CastType::Int, e_var("prefix"))])),
        ])
        .build()
}

/// `__elephc_simplexml_stream_bytes` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_stream_bytes() -> Stmt {
    function("__elephc_simplexml_stream_bytes")
        .param("uri", TypeExpr::Str)
        .param("caller", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("separator", e_call("strpos", vec![e_var("uri"), e_str("://")])),
            s_if(
                e_binop(e_var("separator"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_call("file_get_contents", vec![e_var("uri")])),
                ],
                vec![],
                None,
            ),
            s_assign("scheme", e_call("strtolower", vec![e_call("substr", vec![e_var("uri"), e_int(0), e_call("intval", vec![e_var("separator")])])])),
            s_if(
                e_binop(e_var("scheme"), BinOp::StrictEq, e_str("file")),
                vec![
                    s_assign("path", e_call("substr", vec![e_var("uri"), e_binop(e_call("intval", vec![e_var("separator")]), BinOp::Add, e_int(3))])),
                    s_if(
                        e_call("str_starts_with", vec![e_var("path"), e_str("localhost/")]),
                        vec![
                            s_assign("path", e_call("substr", vec![e_var("path"), e_int(9)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("str_starts_with", vec![e_var("path"), e_str("/")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("file_get_contents", vec![e_var("path")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("scheme"), BinOp::StrictEq, e_str("data")),
                vec![
                    s_return(e_call("__elephc_simplexml_data_bytes", vec![e_var("uri"), e_var("caller")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("scheme"), BinOp::StrictEq, e_str("php")),
                vec![
                    s_assign("uri", e_binop(e_str("php"), BinOp::Concat, e_call("substr", vec![e_var("uri"), e_call("intval", vec![e_var("separator")])]))),
                    s_if(
                        e_binop(e_var("uri"), BinOp::StrictEq, e_str("php://input")),
                        vec![
                            s_assign("input", e_call("file_get_contents", vec![e_str("php://input")])),
                            s_return(e_ternary(e_binop(e_var("input"), BinOp::StrictEq, e_bool(false)), e_str(""), e_var("input"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("uri"), BinOp::StrictEq, e_str("php://stdin")),
                        vec![
                            s_assign("stream", e_call("fopen", vec![e_str("php://stdin"), e_str("rb")])),
                            s_if(
                                e_binop(e_var("stream"), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("__elephc_simplexml_read_stream", vec![e_var("stream")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("uri"), BinOp::StrictEq, e_str("php://memory")), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_str("php://temp"))), BinOp::Or, e_call("str_starts_with", vec![e_var("uri"), e_str("php://temp/")])),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_starts_with", vec![e_var("uri"), e_str("php://filter/")]),
                        vec![
                            s_return(e_call("__elephc_simplexml_filter_bytes", vec![e_var("uri"), e_var("caller")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_var("scheme"), BinOp::StrictEq, e_str("http")), BinOp::Or, e_binop(e_var("scheme"), BinOp::StrictEq, e_str("https"))), BinOp::Or, e_binop(e_var("scheme"), BinOp::StrictEq, e_str("ftp"))), BinOp::Or, e_binop(e_var("scheme"), BinOp::StrictEq, e_str("ftps"))), BinOp::Or, e_binop(e_var("scheme"), BinOp::StrictEq, e_str("phar"))),
                vec![
                    s_assign("normalized", e_binop(e_var("scheme"), BinOp::Concat, e_call("substr", vec![e_var("uri"), e_call("intval", vec![e_var("separator")])]))),
                    s_return(e_call("file_get_contents", vec![e_var("normalized")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("scheme"), BinOp::StrictEq, e_str("compress.zlib")), BinOp::Or, e_binop(e_var("scheme"), BinOp::StrictEq, e_str("compress.bzip2"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("in_array", vec![e_var("scheme"), e_call("stream_get_wrappers", vec![]), e_bool(true)])),
                vec![
                    s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Warning: "), BinOp::Concat, e_var("caller")), BinOp::Concat, e_str(": Unable to find the wrapper \"")), BinOp::Concat, e_var("scheme")), BinOp::Concat, e_str("\" - did you forget to enable it when you configured PHP?")), BinOp::Concat, e_str("\n"))])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("file_exists", vec![e_var("uri")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("stream", e_call("fopen", vec![e_var("uri"), e_str("rb")])),
            s_if(
                e_binop(e_var("stream"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_simplexml_read_stream", vec![e_var("stream")])),
        ])
        .build()
}

/// `__elephc_simplexml_data_failure` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_data_failure() -> Stmt {
    function("__elephc_simplexml_data_failure")
        .param("uri", TypeExpr::Str)
        .param("caller", TypeExpr::Str)
        .param("reason", TypeExpr::Str)
        .returns(TypeExpr::False)
        .body(vec![
            s_assign("function", e_call("substr", vec![e_var("caller"), e_int(0), e_binop(e_call("strlen", vec![e_var("caller")]), BinOp::Sub, e_int(2))])),
            s_assign("prefix", e_ternary(e_binop(e_var("reason"), BinOp::StrictEq, e_str("operation failed")), e_str(""), e_str("rfc2397: "))),
            s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Warning: "), BinOp::Concat, e_var("function")), BinOp::Concat, e_str("(")), BinOp::Concat, e_var("uri")), BinOp::Concat, e_str("): Failed to open stream: ")), BinOp::Concat, e_var("prefix")), BinOp::Concat, e_var("reason")), BinOp::Concat, e_str("\n"))])),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `__elephc_simplexml_data_bytes` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_data_bytes() -> Stmt {
    function("__elephc_simplexml_data_bytes")
        .param("uri", TypeExpr::Str)
        .param("caller", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_if(
                e_not(e_call("str_starts_with", vec![e_var("uri"), e_str("data://")])),
                vec![
                    s_return(e_call("__elephc_simplexml_data_failure", vec![e_var("uri"), e_var("caller"), e_str("operation failed")])),
                ],
                vec![],
                None,
            ),
            s_assign("rest", e_call("substr", vec![e_var("uri"), e_int(7)])),
            s_assign("comma", e_call("strpos", vec![e_var("rest"), e_str(",")])),
            s_if(
                e_binop(e_var("comma"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_call("__elephc_simplexml_data_failure", vec![e_var("uri"), e_var("caller"), e_str("no comma in URL")])),
                ],
                vec![],
                None,
            ),
            s_assign("metadata", e_call("substr", vec![e_var("rest"), e_int(0), e_call("intval", vec![e_var("comma")])])),
            s_assign("payload", e_call("substr", vec![e_var("rest"), e_binop(e_call("intval", vec![e_var("comma")]), BinOp::Add, e_int(1))])),
            s_assign("base64", e_bool(false)),
            s_if(
                e_binop(e_var("metadata"), BinOp::StrictNotEq, e_str("")),
                vec![
                    s_assign("semi", e_call("strpos", vec![e_var("metadata"), e_str(";")])),
                    s_assign("slash", e_call("strpos", vec![e_var("metadata"), e_str("/")])),
                    s_if(
                        e_binop(e_var("semi"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_if(
                                e_binop(e_var("slash"), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_return(e_call("__elephc_simplexml_data_failure", vec![e_var("uri"), e_var("caller"), e_str("illegal media type")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_binop(e_binop(e_var("slash"), BinOp::StrictEq, e_bool(false)), BinOp::Or, e_binop(e_var("slash"), BinOp::Gt, e_var("semi"))), BinOp::And, e_binop(e_var("metadata"), BinOp::StrictNotEq, e_str(";base64"))),
                            vec![
                                s_return(e_call("__elephc_simplexml_data_failure", vec![e_var("uri"), e_var("caller"), e_str("illegal media type")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_foreach(e_call("explode", vec![e_str(";"), e_call("substr", vec![e_var("metadata"), e_binop(e_call("intval", vec![e_var("semi")]), BinOp::Add, e_int(1))])]), None, "parameter", vec![
                            s_if(
                                e_var("base64"),
                                vec![
                                    s_return(e_call("__elephc_simplexml_data_failure", vec![e_var("uri"), e_var("caller"), e_str("illegal parameter")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_call("strpos", vec![e_var("parameter"), e_str("=")]), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_if(
                                        e_binop(e_var("parameter"), BinOp::StrictNotEq, e_str("base64")),
                                        vec![
                                            s_return(e_call("__elephc_simplexml_data_failure", vec![e_var("uri"), e_var("caller"), e_str("illegal parameter")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("base64", e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                    ]),
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("base64"),
                vec![
                    s_assign("decoded", e_call("base64_decode", vec![e_var("payload"), e_bool(true)])),
                    s_if(
                        e_binop(e_var("decoded"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_call("__elephc_simplexml_data_failure", vec![e_var("uri"), e_var("caller"), e_str("unable to decode")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("decoded")),
                ],
                vec![],
                None,
            ),
            s_return(e_call("urldecode", vec![e_var("payload")])),
        ])
        .build()
}

/// `__elephc_simplexml_filter_bytes` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_filter_bytes() -> Stmt {
    function("__elephc_simplexml_filter_bytes")
        .param("uri", TypeExpr::Str)
        .param("caller", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("spec", e_call("substr", vec![e_var("uri"), e_int(13)])),
            s_if(
                e_call("str_starts_with", vec![e_var("spec"), e_str("resource=")]),
                vec![
                    s_return(e_call("__elephc_simplexml_stream_bytes", vec![e_call("substr", vec![e_var("spec"), e_int(9)]), e_var("caller")])),
                ],
                vec![],
                None,
            ),
            s_assign("resource", e_call("strpos", vec![e_var("spec"), e_str("/resource=")])),
            s_if(
                e_binop(e_var("resource"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("filters", e_call("substr", vec![e_var("spec"), e_int(0), e_call("intval", vec![e_var("resource")])])),
            s_assign("source", e_call("substr", vec![e_var("spec"), e_binop(e_call("intval", vec![e_var("resource")]), BinOp::Add, e_int(10))])),
            s_assign("data", e_call("__elephc_simplexml_stream_bytes", vec![e_var("source"), e_var("caller")])),
            s_if(
                e_binop(e_var("data"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("stream", e_call("fopen", vec![e_str("php://temp"), e_str("w+")])),
            s_if(
                e_binop(e_var("stream"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_expr(e_call("fwrite", vec![e_var("stream"), e_var("data")])),
            s_expr(e_call("rewind", vec![e_var("stream")])),
            s_assign("failed", e_bool(false)),
            s_try(vec![
                s_foreach(e_call("explode", vec![e_str("/"), e_var("filters")]), None, "filter_list", vec![
                    s_if(
                        e_call("str_starts_with", vec![e_var("filter_list"), e_str("write=")]),
                        vec![
                            s_continue(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_starts_with", vec![e_var("filter_list"), e_str("read=")]),
                        vec![
                            s_assign("filter_list", e_call("substr", vec![e_var("filter_list"), e_int(5)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_foreach(e_call("explode", vec![e_str("|"), e_var("filter_list")]), None, "filter", vec![
                        s_if(
                            e_binop(e_call("stream_filter_append", vec![e_var("stream"), e_var("filter"), e_const("STREAM_FILTER_READ")]), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_assign("known", e_bool(false)),
                                s_foreach(e_call("stream_get_filters", vec![]), None, "pattern", vec![
                                    s_if(
                                        e_call("fnmatch", vec![e_var("pattern"), e_var("filter")]),
                                        vec![
                                            s_assign("known", e_bool(true)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                                s_assign("reason", e_ternary(e_var("known"), e_str("Unable to create or locate filter \""), e_str("Unable to locate filter \""))),
                                s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Warning: "), BinOp::Concat, e_var("caller")), BinOp::Concat, e_str(": ")), BinOp::Concat, e_var("reason")), BinOp::Concat, e_var("filter")), BinOp::Concat, e_str("\"")), BinOp::Concat, e_str("\n"))])),
                                s_expr(e_call("__elephc_simplexml_warn", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Warning: "), BinOp::Concat, e_var("caller")), BinOp::Concat, e_str(": Unable to create filter (")), BinOp::Concat, e_var("filter")), BinOp::Concat, e_str(")")), BinOp::Concat, e_str("\n"))])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                ]),
                s_return(e_call("stream_get_contents", vec![e_var("stream")])),
            ], vec![
                (vec!["Throwable"], Some("error"), vec![
                    s_assign("failed", e_bool(true)),
                    s_throw(e_var("error")),
                ]),
            ], Some(vec![
                s_expr(e_call("__elephc_simplexml_close_stream", vec![e_var("stream"), e_var("failed")])),
            ])),
        ])
        .build()
}

/// `__elephc_simplexml_read_stream` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_read_stream() -> Stmt {
    function("__elephc_simplexml_read_stream")
        .param("stream", t_mixed())
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("failed", e_bool(false)),
            s_try(vec![
                s_return(e_call("stream_get_contents", vec![e_var("stream")])),
            ], vec![
                (vec!["Throwable"], Some("error"), vec![
                    s_assign("failed", e_bool(true)),
                    s_throw(e_var("error")),
                ]),
            ], Some(vec![
                s_expr(e_call("__elephc_simplexml_close_stream", vec![e_var("stream"), e_var("failed")])),
            ])),
        ])
        .build()
}

/// `__elephc_simplexml_close_stream` — transcribed from the PHP form.
fn decl_fn_elephc_simplexml_close_stream() -> Stmt {
    function("__elephc_simplexml_close_stream")
        .param("stream", t_mixed())
        .param("failed", TypeExpr::Bool)
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("previous", e_call("elephc_runtime_simplexml_stream_close_mode", vec![e_ternary(e_var("failed"), e_call("get_resource_id", vec![e_var("stream")]), e_int(0))])),
            s_try(vec![
                s_expr(e_call("fclose", vec![e_var("stream")])),
            ], vec![
            ], Some(vec![
                s_expr(e_call("elephc_runtime_simplexml_stream_close_mode", vec![e_var("previous")])),
            ])),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn declarations() -> Program {
    let mut program = internal_declarations(|| {
        vec![
            decl_extern_elephc_xml_sx_load(),
            decl_extern_elephc_xml_sx_load_dom(),
            decl_extern_elephc_xml_sx_html_document(),
            decl_extern_elephc_xml_sx_html_append(),
            decl_extern_elephc_xml_sx_html_root(),
            decl_extern_elephc_xml_sx_load_file(),
            decl_extern_elephc_xml_sx_load_stream(),
            decl_extern_elephc_xml_sx_diagnostics(),
            decl_extern_elephc_xml_sx_file_error(),
            decl_extern_elephc_xml_sx_free(),
            decl_extern_elephc_xml_sx_clone(),
            decl_extern_elephc_xml_sx_alias(),
            decl_extern_elephc_xml_sx_compare(),
            decl_extern_elephc_xml_sx_select(),
            decl_extern_elephc_xml_sx_int(),
            decl_extern_elephc_xml_sx_projection_base(),
            decl_extern_elephc_xml_sx_string(),
            decl_extern_elephc_xml_sx_dom_attribute(),
            decl_extern_elephc_xml_sx_xml_hex(),
            decl_extern_elephc_xml_sx_item(),
            decl_extern_elephc_xml_sx_iterator(),
            decl_extern_elephc_xml_sx_mutate(),
            decl_extern_elephc_xml_sx_register(),
            decl_extern_elephc_xml_sx_xpath(),
            decl_extern_elephc_xml_sx_namespace(),
            decl_extern_elephc_xml_sx_set_encoding(),
            decl_fn_elephc_simplexml_warn(),
            decl_fn_elephc_simplexml_wrap(),
            decl_fn_elephc_simplexml_class(),
            decl_fn_elephc_simplexml_assigned_value(),
            decl_fn_elephc_simplexml_numeric_value(),
            decl_fn_elephc_simplexml_parse_number(),
            decl_fn_elephc_simplexml_write_warning(),
            decl_fn_elephc_simplexml_report_diagnostics(),
            decl_fn_elephc_simplexml_report_text(),
            decl_class_elephcsimplexmllease(),
            decl_class_simplexmlelement(),
            decl_class_simplexmliterator(),
            decl_fn_simplexml_load_string(),
            decl_fn_simplexml_load_file(),
            decl_fn_elephc_simplexml_load_dom(),
            decl_fn_elephc_simplexml_bind_dom(),
            decl_fn_simplexml_import_dom(),
            decl_fn_elephc_simplexml_import_node(),
            decl_fn_elephc_simplexml_import_document(),
            decl_fn_elephc_simplexml_import_text_kind(),
            decl_fn_elephc_simplexml_import_child(),
            decl_extern_elephc_runtime_simplexml_stream_close_mode(),
            decl_fn_elephc_simplexml_file_handle(),
            decl_fn_elephc_simplexml_stream_bytes(),
            decl_fn_elephc_simplexml_data_failure(),
            decl_fn_elephc_simplexml_data_bytes(),
            decl_fn_elephc_simplexml_filter_bytes(),
            decl_fn_elephc_simplexml_read_stream(),
            decl_fn_elephc_simplexml_close_stream(),
        ]
    });
    // A non-dummy declaration span marks an elephc prelude class as user-declared for PHP's
    // coercive parameter binding; `Span::dummy()` would make `new SimpleXMLElement(0, ...)`
    // reject the integer instead of coercing it to "0".
    for declaration in &mut program {
        if !declaration.span.identifies_a_node() {
            declaration.span = crate::span::Span::synthetic();
        }
    }
    program
}
