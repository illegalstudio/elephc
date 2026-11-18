//! Purpose:
//! Rust AST builders for the Termwind-facing DOM base: `DOMDocument` / `DOMNode` / `DOMElement` /
//! `DOMText` / `DOMComment` / `DOMProcessingInstruction` / `DOMNodeList` and the serializer
//! helpers. The HTML4 parser `loadHTML` runs lives in `html_parser` (`Part::Html`).
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
        .hooked(true, true, true)
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
                    s_if(
                        e_instance_of(e_this(), "DOMElement"),
                        vec![
                            s_return(e_call("__elephc_dom_descendant_text", vec![e_this()])),
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
                            s_if(
                                e_binop(e_binop(e_var("text"), BinOp::StrictEq, e_str("")), BinOp::And, e_instance_of(e_this(), "DOMElement")),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_dom_replace_text", vec![e_var("text")])),
                                ],
                                vec![],
                                Some(vec![
                                    s_expr(e_method_call(e_var("bound"), "__elephc_dom_set_text", vec![e_var("text"), e_bool(true)])),
                                ]),
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_instance_of(e_this(), "DOMElement"), BinOp::Or, e_instance_of(e_this(), "DOMDocumentFragment")),
                            vec![
                                s_expr(e_method_call(e_this(), "__elephc_dom_replace_text", vec![e_var("text")])),
                            ],
                            vec![],
                            Some(vec![
                                s_prop_assign(e_this(), "nodeValue", e_var("text")),
                            ]),
                        ),
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
                    s_if(
                        e_instance_of(e_this(), "DOMDocumentFragment"),
                        vec![
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
                            s_if(
                                e_binop(e_binop(e_var("text"), BinOp::StrictEq, e_str("")), BinOp::And, e_instance_of(e_this(), "DOMElement")),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_dom_replace_text", vec![e_var("text")])),
                                ],
                                vec![],
                                Some(vec![
                                    s_expr(e_method_call(e_var("bound"), "__elephc_dom_set_text", vec![e_var("text")])),
                                ]),
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_instance_of(e_this(), "DOMElement"), BinOp::Or, e_instance_of(e_this(), "DOMDocumentFragment")),
                            vec![
                                s_expr(e_method_call(e_this(), "__elephc_dom_replace_text", vec![e_var("text")])),
                            ],
                            vec![],
                            Some(vec![
                                s_prop_assign(e_this(), "nodeValue", e_var("text")),
                            ]),
                        ),
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
                    s_assign("_raw", e_this_prop("previousSibling")),
                    s_if(e_instance_of(e_var("_raw"), "WeakReference"), vec![s_return(e_method_call(e_var("_raw"), "get", vec![]))], vec![], None),
                    s_return(e_var("_raw")),
                ]),
        )
        .method(
            method("__propset_previousSibling")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "previousSibling", e_ternary(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]), e_null(), e_ternary(e_instance_of(e_var("value"), "DOMNode"), e_static_call("WeakReference", "create", vec![e_var("value")]), e_var("value")))),
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
                    s_assign("_raw", e_this_prop("nextSibling")),
                    s_if(e_instance_of(e_var("_raw"), "WeakReference"), vec![s_return(e_method_call(e_var("_raw"), "get", vec![]))], vec![], None),
                    s_return(e_var("_raw")),
                ]),
        )
        .method(
            method("__propset_nextSibling")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "nextSibling", e_ternary(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]), e_null(), e_ternary(e_instance_of(e_var("value"), "DOMNode"), e_static_call("WeakReference", "create", vec![e_var("value")]), e_var("value")))),
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
                    s_assign("_raw", e_this_prop("parentNode")),
                    s_if(e_instance_of(e_var("_raw"), "WeakReference"), vec![s_return(e_method_call(e_var("_raw"), "get", vec![]))], vec![], None),
                    s_return(e_var("_raw")),
                ]),
        )
        .method(
            method("__propset_parentNode")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "parentNode", e_ternary(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]), e_null(), e_ternary(e_instance_of(e_var("value"), "DOMNode"), e_static_call("WeakReference", "create", vec![e_var("value")]), e_var("value")))),
                ]),
        )
        .method(
            method("__propget_ownerDocument")
                .returns(t_mixed())
                .body(vec![
                    s_assign("_raw", e_this_prop("ownerDocument")),
                    s_if(e_instance_of(e_var("_raw"), "WeakReference"), vec![s_return(e_method_call(e_var("_raw"), "get", vec![]))], vec![], None),
                    s_return(e_var("_raw")),
                ]),
        )
        .method(
            // A node attached to a tree holds its document weakly: the document owns the tree, so a
            // strong link back would be a reference cycle that is never freed.
            method("__propset_ownerDocument")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_weak", e_binop(e_binop(e_instance_of(e_var("value"), "DOMDocument"), BinOp::And, e_not(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]))), BinOp::And, e_binop(e_this_prop("parentNode"), BinOp::StrictNotEq, e_null()))),
                    s_prop_assign(e_this(), "ownerDocument", e_ternary(e_var("_weak"), e_static_call("WeakReference", "create", vec![e_var("value")]), e_var("value"))),
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
                    s_prop_assign(e_this(), "__elephc_dom_decl", e_array_assoc(vec![(e_str("version"), e_var("version")), (e_str("encoding"), e_ternary(e_binop(e_var("encoding"), BinOp::StrictEq, e_str("")), e_null(), e_var("encoding"))), (e_str("standalone"), e_bool(false))])),
                    s_prop_assign(e_this(), "__elephc_dom_version", e_var("version")),
                    s_prop_assign(e_this(), "__elephc_dom_encoding", e_ternary(e_binop(e_var("encoding"), BinOp::StrictEq, e_str("")), e_null(), e_var("encoding"))),
                    s_prop_assign(e_this(), "nodeName", e_str("#document")),
                    s_prop_assign(e_this(), "nodeValue", e_null()),
                    s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                    s_prop_assign(e_this(), "ownerDocument", e_null()),
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
                    s_assign("loaded", e_method_call(e_this(), "loadXML", vec![e_var("source"), e_var("options")])),
                    s_if(
                        e_var("loaded"),
                        vec![
                            s_assign("real", e_call("realpath", vec![e_var("filename")])),
                            s_prop_assign(e_this(), "__elephc_dom_uri", e_ternary(e_binop(e_var("real"), BinOp::StrictEq, e_bool(false)), e_var("filename"), e_var("real"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("loaded")),
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
                    s_prop_assign(e_this(), "__elephc_dom_decl", e_null()),
                    s_prop_assign(e_this(), "__elephc_dom_uri", e_null()),
                    // php maps the document's parse switches onto libxml2 options.
                    s_if(e_not(e_this_prop("preserveWhiteSpace")), vec![s_assign("options", e_binop(e_var("options"), BinOp::BitOr, e_int(256)))], vec![], None),
                    s_if(e_this_prop("recover"), vec![s_assign("options", e_binop(e_var("options"), BinOp::BitOr, e_int(1)))], vec![], None),
                    s_if(e_this_prop("substituteEntities"), vec![s_assign("options", e_binop(e_var("options"), BinOp::BitOr, e_int(2)))], vec![], None),
                    s_return(e_expr_call(e_str("__elephc_simplexml_load_dom"), vec![e_this(), e_var("source"), e_var("options")])),
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
            decl_fn_elephc_dom_attr_get(),
            decl_fn_elephc_dom_escape(),
            decl_fn_elephc_dom_escape_attr(),
            decl_fn_elephc_dom_escape_attr_non_ascii(),
            decl_fn_elephc_dom_serialize_node(),
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
