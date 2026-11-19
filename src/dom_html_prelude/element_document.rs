//! Purpose:
//! Package P5 of the DOM migration: `DOMElement` attributes and traversal, the ParentNode /
//! ChildNode helpers, `getElementsByTagNameNS`, ID attributes, the `DOMDocument` `create*` family,
//! `importNode` / `adoptNode`, the document properties, `cloneNode`, `DOMImplementation` and
//! `DOMEntityReference`.
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - A node bound to the shared libxml2 tree reads and writes its attributes through
//!   `SimpleXMLElement::__elephc_dom_element_op` (`crates/elephc-xml/src/simplexml/dom_element.rs`);
//!   a free-standing element keeps `[qualified name, value]` pairs in `__attrs`, with the
//!   namespace URI as a third slot for a namespaced attribute, and its own namespace in
//!   `__elephc_dom_ns` (`__elephc_dom_xmlns` for a default declaration).
//! - Attribute nodes (`DOMAttr`) are snapshots built on demand (`__elephc_dom_make_attr`): setting
//!   `value` writes through to the owner element, `isId()` asks the element, but two lookups of the
//!   same attribute are two objects.
//! - The method bodies were written as PHP and converted with `synthetic_class::transcribe`.
//! - A bound document's version / encoding / standalone are read once from the declaration line of
//!   its native serialization (`__elephc_dom_declaration`).

use super::extend::{Contributions, Part};
use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::synthetic_class::*;

