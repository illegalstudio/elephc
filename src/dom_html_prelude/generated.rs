//! Purpose:
//! Rust AST builders for the Termwind-facing DOM HTML subset: the HTML tag scanner and tree
//! builder plus `DOMDocument` / `DOMNode` / `DOMElement` / `DOMText` / `DOMComment` /
//! `DOMProcessingInstruction` / `DOMNodeList`.
//!
//! Called from:
//! - `crate::dom_html_prelude::declarations()`.
//!
//! Key details:
//! - Generated once from the former `surface.rs` PHP source with
//!   `synthetic_class::transcribe::transcribe_split`; the nodes are exactly the ones the parser
//!   produced, but no PHP text is parsed at compile time.
//! - This is an interim userland surface, not a second native libxml stack. Draft PR #654
//!   (`crates/elephc-dom`, libxml2 + Lexbor) remains the path to full PHP 8.5 DOM. Same class
//!   names and `LIBXML_*` integers so that work can replace this prelude without changing
//!   Termwind call sites.
//! - Attribute maps are stored as indexed `[name, value]` pairs. Nested string-keyed arrays
//!   lose their keys when they pass through mixed token/tree slots, which dropped `class` on
//!   Termwind's styled `div`.
//! - Sibling pointers are assigned through typed `DOMNode` parameters on `instanceof`-narrowed
//!   locals, then those locals are stored. Writes on `mixed` array elements copy the object and
//!   drop `nextSibling`.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `__elephc_dom_char` — transcribed from the PHP form.
fn decl_fn_elephc_dom_char() -> Stmt {
    function("__elephc_dom_char")
        .param("s", TypeExpr::Str)
        .param("i", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_var("i"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("i"), BinOp::GtEq, e_call("strlen", vec![e_var("s")]))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_return(e_call("substr", vec![e_var("s"), e_var("i"), e_int(1)])),
        ])
        .build()
}

/// `__elephc_dom_is_ws` — transcribed from the PHP form.
fn decl_fn_elephc_dom_is_ws() -> Stmt {
    function("__elephc_dom_is_ws")
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\n"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\r")))),
        ])
        .build()
}

/// `__elephc_dom_is_letter` — transcribed from the PHP form.
fn decl_fn_elephc_dom_is_letter() -> Stmt {
    function("__elephc_dom_is_letter")
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_o", e_call("ord", vec![e_var("c")])),
            s_return(e_binop(e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(122))))),
        ])
        .build()
}

/// `__elephc_dom_starts_markup` — transcribed from the PHP form.
fn decl_fn_elephc_dom_starts_markup() -> Stmt {
    function("__elephc_dom_starts_markup")
        .param("html", TypeExpr::Str)
        .param("i", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")]), BinOp::StrictNotEq, e_str("<")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_next", e_call("__elephc_dom_char", vec![e_var("html"), e_binop(e_var("i"), BinOp::Add, e_int(1))])),
            s_if(
                e_binop(e_var("_next"), BinOp::StrictEq, e_str("!")),
                vec![
                    s_return(e_binop(e_binop(e_call("substr", vec![e_var("html"), e_var("i"), e_int(4)]), BinOp::StrictEq, e_str("<!--")), BinOp::Or, e_binop(e_binop(e_call("strtoupper", vec![e_call("substr", vec![e_var("html"), e_var("i"), e_int(9)])]), BinOp::StrictEq, e_str("<!DOCTYPE")), BinOp::And, e_binop(e_call("__elephc_dom_is_ws", vec![e_call("__elephc_dom_char", vec![e_var("html"), e_binop(e_var("i"), BinOp::Add, e_int(9))])]), BinOp::Or, e_binop(e_call("__elephc_dom_char", vec![e_var("html"), e_binop(e_var("i"), BinOp::Add, e_int(9))]), BinOp::StrictEq, e_str(">")))))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_next"), BinOp::StrictEq, e_str("/")),
                vec![
                    s_assign("_name_start", e_call("__elephc_dom_char", vec![e_var("html"), e_binop(e_var("i"), BinOp::Add, e_int(2))])),
                    s_return(e_binop(e_binop(e_call("__elephc_dom_is_letter", vec![e_var("_name_start")]), BinOp::Or, e_binop(e_var("_name_start"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("_name_start"), BinOp::StrictEq, e_str(":")))),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_call("__elephc_dom_is_letter", vec![e_var("_next")]), BinOp::Or, e_binop(e_var("_next"), BinOp::StrictEq, e_str("?")))),
        ])
        .build()
}

/// `__elephc_dom_is_boolean_attr` — transcribed from the PHP form.
fn decl_fn_elephc_dom_is_boolean_attr() -> Stmt {
    function("__elephc_dom_is_boolean_attr")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("checked")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("compact"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("declare"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("defer"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("disabled"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("ismap"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("multiple"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("nohref"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("noresize"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("noshade"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("nowrap"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("readonly"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("selected")))),
        ])
        .build()
}

/// `__elephc_dom_is_name` — transcribed from the PHP form.
fn decl_fn_elephc_dom_is_name() -> Stmt {
    function("__elephc_dom_is_name")
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("_o", e_call("ord", vec![e_var("c")])),
            s_if(
                e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(90))),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(122))),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("_o"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("_o"), BinOp::LtEq, e_int(57))),
                vec![
                    s_return(e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("-")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":")))),
        ])
        .build()
}

/// `__elephc_dom_is_void` — transcribed from the PHP form.
fn decl_fn_elephc_dom_is_void() -> Stmt {
    function("__elephc_dom_is_void")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("area")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("base"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("br"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("col"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("hr"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("img"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("input"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("link"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("meta"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("param"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("frame"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("isindex")))),
        ])
        .build()
}

/// `__elephc_dom_is_head_element` — transcribed from the PHP form.
fn decl_fn_elephc_dom_is_head_element() -> Stmt {
    function("__elephc_dom_is_head_element")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("head")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("base"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("link"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("meta"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("title"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("script"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("style")))),
        ])
        .build()
}

/// `__elephc_dom_is_frame_element` — transcribed from the PHP form.
fn decl_fn_elephc_dom_is_frame_element() -> Stmt {
    function("__elephc_dom_is_frame_element")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("frame")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("frameset"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("noframes")))),
        ])
        .build()
}

/// `__elephc_dom_skip_ws` — transcribed from the PHP form.
fn decl_fn_elephc_dom_skip_ws() -> Stmt {
    function("__elephc_dom_skip_ws")
        .param("html", TypeExpr::Str)
        .param("i", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("_len", e_call("strlen", vec![e_var("html")])),
            s_while(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("_len")), BinOp::And, e_call("__elephc_dom_is_ws", vec![e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")])])), vec![
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
            ]),
            s_return(e_var("i")),
        ])
        .build()
}

/// `__elephc_dom_read_name` — transcribed from the PHP form.
fn decl_fn_elephc_dom_read_name() -> Stmt {
    function("__elephc_dom_read_name")
        .param("html", TypeExpr::Str)
        .param("i", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_start", e_var("i")),
            s_assign("_len", e_call("strlen", vec![e_var("html")])),
            s_while(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("_len")), BinOp::And, e_call("__elephc_dom_is_name", vec![e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")])])), vec![
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
            ]),
            s_return(e_array(vec![e_call("strtolower", vec![e_call("substr", vec![e_var("html"), e_var("_start"), e_binop(e_var("i"), BinOp::Sub, e_var("_start"))])]), e_var("i")])),
        ])
        .build()
}

/// `__elephc_dom_read_attr_value` — transcribed from the PHP form.
fn decl_fn_elephc_dom_read_attr_value() -> Stmt {
    function("__elephc_dom_read_attr_value")
        .param("html", TypeExpr::Str)
        .param("i", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_len", e_call("strlen", vec![e_var("html")])),
            s_assign("i", e_call("__elephc_dom_skip_ws", vec![e_var("html"), e_var("i")])),
            s_assign("_q", e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")])),
            s_if(
                e_binop(e_binop(e_var("_q"), BinOp::StrictEq, e_str("\"")), BinOp::Or, e_binop(e_var("_q"), BinOp::StrictEq, e_str("'"))),
                vec![
                    s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                    s_assign("_start", e_var("i")),
                    s_while(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("_len")), BinOp::And, e_binop(e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")]), BinOp::StrictNotEq, e_var("_q"))), vec![
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                    ]),
                    s_assign("_val", e_call("__elephc_dom_decode_entities", vec![e_call("substr", vec![e_var("html"), e_var("_start"), e_binop(e_var("i"), BinOp::Sub, e_var("_start"))])])),
                    s_if(
                        e_binop(e_var("i"), BinOp::Lt, e_var("_len")),
                        vec![
                            s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array(vec![e_var("_val"), e_var("i")])),
                ],
                vec![],
                None,
            ),
            s_assign("_start", e_var("i")),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_var("_len")), vec![
                s_assign("_c", e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")])),
                s_if(
                    e_binop(e_call("__elephc_dom_is_ws", vec![e_var("_c")]), BinOp::Or, e_binop(e_var("_c"), BinOp::StrictEq, e_str(">"))),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
            ]),
            s_return(e_array(vec![e_call("__elephc_dom_decode_entities", vec![e_call("substr", vec![e_var("html"), e_var("_start"), e_binop(e_var("i"), BinOp::Sub, e_var("_start"))])]), e_var("i")])),
        ])
        .build()
}

