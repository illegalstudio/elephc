//! Purpose:
//! The eager-tree helpers `loadHTML` uses to turn the parser's tree arrays into `DOMElement`,
//! `DOMText`, `DOMComment`, `DOMProcessingInstruction` and `DOMDocumentType` objects.
//!
//! Called from:
//! - `super::contribute`, which registers them under `Part::Html`.
//!
//! Key details:
//! - Moved out of `generated.rs` so a program that never loads HTML does not carry them.
//! - A tree node is `["kind", "name", "value", "attrs", "children"]` (see `__elephc_dom_new_node`);
//!   `attrs` is a list of `[name, value]` pairs (a valueless HTML attribute adds a third `true`).
//!   A `doctype` node keeps its public id in `value` and its system id in `attrs[0]`.
//! - Sibling pointers are assigned through typed `DOMNode` parameters on `instanceof`-narrowed
//!   locals, then those locals are stored: writes on `mixed` array elements copy the object and
//!   drop `nextSibling`.

use crate::parser::ast::*;
use crate::synthetic_class::*;

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
        .param("prev_doctype", t_class("DOMDocumentType"))
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
                (e_binop(e_var("prev_kind"), BinOp::StrictEq, e_int(4)), vec![
                    s_expr(e_call("__elephc_dom_link_siblings", vec![e_var("prev_pi"), e_var("right")])),
                ]),
            ],
                Some(vec![
                s_expr(e_call("__elephc_dom_link_siblings", vec![e_var("prev_doctype"), e_var("right")])),
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
            s_assign("_prev_doctype", e_new("DOMDocumentType", vec![e_str("")])),
            s_foreach(e_var("trees"), None, "_child", vec![
                s_assign("_kid", e_call("__elephc_dom_make_node", vec![e_var("doc"), e_var("_child")])),
                s_if(
                    e_instance_of(e_var("_kid"), "DOMElement"),
                    vec![
                        s_assign("_el_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_el_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_prev_doctype"), e_var("_el_kid")])),
                        s_array_push("_kids", e_var("_el_kid")),
                        s_assign("_prev_el", e_var("_el_kid")),
                        s_assign("_prev_kind", e_int(1)),
                        s_assign("_has_prev", e_int(1)),
                    ],
                    vec![
                    (e_instance_of(e_var("_kid"), "DOMText"), vec![
                        s_assign("_text_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_text_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_prev_doctype"), e_var("_text_kid")])),
                        s_array_push("_kids", e_var("_text_kid")),
                        s_assign("_prev_text", e_var("_text_kid")),
                        s_assign("_prev_kind", e_int(2)),
                        s_assign("_has_prev", e_int(1)),
                    ]),
                    (e_instance_of(e_var("_kid"), "DOMComment"), vec![
                        s_assign("_comment_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_comment_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_prev_doctype"), e_var("_comment_kid")])),
                        s_array_push("_kids", e_var("_comment_kid")),
                        s_assign("_prev_comment", e_var("_comment_kid")),
                        s_assign("_prev_kind", e_int(3)),
                        s_assign("_has_prev", e_int(1)),
                    ]),
                    (e_instance_of(e_var("_kid"), "DOMProcessingInstruction"), vec![
                        s_assign("_pi_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_pi_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_prev_doctype"), e_var("_pi_kid")])),
                        s_array_push("_kids", e_var("_pi_kid")),
                        s_assign("_prev_pi", e_var("_pi_kid")),
                        s_assign("_prev_kind", e_int(4)),
                        s_assign("_has_prev", e_int(1)),
                    ]),
                    (e_instance_of(e_var("_kid"), "DOMDocumentType"), vec![
                        s_assign("_dtd_kid", e_var("_kid")),
                        s_expr(e_call("__elephc_dom_attach_parent", vec![e_var("_dtd_kid"), e_var("parent"), e_var("doc")])),
                        s_expr(e_call("__elephc_dom_link_prev", vec![e_var("_has_prev"), e_var("_prev_kind"), e_var("_prev_el"), e_var("_prev_text"), e_var("_prev_comment"), e_var("_prev_pi"), e_var("_prev_doctype"), e_var("_dtd_kid")])),
                        s_array_push("_kids", e_var("_dtd_kid")),
                        s_assign("_prev_doctype", e_var("_dtd_kid")),
                        s_assign("_prev_kind", e_int(5)),
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
                e_binop(e_var("_kind"), BinOp::StrictEq, e_str("cdata")),
                vec![
                    s_assign("_cdata", e_new("DOMCdataSection", vec![e_cast(CastType::String, e_index(e_var("tree"), e_str("value")))])),
                    s_prop_assign(e_var("_cdata"), "ownerDocument", e_var("doc")),
                    s_return(e_var("_cdata")),
                ],
                vec![],
                None,
            ),
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
            s_if(
                e_binop(e_var("_kind"), BinOp::StrictEq, e_str("doctype")),
                vec![
                    s_assign("_dtd", e_new("DOMDocumentType", vec![e_cast(CastType::String, e_index(e_var("tree"), e_str("name"))), e_cast(CastType::String, e_index(e_var("tree"), e_str("value"))), e_cast(CastType::String, e_index(e_index(e_var("tree"), e_str("attrs")), e_int(0)))])),
                    s_prop_assign(e_var("_dtd"), "ownerDocument", e_var("doc")),
                    s_return(e_var("_dtd")),
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
            s_return(e_var("_el")),
        ])
        .build()
}

/// The helper functions, in declaration order.
pub(super) fn declarations() -> Vec<Stmt> {
    vec![
        decl_fn_elephc_dom_text_of(),
        decl_fn_elephc_dom_new_node(),
        decl_fn_elephc_dom_link_siblings(),
        decl_fn_elephc_dom_attach_parent(),
        decl_fn_elephc_dom_link_prev(),
        decl_fn_elephc_dom_make_children(),
        decl_fn_elephc_dom_make_node(),
    ]
}