fn node_members() -> ClassBuilder {
            class("DOMNode")
                .method(
                    method("__propget_attributes")
                        .returns(t_nullable(t_class("DOMNamedNodeMap")))
                        .body(vec![
                            s_if(
                                e_not(e_instance_of(e_this(), "DOMElement")),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("items", e_array(vec![])),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_attr_records", vec![e_bool(false)]), None, "rec", vec![
                                s_assign("attr", e_method_call(e_this(), "__elephc_dom_make_attr", vec![e_var("rec")])),
                                s_assign("name", e_index(e_var("rec"), e_int(2))),
                                s_assign("key", e_var("name")),
                                s_assign("n", e_int(1)),
                                s_while(e_call("isset", vec![e_index(e_var("items"), e_var("key"))]), vec![
                                    s_assign("key", e_binop(e_var("name"), BinOp::Concat, e_var("n"))),
                                    s_assign("n", e_binop(e_var("n"), BinOp::Add, e_int(1))),
                                ]),
                                s_array_assign("items", e_var("key"), e_var("attr")),
                            ]),
                            s_return(e_new("DOMNamedNodeMap", vec![e_var("items")])),
                        ]),
                )
                .method(
                    method("__elephc_dom_attr_records")
                        .param("declarations", TypeExpr::Bool)
                        .returns(t_array())
                        .body(vec![
                            s_assign("out", e_array(vec![])),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_assign("answer", e_method_call(e_var("bound"), "__elephc_dom_element_op", vec![e_int(6), e_str(""), e_str(""), e_ternary(e_var("declarations"), e_str("1"), e_str(""))])),
                                    s_if(
                                        e_binop(e_call("strlen", vec![e_var("answer")]), BinOp::Lt, e_int(2)),
                                        vec![
                                            s_return(e_var("out")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("raw", e_call("hex2bin", vec![e_call("substr", vec![e_var("answer"), e_int(1)])])),
                                    s_if(
                                        e_binop(e_binop(e_var("raw"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("raw"), BinOp::StrictEq, e_bool(false))),
                                        vec![
                                            s_return(e_var("out")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_foreach(e_call("explode", vec![e_str("\u{1}"), e_var("raw")]), None, "record", vec![
                                        s_array_push("out", e_call("explode", vec![e_str("\0"), e_var("record")])),
                                    ]),
                                    s_return(e_var("out")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("declarations"), BinOp::And, e_binop(e_this_prop("__elephc_dom_xmlns"), BinOp::StrictNotEq, e_null())),
                                vec![
                                    s_array_push("out", e_array(vec![e_str("n"), e_str("xmlns"), e_str("xmlns"), e_str(""), e_str("http://www.w3.org/2000/xmlns/"), e_this_prop("__elephc_dom_xmlns")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_this_prop("__attrs"), None, "pair", vec![
                                s_assign("name", e_cast(CastType::String, e_index(e_var("pair"), e_int(0)))),
                                s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                                s_assign("local", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_var("name"), e_call("substr", vec![e_var("name"), e_binop(e_var("colon"), BinOp::Add, e_int(1))]))),
                                s_assign("prefix", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_str(""), e_call("substr", vec![e_var("name"), e_int(0), e_var("colon")]))),
                                s_assign("uri", e_ternary(e_binop(e_call("array_key_exists", vec![e_int(2), e_var("pair")]), BinOp::And, e_call("is_string", vec![e_index(e_var("pair"), e_int(2))])), e_cast(CastType::String, e_index(e_var("pair"), e_int(2))), e_str(""))),
                                s_array_push("out", e_array(vec![e_str("a"), e_var("name"), e_var("local"), e_var("prefix"), e_var("uri"), e_cast(CastType::String, e_index(e_var("pair"), e_int(1)))])),
                            ]),
                            s_return(e_var("out")),
                        ]),
                )
                .method(
                    method("__elephc_dom_make_attr")
                        .param("record", t_array())
                        .returns(t_class("DOMAttr"))
                        .body(vec![
                            s_assign("attr", e_new("DOMAttr", vec![e_index(e_var("record"), e_int(1)), e_index(e_var("record"), e_int(5))])),
                            s_prop_assign(e_var("attr"), "name", e_index(e_var("record"), e_int(2))),
                            s_if(
                                e_binop(e_index(e_var("record"), e_int(4)), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_prop_assign(e_var("attr"), "__elephc_dom_ns", e_index(e_var("record"), e_int(4))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMElement"),
                                vec![
                                    s_prop_assign(e_var("attr"), "ownerElement", e_this()),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_var("attr"), "parentNode", e_this()),
                            s_assign("doc", e_this_prop("ownerDocument")),
                            s_if(
                                e_instance_of(e_var("doc"), "DOMDocument"),
                                vec![
                                    s_prop_assign(e_var("attr"), "ownerDocument", e_var("doc")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("attr")),
                        ]),
                )
                .method(
                    method("__propget_baseURI").returns(t_mixed())
                        .body(vec![
                            s_assign("doc", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                            s_if(
                                e_instance_of(e_var("doc"), "DOMDocument"),
                                vec![
                                    s_return(e_prop(e_var("doc"), "documentURI")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("hasAttributes")
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_if(
                                e_not(e_instance_of(e_this(), "DOMElement")),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_return(e_binop(e_call("count", vec![e_method_call(e_this(), "__elephc_dom_attr_records", vec![e_bool(false)])]), BinOp::Gt, e_int(0))),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_binop(e_call("count", vec![e_this_prop("__attrs")]), BinOp::Gt, e_int(0))),
                        ]),
                )
                .method(
                    method("__elephc_dom_text_args")
                        .param("nodes", t_array())
                        .returns(t_array())
                        .body(vec![
                            s_assign("doc", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                            s_assign("out", e_array(vec![])),
                            s_foreach(e_var("nodes"), None, "node", vec![
                                s_if(
                                    e_instance_of(e_var("node"), "DOMNode"),
                                    vec![
                                        s_array_push("out", e_var("node")),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("text", e_new("DOMText", vec![e_cast(CastType::String, e_var("node"))])),
                                    s_if(
                                        e_instance_of(e_var("doc"), "DOMDocument"),
                                        vec![
                                            s_prop_assign(e_var("text"), "ownerDocument", e_var("doc")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_array_push("out", e_var("text")),
                                ]),
                                ),
                            ]),
                            s_return(e_var("out")),
                        ]),
                )
                .method(
                    method("prepend")
                        .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("first", e_this_prop("firstChild")),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_text_args", vec![e_var("nodes")]), None, "node", vec![
                                s_expr(e_method_call(e_this(), "insertBefore", vec![e_var("node"), e_var("first")])),
                            ]),
                        ]),
                )
                .method(
                    method("replaceChildren")
                        .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("items", e_method_call(e_this(), "__elephc_dom_text_args", vec![e_var("nodes")])),
                            s_assign("old", e_array(vec![])),
                            s_foreach(e_this_prop("childNodes"), None, "child", vec![
                                s_array_push("old", e_var("child")),
                            ]),
                            s_foreach(e_var("old"), None, "child", vec![
                                s_expr(e_method_call(e_this(), "removeChild", vec![e_var("child")])),
                            ]),
                            s_foreach(e_var("items"), None, "node", vec![
                                s_expr(e_method_call(e_this(), "appendChild", vec![e_var("node")])),
                            ]),
                        ]),
                )
                .method(
                    method("before")
                        .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("parent", e_this_prop("parentNode")),
                            s_if(
                                e_not(e_instance_of(e_var("parent"), "DOMNode")),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_text_args", vec![e_var("nodes")]), None, "node", vec![
                                s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_var("node"), e_this()])),
                            ]),
                        ]),
                )
                .method(
                    method("after")
                        .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("parent", e_this_prop("parentNode")),
                            s_if(
                                e_not(e_instance_of(e_var("parent"), "DOMNode")),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("anchor", e_this_prop("nextSibling")),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_text_args", vec![e_var("nodes")]), None, "node", vec![
                                s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_var("node"), e_var("anchor")])),
                            ]),
                        ]),
                )
                .method(
                    method("replaceWith")
                        .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("parent", e_this_prop("parentNode")),
                            s_if(
                                e_not(e_instance_of(e_var("parent"), "DOMNode")),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("anchor", e_this_prop("nextSibling")),
                            s_assign("items", e_method_call(e_this(), "__elephc_dom_text_args", vec![e_var("nodes")])),
                            s_expr(e_method_call(e_var("parent"), "removeChild", vec![e_this()])),
                            s_foreach(e_var("items"), None, "node", vec![
                                s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_var("node"), e_var("anchor")])),
                            ]),
                        ]),
                )
                .method(
                    method("remove")
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("parent", e_this_prop("parentNode")),
                            s_if(
                                e_instance_of(e_var("parent"), "DOMNode"),
                                vec![
                                    s_expr(e_method_call(e_var("parent"), "removeChild", vec![e_this()])),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                )
                .method(
                    method("getElementsByTagNameNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("localName", TypeExpr::Str)
                        .returns(t_class("DOMNodeList"))
                        .body(vec![
                            s_return(e_new("DOMNodeList", vec![e_call("__elephc_dom_collect_ns", vec![e_this(), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_var("namespace")), e_var("localName")])])),
                        ]),
                )

        .prop("attributes", t_nullable(t_class("DOMNamedNodeMap")), None)
        .hooked(true, false, false)
        .prop("baseURI", t_mixed(), None)
        .hooked(true, false, false)
}

fn collect_ns_function() -> Stmt {
            function("__elephc_dom_collect_ns")
                .param("node", t_class("DOMNode"))
                .param("namespace", TypeExpr::Str)
                .param("localName", TypeExpr::Str)
                .returns(t_array())
                .body(vec![
                    s_assign("found", e_array(vec![])),
                    s_assign("children", e_prop(e_var("node"), "childNodes")),
                    s_if(
                        e_not(e_instance_of(e_var("children"), "DOMNodeList")),
                        vec![
                            s_return(e_var("found")),
                        ],
                        vec![],
                        None,
                    ),
                    s_foreach(e_var("children"), None, "child", vec![
                        s_if(
                            e_instance_of(e_var("child"), "DOMElement"),
                            vec![
                                s_assign("uri", e_prop(e_var("child"), "namespaceURI")),
                                s_assign("uri", e_ternary(e_binop(e_var("uri"), BinOp::StrictEq, e_null()), e_str(""), e_var("uri"))),
                                s_if(
                                    e_binop(e_binop(e_binop(e_var("localName"), BinOp::StrictEq, e_str("*")), BinOp::Or, e_binop(e_prop(e_var("child"), "localName"), BinOp::StrictEq, e_var("localName"))), BinOp::And, e_binop(e_binop(e_var("namespace"), BinOp::StrictEq, e_str("*")), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_var("namespace")))),
                                    vec![
                                        s_array_push("found", e_var("child")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_foreach(e_call("__elephc_dom_collect_ns", vec![e_var("child"), e_var("namespace"), e_var("localName")]), None, "more", vec![
                                    s_array_push("found", e_var("more")),
                                ]),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_var("found")),
                ])
.build()
}

fn element_members() -> ClassBuilder {
            class("DOMElement")
                .method(
                    method("__construct")
                        .param_default("qualifiedName", t_nullable(TypeExpr::Str), e_null())
                        .param_default("value", t_nullable(TypeExpr::Str), e_null())
                        .param_default("namespace", TypeExpr::Str, e_str(""))
                        .body(vec![
                            s_if(
                                e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("qualifiedName")]))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("colon", e_call("strpos", vec![e_var("qualifiedName"), e_str(":")])),
                            s_if(
                                e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)),
                                vec![
                                    s_if(
                                        e_binop(e_binop(e_binop(e_binop(e_var("colon"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("colon"), BinOp::StrictEq, e_binop(e_call("strlen", vec![e_var("qualifiedName")]), BinOp::Sub, e_int(1)))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("qualifiedName"), e_str(":"), e_binop(e_var("colon"), BinOp::Add, e_int(1))]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_var("namespace"), BinOp::StrictEq, e_str(""))),
                                        vec![
                                            s_throw(e_new("DOMException", vec![e_str("Namespace Error"), e_int(14)])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "nodeName", e_var("qualifiedName")),
                            s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                            s_if(
                                e_binop(e_var("namespace"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_prop_assign(e_this(), "__elephc_dom_ns", e_var("namespace")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("value"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str(""))),
                                vec![
                                    s_expr(e_method_call(e_this(), "appendChild", vec![e_new("DOMText", vec![e_var("value")])])),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                )
                .method(
                    method("__propget_tagName")
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_return(e_this_prop("nodeName")),
                        ]),
                )
                .method(
                    method("__propget_schemaTypeInfo").returns(t_mixed())
                        .body(vec![
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("__propget_firstElementChild")
                        .returns(t_nullable(t_class("DOMElement")))
                        .body(vec![
                            s_foreach(e_this_prop("childNodes"), None, "child", vec![
                                s_if(
                                    e_instance_of(e_var("child"), "DOMElement"),
                                    vec![
                                        s_return(e_var("child")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("__propget_lastElementChild")
                        .returns(t_nullable(t_class("DOMElement")))
                        .body(vec![
                            s_assign("found", e_null()),
                            s_foreach(e_this_prop("childNodes"), None, "child", vec![
                                s_if(
                                    e_instance_of(e_var("child"), "DOMElement"),
                                    vec![
                                        s_assign("found", e_var("child")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_var("found")),
                        ]),
                )
                .method(
                    method("__propget_nextElementSibling")
                        .returns(t_nullable(t_class("DOMElement")))
                        .body(vec![
                            s_assign("node", e_this_prop("nextSibling")),
                            s_while(e_instance_of(e_var("node"), "DOMNode"), vec![
                                s_if(
                                    e_instance_of(e_var("node"), "DOMElement"),
                                    vec![
                                        s_return(e_var("node")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("node", e_prop(e_var("node"), "nextSibling")),
                            ]),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("__propget_previousElementSibling")
                        .returns(t_nullable(t_class("DOMElement")))
                        .body(vec![
                            s_assign("node", e_this_prop("previousSibling")),
                            s_while(e_instance_of(e_var("node"), "DOMNode"), vec![
                                s_if(
                                    e_instance_of(e_var("node"), "DOMElement"),
                                    vec![
                                        s_return(e_var("node")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("node", e_prop(e_var("node"), "previousSibling")),
                            ]),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("__propget_childElementCount")
                        .returns(TypeExpr::Int)
                        .body(vec![
                            s_assign("count", e_int(0)),
                            s_foreach(e_this_prop("childNodes"), None, "child", vec![
                                s_if(
                                    e_instance_of(e_var("child"), "DOMElement"),
                                    vec![
                                        s_assign("count", e_binop(e_var("count"), BinOp::Add, e_int(1))),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_var("count")),
                        ]),
                )
                .method(
                    method("getAttribute")
                        .param("qualifiedName", TypeExpr::Str)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("nul", e_call("strpos", vec![e_var("qualifiedName"), e_str("\0")])),
                            s_if(
                                e_binop(e_var("nul"), BinOp::StrictNotEq, e_bool(false)),
                                vec![
                                    s_assign("qualifiedName", e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("nul")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_attribute", vec![e_var("qualifiedName")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")),
                                vec![
                                    s_return(e_null_coalesce(e_this_prop("__elephc_dom_xmlns"), e_str(""))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_call("str_starts_with", vec![e_var("qualifiedName"), e_str("xmlns:")]),
                                vec![
                                    s_return(e_str("")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("__elephc_dom_attr_get", vec![e_this_prop("__attrs"), e_var("qualifiedName")])),
                        ]),
                )
                .method(
                    method("hasAttribute")
                        .param("qualifiedName", TypeExpr::Str)
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_assign("nul", e_call("strpos", vec![e_var("qualifiedName"), e_str("\0")])),
                            s_if(
                                e_binop(e_var("nul"), BinOp::StrictNotEq, e_bool(false)),
                                vec![
                                    s_assign("qualifiedName", e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("nul")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_return(e_binop(e_method_call(e_var("bound"), "__elephc_dom_element_op", vec![e_int(0), e_str(""), e_var("qualifiedName")]), BinOp::StrictNotEq, e_str(""))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")),
                                vec![
                                    s_return(e_binop(e_this_prop("__elephc_dom_xmlns"), BinOp::StrictNotEq, e_null())),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_this_prop("__attrs"), None, "pair", vec![
                                s_if(
                                    e_binop(e_cast(CastType::String, e_index(e_var("pair"), e_int(0))), BinOp::StrictEq, e_var("qualifiedName")),
                                    vec![
                                        s_return(e_bool(true)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("setAttribute")
                        .param("qualifiedName", TypeExpr::Str)
                        .param("value", TypeExpr::Str)
                        .returns(t_union(vec![t_class("DOMAttr"), TypeExpr::Bool]))
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
                                    s_if(
                                        e_not(e_method_call(e_var("bound"), "__elephc_dom_set_attribute", vec![e_var("qualifiedName"), e_var("value")])),
                                        vec![
                                            s_return(e_bool(false)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![
                                (e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")), vec![
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
                                ]),
                            ],
                                Some(vec![
                                s_assign("updated", e_array(vec![])),
                                s_assign("found", e_bool(false)),
                                s_foreach(e_this_prop("__attrs"), None, "pair", vec![
                                    s_if(
                                        e_binop(e_cast(CastType::String, e_index(e_var("pair"), e_int(0))), BinOp::StrictEq, e_var("qualifiedName")),
                                        vec![
                                            s_assign("keep", e_var("pair")),
                                            s_array_assign("keep", e_int(1), e_var("value")),
                                            s_array_push("updated", e_var("keep")),
                                            s_assign("found", e_bool(true)),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_array_push("updated", e_var("pair")),
                                    ]),
                                    ),
                                ]),
                                s_if(
                                    e_not(e_var("found")),
                                    vec![
                                        s_array_push("updated", e_array(vec![e_var("qualifiedName"), e_var("value")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_prop_assign(e_this(), "__attrs", e_var("updated")),
                            ]),
                            ),
                            s_if(
                                e_binop(e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")), BinOp::Or, e_call("str_starts_with", vec![e_var("qualifiedName"), e_str("xmlns:")])),
                                vec![
                                    s_return(e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("colon", e_call("strpos", vec![e_var("qualifiedName"), e_str(":")])),
                            s_assign("local", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_var("qualifiedName"), e_call("substr", vec![e_var("qualifiedName"), e_binop(e_var("colon"), BinOp::Add, e_int(1))]))),
                            s_assign("prefix", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_str(""), e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("colon")]))),
                            s_assign("uri", e_ternary(e_binop(e_var("prefix"), BinOp::StrictEq, e_str("")), e_null(), e_method_call(e_this(), "lookupNamespaceURI", vec![e_var("prefix")]))),
                            s_return(e_method_call(e_this(), "__elephc_dom_make_attr", vec![e_array(vec![e_str("a"), e_var("qualifiedName"), e_var("local"), e_var("prefix"), e_ternary(e_binop(e_var("uri"), BinOp::StrictEq, e_null()), e_str(""), e_var("uri")), e_var("value")])])),
                        ]),
                )
                .method(
                    method("removeAttribute")
                        .param("qualifiedName", TypeExpr::Str)
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_assign("nul", e_call("strpos", vec![e_var("qualifiedName"), e_str("\0")])),
                            s_if(
                                e_binop(e_var("nul"), BinOp::StrictNotEq, e_bool(false)),
                                vec![
                                    s_assign("qualifiedName", e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("nul")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_return(e_binop(e_method_call(e_var("bound"), "__elephc_dom_element_op", vec![e_int(4), e_str(""), e_var("qualifiedName")]), BinOp::StrictNotEq, e_str(""))),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("kept", e_array(vec![])),
                            s_assign("found", e_bool(false)),
                            s_foreach(e_this_prop("__attrs"), None, "pair", vec![
                                s_if(
                                    e_binop(e_cast(CastType::String, e_index(e_var("pair"), e_int(0))), BinOp::StrictEq, e_var("qualifiedName")),
                                    vec![
                                        s_assign("found", e_bool(true)),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_array_push("kept", e_var("pair")),
                                ]),
                                ),
                            ]),
                            s_if(
                                e_var("found"),
                                vec![
                                    s_prop_assign(e_this(), "__attrs", e_var("kept")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("found")),
                        ]),
                )
                .method(
                    method("getAttributeNode")
                        .param("qualifiedName", TypeExpr::Str)
                        .returns(t_union(vec![t_class("DOMAttr"), TypeExpr::Bool]))
                        .body(vec![
                            s_foreach(e_method_call(e_this(), "__elephc_dom_attr_records", vec![e_bool(false)]), None, "record", vec![
                                s_if(
                                    e_binop(e_index(e_var("record"), e_int(1)), BinOp::StrictEq, e_var("qualifiedName")),
                                    vec![
                                        s_return(e_method_call(e_this(), "__elephc_dom_make_attr", vec![e_var("record")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("getAttributeNames")
                        .returns(t_array())
                        .body(vec![
                            s_assign("names", e_array(vec![])),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_attr_records", vec![e_bool(true)]), None, "record", vec![
                                s_array_push("names", e_index(e_var("record"), e_int(1))),
                            ]),
                            s_return(e_var("names")),
                        ]),
                )
                .method(
                    method("getAttributeNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("localName", TypeExpr::Str)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("uri", e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_str("")), e_str("\x01"), e_var("namespace")))),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_assign("answer", e_method_call(e_var("bound"), "__elephc_dom_element_op", vec![e_int(1), e_var("uri"), e_var("localName")])),
                                    s_return(e_ternary(e_binop(e_call("strlen", vec![e_var("answer")]), BinOp::Gt, e_int(1)), e_call("substr", vec![e_var("answer"), e_int(1)]), e_str(""))),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_attr_records", vec![e_bool(true)]), None, "record", vec![
                                s_if(
                                    e_binop(e_binop(e_binop(e_index(e_var("record"), e_int(2)), BinOp::StrictEq, e_var("localName")), BinOp::And, e_binop(e_index(e_var("record"), e_int(4)), BinOp::StrictEq, e_var("uri"))), BinOp::And, e_binop(e_binop(e_index(e_var("record"), e_int(0)), BinOp::StrictEq, e_str("a")), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_str("http://www.w3.org/2000/xmlns/")))),
                                    vec![
                                        s_return(e_index(e_var("record"), e_int(5))),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_str("")),
                        ]),
                )
                .method(
                    method("hasAttributeNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("localName", TypeExpr::Str)
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_assign("uri", e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_str("")), e_str("\x01"), e_var("namespace")))),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_return(e_binop(e_method_call(e_var("bound"), "__elephc_dom_element_op", vec![e_int(2), e_var("uri"), e_var("localName")]), BinOp::StrictNotEq, e_str(""))),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_attr_records", vec![e_bool(true)]), None, "record", vec![
                                s_if(
                                    e_binop(e_binop(e_index(e_var("record"), e_int(2)), BinOp::StrictEq, e_var("localName")), BinOp::And, e_binop(e_index(e_var("record"), e_int(4)), BinOp::StrictEq, e_var("uri"))),
                                    vec![
                                        s_return(e_bool(true)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("getAttributeNodeNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("localName", TypeExpr::Str)
                        .returns(t_nullable(t_class("DOMAttr")))
                        .body(vec![
                            s_assign("uri", e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_str("")), e_str("\x01"), e_var("namespace")))),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_attr_records", vec![e_bool(false)]), None, "record", vec![
                                s_if(
                                    e_binop(e_binop(e_index(e_var("record"), e_int(2)), BinOp::StrictEq, e_var("localName")), BinOp::And, e_binop(e_index(e_var("record"), e_int(4)), BinOp::StrictEq, e_var("uri"))),
                                    vec![
                                        s_return(e_method_call(e_this(), "__elephc_dom_make_attr", vec![e_var("record")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("setAttributeNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("qualifiedName", TypeExpr::Str)
                        .param("value", TypeExpr::Str)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_if(
                                e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("DOMElement::setAttributeNS(): Argument #2 ($qualifiedName) must not be empty")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("uri", e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_var("namespace"))),
                            s_if(
                                e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("qualifiedName")])),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("colon", e_call("strpos", vec![e_var("qualifiedName"), e_str(":")])),
                            s_assign("prefix", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_str(""), e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("colon")]))),
                            s_assign("local", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_var("qualifiedName"), e_call("substr", vec![e_var("qualifiedName"), e_binop(e_var("colon"), BinOp::Add, e_int(1))]))),
                            s_if(
                                e_binop(e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_var("colon"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("local"), BinOp::StrictEq, e_str(""))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("local"), e_str(":")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_str("")))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Namespace Error"), e_int(14)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xml")), BinOp::And, e_binop(e_var("uri"), BinOp::StrictNotEq, e_str("http://www.w3.org/XML/1998/namespace"))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Namespace Error"), e_int(14)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("declaration", e_binop(e_binop(e_binop(e_var("uri"), BinOp::StrictEq, e_str("http://www.w3.org/2000/xmlns/")), BinOp::Or, e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns"))), BinOp::Or, e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xmlns")))),
                            s_if(
                                e_binop(e_var("declaration"), BinOp::And, e_not(e_binop(e_binop(e_var("uri"), BinOp::StrictEq, e_str("http://www.w3.org/2000/xmlns/")), BinOp::And, e_binop(e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")), BinOp::Or, e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xmlns")))))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Namespace Error"), e_int(14)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_expr(e_method_call(e_var("bound"), "__elephc_dom_element_op", vec![e_int(3), e_var("uri"), e_var("qualifiedName"), e_var("value")])),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_var("declaration"),
                                vec![
                                    s_if(
                                        e_binop(e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")), BinOp::And, e_binop(e_this_prop("__elephc_dom_xmlns"), BinOp::StrictEq, e_null())),
                                        vec![
                                            s_prop_assign(e_this(), "__elephc_dom_xmlns", e_var("value")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("updated", e_array(vec![])),
                            s_assign("found", e_bool(false)),
                            s_foreach(e_this_prop("__attrs"), None, "pair", vec![
                                s_assign("pairUri", e_ternary(e_binop(e_call("array_key_exists", vec![e_int(2), e_var("pair")]), BinOp::And, e_call("is_string", vec![e_index(e_var("pair"), e_int(2))])), e_cast(CastType::String, e_index(e_var("pair"), e_int(2))), e_str(""))),
                                s_assign("pairName", e_cast(CastType::String, e_index(e_var("pair"), e_int(0)))),
                                s_assign("pairColon", e_call("strpos", vec![e_var("pairName"), e_str(":")])),
                                s_assign("pairLocal", e_ternary(e_binop(e_var("pairColon"), BinOp::StrictEq, e_bool(false)), e_var("pairName"), e_call("substr", vec![e_var("pairName"), e_binop(e_var("pairColon"), BinOp::Add, e_int(1))]))),
                                s_if(
                                    e_binop(e_binop(e_var("pairLocal"), BinOp::StrictEq, e_var("local")), BinOp::And, e_binop(e_var("pairUri"), BinOp::StrictEq, e_var("uri"))),
                                    vec![
                                        s_array_push("updated", e_array(vec![e_var("pairName"), e_var("value"), e_var("uri")])),
                                        s_assign("found", e_bool(true)),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_array_push("updated", e_var("pair")),
                                ]),
                                ),
                            ]),
                            s_if(
                                e_not(e_var("found")),
                                vec![
                                    s_array_push("updated", e_array(vec![e_var("qualifiedName"), e_var("value"), e_var("uri")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "__attrs", e_var("updated")),
                        ]),
                )
                .method(
                    method("removeAttributeNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("localName", TypeExpr::Str)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("uri", e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_var("namespace"))),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_expr(e_method_call(e_var("bound"), "__elephc_dom_element_op", vec![e_int(5), e_var("uri"), e_var("localName")])),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("kept", e_array(vec![])),
                            s_foreach(e_this_prop("__attrs"), None, "pair", vec![
                                s_assign("pairUri", e_ternary(e_binop(e_call("array_key_exists", vec![e_int(2), e_var("pair")]), BinOp::And, e_call("is_string", vec![e_index(e_var("pair"), e_int(2))])), e_cast(CastType::String, e_index(e_var("pair"), e_int(2))), e_str(""))),
                                s_assign("pairName", e_cast(CastType::String, e_index(e_var("pair"), e_int(0)))),
                                s_assign("pairColon", e_call("strpos", vec![e_var("pairName"), e_str(":")])),
                                s_assign("pairLocal", e_ternary(e_binop(e_var("pairColon"), BinOp::StrictEq, e_bool(false)), e_var("pairName"), e_call("substr", vec![e_var("pairName"), e_binop(e_var("pairColon"), BinOp::Add, e_int(1))]))),
                                s_if(
                                    e_not(e_binop(e_binop(e_var("pairLocal"), BinOp::StrictEq, e_var("localName")), BinOp::And, e_binop(e_var("pairUri"), BinOp::StrictEq, e_var("uri")))),
                                    vec![
                                        s_array_push("kept", e_var("pair")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_prop_assign(e_this(), "__attrs", e_var("kept")),
                        ]),
                )
                .method(
                    method("setAttributeNode")
                        .param("attr", t_class("DOMAttr"))
                        .returns(t_nullable(t_class("DOMAttr")))
                        .body(vec![
                            s_assign("doc", e_this_prop("ownerDocument")),
                            s_assign("theirs", e_prop(e_var("attr"), "ownerDocument")),
                            s_if(
                                e_binop(e_binop(e_instance_of(e_var("doc"), "DOMDocument"), BinOp::And, e_instance_of(e_var("theirs"), "DOMDocument")), BinOp::And, e_binop(e_var("doc"), BinOp::StrictNotEq, e_var("theirs"))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("owner", e_prop(e_var("attr"), "ownerElement")),
                            s_if(
                                e_binop(e_instance_of(e_var("owner"), "DOMElement"), BinOp::And, e_binop(e_var("owner"), BinOp::StrictNotEq, e_this())),
                                vec![
                                    // legacy php moves an attribute that another element owns
                                    s_expr(e_method_call(e_var("owner"), "removeAttribute", vec![e_prop(e_var("attr"), "nodeName")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("old", e_method_call(e_this(), "getAttributeNode", vec![e_prop(e_var("attr"), "nodeName")])),
                            s_expr(e_method_call(e_this(), "setAttribute", vec![e_prop(e_var("attr"), "nodeName"), e_cast(CastType::String, e_prop(e_var("attr"), "nodeValue"))])),
                            s_prop_assign(e_var("attr"), "ownerElement", e_this()),
                            s_prop_assign(e_var("attr"), "parentNode", e_this()),
                            s_if(
                                e_instance_of(e_var("old"), "DOMAttr"),
                                vec![
                                    s_prop_assign(e_var("old"), "ownerElement", e_null()),
                                    s_prop_assign(e_var("old"), "parentNode", e_null()),
                                    s_return(e_var("old")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("setAttributeNodeNS")
                        .param("attr", t_class("DOMAttr"))
                        .returns(t_nullable(t_class("DOMAttr")))
                        .body(vec![
                            s_assign("uri", e_prop(e_var("attr"), "namespaceURI")),
                            s_assign("old", e_method_call(e_this(), "getAttributeNodeNS", vec![e_var("uri"), e_prop(e_var("attr"), "localName")])),
                            s_assign("owner", e_prop(e_var("attr"), "ownerElement")),
                            s_if(
                                e_binop(e_instance_of(e_var("owner"), "DOMElement"), BinOp::And, e_binop(e_var("owner"), BinOp::StrictNotEq, e_this())),
                                vec![
                                    s_expr(e_method_call(e_var("owner"), "removeAttribute", vec![e_prop(e_var("attr"), "nodeName")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("uri"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_str(""))),
                                vec![
                                    s_expr(e_method_call(e_this(), "setAttribute", vec![e_prop(e_var("attr"), "nodeName"), e_cast(CastType::String, e_prop(e_var("attr"), "nodeValue"))])),
                                ],
                                vec![],
                                Some(vec![
                                s_expr(e_method_call(e_this(), "setAttributeNS", vec![e_var("uri"), e_prop(e_var("attr"), "nodeName"), e_cast(CastType::String, e_prop(e_var("attr"), "nodeValue"))])),
                            ]),
                            ),
                            s_prop_assign(e_var("attr"), "ownerElement", e_this()),
                            s_prop_assign(e_var("attr"), "parentNode", e_this()),
                            s_if(
                                e_instance_of(e_var("old"), "DOMAttr"),
                                vec![
                                    s_prop_assign(e_var("old"), "ownerElement", e_null()),
                                    s_prop_assign(e_var("old"), "parentNode", e_null()),
                                    s_return(e_var("old")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("removeAttributeNode")
                        .param("attr", t_class("DOMAttr"))
                        .returns(t_class("DOMAttr"))
                        .body(vec![
                            s_if(
                                e_binop(e_prop(e_var("attr"), "ownerElement"), BinOp::StrictNotEq, e_this()),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("uri", e_prop(e_var("attr"), "namespaceURI")),
                            s_if(
                                e_binop(e_binop(e_var("uri"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_str(""))),
                                vec![
                                    s_expr(e_method_call(e_this(), "removeAttribute", vec![e_prop(e_var("attr"), "nodeName")])),
                                ],
                                vec![],
                                Some(vec![
                                s_expr(e_method_call(e_this(), "removeAttributeNS", vec![e_var("uri"), e_prop(e_var("attr"), "localName")])),
                            ]),
                            ),
                            s_prop_assign(e_var("attr"), "ownerElement", e_null()),
                            s_prop_assign(e_var("attr"), "parentNode", e_null()),
                            s_return(e_var("attr")),
                        ]),
                )
                .method(
                    method("toggleAttribute")
                        .param("qualifiedName", TypeExpr::Str)
                        .param_default("force", t_nullable(TypeExpr::Bool), e_null())
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_if(
                                e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("qualifiedName")])),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_method_call(e_this(), "hasAttribute", vec![e_var("qualifiedName")]),
                                vec![
                                    s_if(
                                        e_binop(e_var("force"), BinOp::StrictEq, e_bool(true)),
                                        vec![
                                            s_return(e_bool(true)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_expr(e_method_call(e_this(), "removeAttribute", vec![e_var("qualifiedName")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("force"), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "setAttribute", vec![e_var("qualifiedName"), e_str("")])),
                            s_return(e_bool(true)),
                        ]),
                )
                .method(
                    method("__elephc_dom_is_id_name")
                        .param("name", TypeExpr::Str)
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_foreach(e_this_prop("__elephc_dom_ids"), None, "id", vec![
                                s_if(
                                    e_binop(e_var("id"), BinOp::StrictEq, e_var("name")),
                                    vec![
                                        s_return(e_bool(true)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_if(
                                e_binop(e_var("name"), BinOp::StrictEq, e_str("xml:id")),
                                vec![
                                    s_return(e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("doc", e_this_prop("ownerDocument")),
                            s_return(e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("id")), BinOp::And, e_instance_of(e_var("doc"), "DOMDocument")), BinOp::And, e_prop(e_var("doc"), "__elephc_dom_html"))),
                        ]),
                )
                .method(
                    method("setIdAttribute")
                        .param("qualifiedName", TypeExpr::Str)
                        .param("isId", TypeExpr::Bool)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_if(
                                e_not(e_method_call(e_this(), "hasAttribute", vec![e_var("qualifiedName")])),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("ids", e_array(vec![])),
                            s_foreach(e_this_prop("__elephc_dom_ids"), None, "id", vec![
                                s_if(
                                    e_binop(e_var("id"), BinOp::StrictNotEq, e_var("qualifiedName")),
                                    vec![
                                        s_array_push("ids", e_var("id")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_if(
                                e_var("isId"),
                                vec![
                                    s_array_push("ids", e_var("qualifiedName")),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "__elephc_dom_ids", e_var("ids")),
                        ]),
                )
                .method(
                    method("setIdAttributeNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("localName", TypeExpr::Str)
                        .param("isId", TypeExpr::Bool)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("attr", e_method_call(e_this(), "getAttributeNodeNS", vec![e_var("namespace"), e_var("localName")])),
                            s_if(
                                e_not(e_instance_of(e_var("attr"), "DOMAttr")),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "setIdAttribute", vec![e_prop(e_var("attr"), "nodeName"), e_var("isId")])),
                        ]),
                )
                .method(
                    method("setIdAttributeNode")
                        .param("attr", t_class("DOMAttr"))
                        .param("isId", TypeExpr::Bool)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_if(
                                e_binop(e_prop(e_var("attr"), "ownerElement"), BinOp::StrictNotEq, e_this()),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "setIdAttribute", vec![e_prop(e_var("attr"), "nodeName"), e_var("isId")])),
                        ]),
                )
                .method(
                    method("insertAdjacentElement")
                        .param("where", TypeExpr::Str)
                        .param("element", t_class("DOMElement"))
                        .returns(t_nullable(t_class("DOMElement")))
                        .body(vec![
                            s_assign("parent", e_method_call(e_this(), "__elephc_dom_adjacent", vec![e_var("where"), e_str("insertAdjacentElement")])),
                            s_if(
                                e_binop(e_var("parent"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "__elephc_dom_adjacent_insert", vec![e_var("where"), e_var("element")])),
                            s_return(e_var("element")),
                        ]),
                )
                .method(
                    method("insertAdjacentText")
                        .param("where", TypeExpr::Str)
                        .param("data", TypeExpr::Str)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("parent", e_method_call(e_this(), "__elephc_dom_adjacent", vec![e_var("where"), e_str("insertAdjacentText")])),
                            s_if(
                                e_binop(e_var("parent"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("text", e_new("DOMText", vec![e_var("data")])),
                            s_assign("doc", e_this_prop("ownerDocument")),
                            s_if(
                                e_instance_of(e_var("doc"), "DOMDocument"),
                                vec![
                                    s_prop_assign(e_var("text"), "ownerDocument", e_var("doc")),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "__elephc_dom_adjacent_insert", vec![e_var("where"), e_var("text")])),
                        ]),
                )
                .method(
                    method("__elephc_dom_adjacent")
                        .param("where", TypeExpr::Str)
                        .param("method", TypeExpr::Str)
                        .keep_unread_params()
                        .returns(t_nullable(t_class("DOMNode")))
                        .body(vec![
                            s_assign("position", e_call("strtolower", vec![e_var("where")])),
                            s_if(
                                e_binop(e_binop(e_var("position"), BinOp::StrictEq, e_str("afterbegin")), BinOp::Or, e_binop(e_var("position"), BinOp::StrictEq, e_str("beforeend"))),
                                vec![
                                    s_return(e_this()),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("position"), BinOp::StrictEq, e_str("beforebegin")), BinOp::Or, e_binop(e_var("position"), BinOp::StrictEq, e_str("afterend"))),
                                vec![
                                    s_assign("parent", e_this_prop("parentNode")),
                                    s_return(e_ternary(e_instance_of(e_var("parent"), "DOMNode"), e_var("parent"), e_null())),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new("DOMException", vec![e_str("Syntax Error"), e_int(12)])),
                        ]),
                )
                .method(
                    method("__elephc_dom_adjacent_insert")
                        .param("where", TypeExpr::Str)
                        .param("node", t_class("DOMNode"))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("position", e_call("strtolower", vec![e_var("where")])),
                            s_if(
                                e_binop(e_var("position"), BinOp::StrictEq, e_str("afterbegin")),
                                vec![
                                    s_expr(e_method_call(e_this(), "insertBefore", vec![e_var("node"), e_this_prop("firstChild")])),
                                ],
                                vec![
                                (e_binop(e_var("position"), BinOp::StrictEq, e_str("beforeend")), vec![
                                    s_expr(e_method_call(e_this(), "appendChild", vec![e_var("node")])),
                                ]),
                                (e_binop(e_var("position"), BinOp::StrictEq, e_str("beforebegin")), vec![
                                    s_expr(e_method_call(e_this_prop("parentNode"), "insertBefore", vec![e_var("node"), e_this()])),
                                ]),
                            ],
                                Some(vec![
                                s_expr(e_method_call(e_this_prop("parentNode"), "insertBefore", vec![e_var("node"), e_this_prop("nextSibling")])),
                            ]),
                            ),
                        ]),
                )

        .prop("tagName", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("schemaTypeInfo", t_mixed(), None)
        .hooked(true, false, false)
        .prop("firstElementChild", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("lastElementChild", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("nextElementSibling", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("previousElementSibling", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("childElementCount", TypeExpr::Int, None)
        .hooked(true, false, false)
        .prop("__elephc_dom_ids", t_array(), Some(e_array(vec![])))
}

fn document_members() -> ClassBuilder {
            class("DOMDocument")
                .method(
                    method("__elephc_dom_declaration")
                        .returns(t_array())
                        .body(vec![
                            s_assign("decl", e_this_prop("__elephc_dom_decl")),
                            s_if(
                                e_binop(e_var("decl"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_return(e_var("decl")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("decl", e_array_assoc(vec![(e_str("version"), e_str("1.0")), (e_str("encoding"), e_null()), (e_str("standalone"), e_bool(false))])),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_assign("xml", e_method_call(e_var("bound"), "__elephc_dom_document_xml", vec![])),
                                    s_if(
                                        e_call("str_starts_with", vec![e_var("xml"), e_str("<?xml")]),
                                        vec![
                                            s_assign("end", e_call("strpos", vec![e_var("xml"), e_str("?>")])),
                                            s_assign("head", e_ternary(e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)), e_var("xml"), e_call("substr", vec![e_var("xml"), e_int(0), e_var("end")]))),
                                            s_foreach(e_array(vec![e_str("version"), e_str("encoding"), e_str("standalone")]), None, "key", vec![
                                                s_assign("at", e_call("strpos", vec![e_var("head"), e_binop(e_binop(e_str(" "), BinOp::Concat, e_var("key")), BinOp::Concat, e_str("=\""))])),
                                                s_if(
                                                    e_binop(e_var("at"), BinOp::StrictNotEq, e_bool(false)),
                                                    vec![
                                                        s_assign("from", e_binop(e_binop(e_var("at"), BinOp::Add, e_call("strlen", vec![e_var("key")])), BinOp::Add, e_int(3))),
                                                        s_assign("close", e_call("strpos", vec![e_var("head"), e_str("\""), e_var("from")])),
                                                        s_assign("text", e_ternary(e_binop(e_var("close"), BinOp::StrictEq, e_bool(false)), e_str(""), e_call("substr", vec![e_var("head"), e_var("from"), e_binop(e_var("close"), BinOp::Sub, e_var("from"))]))),
                                                        s_array_assign("decl", e_var("key"), e_ternary(e_binop(e_var("key"), BinOp::StrictEq, e_str("standalone")), e_binop(e_var("text"), BinOp::StrictEq, e_str("yes")), e_var("text"))),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                            ]),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "__elephc_dom_decl", e_var("decl")),
                            s_return(e_var("decl")),
                        ]),
                )
                .method(
                    method("__propget_encoding")
                        .returns(t_nullable(TypeExpr::Str))
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_return(e_index(e_var("decl"), e_str("encoding"))),
                        ]),
                )
                .method(
                    method("__propset_encoding")
                        .param("value", t_nullable(TypeExpr::Str))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_binop(e_binop(e_var("value"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_str(""))),
                                vec![
                                    s_array_assign("decl", e_str("encoding"), e_null()),
                                ],
                                vec![],
                                Some(vec![
                                s_array_assign("decl", e_str("encoding"), e_var("value")),
                                s_if(
                                    e_call("is_object", vec![e_var("bound")]),
                                    vec![
                                        s_expr(e_method_call(e_var("bound"), "__elephc_set_document_encoding", vec![e_var("value")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                            s_prop_assign(e_this(), "__elephc_dom_decl", e_var("decl")),
                            s_prop_assign(e_this(), "__elephc_dom_encoding", e_index(e_var("decl"), e_str("encoding"))),
                        ]),
                )
                .method(
                    method("__propget_xmlEncoding")
                        .returns(t_nullable(TypeExpr::Str))
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_return(e_index(e_var("decl"), e_str("encoding"))),
                        ]),
                )
                .method(
                    method("__propget_version")
                        .returns(t_nullable(TypeExpr::Str))
                        .body(vec![
                            s_if(
                                e_this_prop("__elephc_dom_html"),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_return(e_index(e_var("decl"), e_str("version"))),
                        ]),
                )
                .method(
                    method("__propset_version")
                        .param("value", t_nullable(TypeExpr::Str))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_array_assign("decl", e_str("version"), e_ternary(e_binop(e_var("value"), BinOp::StrictEq, e_null()), e_str("1.0"), e_var("value"))),
                            s_prop_assign(e_this(), "__elephc_dom_decl", e_var("decl")),
                            s_prop_assign(e_this(), "__elephc_dom_version", e_index(e_var("decl"), e_str("version"))),
                        ]),
                )
                .method(
                    method("__propget_xmlVersion")
                        .returns(t_nullable(TypeExpr::Str))
                        .body(vec![
                            s_if(
                                e_this_prop("__elephc_dom_html"),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_return(e_index(e_var("decl"), e_str("version"))),
                        ]),
                )
                .method(
                    method("__propset_xmlVersion")
                        .param("value", t_nullable(TypeExpr::Str))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_array_assign("decl", e_str("version"), e_ternary(e_binop(e_var("value"), BinOp::StrictEq, e_null()), e_str("1.0"), e_var("value"))),
                            s_prop_assign(e_this(), "__elephc_dom_decl", e_var("decl")),
                            s_prop_assign(e_this(), "__elephc_dom_version", e_index(e_var("decl"), e_str("version"))),
                        ]),
                )
                .method(
                    method("__propget_standalone")
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_return(e_index(e_var("decl"), e_str("standalone"))),
                        ]),
                )
                .method(
                    method("__propset_standalone")
                        .param("value", TypeExpr::Bool)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_array_assign("decl", e_str("standalone"), e_var("value")),
                            s_prop_assign(e_this(), "__elephc_dom_decl", e_var("decl")),
                        ]),
                )
                .method(
                    method("__propget_xmlStandalone")
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_return(e_index(e_var("decl"), e_str("standalone"))),
                        ]),
                )
                .method(
                    method("__propset_xmlStandalone")
                        .param("value", TypeExpr::Bool)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_array_assign("decl", e_str("standalone"), e_var("value")),
                            s_prop_assign(e_this(), "__elephc_dom_decl", e_var("decl")),
                        ]),
                )
                .method(
                    method("__propget_documentURI")
                        .returns(t_nullable(TypeExpr::Str))
                        .body(vec![
                            s_assign("uri", e_this_prop("__elephc_dom_uri")),
                            s_if(
                                e_binop(e_binop(e_binop(e_var("uri"), BinOp::StrictEq, e_null()), BinOp::And, e_call("is_object", vec![e_this_prop("__elephc_simplexml")])), BinOp::And, e_not(e_this_prop("__elephc_dom_html"))),
                                vec![
                                    s_return(e_binop(e_call("getcwd", vec![]), BinOp::Concat, e_str("/"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("uri")),
                        ]),
                )
                .method(
                    method("__propset_documentURI")
                        .param("value", t_nullable(TypeExpr::Str))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_prop_assign(e_this(), "__elephc_dom_uri", e_var("value")),
                        ]),
                )
                .method(
                    method("__propget_doctype")
                        .returns(t_nullable(t_class("DOMDocumentType")))
                        .body(vec![
                            s_foreach(e_this_prop("childNodes"), None, "child", vec![
                                s_if(
                                    e_instance_of(e_var("child"), "DOMDocumentType"),
                                    vec![
                                        s_return(e_var("child")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("__propget_implementation")
                        .returns(t_class("DOMImplementation"))
                        .body(vec![
                            s_return(e_new("DOMImplementation", vec![])),
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
                            s_prop_assign(e_var("element"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                            s_prop_assign(e_var("element"), "ownerDocument", e_this()),
                            s_if(
                                e_binop(e_var("value"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_assign("text", e_new("DOMText", vec![e_call("__elephc_dom_decode_entities", vec![e_var("value")])])),
                                    s_prop_assign(e_var("text"), "ownerDocument", e_this()),
                                    s_expr(e_method_call(e_var("element"), "appendChild", vec![e_var("text")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("element")),
                        ]),
                )
                .method(
                    method("createElementNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("qualifiedName", TypeExpr::Str)
                        .param_default("value", TypeExpr::Str, e_str(""))
                        .returns(t_class("DOMElement"))
                        .body(vec![
                            s_assign("uri", e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_var("namespace"))),
                            s_if(
                                e_binop(e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("qualifiedName")]))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("colon", e_call("strpos", vec![e_var("qualifiedName"), e_str(":")])),
                            s_if(
                                e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)),
                                vec![
                                    s_assign("prefix", e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("colon")])),
                                    s_if(
                                        e_binop(e_binop(e_binop(e_binop(e_var("colon"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("colon"), BinOp::StrictEq, e_binop(e_call("strlen", vec![e_var("qualifiedName")]), BinOp::Sub, e_int(1)))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("qualifiedName"), e_str(":"), e_binop(e_var("colon"), BinOp::Add, e_int(1))]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_str(""))),
                                        vec![
                                            s_throw(e_new("DOMException", vec![e_str("Namespace Error"), e_int(14)])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_binop(e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xml")), BinOp::And, e_binop(e_var("uri"), BinOp::StrictNotEq, e_str("http://www.w3.org/XML/1998/namespace"))), BinOp::Or, e_binop(e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xmlns")), BinOp::And, e_binop(e_var("uri"), BinOp::StrictNotEq, e_str("http://www.w3.org/2000/xmlns/")))),
                                        vec![
                                            s_throw(e_new("DOMException", vec![e_str("Namespace Error"), e_int(14)])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("element", e_new("DOMElement", vec![])),
                            s_prop_assign(e_var("element"), "nodeName", e_var("qualifiedName")),
                            s_prop_assign(e_var("element"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                            s_prop_assign(e_var("element"), "ownerDocument", e_this()),
                            s_if(
                                e_binop(e_var("uri"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_prop_assign(e_var("element"), "__elephc_dom_ns", e_var("uri")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("value"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_assign("text", e_new("DOMText", vec![e_call("__elephc_dom_decode_entities", vec![e_var("value")])])),
                                    s_prop_assign(e_var("text"), "ownerDocument", e_this()),
                                    s_expr(e_method_call(e_var("element"), "appendChild", vec![e_var("text")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("element")),
                        ]),
                )
                .method(
                    method("createTextNode")
                        .param("data", TypeExpr::Str)
                        .returns(t_class("DOMText"))
                        .body(vec![
                            s_assign("text", e_new("DOMText", vec![e_var("data")])),
                            s_prop_assign(e_var("text"), "ownerDocument", e_this()),
                            s_return(e_var("text")),
                        ]),
                )
                .method(
                    method("createComment")
                        .param("data", TypeExpr::Str)
                        .returns(t_class("DOMComment"))
                        .body(vec![
                            s_assign("comment", e_new("DOMComment", vec![e_var("data")])),
                            s_prop_assign(e_var("comment"), "ownerDocument", e_this()),
                            s_return(e_var("comment")),
                        ]),
                )
                .method(
                    method("createCDATASection")
                        .param("data", TypeExpr::Str)
                        .returns(t_class("DOMCdataSection"))
                        .body(vec![
                            s_assign("section", e_new("DOMCdataSection", vec![e_var("data")])),
                            s_prop_assign(e_var("section"), "ownerDocument", e_this()),
                            s_return(e_var("section")),
                        ]),
                )
                .method(
                    method("createProcessingInstruction")
                        .param("target", TypeExpr::Str)
                        .param_default("data", TypeExpr::Str, e_str(""))
                        .returns(t_class("DOMProcessingInstruction"))
                        .body(vec![
                            s_if(
                                e_binop(e_binop(e_binop(e_var("target"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("target")]))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("target"), e_str(":")]), BinOp::StrictNotEq, e_bool(false))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("instruction", e_new("DOMProcessingInstruction", vec![e_var("target"), e_var("data")])),
                            s_prop_assign(e_var("instruction"), "ownerDocument", e_this()),
                            s_return(e_var("instruction")),
                        ]),
                )
                .method(
                    method("createEntityReference")
                        .param("name", TypeExpr::Str)
                        .returns(t_class("DOMEntityReference"))
                        .body(vec![
                            s_if(
                                e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("name")]))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("name"), e_str(":")]), BinOp::StrictNotEq, e_bool(false))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("reference", e_new("DOMEntityReference", vec![e_var("name")])),
                            s_prop_assign(e_var("reference"), "ownerDocument", e_this()),
                            s_return(e_var("reference")),
                        ]),
                )
                .method(
                    method("createDocumentFragment")
                        .returns(t_class("DOMDocumentFragment"))
                        .body(vec![
                            s_assign("fragment", e_new("DOMDocumentFragment", vec![])),
                            s_prop_assign(e_var("fragment"), "ownerDocument", e_this()),
                            s_return(e_var("fragment")),
                        ]),
                )
                .method(
                    method("createAttribute")
                        .param("localName", TypeExpr::Str)
                        .returns(t_class("DOMAttr"))
                        .body(vec![
                            s_if(
                                e_binop(e_binop(e_binop(e_var("localName"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("localName")]))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("localName"), e_str(":")]), BinOp::StrictNotEq, e_bool(false))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("attr", e_new("DOMAttr", vec![e_var("localName")])),
                            s_prop_assign(e_var("attr"), "ownerDocument", e_this()),
                            s_return(e_var("attr")),
                        ]),
                )
                .method(
                    method("createAttributeNS")
                        .param("namespace", t_nullable(TypeExpr::Str))
                        .param("qualifiedName", TypeExpr::Str)
                        .returns(t_union(vec![t_class("DOMAttr"), TypeExpr::Bool]))
                        .body(vec![
                            s_assign("uri", e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_var("namespace"))),
                            s_if(
                                e_binop(e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("qualifiedName")]))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("colon", e_call("strpos", vec![e_var("qualifiedName"), e_str(":")])),
                            s_if(
                                e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)),
                                vec![
                                    s_assign("prefix", e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("colon")])),
                                    s_if(
                                        e_binop(e_binop(e_binop(e_binop(e_var("colon"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("colon"), BinOp::StrictEq, e_binop(e_call("strlen", vec![e_var("qualifiedName")]), BinOp::Sub, e_int(1)))), BinOp::Or, e_binop(e_call("strpos", vec![e_var("qualifiedName"), e_str(":"), e_binop(e_var("colon"), BinOp::Add, e_int(1))]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_str(""))),
                                        vec![
                                            s_throw(e_new("DOMException", vec![e_str("Namespace Error"), e_int(14)])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_binop(e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xml")), BinOp::And, e_binop(e_var("uri"), BinOp::StrictNotEq, e_str("http://www.w3.org/XML/1998/namespace"))), BinOp::Or, e_binop(e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xmlns")), BinOp::And, e_binop(e_var("uri"), BinOp::StrictNotEq, e_str("http://www.w3.org/2000/xmlns/")))),
                                        vec![
                                            s_throw(e_new("DOMException", vec![e_str("Namespace Error"), e_int(14)])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_var("uri"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_not(e_instance_of(e_this_prop("documentElement"), "DOMElement"))),
                                vec![
                                    s_expr(e_call("trigger_error", vec![e_str("DOMDocument::createAttributeNS(): Document Missing Root Element"), e_const("E_USER_WARNING")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("attr", e_new("DOMAttr", vec![e_var("qualifiedName")])),
                            s_prop_assign(e_var("attr"), "name", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_var("qualifiedName"), e_call("substr", vec![e_var("qualifiedName"), e_binop(e_var("colon"), BinOp::Add, e_int(1))]))),
                            s_if(
                                e_binop(e_var("uri"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_prop_assign(e_var("attr"), "__elephc_dom_ns", e_var("uri")),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_var("attr"), "ownerDocument", e_this()),
                            s_return(e_var("attr")),
                        ]),
                )
                .method(
                    method("importNode")
                        .param("node", t_class("DOMNode"))
                        .param_default("deep", TypeExpr::Bool, e_bool(false))
                        .returns(t_union(vec![t_class("DOMNode"), TypeExpr::Bool]))
                        .body(vec![
                            s_assign("kind", e_prop(e_var("node"), "nodeType")),
                            s_if(
                                e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(10))), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(13))),
                                vec![
                                    s_expr(e_call("trigger_error", vec![e_str("DOMDocument::importNode(): Cannot import: Node Type Not Supported"), e_const("E_USER_WARNING")])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_var("node"), "DOMAttr"),
                                vec![
                                    s_assign("copy", e_new("DOMAttr", vec![e_prop(e_var("node"), "nodeName"), e_cast(CastType::String, e_prop(e_var("node"), "nodeValue"))])),
                                    s_prop_assign(e_var("copy"), "name", e_prop(e_var("node"), "name")),
                                    s_prop_assign(e_var("copy"), "__elephc_dom_ns", e_prop(e_var("node"), "__elephc_dom_ns")),
                                    s_prop_assign(e_var("copy"), "ownerDocument", e_this()),
                                    s_return(e_var("copy")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("copy", e_method_call(e_var("node"), "cloneNode", vec![e_var("deep")])),
                            s_expr(e_method_call(e_var("copy"), "__elephc_dom_set_owner", vec![e_this()])),
                            s_return(e_var("copy")),
                        ]),
                )
                .method(
                    method("adoptNode")
                        .param("node", t_class("DOMNode"))
                        .returns(t_class("DOMNode"))
                        .body(vec![
                            s_assign("kind", e_prop(e_var("node"), "nodeType")),
                            s_if(
                                e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(10))), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(13))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Supported Error"), e_int(9)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_var("node"), "DOMAttr"),
                                vec![
                                    s_assign("owner", e_prop(e_var("node"), "ownerElement")),
                                    s_if(
                                        e_instance_of(e_var("owner"), "DOMElement"),
                                        vec![
                                            s_expr(e_method_call(e_var("owner"), "removeAttributeNode", vec![e_var("node")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![],
                                Some(vec![
                                s_assign("parent", e_prop(e_var("node"), "parentNode")),
                                s_if(
                                    e_instance_of(e_var("parent"), "DOMNode"),
                                    vec![
                                        s_expr(e_method_call(e_var("parent"), "removeChild", vec![e_var("node")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                            s_expr(e_method_call(e_var("node"), "__elephc_dom_set_owner", vec![e_this()])),
                            s_return(e_var("node")),
                        ]),
                )
                .method(
                    method("getElementById")
                        .param("elementId", TypeExpr::Str)
                        .returns(t_nullable(t_class("DOMElement")))
                        .body(vec![
                            s_return(e_call("__elephc_dom_find_id", vec![e_this(), e_var("elementId")])),
                        ]),
                )
                .method(
                    method("normalizeDocument")
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_expr(e_method_call(e_this(), "normalize", vec![])),
                        ]),
                )
                .method(
                    method("cloneNode")
                        .param_default("deep", TypeExpr::Bool, e_bool(false))
                        .returns(t_class("DOMDocument"))
                        .body(vec![
                            s_assign("decl", e_method_call(e_this(), "__elephc_dom_declaration", vec![])),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_binop(e_binop(e_var("deep"), BinOp::And, e_call("is_object", vec![e_var("bound")])), BinOp::And, e_not(e_this_prop("__elephc_dom_html"))),
                                vec![
                                    s_assign("copy", e_new("DOMDocument", vec![e_index(e_var("decl"), e_str("version")), e_ternary(e_binop(e_index(e_var("decl"), e_str("encoding")), BinOp::StrictEq, e_null()), e_str(""), e_index(e_var("decl"), e_str("encoding")))])),
                                    s_expr(e_method_call(e_var("copy"), "loadXML", vec![e_method_call(e_var("bound"), "__elephc_dom_document_xml", vec![])])),
                                ],
                                vec![],
                                Some(vec![
                                s_assign("copy", e_new("DOMDocument", vec![e_index(e_var("decl"), e_str("version")), e_ternary(e_binop(e_index(e_var("decl"), e_str("encoding")), BinOp::StrictEq, e_null()), e_str(""), e_index(e_var("decl"), e_str("encoding")))])),
                                s_prop_assign(e_var("copy"), "__elephc_dom_html", e_this_prop("__elephc_dom_html")),
                                s_if(
                                    e_var("deep"),
                                    vec![
                                        s_foreach(e_this_prop("childNodes"), None, "child", vec![
                                            s_assign("clone", e_method_call(e_var("child"), "cloneNode", vec![e_bool(true)])),
                                            s_expr(e_method_call(e_var("clone"), "__elephc_dom_set_owner", vec![e_var("copy")])),
                                            s_expr(e_method_call(e_var("copy"), "appendChild", vec![e_var("clone")])),
                                        ]),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                            s_prop_assign(e_var("copy"), "__elephc_dom_decl", e_var("decl")),
                            s_return(e_var("copy")),
                        ]),
                )
                .method(
                    method("validate")
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_expr(e_call("trigger_error", vec![e_str("DOMDocument::validate(): no DTD found!"), e_const("E_USER_WARNING")])),
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("schemaValidate")
                        .param("filename", TypeExpr::Str)
                        .param_default("flags", TypeExpr::Int, e_int(0))
                        .keep_unread_params()
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("schemaValidateSource")
                        .param("source", TypeExpr::Str)
                        .param_default("flags", TypeExpr::Int, e_int(0))
                        .keep_unread_params()
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("relaxNGValidate")
                        .param("filename", TypeExpr::Str)
                        .keep_unread_params()
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("relaxNGValidateSource")
                        .param("source", TypeExpr::Str)
                        .keep_unread_params()
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("xinclude")
                        .param_default("options", TypeExpr::Int, e_int(0))
                        .keep_unread_params()
                        .returns(t_mixed())
                        .body(vec![
                            s_return(e_bool(false)),
                        ]),
                )

        .prop("encoding", t_nullable(TypeExpr::Str), None)
        .hooked(true, true, false)
        .prop("xmlEncoding", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("version", t_nullable(TypeExpr::Str), None)
        .hooked(true, true, false)
        .prop("xmlVersion", t_nullable(TypeExpr::Str), None)
        .hooked(true, true, false)
        .prop("standalone", TypeExpr::Bool, None)
        .hooked(true, true, false)
        .prop("xmlStandalone", TypeExpr::Bool, None)
        .hooked(true, true, false)
        .prop("documentURI", t_nullable(TypeExpr::Str), None)
        .hooked(true, true, false)
        .prop("doctype", t_nullable(t_class("DOMDocumentType")), None)
        .hooked(true, false, false)
        .prop("implementation", t_class("DOMImplementation"), None)
        .hooked(true, false, false)
        .prop("preserveWhiteSpace", TypeExpr::Bool, Some(e_bool(true)))
        .prop("recover", TypeExpr::Bool, Some(e_bool(false)))
        .prop("resolveExternals", TypeExpr::Bool, Some(e_bool(false)))
        .prop("substituteEntities", TypeExpr::Bool, Some(e_bool(false)))
        .prop("validateOnParse", TypeExpr::Bool, Some(e_bool(false)))
        .prop("strictErrorChecking", TypeExpr::Bool, Some(e_bool(true)))
        .prop("config", t_mixed(), Some(e_null()))
        .prop("__elephc_dom_decl", t_mixed(), Some(e_null()))
        .prop("__elephc_dom_uri", t_nullable(TypeExpr::Str), Some(e_null()))
}

fn find_id_function() -> Stmt {
            function("__elephc_dom_find_id")
                .param("root", t_class("DOMNode"))
                .param("id", TypeExpr::Str)
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_foreach(e_prop(e_var("root"), "childNodes"), None, "child", vec![
                        s_if(
                            e_instance_of(e_var("child"), "DOMElement"),
                            vec![
                                s_foreach(e_method_call(e_var("child"), "__elephc_dom_attr_records", vec![e_bool(false)]), None, "record", vec![
                                    s_if(
                                        e_binop(e_binop(e_index(e_var("record"), e_int(5)), BinOp::StrictEq, e_var("id")), BinOp::And, e_method_call(e_var("child"), "__elephc_dom_is_id_name", vec![e_index(e_var("record"), e_int(1))])),
                                        vec![
                                            s_return(e_var("child")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                                s_assign("deeper", e_call("__elephc_dom_find_id", vec![e_var("child"), e_var("id")])),
                                s_if(
                                    e_binop(e_var("deeper"), BinOp::StrictNotEq, e_null()),
                                    vec![
                                        s_return(e_var("deeper")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_null()),
                ])
.build()
}

fn entity_reference_class() -> Stmt {
            class("DOMEntityReference")
                .extends("DOMNode")
                .method(
                    method("__construct")
                        .param("name", TypeExpr::Str)
                        .body(vec![
                            s_prop_assign(e_this(), "nodeName", e_var("name")),
                            s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                        ]),
                )
.build()
}

fn implementation_class() -> Stmt {
            class("DOMImplementation")
                .method(
                    method("hasFeature")
                        .param("feature", TypeExpr::Str)
                        .param("version", TypeExpr::Str)
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_assign("name", e_call("strtolower", vec![e_var("feature")])),
                            s_return(e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("core")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("xml"))), BinOp::And, e_binop(e_binop(e_binop(e_var("version"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("version"), BinOp::StrictEq, e_str("1.0"))), BinOp::Or, e_binop(e_var("version"), BinOp::StrictEq, e_str("2.0"))))),
                        ]),
                )
                .method(
                    method("createDocumentType")
                        .param("qualifiedName", TypeExpr::Str)
                        .param_default("publicId", TypeExpr::Str, e_str(""))
                        .param_default("systemId", TypeExpr::Str, e_str(""))
                        .returns(t_class("DOMDocumentType"))
                        .body(vec![
                            s_if(
                                e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("DOMImplementation::createDocumentType(): Argument #1 ($qualifiedName) must not be empty")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_new("DOMDocumentType", vec![e_var("qualifiedName"), e_var("publicId"), e_var("systemId")])),
                        ]),
                )
                .method(
                    method("createDocument")
                        .param_default("namespace", t_nullable(TypeExpr::Str), e_null())
                        .param_default("qualifiedName", TypeExpr::Str, e_str(""))
                        .param_default("doctype", t_nullable(t_class("DOMDocumentType")), e_null())
                        .returns(t_class("DOMDocument"))
                        .body(vec![
                            s_assign("document", e_new("DOMDocument", vec![])),
                            s_if(
                                e_binop(e_var("doctype"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_expr(e_method_call(e_var("document"), "appendChild", vec![e_var("doctype")])),
                                    s_prop_assign(e_var("doctype"), "ownerDocument", e_var("document")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("qualifiedName"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_expr(e_method_call(e_var("document"), "appendChild", vec![e_method_call(e_var("document"), "createElementNS", vec![e_var("namespace"), e_var("qualifiedName")])])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("document")),
                        ]),
                )
.build()
}


/// Registers this package's declarations.
pub(super) fn contribute(contributions: &mut Contributions) {
    contributions.add_class(Part::Walker, entity_reference_class());
    contributions.add_class(Part::Walker, implementation_class());
    contributions.add_function(Part::Walker, collect_ns_function());
    contributions.add_function(Part::Walker, find_id_function());
    contributions.members(Part::Walker, node_members());
    contributions.members(Part::Walker, element_members());
    contributions.members(Part::Walker, document_members());
}