/// `__elephc_dom_read_attrs` — transcribed from the PHP form.
fn decl_fn_elephc_dom_read_attrs() -> Stmt {
    function("__elephc_dom_read_attrs")
        .param("html", TypeExpr::Str)
        .param("i", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_attrs", e_array(vec![])),
            s_assign("_self", e_int(0)),
            s_assign("_len", e_call("strlen", vec![e_var("html")])),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_var("_len")), vec![
                s_assign("i", e_call("__elephc_dom_skip_ws", vec![e_var("html"), e_var("i")])),
                s_assign("_c", e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")])),
                s_if(
                    e_binop(e_binop(e_var("_c"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("_c"), BinOp::StrictEq, e_str(">"))),
                    vec![
                        s_if(
                            e_binop(e_var("_c"), BinOp::StrictEq, e_str(">")),
                            vec![
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("_c"), BinOp::StrictEq, e_str("/")),
                    vec![
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                        s_if(
                            e_binop(e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")]), BinOp::StrictEq, e_str(">")),
                            vec![
                                s_assign("_self", e_int(1)),
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_while(e_binop(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("_len")), BinOp::And, e_not(e_call("__elephc_dom_is_ws", vec![e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")])]))), BinOp::And, e_binop(e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")]), BinOp::StrictNotEq, e_str(">"))), vec![
                            s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                        ]),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_pair", e_call("__elephc_dom_read_name", vec![e_var("html"), e_var("i")])),
                s_assign("_name", e_cast(CastType::String, e_index(e_var("_pair"), e_int(0)))),
                s_assign("i", e_cast(CastType::Int, e_index(e_var("_pair"), e_int(1)))),
                s_if(
                    e_binop(e_var("_name"), BinOp::StrictEq, e_str("")),
                    vec![
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("i", e_call("__elephc_dom_skip_ws", vec![e_var("html"), e_var("i")])),
                s_assign("_val", e_str("")),
                s_if(
                    e_binop(e_call("__elephc_dom_char", vec![e_var("html"), e_var("i")]), BinOp::StrictEq, e_str("=")),
                    vec![
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                        s_assign("_av", e_call("__elephc_dom_read_attr_value", vec![e_var("html"), e_var("i")])),
                        s_assign("_val", e_cast(CastType::String, e_index(e_var("_av"), e_int(0)))),
                        s_assign("i", e_cast(CastType::Int, e_index(e_var("_av"), e_int(1)))),
                    ],
                    vec![
                    (e_call("__elephc_dom_is_boolean_attr", vec![e_var("_name")]), vec![
                        s_assign("_val", e_var("_name")),
                    ]),
                ],
                    None,
                ),
                s_if(
                    e_not(e_call("__elephc_dom_has_attr", vec![e_var("_attrs"), e_var("_name")])),
                    vec![
                        s_array_push("_attrs", e_array(vec![e_var("_name"), e_var("_val")])),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_array(vec![e_var("_attrs"), e_var("_self"), e_var("i")])),
        ])
        .build()
}

/// `__elephc_dom_has_attr` — transcribed from the PHP form.
fn decl_fn_elephc_dom_has_attr() -> Stmt {
    function("__elephc_dom_has_attr")
        .param("attrs", t_mixed())
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_not(e_call("is_array", vec![e_var("attrs")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("attrs"), None, "_pair", vec![
                s_if(
                    e_binop(e_call("is_array", vec![e_var("_pair")]), BinOp::And, e_binop(e_cast(CastType::String, e_index(e_var("_pair"), e_int(0))), BinOp::StrictEq, e_var("name"))),
                    vec![
                        s_return(e_bool(true)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(false)),
        ])
        .build()
}

/// `__elephc_dom_attr_get` — transcribed from the PHP form.
fn decl_fn_elephc_dom_attr_get() -> Stmt {
    function("__elephc_dom_attr_get")
        .param("attrs", t_mixed())
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_not(e_call("is_array", vec![e_var("attrs")])),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("attrs"), None, "_pair", vec![
                s_if(
                    e_binop(e_call("is_array", vec![e_var("_pair")]), BinOp::And, e_binop(e_cast(CastType::String, e_index(e_var("_pair"), e_int(0))), BinOp::StrictEq, e_var("name"))),
                    vec![
                        s_return(e_cast(CastType::String, e_index(e_var("_pair"), e_int(1)))),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_dom_append_text_token` — transcribed from the PHP form.
fn decl_fn_elephc_dom_append_text_token() -> Stmt {
    function("__elephc_dom_append_text_token")
        .param("tokens", t_array())
        .param("text", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_if(
                e_binop(e_var("text"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_var("tokens")),
                ],
                vec![],
                None,
            ),
            s_assign("_count", e_call("count", vec![e_var("tokens")])),
            s_if(
                e_binop(e_binop(e_var("_count"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_index(e_index(e_var("tokens"), e_binop(e_var("_count"), BinOp::Sub, e_int(1))), e_str("k")), BinOp::StrictEq, e_str("text"))),
                vec![
                    s_assign("_last", e_index(e_var("tokens"), e_binop(e_var("_count"), BinOp::Sub, e_int(1)))),
                    s_array_assign("_last", e_str("v"), e_binop(e_cast(CastType::String, e_index(e_var("_last"), e_str("v"))), BinOp::Concat, e_var("text"))),
                    s_array_assign("tokens", e_binop(e_var("_count"), BinOp::Sub, e_int(1)), e_var("_last")),
                ],
                vec![],
                Some(vec![
                s_array_push("tokens", e_array_assoc(vec![(e_str("k"), e_str("text")), (e_str("v"), e_var("text"))])),
            ]),
            ),
            s_return(e_var("tokens")),
        ])
        .build()
}

/// `__elephc_dom_token_stream` — transcribed from the PHP form.
fn decl_fn_elephc_dom_token_stream() -> Stmt {
    function("__elephc_dom_token_stream")
        .param("html", TypeExpr::Str)
        .param("flags", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_tokens", e_array(vec![])),
            s_assign("_i", e_call("__elephc_dom_skip_ws", vec![e_var("html"), e_int(0)])),
            s_assign("_len", e_call("strlen", vec![e_var("html")])),
            s_assign("_unused_flags", e_var("flags")),
            s_while(e_binop(e_var("_i"), BinOp::Lt, e_var("_len")), vec![
                s_if(
                    e_not(e_call("__elephc_dom_starts_markup", vec![e_var("html"), e_var("_i")])),
                    vec![
                        s_assign("_start", e_var("_i")),
                        s_assign("_text", e_str("")),
                        s_while(e_binop(e_binop(e_var("_i"), BinOp::Lt, e_var("_len")), BinOp::And, e_not(e_call("__elephc_dom_starts_markup", vec![e_var("html"), e_var("_i")]))), vec![
                            s_if(
                                e_binop(e_call("substr", vec![e_var("html"), e_var("_i"), e_int(2)]), BinOp::StrictEq, e_str("</")),
                                vec![
                                    s_assign("_text", e_binop(e_var("_text"), BinOp::Concat, e_call("substr", vec![e_var("html"), e_var("_start"), e_binop(e_var("_i"), BinOp::Sub, e_var("_start"))]))),
                                    s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(2))),
                                    s_assign("_start", e_var("_i")),
                                    s_continue(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(1))),
                        ]),
                        s_assign("_text", e_call("__elephc_dom_decode_entities", vec![e_binop(e_var("_text"), BinOp::Concat, e_call("substr", vec![e_var("html"), e_var("_start"), e_binop(e_var("_i"), BinOp::Sub, e_var("_start"))]))])),
                        s_assign("_tokens", e_call("__elephc_dom_append_text_token", vec![e_var("_tokens"), e_var("_text")])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("substr", vec![e_var("html"), e_var("_i"), e_int(4)]), BinOp::StrictEq, e_str("<!--")),
                    vec![
                        s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(4))),
                        s_assign("_comment_end", e_call("strpos", vec![e_var("html"), e_str("-->"), e_var("_i")])),
                        s_if(
                            e_binop(e_var("_comment_end"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_body", e_call("substr", vec![e_var("html"), e_var("_i"), e_binop(e_cast(CastType::Int, e_var("_comment_end")), BinOp::Sub, e_var("_i"))])),
                        s_assign("_i", e_binop(e_cast(CastType::Int, e_var("_comment_end")), BinOp::Add, e_int(3))),
                        s_array_push("_tokens", e_array_assoc(vec![(e_str("k"), e_str("comment")), (e_str("v"), e_var("_body"))])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("substr", vec![e_var("html"), e_var("_i"), e_int(2)]), BinOp::StrictEq, e_str("<?")),
                    vec![
                        s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(2))),
                        s_assign("_start", e_var("_i")),
                        s_assign("_first", e_call("__elephc_dom_char", vec![e_var("html"), e_var("_i")])),
                        s_if(
                            e_binop(e_binop(e_not(e_call("__elephc_dom_is_letter", vec![e_var("_first")])), BinOp::And, e_binop(e_var("_first"), BinOp::StrictNotEq, e_str("_"))), BinOp::And, e_binop(e_var("_first"), BinOp::StrictNotEq, e_str(":"))),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_while(e_binop(e_binop(e_var("_i"), BinOp::Lt, e_var("_len")), BinOp::And, e_binop(e_call("__elephc_dom_is_name", vec![e_call("__elephc_dom_char", vec![e_var("html"), e_var("_i")])]), BinOp::Or, e_binop(e_call("__elephc_dom_char", vec![e_var("html"), e_var("_i")]), BinOp::StrictEq, e_str(".")))), vec![
                            s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(1))),
                        ]),
                        s_assign("_target", e_call("substr", vec![e_var("html"), e_var("_start"), e_binop(e_var("_i"), BinOp::Sub, e_var("_start"))])),
                        s_if(
                            e_binop(e_var("_i"), BinOp::StrictEq, e_var("_len")),
                            vec![
                                s_assign("_tokens", e_call("__elephc_dom_append_text_token", vec![e_var("_tokens"), e_var("_target")])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_data_start", e_call("__elephc_dom_skip_ws", vec![e_var("html"), e_var("_i")])),
                        s_assign("_pi_end", e_call("strpos", vec![e_var("html"), e_str(">"), e_var("_data_start")])),
                        s_if(
                            e_binop(e_var("_pi_end"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_array_push("_tokens", e_array_assoc(vec![(e_str("k"), e_str("pi")), (e_str("n"), e_var("_target")), (e_str("v"), e_call("substr", vec![e_var("html"), e_var("_data_start"), e_binop(e_cast(CastType::Int, e_var("_pi_end")), BinOp::Sub, e_var("_data_start"))]))])),
                        s_assign("_i", e_binop(e_cast(CastType::Int, e_var("_pi_end")), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("substr", vec![e_var("html"), e_var("_i"), e_int(2)]), BinOp::StrictEq, e_str("<!")),
                    vec![
                        s_assign("_decl_end", e_call("strpos", vec![e_var("html"), e_str(">"), e_var("_i")])),
                        s_if(
                            e_binop(e_var("_decl_end"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_i", e_binop(e_cast(CastType::Int, e_var("_decl_end")), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("substr", vec![e_var("html"), e_var("_i"), e_int(2)]), BinOp::StrictEq, e_str("</")),
                    vec![
                        s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(2))),
                        s_assign("_pair", e_call("__elephc_dom_read_name", vec![e_var("html"), e_var("_i")])),
                        s_assign("_name", e_cast(CastType::String, e_index(e_var("_pair"), e_int(0)))),
                        s_assign("_i", e_cast(CastType::Int, e_index(e_var("_pair"), e_int(1)))),
                        s_assign("_close_end", e_call("strpos", vec![e_var("html"), e_str(">"), e_var("_i")])),
                        s_if(
                            e_binop(e_var("_close_end"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_assign("_i", e_var("_len")),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("_i", e_binop(e_cast(CastType::Int, e_var("_close_end")), BinOp::Add, e_int(1))),
                        ]),
                        ),
                        s_array_push("_tokens", e_array_assoc(vec![(e_str("k"), e_str("end")), (e_str("n"), e_var("_name"))])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_int(1))),
                s_assign("_pair", e_call("__elephc_dom_read_name", vec![e_var("html"), e_var("_i")])),
                s_assign("_name", e_cast(CastType::String, e_index(e_var("_pair"), e_int(0)))),
                s_assign("_i", e_cast(CastType::Int, e_index(e_var("_pair"), e_int(1)))),
                s_if(
                    e_binop(e_var("_name"), BinOp::StrictEq, e_str("")),
                    vec![
                        s_assign("_tokens", e_call("__elephc_dom_append_text_token", vec![e_var("_tokens"), e_str("<")])),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_ap", e_call("__elephc_dom_read_attrs", vec![e_var("html"), e_var("_i")])),
                s_assign("_attrs", e_index(e_var("_ap"), e_int(0))),
                s_assign("_self", e_cast(CastType::Int, e_index(e_var("_ap"), e_int(1)))),
                s_assign("_i", e_cast(CastType::Int, e_index(e_var("_ap"), e_int(2)))),
                s_if(
                    e_binop(e_binop(e_var("_self"), BinOp::StrictEq, e_int(1)), BinOp::Or, e_call("__elephc_dom_is_void", vec![e_var("_name")])),
                    vec![
                        s_assign("_self", e_int(1)),
                    ],
                    vec![],
                    None,
                ),
                s_array_push("_tokens", e_array_assoc(vec![(e_str("k"), e_str("start")), (e_str("n"), e_var("_name")), (e_str("a"), e_var("_attrs")), (e_str("void"), e_var("_self"))])),
                s_if(
                    e_binop(e_binop(e_var("_self"), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("_name"), BinOp::StrictEq, e_str("script")), BinOp::Or, e_binop(e_var("_name"), BinOp::StrictEq, e_str("style")))),
                    vec![
                        s_assign("_start", e_var("_i")),
                        s_while(e_binop(e_var("_i"), BinOp::Lt, e_var("_len")), vec![
                            s_if(
                                e_binop(e_binop(e_call("substr", vec![e_var("html"), e_var("_i"), e_int(2)]), BinOp::StrictEq, e_str("</")), BinOp::And, e_binop(e_call("strtolower", vec![e_call("substr", vec![e_var("html"), e_binop(e_var("_i"), BinOp::Add, e_int(2)), e_call("strlen", vec![e_var("_name")])])]), BinOp::StrictEq, e_var("_name"))),
                                vec![
                                    s_assign("_after", e_call("__elephc_dom_char", vec![e_var("html"), e_binop(e_binop(e_var("_i"), BinOp::Add, e_int(2)), BinOp::Add, e_call("strlen", vec![e_var("_name")]))])),
                                    s_if(
                                        e_binop(e_binop(e_binop(e_var("_after"), BinOp::StrictEq, e_str(">")), BinOp::Or, e_binop(e_var("_after"), BinOp::StrictEq, e_str("/"))), BinOp::Or, e_call("__elephc_dom_is_ws", vec![e_var("_after")])),
                                        vec![
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_post_inc("_i")),
                        ]),
                        s_assign("_raw", e_call("substr", vec![e_var("html"), e_var("_start"), e_binop(e_var("_i"), BinOp::Sub, e_var("_start"))])),
                        s_if(
                            e_binop(e_var("_raw"), BinOp::StrictNotEq, e_str("")),
                            vec![
                                s_array_push("_tokens", e_array_assoc(vec![(e_str("k"), e_str("cdata")), (e_str("v"), e_var("_raw"))])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_var("_tokens")),
        ])
        .build()
}

/// `__elephc_dom_text_of` — transcribed from the PHP form.
fn decl_fn_elephc_dom_text_of() -> Stmt {
    function("__elephc_dom_text_of")
        .param("node", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_binop(e_index(e_var("node"), e_str("kind")), BinOp::StrictEq, e_str("text")), BinOp::Or, e_binop(e_index(e_var("node"), e_str("kind")), BinOp::StrictEq, e_str("cdata"))),
                vec![
                    s_return(e_cast(CastType::String, e_index(e_var("node"), e_str("value")))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_index(e_var("node"), e_str("kind")), BinOp::StrictEq, e_str("comment")), BinOp::Or, e_binop(e_index(e_var("node"), e_str("kind")), BinOp::StrictEq, e_str("pi"))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_assign("_out", e_str("")),
            s_foreach(e_index(e_var("node"), e_str("children")), None, "_child", vec![
                s_assign("_out", e_binop(e_var("_out"), BinOp::Concat, e_call("__elephc_dom_text_of", vec![e_var("_child")]))),
            ]),
            s_return(e_var("_out")),
        ])
        .build()
}

/// `__elephc_dom_new_node` — transcribed from the PHP form.
fn decl_fn_elephc_dom_new_node() -> Stmt {
    function("__elephc_dom_new_node")
        .param("kind", TypeExpr::Str)
        .param("name", TypeExpr::Str)
        .param("value", TypeExpr::Str)
        .param("attrs", t_mixed())
        .param("children", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_return(e_array_assoc(vec![(e_str("kind"), e_var("kind")), (e_str("name"), e_var("name")), (e_str("value"), e_var("value")), (e_str("attrs"), e_var("attrs")), (e_str("children"), e_var("children"))])),
        ])
        .build()
}

/// `__elephc_dom_append_child` — transcribed from the PHP form.
fn decl_fn_elephc_dom_append_child() -> Stmt {
    function("__elephc_dom_append_child")
        .param("stack", t_array())
        .param("node", t_mixed())
        .returns(t_array())
        .body(vec![
            s_assign("_top", e_index(e_var("stack"), e_binop(e_call("count", vec![e_var("stack")]), BinOp::Sub, e_int(1)))),
            s_assign("_children", e_index(e_var("_top"), e_str("children"))),
            s_assign("_count", e_call("count", vec![e_var("_children")])),
            s_if(
                e_binop(e_binop(e_binop(e_index(e_var("node"), e_str("kind")), BinOp::StrictEq, e_str("text")), BinOp::And, e_binop(e_var("_count"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_index(e_index(e_var("_children"), e_binop(e_var("_count"), BinOp::Sub, e_int(1))), e_str("kind")), BinOp::StrictEq, e_str("text"))),
                vec![
                    s_assign("_last", e_index(e_var("_children"), e_binop(e_var("_count"), BinOp::Sub, e_int(1)))),
                    s_array_assign("_last", e_str("value"), e_binop(e_cast(CastType::String, e_index(e_var("_last"), e_str("value"))), BinOp::Concat, e_cast(CastType::String, e_index(e_var("node"), e_str("value"))))),
                    s_array_assign("_children", e_binop(e_var("_count"), BinOp::Sub, e_int(1)), e_var("_last")),
                ],
                vec![],
                Some(vec![
                s_array_push("_children", e_var("node")),
            ]),
            ),
            s_array_assign("_top", e_str("children"), e_var("_children")),
            s_array_assign("stack", e_binop(e_call("count", vec![e_var("stack")]), BinOp::Sub, e_int(1)), e_var("_top")),
            s_return(e_var("stack")),
        ])
        .build()
}

/// `__elephc_dom_build_forest` — transcribed from the PHP form.
fn decl_fn_elephc_dom_build_forest() -> Stmt {
    function("__elephc_dom_build_forest")
        .param("tokens", t_mixed())
        .param("flags", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_stack", e_array(vec![])),
            s_assign("_roots", e_array(vec![])),
            s_assign("_body_seen", e_bool(false)),
            s_assign("_implied_root_body", e_bool(false)),
            s_assign("_ignored_body_ends", e_int(0)),
            s_assign("_token_position", e_int(0)),
            s_foreach(e_var("tokens"), None, "_tok", vec![
                s_assign("_token_position", e_binop(e_var("_token_position"), BinOp::Add, e_int(1))),
                s_assign("_k", e_index(e_var("_tok"), e_str("k"))),
                s_assign("_head_content", e_bool(false)),
                s_assign("_body_open", e_bool(false)),
                s_for(Some(s_assign("_scope_index", e_int(0))), Some(e_binop(e_var("_scope_index"), BinOp::Lt, e_call("count", vec![e_var("_stack")]))), Some(s_expr(e_post_inc("_scope_index"))), vec![
                    s_assign("_scope", e_index(e_var("_stack"), e_var("_scope_index"))),
                    s_if(
                        e_call("__elephc_dom_is_head_element", vec![e_cast(CastType::String, e_index(e_var("_scope"), e_str("name")))]),
                        vec![
                            s_assign("_head_content", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_index(e_var("_scope"), e_str("name")), BinOp::StrictEq, e_str("body")),
                        vec![
                            s_assign("_body_open", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_binop(e_binop(e_var("_k"), BinOp::StrictEq, e_str("text")), BinOp::Or, e_binop(e_var("_k"), BinOp::StrictEq, e_str("cdata"))),
                    vec![
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_k"), BinOp::StrictEq, e_str("text")), BinOp::And, e_not(e_var("_body_seen"))), BinOp::And, e_not(e_var("_head_content"))), BinOp::And, e_binop(e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_index(e_index(e_var("_stack"), e_binop(e_call("count", vec![e_var("_stack")]), BinOp::Sub, e_int(1))), e_str("name")), BinOp::StrictEq, e_str("html")))), BinOp::And, e_binop(e_call("trim", vec![e_cast(CastType::String, e_index(e_var("_tok"), e_str("v")))]), BinOp::StrictNotEq, e_str(""))),
                            vec![
                                s_assign("_implied_root_body", e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0))),
                                s_assign("_node", e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("body"), e_str(""), e_array(vec![]), e_array(vec![])])),
                                s_array_push("_stack", e_var("_node")),
                                s_assign("_body_seen", e_bool(true)),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("_k"), BinOp::StrictEq, e_str("text")), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_const("LIBXML_NOBLANKS")), BinOp::StrictNotEq, e_int(0))), BinOp::And, e_binop(e_call("trim", vec![e_cast(CastType::String, e_index(e_var("_tok"), e_str("v")))]), BinOp::StrictEq, e_str(""))), BinOp::And, e_binop(e_call("count", vec![e_var("_stack")]), BinOp::Gt, e_int(0))),
                            vec![
                                s_assign("_parent", e_index(e_var("_stack"), e_binop(e_call("count", vec![e_var("_stack")]), BinOp::Sub, e_int(1)))),
                                s_if(
                                    e_binop(e_index(e_var("_parent"), e_str("name")), BinOp::StrictEq, e_str("head")),
                                    vec![
                                        s_continue(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_binop(e_binop(e_var("_implied_root_body"), BinOp::And, e_binop(e_index(e_var("_parent"), e_str("name")), BinOp::StrictEq, e_str("body"))), BinOp::And, e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(1))), BinOp::And, e_binop(e_var("_token_position"), BinOp::StrictEq, e_call("count", vec![e_var("tokens")]))),
                                    vec![
                                        s_continue(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_node", e_call("__elephc_dom_new_node", vec![e_var("_k"), e_str("#text"), e_cast(CastType::String, e_index(e_var("_tok"), e_str("v"))), e_array(vec![]), e_array(vec![])])),
                        s_if(
                            e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_assign("_count", e_call("count", vec![e_var("_roots")])),
                                s_if(
                                    e_binop(e_binop(e_binop(e_var("_k"), BinOp::StrictEq, e_str("text")), BinOp::And, e_binop(e_var("_count"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_index(e_index(e_var("_roots"), e_binop(e_var("_count"), BinOp::Sub, e_int(1))), e_str("kind")), BinOp::StrictEq, e_str("text"))),
                                    vec![
                                        s_assign("_last", e_index(e_var("_roots"), e_binop(e_var("_count"), BinOp::Sub, e_int(1)))),
                                        s_array_assign("_last", e_str("value"), e_binop(e_cast(CastType::String, e_index(e_var("_last"), e_str("value"))), BinOp::Concat, e_cast(CastType::String, e_index(e_var("_node"), e_str("value"))))),
                                        s_array_assign("_roots", e_binop(e_var("_count"), BinOp::Sub, e_int(1)), e_var("_last")),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_array_push("_roots", e_var("_node")),
                                ]),
                                ),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("_stack", e_call("__elephc_dom_append_child", vec![e_var("_stack"), e_var("_node")])),
                        ]),
                        ),
                    ],
                    vec![
                    (e_binop(e_binop(e_var("_k"), BinOp::StrictEq, e_str("comment")), BinOp::Or, e_binop(e_var("_k"), BinOp::StrictEq, e_str("pi"))), vec![
                        s_assign("_name", e_str("#comment")),
                        s_if(
                            e_binop(e_var("_k"), BinOp::StrictEq, e_str("pi")),
                            vec![
                                s_assign("_name", e_cast(CastType::String, e_index(e_var("_tok"), e_str("n")))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_node", e_call("__elephc_dom_new_node", vec![e_var("_k"), e_var("_name"), e_cast(CastType::String, e_index(e_var("_tok"), e_str("v"))), e_array(vec![]), e_array(vec![])])),
                        s_if(
                            e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_array_push("_roots", e_var("_node")),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("_stack", e_call("__elephc_dom_append_child", vec![e_var("_stack"), e_var("_node")])),
                        ]),
                        ),
                    ]),
                    (e_binop(e_var("_k"), BinOp::StrictEq, e_str("start")), vec![
                        s_assign("_tag_name", e_cast(CastType::String, e_index(e_var("_tok"), e_str("n")))),
                        s_if(
                            e_binop(e_var("_tag_name"), BinOp::StrictEq, e_str("body")),
                            vec![
                                s_if(
                                    e_var("_body_open"),
                                    vec![
                                        s_if(
                                            e_binop(e_cast(CastType::Int, e_index(e_var("_tok"), e_str("void"))), BinOp::StrictEq, e_int(1)),
                                            vec![
                                                s_assign("_done", e_call("array_pop", vec![e_var("_stack")])),
                                                s_if(
                                                    e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0)),
                                                    vec![
                                                        s_array_push("_roots", e_var("_done")),
                                                    ],
                                                    vec![],
                                                    Some(vec![
                                                    s_assign("_stack", e_call("__elephc_dom_append_child", vec![e_var("_stack"), e_var("_done")])),
                                                ]),
                                                ),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_assign("_ignored_body_ends", e_binop(e_var("_ignored_body_ends"), BinOp::Add, e_int(1))),
                                        s_continue(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("_body_seen", e_bool(true)),
                                s_assign("_implied_root_body", e_bool(false)),
                            ],
                            vec![
                            (e_binop(e_binop(e_binop(e_binop(e_not(e_var("_body_seen")), BinOp::And, e_not(e_var("_head_content"))), BinOp::And, e_binop(e_var("_tag_name"), BinOp::StrictNotEq, e_str("html"))), BinOp::And, e_not(e_call("__elephc_dom_is_head_element", vec![e_var("_tag_name")]))), BinOp::And, e_not(e_call("__elephc_dom_is_frame_element", vec![e_var("_tag_name")]))), vec![
                                s_assign("_implied_root_body", e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0))),
                                s_assign("_node", e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("body"), e_str(""), e_array(vec![]), e_array(vec![])])),
                                s_array_push("_stack", e_var("_node")),
                                s_assign("_body_seen", e_bool(true)),
                            ]),
                        ],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("_tag_name"), BinOp::StrictEq, e_str("html")),
                            vec![
                                s_assign("_html_open", e_bool(false)),
                                s_for(Some(s_assign("_open_index", e_int(0))), Some(e_binop(e_var("_open_index"), BinOp::Lt, e_call("count", vec![e_var("_stack")]))), Some(s_expr(e_post_inc("_open_index"))), vec![
                                    s_assign("_open", e_index(e_var("_stack"), e_var("_open_index"))),
                                    s_if(
                                        e_binop(e_index(e_var("_open"), e_str("name")), BinOp::StrictEq, e_str("html")),
                                        vec![
                                            s_assign("_html_open", e_bool(true)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                                s_if(
                                    e_var("_html_open"),
                                    vec![
                                        s_continue(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_node", e_call("__elephc_dom_new_node", vec![e_str("element"), e_var("_tag_name"), e_str(""), e_index(e_var("_tok"), e_str("a")), e_array(vec![])])),
                        s_if(
                            e_binop(e_cast(CastType::Int, e_index(e_var("_tok"), e_str("void"))), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_if(
                                    e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0)),
                                    vec![
                                        s_array_push("_roots", e_var("_node")),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("_stack", e_call("__elephc_dom_append_child", vec![e_var("_stack"), e_var("_node")])),
                                ]),
                                ),
                            ],
                            vec![],
                            Some(vec![
                            s_array_push("_stack", e_var("_node")),
                        ]),
                        ),
                    ]),
                ],
                    Some(vec![
                    s_assign("_tag_name", e_cast(CastType::String, e_index(e_var("_tok"), e_str("n")))),
                    s_if(
                        e_binop(e_binop(e_var("_ignored_body_ends"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_binop(e_binop(e_var("_tag_name"), BinOp::StrictEq, e_str("body")), BinOp::Or, e_binop(e_var("_tag_name"), BinOp::StrictEq, e_str("html"))), BinOp::Or, e_binop(e_var("_tag_name"), BinOp::StrictEq, e_str("head")))),
                        vec![
                            s_assign("_ignored_body_ends", e_binop(e_var("_ignored_body_ends"), BinOp::Sub, e_int(1))),
                            s_continue(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_match", e_neg(e_int(1))),
                    s_for(Some(s_assign("_pos", e_binop(e_call("count", vec![e_var("_stack")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("_pos"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("_pos"))), vec![
                        s_if(
                            e_binop(e_index(e_index(e_var("_stack"), e_var("_pos")), e_str("name")), BinOp::StrictEq, e_var("_tag_name")),
                            vec![
                                s_assign("_match", e_var("_pos")),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_var("_match"), BinOp::Lt, e_int(0)),
                        vec![
                            s_continue(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_while(e_binop(e_call("count", vec![e_var("_stack")]), BinOp::Gt, e_var("_match")), vec![
                        s_assign("_done", e_call("array_pop", vec![e_var("_stack")])),
                        s_if(
                            e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_array_push("_roots", e_var("_done")),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("_stack", e_call("__elephc_dom_append_child", vec![e_var("_stack"), e_var("_done")])),
                        ]),
                        ),
                    ]),
                ]),
                ),
            ]),
            s_while(e_binop(e_call("count", vec![e_var("_stack")]), BinOp::Gt, e_int(0)), vec![
                s_assign("_done", e_call("array_pop", vec![e_var("_stack")])),
                s_if(
                    e_binop(e_call("count", vec![e_var("_stack")]), BinOp::StrictEq, e_int(0)),
                    vec![
                        s_array_push("_roots", e_var("_done")),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("_stack", e_call("__elephc_dom_append_child", vec![e_var("_stack"), e_var("_done")])),
                ]),
                ),
            ]),
            s_return(e_var("_roots")),
        ])
        .build()
}

/// `__elephc_dom_find_child` — transcribed from the PHP form.
fn decl_fn_elephc_dom_find_child() -> Stmt {
    function("__elephc_dom_find_child")
        .param("node", t_mixed())
        .param("name", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_foreach(e_index(e_var("node"), e_str("children")), None, "_child", vec![
                s_if(
                    e_binop(e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("element")), BinOp::And, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_var("name"))),
                    vec![
                        s_return(e_var("_child")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_null()),
        ])
        .build()
}

/// `__elephc_dom_wrap_html` — transcribed from the PHP form.
fn decl_fn_elephc_dom_wrap_html() -> Stmt {
    function("__elephc_dom_wrap_html")
        .param("roots", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_binop(e_binop(e_call("count", vec![e_var("roots")]), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_index(e_index(e_var("roots"), e_int(0)), e_str("kind")), BinOp::StrictEq, e_str("element"))), BinOp::And, e_binop(e_index(e_index(e_var("roots"), e_int(0)), e_str("name")), BinOp::StrictEq, e_str("html"))),
                vec![
                    s_assign("_html", e_index(e_var("roots"), e_int(0))),
                    s_assign("_children", e_index(e_var("_html"), e_str("children"))),
                    s_assign("_needs_body", e_bool(false)),
                    s_if(
                        e_binop(e_call("__elephc_dom_find_child", vec![e_var("_html"), e_str("body")]), BinOp::StrictEq, e_null()),
                        vec![
                            s_assign("_needs_body", e_bool(true)),
                            s_assign("_prefix_children", e_array(vec![])),
                            s_assign("_body_children", e_array(vec![])),
                            s_assign("_original_children", e_index(e_var("_html"), e_str("children"))),
                            s_for(Some(s_assign("_child_index", e_int(0))), Some(e_binop(e_var("_child_index"), BinOp::Lt, e_call("count", vec![e_var("_original_children")]))), Some(s_expr(e_post_inc("_child_index"))), vec![
                                s_assign("_child", e_index(e_var("_original_children"), e_var("_child_index"))),
                                s_assign("_prefix", e_binop(e_binop(e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("comment")), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("pi"))), BinOp::Or, e_binop(e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("element")), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("head")), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("base"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("link"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("meta"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("title"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("script"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("style"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("frame"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("frameset"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("noframes")))))),
                                s_if(
                                    e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("text")),
                                    vec![
                                        s_assign("_text", e_cast(CastType::String, e_index(e_var("_child"), e_str("value")))),
                                        s_assign("_prefix", e_binop(e_call("trim", vec![e_var("_text")]), BinOp::StrictEq, e_str(""))),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_binop(e_call("count", vec![e_var("_body_children")]), BinOp::StrictEq, e_int(0)), BinOp::And, e_var("_prefix")),
                                    vec![
                                        s_array_push("_prefix_children", e_var("_child")),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_array_push("_body_children", e_var("_child")),
                                ]),
                                ),
                            ]),
                            s_if(
                                e_binop(e_call("count", vec![e_var("_body_children")]), BinOp::Gt, e_int(0)),
                                vec![
                                    s_assign("_body", e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("body"), e_str(""), e_array(vec![]), e_var("_body_children")])),
                                    s_array_push("_prefix_children", e_var("_body")),
                                    s_assign("_needs_body", e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("_children", e_var("_prefix_children")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("count", vec![e_var("roots")]), BinOp::Gt, e_int(1)),
                        vec![
                            s_assign("_later", e_array(vec![])),
                            s_for(Some(s_assign("_i", e_int(1))), Some(e_binop(e_var("_i"), BinOp::Lt, e_call("count", vec![e_var("roots")]))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_assign("_later_root", e_index(e_var("roots"), e_var("_i"))),
                                s_array_push("_later", e_var("_later_root")),
                            ]),
                            s_assign("_first_later", e_index(e_var("_later"), e_int(0))),
                            s_if(
                                e_binop(e_binop(e_binop(e_call("count", vec![e_var("_later")]), BinOp::StrictEq, e_int(1)), BinOp::And, e_binop(e_index(e_var("_first_later"), e_str("kind")), BinOp::StrictEq, e_str("element"))), BinOp::And, e_binop(e_index(e_var("_first_later"), e_str("name")), BinOp::StrictEq, e_str("html"))),
                                vec![
                                    s_assign("_reopened", e_var("_first_later")),
                                ],
                                vec![],
                                Some(vec![
                                s_assign("_reopened", e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("html"), e_str(""), e_array(vec![]), e_var("_later")])),
                            ]),
                            ),
                            s_assign("_reopened_has_body", e_bool(false)),
                            s_assign("_reopened_children", e_index(e_var("_reopened"), e_str("children"))),
                            s_for(Some(s_assign("_reopened_scan_index", e_int(0))), Some(e_binop(e_var("_reopened_scan_index"), BinOp::Lt, e_call("count", vec![e_var("_reopened_children")]))), Some(s_expr(e_post_inc("_reopened_scan_index"))), vec![
                                s_assign("_child", e_index(e_var("_reopened_children"), e_var("_reopened_scan_index"))),
                                s_if(
                                    e_binop(e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("element")), BinOp::And, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("body"))),
                                    vec![
                                        s_assign("_reopened_has_body", e_bool(true)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_if(
                                e_binop(e_var("_needs_body"), BinOp::And, e_not(e_var("_reopened_has_body"))),
                                vec![
                                    s_assign("_reopened_prefix", e_array(vec![])),
                                    s_assign("_reopened_body", e_array(vec![])),
                                    s_assign("_reopened_children", e_index(e_var("_reopened"), e_str("children"))),
                                    s_for(Some(s_assign("_reopened_child_index", e_int(0))), Some(e_binop(e_var("_reopened_child_index"), BinOp::Lt, e_call("count", vec![e_var("_reopened_children")]))), Some(s_expr(e_post_inc("_reopened_child_index"))), vec![
                                        s_assign("_child", e_index(e_var("_reopened_children"), e_var("_reopened_child_index"))),
                                        s_assign("_prefix", e_binop(e_binop(e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("comment")), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("pi"))), BinOp::Or, e_binop(e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("element")), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("head")), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("base"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("link"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("meta"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("title"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("script"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("style"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("frame"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("frameset"))), BinOp::Or, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("noframes")))))),
                                        s_if(
                                            e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("text")),
                                            vec![
                                                s_assign("_prefix", e_binop(e_call("trim", vec![e_cast(CastType::String, e_index(e_var("_child"), e_str("value")))]), BinOp::StrictEq, e_str(""))),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_if(
                                            e_binop(e_binop(e_call("count", vec![e_var("_reopened_body")]), BinOp::StrictEq, e_int(0)), BinOp::And, e_var("_prefix")),
                                            vec![
                                                s_array_push("_reopened_prefix", e_var("_child")),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_array_push("_reopened_body", e_var("_child")),
                                        ]),
                                        ),
                                    ]),
                                    s_if(
                                        e_binop(e_call("count", vec![e_var("_reopened_body")]), BinOp::Gt, e_int(0)),
                                        vec![
                                            s_assign("_reopened_body_node", e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("body"), e_str(""), e_array(vec![]), e_var("_reopened_body")])),
                                            s_array_push("_reopened_prefix", e_var("_reopened_body_node")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("_reopened_attrs", e_index(e_var("_reopened"), e_str("attrs"))),
                                    s_assign("_reopened", e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("html"), e_str(""), e_var("_reopened_attrs"), e_var("_reopened_prefix")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_array_push("_children", e_var("_reopened")),
                        ],
                        vec![],
                        None,
                    ),
                    s_array_assign("_html", e_str("children"), e_var("_children")),
                    s_return(e_var("_html")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_call("count", vec![e_var("roots")]), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_index(e_index(e_var("roots"), e_int(0)), e_str("kind")), BinOp::StrictEq, e_str("element"))), BinOp::And, e_binop(e_index(e_index(e_var("roots"), e_int(0)), e_str("name")), BinOp::StrictEq, e_str("body"))),
                vec![
                    s_return(e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("html"), e_str(""), e_array(vec![]), e_var("roots")])),
                ],
                vec![],
                None,
            ),
            s_assign("_has_body_root", e_bool(false)),
            s_assign("_has_frame_root", e_bool(false)),
            s_assign("_has_ordinary_root", e_bool(false)),
            s_for(Some(s_assign("_root_kind_index", e_int(0))), Some(e_binop(e_var("_root_kind_index"), BinOp::Lt, e_call("count", vec![e_var("roots")]))), Some(s_expr(e_post_inc("_root_kind_index"))), vec![
                s_assign("_root_kind", e_index(e_var("roots"), e_var("_root_kind_index"))),
                s_if(
                    e_binop(e_index(e_var("_root_kind"), e_str("kind")), BinOp::StrictEq, e_str("element")),
                    vec![
                        s_if(
                            e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictEq, e_str("body")),
                            vec![
                                s_assign("_has_body_root", e_bool(true)),
                            ],
                            vec![
                            (e_binop(e_binop(e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictEq, e_str("frame")), BinOp::Or, e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictEq, e_str("frameset"))), BinOp::Or, e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictEq, e_str("noframes"))), vec![
                                s_assign("_has_frame_root", e_bool(true)),
                            ]),
                            (e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictNotEq, e_str("head")), BinOp::And, e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictNotEq, e_str("base"))), BinOp::And, e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictNotEq, e_str("link"))), BinOp::And, e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictNotEq, e_str("meta"))), BinOp::And, e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictNotEq, e_str("title"))), BinOp::And, e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictNotEq, e_str("script"))), BinOp::And, e_binop(e_index(e_var("_root_kind"), e_str("name")), BinOp::StrictNotEq, e_str("style"))), vec![
                                s_assign("_has_ordinary_root", e_bool(true)),
                            ]),
                        ],
                            None,
                        ),
                    ],
                    vec![
                    (e_binop(e_binop(e_index(e_var("_root_kind"), e_str("kind")), BinOp::StrictEq, e_str("text")), BinOp::And, e_binop(e_call("trim", vec![e_cast(CastType::String, e_index(e_var("_root_kind"), e_str("value")))]), BinOp::StrictNotEq, e_str(""))), vec![
                        s_assign("_has_ordinary_root", e_bool(true)),
                    ]),
                ],
                    None,
                ),
            ]),
            s_if(
                e_binop(e_var("_has_body_root"), BinOp::Or, e_binop(e_var("_has_frame_root"), BinOp::And, e_not(e_var("_has_ordinary_root")))),
                vec![
                    s_return(e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("html"), e_str(""), e_array(vec![]), e_var("roots")])),
                ],
                vec![],
                None,
            ),
            s_assign("_body", e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("body"), e_str(""), e_array(vec![]), e_var("roots")])),
            s_return(e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("html"), e_str(""), e_array(vec![]), e_array(vec![e_var("_body")])])),
        ])
        .build()
}

/// `__elephc_dom_filter_html_children` — transcribed from the PHP form.
fn decl_fn_elephc_dom_filter_html_children() -> Stmt {
    function("__elephc_dom_filter_html_children")
        .param("children", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_assign("_out", e_array(vec![])),
            s_for(Some(s_assign("_index", e_int(0))), Some(e_binop(e_var("_index"), BinOp::Lt, e_call("count", vec![e_var("children")]))), Some(s_expr(e_post_inc("_index"))), vec![
                s_assign("_child", e_index(e_var("children"), e_var("_index"))),
                s_if(
                    e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("text")),
                    vec![
                        s_assign("_blank_text", e_cast(CastType::String, e_index(e_var("_child"), e_str("value")))),
                        s_assign("_blank_trimmed", e_call("trim", vec![e_var("_blank_text")])),
                        s_if(
                            e_binop(e_var("_blank_trimmed"), BinOp::StrictEq, e_str("")),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_index(e_var("_child"), e_str("kind")), BinOp::StrictEq, e_str("element")), BinOp::And, e_binop(e_index(e_var("_child"), e_str("name")), BinOp::StrictEq, e_str("html"))),
                    vec![
                        s_assign("_nested", e_index(e_var("_child"), e_str("children"))),
                        s_assign("_filtered", e_call("__elephc_dom_filter_html_children", vec![e_var("_nested")])),
                        s_assign("_child_value", e_cast(CastType::String, e_index(e_var("_child"), e_str("value")))),
                        s_assign("_child_attrs", e_index(e_var("_child"), e_str("attrs"))),
                        s_assign("_child", e_call("__elephc_dom_new_node", vec![e_str("element"), e_str("html"), e_var("_child_value"), e_var("_child_attrs"), e_var("_filtered")])),
                    ],
                    vec![],
                    None,
                ),
                s_array_push("_out", e_var("_child")),
            ]),
            s_return(e_var("_out")),
        ])
        .build()
}

/// `__elephc_dom_parse_html` — transcribed from the PHP form.
fn decl_fn_elephc_dom_parse_html() -> Stmt {
    function("__elephc_dom_parse_html")
        .param("html", TypeExpr::Str)
        .param("flags", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_tokens", e_call("__elephc_dom_token_stream", vec![e_var("html"), e_var("flags")])),
            s_assign("_roots", e_call("__elephc_dom_build_forest", vec![e_var("_tokens"), e_var("flags")])),
            s_return(e_call("__elephc_dom_document_from_roots", vec![e_var("_roots"), e_var("flags")])),
        ])
        .build()
}

/// `__elephc_dom_document_from_roots` — transcribed from the PHP form.
fn decl_fn_elephc_dom_document_from_roots() -> Stmt {
    function("__elephc_dom_document_from_roots")
        .param("roots", t_mixed())
        .param("flags", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_document_children", e_array(vec![])),
            s_assign("_content", e_array(vec![])),
            s_assign("_trailing", e_array(vec![])),
            s_assign("_closed_html", e_bool(false)),
            s_assign("_leading", e_bool(true)),
            s_foreach(e_var("roots"), None, "_root", vec![
                s_if(
                    e_binop(e_binop(e_index(e_var("_root"), e_str("kind")), BinOp::StrictEq, e_str("text")), BinOp::And, e_binop(e_var("_leading"), BinOp::Or, e_var("_closed_html"))),
                    vec![
                        s_assign("_padding", e_cast(CastType::String, e_index(e_var("_root"), e_str("value")))),
                        s_if(
                            e_binop(e_call("trim", vec![e_var("_padding")]), BinOp::StrictEq, e_str("")),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("_leading"), BinOp::And, e_binop(e_binop(e_index(e_var("_root"), e_str("kind")), BinOp::StrictEq, e_str("comment")), BinOp::Or, e_binop(e_index(e_var("_root"), e_str("kind")), BinOp::StrictEq, e_str("pi")))),
                    vec![
                        s_array_push("_document_children", e_var("_root")),
                    ],
                    vec![
                    (e_binop(e_var("_closed_html"), BinOp::And, e_binop(e_binop(e_index(e_var("_root"), e_str("kind")), BinOp::StrictEq, e_str("comment")), BinOp::Or, e_binop(e_index(e_var("_root"), e_str("kind")), BinOp::StrictEq, e_str("pi")))), vec![
                        s_array_push("_trailing", e_var("_root")),
                    ]),
                ],
                    Some(vec![
                    s_assign("_leading", e_bool(false)),
                    s_array_push("_content", e_var("_root")),
                    s_assign("_closed_html", e_binop(e_binop(e_binop(e_call("count", vec![e_var("_content")]), BinOp::StrictEq, e_int(1)), BinOp::And, e_binop(e_index(e_var("_root"), e_str("kind")), BinOp::StrictEq, e_str("element"))), BinOp::And, e_binop(e_index(e_var("_root"), e_str("name")), BinOp::StrictEq, e_str("html")))),
                ]),
                ),
            ]),
            s_if(
                e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_const("LIBXML_NOBLANKS")), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_while(e_binop(e_call("count", vec![e_var("_content")]), BinOp::Gt, e_int(0)), vec![
                        s_assign("_last", e_index(e_var("_content"), e_binop(e_call("count", vec![e_var("_content")]), BinOp::Sub, e_int(1)))),
                        s_if(
                            e_binop(e_binop(e_index(e_var("_last"), e_str("kind")), BinOp::StrictNotEq, e_str("text")), BinOp::Or, e_binop(e_call("trim", vec![e_cast(CastType::String, e_index(e_var("_last"), e_str("value")))]), BinOp::StrictNotEq, e_str(""))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_call("array_pop", vec![e_var("_content")])),
                    ]),
                ],
                vec![],
                None,
            ),
            s_assign("_value", e_str("")),
            s_if(
                e_binop(e_call("count", vec![e_var("_content")]), BinOp::Gt, e_int(0)),
                vec![
                    s_assign("_html", e_call("__elephc_dom_wrap_html", vec![e_var("_content")])),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_const("LIBXML_NOBLANKS")), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_assign("_html_children", e_index(e_var("_html"), e_str("children"))),
                            s_assign("_html_children", e_call("__elephc_dom_filter_html_children", vec![e_var("_html_children")])),
                            s_array_assign("_html", e_str("children"), e_var("_html_children")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_value", e_call("__elephc_dom_text_of", vec![e_var("_html")])),
                    s_array_assign("_html", e_str("value"), e_var("_value")),
                    s_array_push("_document_children", e_var("_html")),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("_trailing"), None, "_node", vec![
                s_array_push("_document_children", e_var("_node")),
            ]),
            s_return(e_call("__elephc_dom_new_node", vec![e_str("document"), e_str("#document"), e_var("_value"), e_array(vec![]), e_var("_document_children")])),
        ])
        .build()
}

/// `__elephc_dom_escape` — transcribed from the PHP form.
fn decl_fn_elephc_dom_escape() -> Stmt {
    function("__elephc_dom_escape")
        .param("s", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_return(e_call("str_replace", vec![e_str("\r"), e_str("&#13;"), e_call("htmlspecialchars", vec![e_var("s"), e_const("ENT_NOQUOTES")])])),
        ])
        .build()
}

/// `__elephc_dom_escape_attr` — transcribed from the PHP form.
fn decl_fn_elephc_dom_escape_attr() -> Stmt {
    function("__elephc_dom_escape_attr")
        .param("s", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("_escaped", e_call("str_replace", vec![e_str("\r"), e_str("&#13;"), e_call("htmlspecialchars", vec![e_var("s"), e_const("ENT_COMPAT")])])),
            s_assign("_escaped", e_call("str_replace", vec![e_str("\n"), e_str("&#10;"), e_var("_escaped")])),
            s_assign("_escaped", e_call("str_replace", vec![e_str("\t"), e_str("&#9;"), e_var("_escaped")])),
            s_return(e_call("__elephc_dom_escape_attr_non_ascii", vec![e_var("_escaped")])),
        ])
        .build()
}

/// `__elephc_dom_escape_attr_non_ascii` — transcribed from the PHP form.
fn decl_fn_elephc_dom_escape_attr_non_ascii() -> Stmt {
    function("__elephc_dom_escape_attr_non_ascii")
        .param("s", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("_out", e_str("")),
            s_assign("_i", e_int(0)),
            s_assign("_length", e_call("strlen", vec![e_var("s")])),
            s_while(e_binop(e_var("_i"), BinOp::Lt, e_var("_length")), vec![
                s_assign("_first", e_call("__elephc_dom_entity_byte", vec![e_var("s"), e_var("_i")])),
                s_if(
                    e_binop(e_binop(e_var("_first"), BinOp::Lt, e_int(194)), BinOp::Or, e_binop(e_var("_first"), BinOp::Gt, e_int(244))),
                    vec![
                        s_assign("_out", e_binop(e_var("_out"), BinOp::Concat, e_call("substr", vec![e_var("s"), e_var("_i"), e_int(1)]))),
                        s_expr(e_post_inc("_i")),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_width", e_int(2)),
                s_assign("_mask", e_int(31)),
                s_if(
                    e_binop(e_var("_first"), BinOp::GtEq, e_int(240)),
                    vec![
                        s_assign("_width", e_int(4)),
                        s_assign("_mask", e_int(7)),
                    ],
                    vec![
                    (e_binop(e_var("_first"), BinOp::GtEq, e_int(224)), vec![
                        s_assign("_width", e_int(3)),
                        s_assign("_mask", e_int(15)),
                    ]),
                ],
                    None,
                ),
                s_assign("_code", e_binop(e_var("_first"), BinOp::BitAnd, e_var("_mask"))),
                s_assign("_valid", e_binop(e_binop(e_var("_i"), BinOp::Add, e_var("_width")), BinOp::LtEq, e_var("_length"))),
                s_for(Some(s_assign("_offset", e_int(1))), Some(e_binop(e_var("_offset"), BinOp::Lt, e_var("_width"))), Some(s_expr(e_post_inc("_offset"))), vec![
                    s_assign("_next", e_call("__elephc_dom_entity_byte", vec![e_var("s"), e_binop(e_var("_i"), BinOp::Add, e_var("_offset"))])),
                    s_if(
                        e_binop(e_binop(e_var("_next"), BinOp::Lt, e_int(128)), BinOp::Or, e_binop(e_var("_next"), BinOp::Gt, e_int(191))),
                        vec![
                            s_assign("_valid", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_code", e_binop(e_binop(e_var("_code"), BinOp::ShiftLeft, e_int(6)), BinOp::BitOr, e_binop(e_var("_next"), BinOp::BitAnd, e_int(63)))),
                ]),
                s_if(
                    e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_width"), BinOp::StrictEq, e_int(3)), BinOp::And, e_binop(e_var("_code"), BinOp::Lt, e_int(2048))), BinOp::Or, e_binop(e_binop(e_var("_width"), BinOp::StrictEq, e_int(4)), BinOp::And, e_binop(e_var("_code"), BinOp::Lt, e_int(65536)))), BinOp::Or, e_binop(e_binop(e_var("_code"), BinOp::GtEq, e_int(55296)), BinOp::And, e_binop(e_var("_code"), BinOp::LtEq, e_int(57343)))), BinOp::Or, e_binop(e_var("_code"), BinOp::Gt, e_int(1114111))),
                    vec![
                        s_assign("_valid", e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_not(e_var("_valid")),
                    vec![
                        s_assign("_out", e_binop(e_var("_out"), BinOp::Concat, e_call("substr", vec![e_var("s"), e_var("_i"), e_int(1)]))),
                        s_expr(e_post_inc("_i")),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("_out", e_binop(e_binop(e_binop(e_var("_out"), BinOp::Concat, e_str("&#x")), BinOp::Concat, e_call("strtoupper", vec![e_call("dechex", vec![e_var("_code")])])), BinOp::Concat, e_str(";"))),
                s_assign("_i", e_binop(e_var("_i"), BinOp::Add, e_var("_width"))),
            ]),
            s_return(e_var("_out")),
        ])
        .build()
}

/// `__elephc_dom_serialize_node` — transcribed from the PHP form.
fn decl_fn_elephc_dom_serialize_node() -> Stmt {
    function("__elephc_dom_serialize_node")
        .param("node", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("bound", e_null()),
            s_if(
                e_instance_of(e_var("node"), "DOMNode"),
                vec![
                    s_assign("bound", e_prop(e_var("node"), "__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_if(
                                e_binop(e_not(e_method_call(e_var("bound"), "__elephc_dom_is_html", vec![])), BinOp::Or, e_binop(e_binop(e_binop(e_binop(e_not(e_instance_of(e_var("node"), "DOMDocument")), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMElement"))), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMText"))), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMComment"))), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMProcessingInstruction")))),
                                vec![
                                    s_if(
                                        e_instance_of(e_var("node"), "DOMDocument"),
                                        vec![
                                            s_return(e_method_call(e_var("bound"), "__elephc_dom_document_xml", vec![e_bool(true)])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_xml", vec![])),
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
            s_if(
                e_instance_of(e_var("node"), "DOMText"),
                vec![
                    s_if(
                        e_prop(e_var("node"), "__elephc_raw_text"),
                        vec![
                            s_return(e_binop(e_binop(e_str("<![CDATA["), BinOp::Concat, e_call("str_replace", vec![e_str("]]>"), e_str("]]]]><![CDATA[>"), e_cast(CastType::String, e_prop(e_var("node"), "nodeValue"))])), BinOp::Concat, e_str("]]>"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_dom_escape", vec![e_cast(CastType::String, e_prop(e_var("node"), "nodeValue"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("node"), "DOMComment"),
                vec![
                    s_return(e_binop(e_binop(e_str("<!--"), BinOp::Concat, e_cast(CastType::String, e_prop(e_var("node"), "nodeValue"))), BinOp::Concat, e_str("-->"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("node"), "DOMProcessingInstruction"),
                vec![
                    s_assign("_separator", e_str("")),
                    s_if(
                        e_binop(e_cast(CastType::String, e_prop(e_var("node"), "nodeValue")), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("_separator", e_str(" ")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_binop(e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("node"), "nodeName")), BinOp::Concat, e_var("_separator")), BinOp::Concat, e_cast(CastType::String, e_prop(e_var("node"), "nodeValue"))), BinOp::Concat, e_str("?>"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("node"), "DOMDocument"),
                vec![
                    s_assign("_prefix", e_ternary(e_prop(e_var("node"), "__elephc_dom_html"), e_str(""), e_str("<?xml version=\"1.0\"?>\n"))),
                    s_assign("_out", e_str("")),
                    s_assign("_kids", e_prop(e_var("node"), "childNodes")),
                    s_if(
                        e_binop(e_var("_kids"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_foreach(e_var("_kids"), None, "_child", vec![
                                s_assign("_out", e_binop(e_var("_out"), BinOp::Concat, e_call("__elephc_dom_serialize_node", vec![e_var("_child")]))),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_var("_prefix"), BinOp::Concat, e_var("_out"))),
                ],
                vec![],
                None,
            ),
            s_assign("_out", e_binop(e_str("<"), BinOp::Concat, e_prop(e_var("node"), "nodeName"))),
            s_assign("_attrs", e_prop(e_var("node"), "__attrs")),
            s_if(
                e_call("is_object", vec![e_var("bound")]),
                vec![
                    s_foreach(e_method_call(e_var("bound"), "getDocNamespaces", vec![e_bool(false), e_bool(false)]), Some("_prefix"), "_uri", vec![
                        s_assign("_name", e_ternary(e_binop(e_var("_prefix"), BinOp::StrictEq, e_str("")), e_str("xmlns"), e_binop(e_str("xmlns:"), BinOp::Concat, e_var("_prefix")))),
                        s_assign("_out", e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_out"), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("_name")), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_call("__elephc_dom_escape_namespace", vec![e_cast(CastType::String, e_var("_uri"))])), BinOp::Concat, e_str("\""))),
                    ]),
                    s_assign("_attrs", e_method_call(e_var("bound"), "__elephc_dom_attributes", vec![])),
                ],
                vec![],
                Some(vec![
                s_assign("_xmlns", e_prop(e_var("node"), "__elephc_dom_xmlns")),
                s_if(
                    e_binop(e_var("_xmlns"), BinOp::StrictNotEq, e_null()),
                    vec![
                        s_assign("_out", e_binop(e_binop(e_binop(e_var("_out"), BinOp::Concat, e_str(" xmlns=\"")), BinOp::Concat, e_call("__elephc_dom_escape_namespace", vec![e_cast(CastType::String, e_var("_xmlns"))])), BinOp::Concat, e_str("\""))),
                    ],
                    vec![],
                    None,
                ),
            ]),
            ),
            s_if(
                e_call("is_array", vec![e_var("_attrs")]),
                vec![
                    s_foreach(e_var("_attrs"), None, "_pair", vec![
                        s_if(
                            e_call("is_array", vec![e_var("_pair")]),
                            vec![
                                s_assign("_out", e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_out"), BinOp::Concat, e_str(" ")), BinOp::Concat, e_cast(CastType::String, e_index(e_var("_pair"), e_int(0)))), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_call("__elephc_dom_escape_attr", vec![e_cast(CastType::String, e_index(e_var("_pair"), e_int(1)))])), BinOp::Concat, e_str("\""))),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                ],
                vec![],
                None,
            ),
            s_assign("_inner", e_str("")),
            s_if(
                e_binop(e_prop(e_var("node"), "childNodes"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_foreach(e_prop(e_var("node"), "childNodes"), None, "_child", vec![
                        s_assign("_inner", e_binop(e_var("_inner"), BinOp::Concat, e_call("__elephc_dom_serialize_node", vec![e_var("_child")]))),
                    ]),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_prop(e_var("node"), "childNodes"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_prop(e_prop(e_var("node"), "childNodes"), "length"), BinOp::StrictEq, e_int(0))),
                vec![
                    s_return(e_binop(e_var("_out"), BinOp::Concat, e_str("/>"))),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("_out"), BinOp::Concat, e_str(">")), BinOp::Concat, e_var("_inner")), BinOp::Concat, e_str("</")), BinOp::Concat, e_prop(e_var("node"), "nodeName")), BinOp::Concat, e_str(">"))),
        ])
        .build()
}

/// `__elephc_dom_link_siblings` — transcribed from the PHP form.
fn decl_fn_elephc_dom_link_siblings() -> Stmt {
    function("__elephc_dom_link_siblings")
        .param("left", t_class("DOMNode"))
        .param("right", t_class("DOMNode"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_var("left"), "nextSibling", e_var("right")),
            s_prop_assign(e_var("right"), "previousSibling", e_var("left")),
        ])
        .build()
}

/// `__elephc_dom_attach_parent` — transcribed from the PHP form.
fn decl_fn_elephc_dom_attach_parent() -> Stmt {
    function("__elephc_dom_attach_parent")
        .param("node", t_class("DOMNode"))
        .param("parent", t_class("DOMNode"))
        .param("doc", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_var("node"), "parentNode", e_var("parent")),
            s_prop_assign(e_var("node"), "ownerDocument", e_var("doc")),
        ])
        .build()
}

/// `__elephc_dom_link_prev` — transcribed from the PHP form.
fn decl_fn_elephc_dom_link_prev() -> Stmt {
    function("__elephc_dom_link_prev")
        .param("has_prev", TypeExpr::Int)
        .param("prev_kind", TypeExpr::Int)
        .param("prev_el", t_class("DOMElement"))
        .param("prev_text", t_class("DOMText"))
        .param("prev_comment", t_class("DOMComment"))
        .param("prev_pi", t_class("DOMProcessingInstruction"))
        .param("right", t_class("DOMNode"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("has_prev"), BinOp::StrictNotEq, e_int(1)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("prev_kind"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_expr(e_call("__elephc_dom_link_siblings", vec![e_var("prev_el"), e_var("right")])),
                ],
                vec![
                (e_binop(e_var("prev_kind"), BinOp::StrictEq, e_int(2)), vec![
                    s_expr(e_call("__elephc_dom_link_siblings", vec![e_var("prev_text"), e_var("right")])),
                ]),
                (e_binop(e_var("prev_kind"), BinOp::StrictEq, e_int(3)), vec![
                    s_expr(e_call("__elephc_dom_link_siblings", vec![e_var("prev_comment"), e_var("right")])),
                ]),
            ],
                Some(vec![
                s_expr(e_call("__elephc_dom_link_siblings", vec![e_var("prev_pi"), e_var("right")])),
            ]),
            ),
        ])
        .build()
}

/// `__elephc_dom_make_children` — transcribed from the PHP form.
fn decl_fn_elephc_dom_make_children() -> Stmt {
    function("__elephc_dom_make_children")
        .param("doc", t_class("DOMDocument"))
        .param("parent", t_class("DOMNode"))
        .param("trees", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_assign("_kids", e_array(vec![])),
            s_assign("_has_prev", e_int(0)),
            s_assign("_prev_kind", e_int(0)),
            s_assign("_prev_el", e_new("DOMElement", vec![])),
            s_assign("_prev_text", e_new("DOMText", vec![])),
            s_assign("_prev_comment", e_new("DOMComment", vec![])),
            s_assign("_prev_pi", e_new("DOMProcessingInstruction", vec![e_str("")])),
            s_foreach(e_var("trees"), None, "_child", vec![
                s_assign("_kid", e_call("__elephc_dom_make_node", vec![e_var("doc"), e_var("_child")])),
                s_if(
                    e_instance_of(e_var("_kid"), "DOMElement"),
                    vec![
                        s_assign("_el_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_el_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_el_kid")])),
                        s_array_push("_kids", e_var("_el_kid")),
                        s_assign("_prev_el", e_var("_el_kid")),
                        s_assign("_prev_kind", e_int(1)),
                        s_assign("_has_prev", e_int(1)),
                    ],
                    vec![
                    (e_instance_of(e_var("_kid"), "DOMText"), vec![
                        s_assign("_text_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_text_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_text_kid")])),
                        s_array_push("_kids", e_var("_text_kid")),
                        s_assign("_prev_text", e_var("_text_kid")),
                        s_assign("_prev_kind", e_int(2)),
                        s_assign("_has_prev", e_int(1)),
                    ]),
                    (e_instance_of(e_var("_kid"), "DOMComment"), vec![
                        s_assign("_comment_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_comment_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_comment_kid")])),
                        s_array_push("_kids", e_var("_comment_kid")),
                        s_assign("_prev_comment", e_var("_comment_kid")),
                        s_assign("_prev_kind", e_int(3)),
                        s_assign("_has_prev", e_int(1)),
                    ]),
                    (e_instance_of(e_var("_kid"), "DOMProcessingInstruction"), vec![
                        s_assign("_pi_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_pi_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_pi_kid")])),
                        s_array_push("_kids", e_var("_pi_kid")),
                        s_assign("_prev_pi", e_var("_pi_kid")),
                        s_assign("_prev_kind", e_int(4)),
                        s_assign("_has_prev", e_int(1)),
                    ]),
                ],
                    None,
                ),
            ]),
            s_return(e_var("_kids")),
        ])
        .build()
}

/// `__elephc_dom_make_node` — transcribed from the PHP form.
fn decl_fn_elephc_dom_make_node() -> Stmt {
    function("__elephc_dom_make_node")
        .param("doc", t_class("DOMDocument"))
        .param("tree", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_assign("_kind", e_cast(CastType::String, e_index(e_var("tree"), e_str("kind")))),
            s_if(
                e_binop(e_binop(e_var("_kind"), BinOp::StrictEq, e_str("text")), BinOp::Or, e_binop(e_var("_kind"), BinOp::StrictEq, e_str("cdata"))),
                vec![
                    s_assign("_text", e_new("DOMText", vec![])),
                    s_if(
                        e_binop(e_var("_kind"), BinOp::StrictEq, e_str("cdata")),
                        vec![
                            s_prop_assign(e_var("_text"), "nodeName", e_str("#cdata-section")),
                            s_prop_assign(e_var("_text"), "__elephc_raw_text", e_bool(true)),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_var("_text"), "nodeName", e_str("#text")),
                    ]),
                    ),
                    s_prop_assign(e_var("_text"), "nodeValue", e_cast(CastType::String, e_index(e_var("tree"), e_str("value")))),
                    s_prop_assign(e_var("_text"), "ownerDocument", e_var("doc")),
                    s_prop_assign(e_var("_text"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                    s_return(e_var("_text")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_kind"), BinOp::StrictEq, e_str("comment")),
                vec![
                    s_assign("_comment", e_new("DOMComment", vec![])),
                    s_prop_assign(e_var("_comment"), "nodeName", e_str("#comment")),
                    s_prop_assign(e_var("_comment"), "nodeValue", e_cast(CastType::String, e_index(e_var("tree"), e_str("value")))),
                    s_prop_assign(e_var("_comment"), "ownerDocument", e_var("doc")),
                    s_prop_assign(e_var("_comment"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                    s_return(e_var("_comment")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_kind"), BinOp::StrictEq, e_str("pi")),
                vec![
                    s_assign("_pi", e_new("DOMProcessingInstruction", vec![e_cast(CastType::String, e_index(e_var("tree"), e_str("name"))), e_cast(CastType::String, e_index(e_var("tree"), e_str("value")))])),
                    s_prop_assign(e_var("_pi"), "ownerDocument", e_var("doc")),
                    s_return(e_var("_pi")),
                ],
                vec![],
                None,
            ),
            s_assign("_el", e_new("DOMElement", vec![])),
            s_prop_assign(e_var("_el"), "nodeName", e_cast(CastType::String, e_index(e_var("tree"), e_str("name")))),
            s_prop_assign(e_var("_el"), "__attrs", e_index(e_var("tree"), e_str("attrs"))),
            s_prop_assign(e_var("_el"), "ownerDocument", e_var("doc")),
            s_assign("_kids", e_call("__elephc_dom_make_children", vec![e_var("doc"), e_var("_el"), e_index(e_var("tree"), e_str("children"))])),
            s_prop_assign(e_var("_el"), "childNodes", e_new("DOMNodeList", vec![e_var("_kids")])),
            s_prop_assign(e_var("_el"), "nodeValue", e_call("__elephc_dom_text_of", vec![e_var("tree")])),
            s_return(e_var("_el")),
        ])
        .build()
}

/// `__elephc_dom_collect` — transcribed from the PHP form.
fn decl_fn_elephc_dom_collect() -> Stmt {
    function("__elephc_dom_collect")
        .param("node", t_class("DOMNode"))
        .param("name", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_assign("_found", e_array(vec![])),
            s_assign("_children", e_prop(e_var("node"), "childNodes")),
            s_if(
                e_not(e_instance_of(e_var("_children"), "DOMNodeList")),
                vec![
                    s_return(e_var("_found")),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("_children"), None, "_child", vec![
                s_if(
                    e_instance_of(e_var("_child"), "DOMElement"),
                    vec![
                        s_assign("_bound", e_prop(e_var("_child"), "__elephc_simplexml")),
                        s_assign("_localName", e_ternary(e_call("is_object", vec![e_var("_bound")]), e_method_call(e_var("_bound"), "__elephc_dom_local_name", vec![]), e_prop(e_var("_child"), "nodeName"))),
                        s_if(
                            e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("*")), BinOp::Or, e_binop(e_var("_localName"), BinOp::StrictEq, e_var("name"))),
                            vec![
                                s_array_push("_found", e_var("_child")),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_more", e_call("__elephc_dom_collect", vec![e_var("_child"), e_var("name")])),
                        s_foreach(e_var("_more"), None, "_item", vec![
                            s_array_push("_found", e_var("_item")),
                        ]),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_var("_found")),
        ])
        .build()
}

/// `DOMNode` — transcribed from the PHP form.
fn decl_class_domnode() -> Stmt {
    class("DOMNode")
        .prop("nodeName", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_simplexml", t_mixed(), Some(e_null()))
        .prop("nodeValue", t_mixed(), Some(e_null()))
        .hooked(true, true, true)
        .untyped_prop("textContent", None)
        .hooked(true, true, false)
        .prop("childNodes", t_mixed(), Some(e_null()))
        .hooked(true, true, true)
        .prop("previousSibling", t_mixed(), Some(e_null()))
        .hooked(true, true, true)
        .prop("nextSibling", t_mixed(), Some(e_null()))
        .hooked(true, true, true)
        .prop("parentNode", t_mixed(), Some(e_null()))
        .hooked(true, true, true)
        .prop("ownerDocument", t_mixed(), Some(e_null()))
        .prop("__attrs", t_mixed(), Some(e_array(vec![])))
        .prop("__elephc_dom_xmlns", t_nullable(TypeExpr::Str), Some(e_null()))
        .method(
            method("__propget_nodeValue")
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_instance_of(e_this(), "DOMDocument"),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_return(e_method_call(e_var("bound"), "__elephc_dom_text", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("nodeValue")),
                ]),
        )
        .method(
            method("__propset_nodeValue")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("text", e_cast(CastType::String, e_var("value"))),
                    s_if(
                        e_instance_of(e_this(), "DOMDocument"),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_expr(e_method_call(e_var("bound"), "__elephc_dom_set_text", vec![e_var("text"), e_bool(true)])),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "nodeValue", e_var("text")),
                    ]),
                    ),
                ]),
        )
        .method(
            method("__propget_textContent")
                .body(vec![
                    s_if(
                        e_instance_of(e_this(), "DOMDocument"),
                        vec![
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_document_text", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("__elephc_dom_descendant_text", vec![e_this()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("nodeValue")),
                ]),
        )
        .method(
            method("__propset_textContent")
                .param_untyped("value")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("text", e_cast(CastType::String, e_var("value"))),
                    s_if(
                        e_instance_of(e_this(), "DOMDocument"),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_expr(e_method_call(e_var("bound"), "__elephc_dom_set_text", vec![e_var("text")])),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "nodeValue", e_var("text")),
                    ]),
                    ),
                ]),
        )
        .method(
            method("__propget_childNodes")
                .returns(t_mixed())
                .body(vec![
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_if(
                                e_instance_of(e_this(), "DOMDocument"),
                                vec![
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_children", vec![e_this(), e_this(), e_bool(true)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_method_call(e_var("bound"), "__elephc_dom_children", vec![e_this_prop("ownerDocument"), e_this()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("childNodes")),
                ]),
        )
        .method(
            method("__propset_childNodes")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "childNodes", e_ternary(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]), e_null(), e_var("value"))),
                ]),
        )
        .method(
            method("__propget_previousSibling")
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_instance_of(e_this(), "DOMDocument"),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_return(e_method_call(e_var("bound"), "__elephc_dom_relation", vec![e_this_prop("ownerDocument"), e_int(13)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("previousSibling")),
                ]),
        )
        .method(
            method("__propset_previousSibling")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "previousSibling", e_ternary(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]), e_null(), e_var("value"))),
                ]),
        )
        .method(
            method("__propget_nextSibling")
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_instance_of(e_this(), "DOMDocument"),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_return(e_method_call(e_var("bound"), "__elephc_dom_relation", vec![e_this_prop("ownerDocument"), e_int(14)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("nextSibling")),
                ]),
        )
        .method(
            method("__propset_nextSibling")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "nextSibling", e_ternary(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]), e_null(), e_var("value"))),
                ]),
        )
        .method(
            method("__propget_parentNode")
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_instance_of(e_this(), "DOMDocument"),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_return(e_method_call(e_var("bound"), "__elephc_dom_relation", vec![e_this_prop("ownerDocument"), e_int(12)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("parentNode")),
                ]),
        )
        .method(
            method("__propset_parentNode")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "parentNode", e_ternary(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]), e_null(), e_var("value"))),
                ]),
        )
        .method(
            method("getAttribute")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("nul", e_call("strpos", vec![e_var("name"), e_str("\0")])),
                    s_if(
                        e_binop(e_var("nul"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_assign("name", e_call("substr", vec![e_var("name"), e_int(0), e_var("nul")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_return(e_method_call(e_var("bound"), "__elephc_dom_attribute", vec![e_var("name")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("xmlns")),
                        vec![
                            s_return(e_null_coalesce(e_this_prop("__elephc_dom_xmlns"), e_str(""))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_starts_with", vec![e_var("name"), e_str("xmlns:")]),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_dom_attr_get", vec![e_this_prop("__attrs"), e_var("name")])),
                ]),
        )
        .method(
            method("setAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("DOMElement::setAttribute(): Argument #1 ($qualifiedName) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("nul", e_call("strpos", vec![e_var("qualifiedName"), e_str("\0")])),
                    s_if(
                        e_binop(e_var("nul"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_assign("qualifiedName", e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("nul")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("qualifiedName")])),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("nul", e_call("strpos", vec![e_var("value"), e_str("\0")])),
                    s_if(
                        e_binop(e_var("nul"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_assign("value", e_call("substr", vec![e_var("value"), e_int(0), e_var("nul")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_return(e_method_call(e_var("bound"), "__elephc_dom_set_attribute", vec![e_var("qualifiedName"), e_var("value")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")),
                        vec![
                            s_if(
                                e_binop(e_this_prop("__elephc_dom_xmlns"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "__elephc_dom_xmlns", e_var("value")),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("attrs", e_this_prop("__attrs")),
                    s_assign("updated", e_array(vec![])),
                    s_assign("found", e_bool(false)),
                    s_if(
                        e_call("is_array", vec![e_var("attrs")]),
                        vec![
                            s_foreach(e_var("attrs"), None, "pair", vec![
                                s_if(
                                    e_binop(e_call("is_array", vec![e_var("pair")]), BinOp::And, e_binop(e_cast(CastType::String, e_index(e_var("pair"), e_int(0))), BinOp::StrictEq, e_var("qualifiedName"))),
                                    vec![
                                        s_array_push("updated", e_array(vec![e_var("qualifiedName"), e_var("value")])),
                                        s_assign("found", e_bool(true)),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_array_push("updated", e_var("pair")),
                                ]),
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_var("found")),
                        vec![
                            s_array_push("updated", e_array(vec![e_var("qualifiedName"), e_var("value")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__attrs", e_var("updated")),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("appendChild")
                .param("node", t_class("DOMNode"))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_if(
                        e_instance_of(e_var("node"), "DOMDocument"),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("bound", e_this_prop("__elephc_simplexml")),
                    s_if(
                        e_call("is_object", vec![e_var("bound")]),
                        vec![
                            s_assign("appended", e_method_call(e_var("bound"), "__elephc_dom_append_child", vec![e_this(), e_var("node")])),
                            s_if(
                                e_instance_of(e_var("appended"), "DOMNode"),
                                vec![
                                    s_return(e_var("appended")),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("previous", e_prop(e_var("node"), "parentNode")),
                    s_if(
                        e_instance_of(e_var("previous"), "DOMNode"),
                        vec![
                            s_expr(e_method_call(e_var("previous"), "removeChild", vec![e_var("node")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("items", e_array(vec![])),
                    s_assign("children", e_this_prop("childNodes")),
                    s_if(
                        e_instance_of(e_var("children"), "DOMNodeList"),
                        vec![
                            s_foreach(e_var("children"), None, "child", vec![
                                s_if(
                                    e_instance_of(e_var("child"), "DOMNode"),
                                    vec![
                                        s_array_push("items", e_var("child")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_array_push("items", e_var("node")),
                    s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_var("items")])),
                    s_prop_assign(e_var("node"), "parentNode", e_this()),
                    s_prop_assign(e_var("node"), "ownerDocument", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                    s_return(e_var("node")),
                ]),
        )
        .method(
            method("append")
                .variadic("nodes", Some(t_union(vec![TypeExpr::Str, t_class("DOMNode")])))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_foreach(e_var("nodes"), None, "node", vec![
                        s_if(
                            e_instance_of(e_var("node"), "DOMNode"),
                            vec![
                                s_expr(e_method_call(e_this(), "appendChild", vec![e_var("node")])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("text", e_new("DOMText", vec![])),
                        s_prop_assign(e_var("text"), "nodeName", e_str("#text")),
                        s_prop_assign(e_var("text"), "nodeValue", e_cast(CastType::String, e_var("node"))),
                        s_prop_assign(e_var("text"), "ownerDocument", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                        s_prop_assign(e_var("text"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                        s_expr(e_method_call(e_this(), "appendChild", vec![e_var("text")])),
                    ]),
                ]),
        )
        .method(
            method("removeChild")
                .param("node", t_class("DOMNode"))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("items", e_array(vec![])),
                    s_assign("children", e_this_prop("childNodes")),
                    s_if(
                        e_instance_of(e_var("children"), "DOMNodeList"),
                        vec![
                            s_foreach(e_var("children"), None, "child", vec![
                                s_if(
                                    e_binop(e_instance_of(e_var("child"), "DOMNode"), BinOp::And, e_binop(e_var("child"), BinOp::StrictNotEq, e_var("node"))),
                                    vec![
                                        s_array_push("items", e_var("child")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_var("items")])),
                    s_prop_assign(e_var("node"), "parentNode", e_null()),
                    s_return(e_var("node")),
                ]),
        )
        .method(
            method("getElementsByTagName")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_return(e_new("DOMNodeList", vec![e_call("__elephc_dom_collect", vec![e_this(), e_var("qualifiedName")])])),
                ]),
        )
        .build()
}

/// `__ElephcDOMNodeListIterator` — transcribed from the PHP form.
fn decl_class_elephcdomnodelistiterator() -> Stmt {
    class("__ElephcDOMNodeListIterator")
        .implements("Iterator")
        .private_prop("__items", t_mixed(), Some(e_array(vec![])))
        .private_prop("__length", TypeExpr::Int, Some(e_int(0)))
        .private_prop("__i", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param("items", t_mixed())
                .param("length", TypeExpr::Int)
                .body(vec![
                    s_prop_assign(e_this(), "__items", e_var("items")),
                    s_prop_assign(e_this(), "__length", e_var("length")),
                ]),
        )
        .method(
            method("rewind")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "__i", e_int(0)),
                ]),
        )
        .method(
            method("valid")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_this_prop("__i"), BinOp::Lt, e_this_prop("__length"))),
                ]),
        )
        .method(
            method("key")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_this_prop("__i")),
                ]),
        )
        .method(
            method("next")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "__i", e_binop(e_this_prop("__i"), BinOp::Add, e_int(1))),
                ]),
        )
        .method(
            method("current")
                .returns(t_mixed())
                .body(vec![
                    s_assign("_items", e_this_prop("__items")),
                    s_if(
                        e_not(e_call("is_array", vec![e_var("_items")])),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_index(e_var("_items"), e_this_prop("__i"))),
                ]),
        )
        .build()
}

/// `DOMNodeList` — transcribed from the PHP form.
fn decl_class_domnodelist() -> Stmt {
    class("DOMNodeList")
        .implements("IteratorAggregate")
        .implements("Countable")
        .prop("length", TypeExpr::Int, Some(e_int(0)))
        .prop("__items", t_mixed(), Some(e_array(vec![])))
        .method(
            method("__construct")
                .param_default("items", t_mixed(), e_array(vec![]))
                .body(vec![
                    s_assign("_list", e_array(vec![])),
                    s_if(
                        e_call("is_array", vec![e_var("items")]),
                        vec![
                            s_assign("_list", e_var("items")),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__items", e_var("_list")),
                    s_prop_assign(e_this(), "length", e_call("count", vec![e_var("_list")])),
                ]),
        )
        .method(
            method("item")
                .param("index", TypeExpr::Int)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("index"), BinOp::GtEq, e_this_prop("length"))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_items", e_this_prop("__items")),
                    s_if(
                        e_not(e_call("is_array", vec![e_var("_items")])),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_node", e_index(e_var("_items"), e_var("index"))),
                    s_if(
                        e_instance_of(e_var("_node"), "DOMNode"),
                        vec![
                            s_return(e_var("_node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("getIterator")
                .returns(t_class("Iterator"))
                .body(vec![
                    s_return(e_new("__ElephcDOMNodeListIterator", vec![e_this_prop("__items"), e_this_prop("length")])),
                ]),
        )
        .method(
            method("count")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("length")),
                ]),
        )
        .build()
}

/// `DOMDocument` — transcribed from the PHP form.
fn decl_class_domdocument() -> Stmt {
    class("DOMDocument")
        .extends("DOMNode")
        .prop("__elephc_flags", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_dom_html", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_dom_nodes", t_array(), Some(e_array(vec![])))
        .prop("__elephc_dom_sweep", TypeExpr::Int, Some(e_int(64)))
        .prop("documentElement", t_mixed(), None)
        .hooked(true, false, false)
        .method(
            method("__propget_documentElement")
                .returns(t_mixed())
                .body(vec![
                    s_assign("children", e_this_prop("childNodes")),
                    s_if(
                        e_instance_of(e_var("children"), "DOMNodeList"),
                        vec![
                            s_foreach(e_var("children"), None, "child", vec![
                                s_if(
                                    e_instance_of(e_var("child"), "DOMElement"),
                                    vec![
                                        s_return(e_var("child")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("__construct")
                .param_default("version", TypeExpr::Str, e_str("1.0"))
                .param_default("encoding", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_assign("_unused_version", e_var("version")),
                    s_assign("_unused_encoding", e_var("encoding")),
                    s_prop_assign(e_this(), "nodeName", e_str("#document")),
                    s_prop_assign(e_this(), "nodeValue", e_null()),
                    s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                    s_prop_assign(e_this(), "ownerDocument", e_null()),
                ]),
        )
        .method(
            method("createElement")
                .param("localName", TypeExpr::Str)
                .param_default("value", TypeExpr::Str, e_str(""))
                .returns(t_class("DOMElement"))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("localName"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("localName")]))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("element", e_new("DOMElement", vec![])),
                    s_prop_assign(e_var("element"), "nodeName", e_var("localName")),
                    s_prop_assign(e_var("element"), "ownerDocument", e_this()),
                    s_prop_assign(e_var("element"), "__attrs", e_array(vec![])),
                    s_prop_assign(e_var("element"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                    s_if(
                        e_binop(e_var("value"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("text", e_new("DOMText", vec![])),
                            s_prop_assign(e_var("text"), "nodeName", e_str("#text")),
                            s_prop_assign(e_var("text"), "nodeValue", e_var("value")),
                            s_prop_assign(e_var("text"), "ownerDocument", e_this()),
                            s_prop_assign(e_var("text"), "parentNode", e_var("element")),
                            s_prop_assign(e_var("text"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                            s_prop_assign(e_var("element"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![e_var("text")])])),
                            s_prop_assign(e_var("element"), "nodeValue", e_var("value")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("element")),
                ]),
        )
        .method(
            method("loadHTML")
                .param("source", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "__elephc_flags", e_var("options")),
                    s_prop_assign(e_this(), "__elephc_dom_html", e_bool(true)),
                    s_assign("_tree", e_call("__elephc_dom_parse_html", vec![e_var("source"), e_var("options")])),
                    s_prop_assign(e_this(), "__elephc_simplexml", e_null()),
                    s_prop_assign(e_this(), "nodeName", e_str("#document")),
                    s_prop_assign(e_this(), "ownerDocument", e_null()),
                    s_assign("_kids", e_call("__elephc_dom_make_children", vec![e_this(), e_this(), e_index(e_var("_tree"), e_str("children"))])),
                    s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_var("_kids")])),
                    s_prop_assign(e_this(), "nodeValue", e_null()),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("load")
                .param("filename", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("source", e_error_suppress(e_call("file_get_contents", vec![e_var("filename")]))),
                    s_if(
                        e_binop(e_var("source"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "loadXML", vec![e_var("source"), e_var("options")])),
                ]),
        )
        .method(
            method("loadXML")
                .param("source", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("source"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("DOMDocument::loadXML(): Argument #1 ($source) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__elephc_dom_html", e_bool(false)),
                    s_return(e_expr_call(e_str("__elephc_simplexml_load_dom"), vec![e_this(), e_var("source"), e_var("options")])),
                ]),
        )
        .method(
            method("saveXML")
                .param_default("node", t_mixed(), e_null())
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("_unused_options", e_var("options")),
                    s_assign("_unused_flags", e_this_prop("__elephc_flags")),
                    s_if(
                        e_binop(e_var("node"), BinOp::StrictEq, e_null()),
                        vec![
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_binop(e_call("is_object", vec![e_var("bound")]), BinOp::And, e_not(e_method_call(e_var("bound"), "__elephc_dom_is_html", vec![]))),
                                vec![
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_document_xml", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("__elephc_dom_serialize_node", vec![e_this()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_dom_serialize_node", vec![e_var("node")])),
                ]),
        )
        .build()
}

/// `DOMElement` — transcribed from the PHP form.
fn decl_class_domelement() -> Stmt {
    class("DOMElement")
        .extends("DOMNode")
        .build()
}

/// `DOMCharacterData` — transcribed from the PHP form.
fn decl_class_domcharacterdata() -> Stmt {
    class("DOMCharacterData")
        .extends("DOMNode")
        .build()
}

/// `DOMText` — transcribed from the PHP form.
fn decl_class_domtext() -> Stmt {
    class("DOMText")
        .extends("DOMCharacterData")
        .prop("__elephc_raw_text", TypeExpr::Bool, Some(e_bool(false)))
        .build()
}

/// `DOMComment` — transcribed from the PHP form.
fn decl_class_domcomment() -> Stmt {
    class("DOMComment")
        .extends("DOMCharacterData")
        .build()
}

/// `DOMProcessingInstruction` — transcribed from the PHP form.
fn decl_class_domprocessinginstruction() -> Stmt {
    class("DOMProcessingInstruction")
        .extends("DOMNode")
        .prop("target", TypeExpr::Str, Some(e_str("")))
        .prop("data", TypeExpr::Str, None)
        .hooked(true, true, false)
        .method(
            method("__propget_data")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_this_prop("nodeValue"))),
                ]),
        )
        .method(
            method("__propset_data")
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "nodeValue", e_var("value")),
                ]),
        )
        .method(
            method("__construct")
                .param("target", TypeExpr::Str)
                .param_default("data", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_prop_assign(e_this(), "target", e_var("target")),
                    s_prop_assign(e_this(), "data", e_var("data")),
                    s_prop_assign(e_this(), "nodeName", e_var("target")),
                    s_prop_assign(e_this(), "nodeValue", e_var("data")),
                    s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                ]),
        )
        .build()
}

/// Builds this module's declarations, one helper per declaration above.
pub(super) fn declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_fn_elephc_dom_char(),
            decl_fn_elephc_dom_is_ws(),
            decl_fn_elephc_dom_is_letter(),
            decl_fn_elephc_dom_starts_markup(),
            decl_fn_elephc_dom_is_boolean_attr(),
            decl_fn_elephc_dom_is_name(),
            decl_fn_elephc_dom_is_void(),
            decl_fn_elephc_dom_is_head_element(),
            decl_fn_elephc_dom_is_frame_element(),
            decl_fn_elephc_dom_skip_ws(),
            decl_fn_elephc_dom_read_name(),
            decl_fn_elephc_dom_read_attr_value(),
            decl_fn_elephc_dom_read_attrs(),
            decl_fn_elephc_dom_has_attr(),
            decl_fn_elephc_dom_attr_get(),
            decl_fn_elephc_dom_append_text_token(),
            decl_fn_elephc_dom_token_stream(),
            decl_fn_elephc_dom_text_of(),
            decl_fn_elephc_dom_new_node(),
            decl_fn_elephc_dom_append_child(),
            decl_fn_elephc_dom_build_forest(),
            decl_fn_elephc_dom_find_child(),
            decl_fn_elephc_dom_wrap_html(),
            decl_fn_elephc_dom_filter_html_children(),
            decl_fn_elephc_dom_parse_html(),
            decl_fn_elephc_dom_document_from_roots(),
            decl_fn_elephc_dom_escape(),
            decl_fn_elephc_dom_escape_attr(),
            decl_fn_elephc_dom_escape_attr_non_ascii(),
            decl_fn_elephc_dom_serialize_node(),
            decl_fn_elephc_dom_link_siblings(),
            decl_fn_elephc_dom_attach_parent(),
            decl_fn_elephc_dom_link_prev(),
            decl_fn_elephc_dom_make_children(),
            decl_fn_elephc_dom_make_node(),
            decl_fn_elephc_dom_collect(),
            decl_class_domnode(),
            decl_class_elephcdomnodelistiterator(),
            decl_class_domnodelist(),
            decl_class_domdocument(),
            decl_class_domelement(),
            decl_class_domcharacterdata(),
            decl_class_domtext(),
            decl_class_domcomment(),
            decl_class_domprocessinginstruction(),
        ]
    })
}
