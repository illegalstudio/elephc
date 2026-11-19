//! Purpose:
//! Package P4 of the DOM migration: `DOMNode` tree mutation, the node-kind and namespace
//! properties, and the native bridge entry points they use.
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - The tree members (`appendChild`, `insertBefore`, `removeChild`, `replaceChild`, `normalize`,
//!   `cloneNode`, ...) work on both node models of the DOM prelude. A node bound to the shared
//!   libxml2 tree (`__elephc_simplexml`) is relinked natively through `SimpleXMLElement::__elephc_dom_tree_op`
//!   (libxml2's non-merging `xmlAddPrevSibling` / `xmlAddNextSibling`, so the PHP object keeps its
//!   native node); a free-standing node keeps its children in `childNodes`. Placing an eager node
//!   under a bound parent materializes it first (`__elephc_dom_append_child`), then moves it before
//!   the reference child.
//! - Free-standing links point upward weakly (`parentNode`, `previousSibling`, `nextSibling` store
//!   a `WeakReference`, see the hooks in `generated.rs`), so a tree built with `createElement` and
//!   `appendChild` is freed with its owner instead of leaking through a parent/child cycle.
//! - `__elephc_dom_ns` (a free-standing element's namespace URI) is set by the element package.
//! - The method bodies were written as PHP and converted with `synthetic_class::transcribe`.

use super::extend::{Contributions, Part};
use crate::parser::ast::{BinOp, CastType, TypeExpr};
use crate::synthetic_class::*;

fn tree() -> ClassBuilder {
            class("DOMNode")
                .method(
                    method("__propget_nodeType")
                        .returns(TypeExpr::Int)
                        .body(vec![
                            s_if(
                                e_instance_of(e_this(), "DOMDocument"),
                                vec![
                                    // An HTML document is XML_HTML_DOCUMENT_NODE (13).
                                    s_return(e_ternary(e_prop(e_this(), "__elephc_dom_html"), e_int(13), e_int(9))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMElement"),
                                vec![
                                    s_return(e_int(1)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMCdataSection"),
                                vec![
                                    s_return(e_int(4)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMText"),
                                vec![
                                    s_return(e_int(3)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMComment"),
                                vec![
                                    s_return(e_int(8)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMProcessingInstruction"),
                                vec![
                                    s_return(e_int(7)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMAttr"),
                                vec![
                                    s_return(e_int(2)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMDocumentFragment"),
                                vec![
                                    s_return(e_int(11)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMDocumentType"),
                                vec![
                                    s_return(e_int(10)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMEntityReference"),
                                vec![
                                    s_return(e_int(5)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_int(0)),
                        ]),
                )
                .method(
                    method("__propget_firstChild")
                        .returns(t_mixed())
                        .body(vec![
                            s_assign("list", e_this_prop("childNodes")),
                            s_if(
                                e_binop(e_instance_of(e_var("list"), "DOMNodeList"), BinOp::And, e_binop(e_prop(e_var("list"), "length"), BinOp::Gt, e_int(0))),
                                vec![
                                    s_return(e_method_call(e_var("list"), "item", vec![e_int(0)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("__propget_lastChild")
                        .returns(t_mixed())
                        .body(vec![
                            s_assign("list", e_this_prop("childNodes")),
                            s_if(
                                e_binop(e_instance_of(e_var("list"), "DOMNodeList"), BinOp::And, e_binop(e_prop(e_var("list"), "length"), BinOp::Gt, e_int(0))),
                                vec![
                                    s_return(e_method_call(e_var("list"), "item", vec![e_binop(e_prop(e_var("list"), "length"), BinOp::Sub, e_int(1))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("__propget_localName")
                        .returns(t_mixed())
                        .body(vec![
                            s_assign("kind", e_this_prop("nodeType")),
                            s_if(
                                e_binop(e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(2))),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_binop(e_call("is_object", vec![e_var("bound")]), BinOp::And, e_binop(e_var("kind"), BinOp::StrictEq, e_int(1))),
                                vec![
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_local_name", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("name", e_this_prop("nodeName")),
                            s_assign("htmlOwner", e_this_prop("ownerDocument")),
                            s_if(
                                e_binop(e_instance_of(e_var("htmlOwner"), "DOMDocument"), BinOp::And, e_prop(e_var("htmlOwner"), "__elephc_dom_html")),
                                vec![
                                    s_return(e_var("name")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                            s_if(
                                e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_return(e_var("name")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("substr", vec![e_var("name"), e_binop(e_var("colon"), BinOp::Add, e_int(1))])),
                        ]),
                )
                .method(
                    method("__propget_prefix")
                        .returns(t_mixed())
                        .body(vec![
                            s_assign("kind", e_this_prop("nodeType")),
                            s_if(
                                e_binop(e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(2))),
                                vec![
                                    s_return(e_str("")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("name", e_this_prop("nodeName")),
                            s_assign("htmlOwner", e_this_prop("ownerDocument")),
                            s_if(
                                e_binop(e_instance_of(e_var("htmlOwner"), "DOMDocument"), BinOp::And, e_prop(e_var("htmlOwner"), "__elephc_dom_html")),
                                vec![
                                    s_return(e_str("")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                            s_if(
                                e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_return(e_str("")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("substr", vec![e_var("name"), e_int(0), e_var("colon")])),
                        ]),
                )
                .method(
                    method("__propget_namespaceURI")
                        .returns(t_mixed())
                        .body(vec![
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_if(
                                        e_binop(e_this_prop("nodeType"), BinOp::StrictNotEq, e_int(1)),
                                        vec![
                                            s_return(e_null()),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("uri", e_method_call(e_var("bound"), "__elephc_dom_ns_op", vec![e_int(0), e_str("")])),
                                    s_if(
                                        e_binop(e_var("uri"), BinOp::StrictEq, e_str("")),
                                        vec![
                                            s_return(e_null()),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_var("uri")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMElement"),
                                vec![
                                    s_return(e_this_prop("__elephc_dom_ns")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_this(), "DOMAttr"),
                                vec![
                                    s_return(e_this_prop("__elephc_dom_ns")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("__elephc_dom_kids")
                        .returns(t_array())
                        .body(vec![
                            s_assign("out", e_array(vec![])),
                            s_assign("list", e_this_prop("childNodes")),
                            s_if(
                                e_instance_of(e_var("list"), "DOMNodeList"),
                                vec![
                                    s_foreach(e_var("list"), None, "kid", vec![
                                        s_array_push("out", e_var("kid")),
                                    ]),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("out")),
                        ]),
                )
                .method(
                    method("__elephc_dom_set_kids")
                        .param("items", t_array())
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_var("items")])),
                            s_assign("previous", e_null()),
                            s_foreach(e_var("items"), None, "item", vec![
                                s_prop_assign(e_var("item"), "previousSibling", e_var("previous")),
                                s_prop_assign(e_var("item"), "nextSibling", e_null()),
                                s_if(
                                    e_binop(e_var("previous"), BinOp::StrictNotEq, e_null()),
                                    vec![
                                        s_prop_assign(e_var("previous"), "nextSibling", e_var("item")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("previous", e_var("item")),
                            ]),
                        ]),
                )
                .method(
                    method("__elephc_dom_unlink")
                        .param("node", t_class("DOMNode"))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("out", e_array(vec![])),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_kids", vec![]), None, "kid", vec![
                                s_if(
                                    e_binop(e_var("kid"), BinOp::StrictNotEq, e_var("node")),
                                    vec![
                                        s_array_push("out", e_var("kid")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_expr(e_method_call(e_this(), "__elephc_dom_set_kids", vec![e_var("out")])),
                            s_prop_assign(e_var("node"), "parentNode", e_null()),
                            s_prop_assign(e_var("node"), "previousSibling", e_null()),
                            s_prop_assign(e_var("node"), "nextSibling", e_null()),
                            // A detached node anchors its document again (attached nodes hold it weakly).
                            s_prop_assign(e_var("node"), "ownerDocument", e_prop(e_var("node"), "ownerDocument")),
                        ]),
                )
                .method(
                    method("__elephc_dom_set_owner")
                        .param("document", t_class("DOMNode"))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_prop_assign(e_this(), "ownerDocument", e_var("document")),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_kids", vec![]), None, "kid", vec![
                                s_expr(e_method_call(e_var("kid"), "__elephc_dom_set_owner", vec![e_var("document")])),
                            ]),
                        ]),
                )
                .method(
                    method("__elephc_dom_place")
                        .param("node", t_class("DOMNode"))
                        .param("child", t_nullable(t_class("DOMNode")))
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_assign("nodeBound", e_prop(e_var("node"), "__elephc_simplexml")),
                            s_assign("mine", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_if(
                                        e_not(e_call("is_object", vec![e_var("nodeBound")])),
                                        vec![
                                            s_assign("old", e_prop(e_var("node"), "parentNode")),
                                            s_if(
                                                e_instance_of(e_var("old"), "DOMNode"),
                                                vec![
                                                    s_expr(e_method_call(e_var("old"), "__elephc_dom_unlink", vec![e_var("node")])),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                            s_expr(e_method_call(e_var("bound"), "__elephc_dom_append_child", vec![e_this(), e_var("node")])),
                                            s_assign("nodeBound", e_prop(e_var("node"), "__elephc_simplexml")),
                                            s_if(
                                                e_binop(e_var("mine"), BinOp::StrictNotEq, e_null()),
                                                vec![s_prop_assign(e_var("node"), "ownerDocument", e_var("mine"))],
                                                vec![],
                                                None,
                                            ),
                                            s_if(
                                                e_not(e_call("is_object", vec![e_var("child")])),
                                                vec![
                                                    s_return_void(),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("before", e_int(0)),
                                    s_if(
                                        e_call("is_object", vec![e_var("child")]),
                                        vec![
                                            s_assign("before", e_method_call(e_prop(e_var("child"), "__elephc_simplexml"), "__elephc_dom_identity", vec![])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("document", e_int(0)),
                                    s_if(
                                        e_instance_of(e_this(), "DOMDocument"),
                                        vec![
                                            s_assign("document", e_int(1)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_method_call(e_var("bound"), "__elephc_dom_tree_op", vec![e_int(1), e_method_call(e_var("nodeBound"), "__elephc_dom_identity", vec![]), e_var("before"), e_var("document")]), BinOp::StrictEq, e_int(0)),
                                        vec![
                                            s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_call("is_object", vec![e_var("nodeBound")]),
                                vec![
                                    s_expr(e_method_call(e_var("nodeBound"), "__elephc_dom_tree_op", vec![e_int(2), e_method_call(e_var("nodeBound"), "__elephc_dom_identity", vec![]), e_int(0), e_int(0)])),
                                ],
                                vec![],
                                Some(vec![
                                s_assign("old", e_prop(e_var("node"), "parentNode")),
                                s_if(
                                    e_instance_of(e_var("old"), "DOMNode"),
                                    vec![
                                        s_expr(e_method_call(e_var("old"), "__elephc_dom_unlink", vec![e_var("node")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                            s_assign("out", e_array(vec![])),
                            s_assign("placed", e_bool(false)),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_kids", vec![]), None, "kid", vec![
                                s_if(
                                    e_binop(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("kid"), BinOp::StrictEq, e_var("child"))),
                                    vec![
                                        s_array_push("out", e_var("node")),
                                        s_assign("placed", e_bool(true)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_array_push("out", e_var("kid")),
                            ]),
                            s_if(
                                e_not(e_var("placed")),
                                vec![
                                    s_array_push("out", e_var("node")),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "__elephc_dom_set_kids", vec![e_var("out")])),
                            s_prop_assign(e_var("node"), "parentNode", e_this()),
                            s_if(
                                e_binop(e_binop(e_var("mine"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("node"), "ownerDocument"), BinOp::StrictEq, e_null())),
                                vec![
                                    s_expr(e_method_call(e_var("node"), "__elephc_dom_set_owner", vec![e_var("mine")])),
                                ],
                                vec![],
                                None,
                            ),
                            // An attached node holds its document weakly (the document owns the tree).
                            s_prop_assign(e_var("node"), "ownerDocument", e_prop(e_var("node"), "ownerDocument")),
                        ]),
                )
                .method(
                    method("__elephc_dom_insert")
                        .param("node", t_class("DOMNode"))
                        .param("child", t_nullable(t_class("DOMNode")))
                        .param("method", TypeExpr::Str)
                        .returns(t_class("DOMNode"))
                        .body(vec![
                            s_assign("kind", e_this_prop("nodeType")),
                            s_if(
                                e_binop(e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(9))), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(13))), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(11))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("nodeKind", e_prop(e_var("node"), "nodeType")),
                            // appendChild of an attribute node sets it on the element, as php does
                            s_if(
                                e_binop(e_binop(e_binop(e_var("nodeKind"), BinOp::StrictEq, e_int(2)), BinOp::And, e_binop(e_var("kind"), BinOp::StrictEq, e_int(1))), BinOp::And, e_binop(e_var("child"), BinOp::StrictEq, e_null())),
                                vec![
                                    s_assign("self", e_this()),
                                    s_if(
                                        e_instance_of(e_var("self"), "DOMElement"),
                                        vec![s_expr(e_method_call(e_var("self"), "setAttributeNode", vec![e_var("node")]))],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_var("node")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_binop(e_var("nodeKind"), BinOp::StrictEq, e_int(2)), BinOp::Or, e_binop(e_var("nodeKind"), BinOp::StrictEq, e_int(9))), BinOp::Or, e_binop(e_var("nodeKind"), BinOp::StrictEq, e_int(13))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("walk", e_this()),
                            s_while(e_binop(e_var("walk"), BinOp::StrictNotEq, e_null()), vec![
                                s_if(
                                    e_binop(e_var("walk"), BinOp::StrictEq, e_var("node")),
                                    vec![
                                        s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("walk", e_prop(e_var("walk"), "parentNode")),
                            ]),
                            s_if(
                                e_binop(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("child"), "parentNode"), BinOp::StrictNotEq, e_this())),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("mine", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                            s_assign("theirs", e_prop(e_var("node"), "ownerDocument")),
                            s_if(
                                e_binop(e_binop(e_binop(e_var("mine"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("theirs"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_var("mine"), BinOp::StrictNotEq, e_var("theirs"))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("child"), BinOp::StrictEq, e_var("node")),
                                vec![
                                    // php unlinks the node before it notices it is its own reference.
                                    s_assign("owner", e_prop(e_var("node"), "parentNode")),
                                    s_if(
                                        e_instance_of(e_var("owner"), "DOMNode"),
                                        vec![s_expr(e_method_call(e_var("owner"), "removeChild", vec![e_var("node")]))],
                                        vec![],
                                        None,
                                    ),
                                    s_throw(e_new("Error", vec![e_str("Cannot add newnode as the previous sibling of refnode")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("nodeKind"), BinOp::StrictNotEq, e_int(11)),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_dom_place", vec![e_var("node"), e_var("child")])),
                                    s_return(e_var("node")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("parts", e_method_call(e_var("node"), "__elephc_dom_kids", vec![])),
                            s_if(
                                e_binop(e_call("count", vec![e_var("parts")]), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_expr(e_call("trigger_error", vec![e_binop(e_binop(e_str("DOMNode::"), BinOp::Concat, e_var("method")), BinOp::Concat, e_str("(): Document Fragment is empty")), e_const("E_USER_WARNING")])),
                                    s_return(e_var("node")),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_var("parts"), None, "part", vec![
                                s_expr(e_method_call(e_this(), "__elephc_dom_place", vec![e_var("part"), e_var("child")])),
                            ]),
                            s_return(e_index(e_var("parts"), e_int(0))),
                        ]),
                )
                .method(
                    method("appendChild")
                        .param("node", t_class("DOMNode"))
                        .returns(t_class("DOMNode"))
                        .body(vec![
                            s_return(e_method_call(e_this(), "__elephc_dom_insert", vec![e_var("node"), e_null(), e_str("appendChild")])),
                        ]),
                )
                .method(
                    method("insertBefore")
                        .param("node", t_class("DOMNode"))
                        .param_default("child", t_nullable(t_class("DOMNode")), e_null())
                        .returns(t_class("DOMNode"))
                        .body(vec![
                            s_return(e_method_call(e_this(), "__elephc_dom_insert", vec![e_var("node"), e_var("child"), e_str("insertBefore")])),
                        ]),
                )
                .method(
                    method("removeChild")
                        .param("child", t_class("DOMNode"))
                        .returns(t_class("DOMNode"))
                        .body(vec![
                            s_if(
                                e_binop(e_prop(e_var("child"), "parentNode"), BinOp::StrictNotEq, e_this()),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_assign("childBound", e_prop(e_var("child"), "__elephc_simplexml")),
                                    s_expr(e_method_call(e_var("bound"), "__elephc_dom_tree_op", vec![e_int(2), e_method_call(e_var("childBound"), "__elephc_dom_identity", vec![]), e_int(0), e_int(0)])),
                                    s_return(e_var("child")),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "__elephc_dom_unlink", vec![e_var("child")])),
                            s_return(e_var("child")),
                        ]),
                )
                .method(
                    method("replaceChild")
                        .param("node", t_class("DOMNode"))
                        .param("child", t_class("DOMNode"))
                        .returns(t_union(vec![t_class("DOMNode"), TypeExpr::Bool]))
                        .body(vec![
                            s_assign("kind", e_this_prop("nodeType")),
                            s_if(
                                e_binop(e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(9))), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(13))), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(11))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("mine", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                            s_assign("theirs", e_prop(e_var("node"), "ownerDocument")),
                            s_if(
                                e_binop(e_binop(e_binop(e_var("mine"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("theirs"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_var("mine"), BinOp::StrictNotEq, e_var("theirs"))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_this_prop("firstChild"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_prop(e_var("child"), "parentNode"), BinOp::StrictNotEq, e_this()),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("node"), BinOp::StrictEq, e_var("child")),
                                vec![
                                    s_return(e_var("child")),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "__elephc_dom_insert", vec![e_var("node"), e_var("child"), e_str("replaceChild")])),
                            s_expr(e_method_call(e_this(), "removeChild", vec![e_var("child")])),
                            s_return(e_var("child")),
                        ]),
                )
                .method(
                    method("hasChildNodes")
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_return(e_binop(e_this_prop("firstChild"), BinOp::StrictNotEq, e_null())),
                        ]),
                )
                .method(
                    method("isSameNode")
                        .param("otherNode", t_class("DOMNode"))
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_return(e_binop(e_this(), BinOp::StrictEq, e_var("otherNode"))),
                        ]),
                )
                .method(
                    method("contains")
                        .param("other", t_nullable(t_class("DOMNode")))
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_assign("walk", e_var("other")),
                            s_while(e_binop(e_var("walk"), BinOp::StrictNotEq, e_null()), vec![
                                s_if(
                                    e_binop(e_var("walk"), BinOp::StrictEq, e_this()),
                                    vec![
                                        s_return(e_bool(true)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("walk", e_prop(e_var("walk"), "parentNode")),
                            ]),
                            s_return(e_bool(false)),
                        ]),
                )
                .method(
                    method("getRootNode")
                        .param_default("options", t_nullable(t_array()), e_null())
                        .keep_unread_params()
                        .returns(t_class("DOMNode"))
                        .body(vec![
                            s_assign("root", e_this()),
                            s_while(e_binop(e_prop(e_var("root"), "parentNode"), BinOp::StrictNotEq, e_null()), vec![
                                s_assign("root", e_prop(e_var("root"), "parentNode")),
                            ]),
                            s_return(e_var("root")),
                        ]),
                )
                .method(
                    method("getLineNo")
                        .returns(TypeExpr::Int)
                        .body(vec![
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_tree_op", vec![e_int(3), e_method_call(e_var("bound"), "__elephc_dom_identity", vec![]), e_int(0), e_int(0)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_int(0)),
                        ]),
                )
                .method(
                    method("lookupNamespaceURI")
                        .param("prefix", t_nullable(TypeExpr::Str))
                        .returns(t_nullable(TypeExpr::Str))
                        .body(vec![
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                            s_if(
                                        e_binop(e_var("prefix"), BinOp::StrictEq, e_null()),
                                        vec![
                                            s_assign("uri", e_method_call(e_var("bound"), "__elephc_dom_ns_op", vec![e_int(3), e_str("")])),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_assign("uri", e_method_call(e_var("bound"), "__elephc_dom_ns_op", vec![e_int(2), e_var("prefix")])),
                                    ]),
                                    ),
                                    s_if(
                                        e_binop(e_var("uri"), BinOp::StrictEq, e_str("")),
                                        vec![
                                            s_return(e_null()),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_var("uri")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xml")),
                                vec![
                                    s_return(e_str("http://www.w3.org/XML/1998/namespace")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("key", e_ternary(e_binop(e_var("prefix"), BinOp::StrictEq, e_null()), e_str(""), e_var("prefix"))),
                            s_assign("node", e_this()),
                            s_if(
                                e_instance_of(e_var("node"), "DOMDocument"),
                                vec![
                                    s_assign("node", e_prop(e_var("node"), "documentElement")),
                                ],
                                vec![],
                                None,
                            ),
                            s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                                s_if(
                                    e_instance_of(e_var("node"), "DOMElement"),
                                    vec![
                                        s_assign("declared", e_prop(e_var("node"), "__elephc_dom_xmlns")),
                                        s_if(
                                            e_binop(e_binop(e_var("key"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_var("declared"), BinOp::StrictNotEq, e_null())),
                                            vec![
                                                s_if(
                                                    e_binop(e_var("declared"), BinOp::StrictEq, e_str("")),
                                                    vec![
                                                        s_return(e_null()),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                                s_return(e_var("declared")),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_assign("uri", e_prop(e_var("node"), "__elephc_dom_ns")),
                                        s_if(
                                            e_binop(e_binop(e_var("uri"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("node"), "prefix"), BinOp::StrictEq, e_var("key"))),
                                            vec![
                                                s_return(e_var("uri")),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("node", e_prop(e_var("node"), "parentNode")),
                            ]),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("lookupPrefix")
                        .param("namespace", TypeExpr::Str)
                        .returns(t_nullable(TypeExpr::Str))
                        .body(vec![
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_assign("found", e_method_call(e_var("bound"), "__elephc_dom_ns_op", vec![e_int(4), e_var("namespace")])),
                                    s_if(
                                        e_binop(e_var("found"), BinOp::StrictEq, e_str("")),
                                        vec![
                                            s_return(e_null()),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_var("found")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("namespace"), BinOp::StrictEq, e_str("")),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("namespace"), BinOp::StrictEq, e_str("http://www.w3.org/XML/1998/namespace")),
                                vec![
                                    s_return(e_str("xml")),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("node", e_this()),
                            s_if(
                                e_instance_of(e_var("node"), "DOMDocument"),
                                vec![
                                    s_assign("node", e_prop(e_var("node"), "documentElement")),
                                ],
                                vec![],
                                None,
                            ),
                            s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                                s_if(
                                    e_instance_of(e_var("node"), "DOMElement"),
                                    vec![
                                        s_if(
                                            e_binop(e_binop(e_prop(e_var("node"), "__elephc_dom_ns"), BinOp::StrictEq, e_var("namespace")), BinOp::And, e_binop(e_prop(e_var("node"), "prefix"), BinOp::StrictNotEq, e_str(""))),
                                            vec![
                                                s_return(e_prop(e_var("node"), "prefix")),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("node", e_prop(e_var("node"), "parentNode")),
                            ]),
                            s_return(e_null()),
                        ]),
                )
                .method(
                    method("isDefaultNamespace")
                        .param("namespace", TypeExpr::Str)
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_return(e_binop(e_method_call(e_this(), "lookupNamespaceURI", vec![e_null()]), BinOp::StrictEq, e_var("namespace"))),
                        ]),
                )
                .method(
                    method("normalize")
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_assign("previous", e_null()),
                            s_foreach(e_method_call(e_this(), "__elephc_dom_kids", vec![]), None, "kid", vec![
                                s_assign("kind", e_prop(e_var("kid"), "nodeType")),
                                s_if(
                                    e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                                    vec![
                                        s_assign("text", e_prop(e_var("kid"), "nodeValue")),
                                        s_if(
                                            e_binop(e_var("text"), BinOp::StrictEq, e_str("")),
                                            vec![
                                                s_expr(e_method_call(e_this(), "removeChild", vec![e_var("kid")])),
                                            ],
                                            vec![
                                            (e_binop(e_var("previous"), BinOp::StrictNotEq, e_null()), vec![
                                                s_prop_assign(e_var("previous"), "nodeValue", e_binop(e_prop(e_var("previous"), "nodeValue"), BinOp::Concat, e_var("text"))),
                                                s_expr(e_method_call(e_this(), "removeChild", vec![e_var("kid")])),
                                            ]),
                                        ],
                                            Some(vec![
                                            s_assign("previous", e_var("kid")),
                                        ]),
                                        ),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("previous", e_null()),
                                    s_if(
                                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                                        vec![
                                            s_expr(e_method_call(e_var("kid"), "normalize", vec![])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                                ),
                            ]),
                        ]),
                )
                .method(
                    method("getNodePath")
                        .returns(t_nullable(TypeExpr::Str))
                        .body(vec![
                            s_assign("parent", e_this_prop("parentNode")),
                            s_if(
                                e_binop(e_var("parent"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_return(e_ternary(e_instance_of(e_this(), "DOMDocument"), e_str("/"), e_null())),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("parentPath", e_method_call(e_var("parent"), "getNodePath", vec![])),
                            s_assign("kind", e_this_prop("nodeType")),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                                vec![
                                    s_return(e_binop(e_binop(e_var("parentPath"), BinOp::Concat, e_str("/@")), BinOp::Concat, e_this_prop("nodeName"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("step", e_str("")),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                                vec![
                                    s_assign("step", e_this_prop("nodeName")),
                                    s_if(
                                        e_binop(e_binop(e_this_prop("namespaceURI"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_this_prop("prefix"), BinOp::StrictEq, e_str(""))),
                                        vec![
                                            s_assign("step", e_str("*")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ],
                                vec![
                                (e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4))), vec![
                                    s_assign("step", e_str("text()")),
                                ]),
                                (e_binop(e_var("kind"), BinOp::StrictEq, e_int(8)), vec![
                                    s_assign("step", e_str("comment()")),
                                ]),
                                (e_binop(e_var("kind"), BinOp::StrictEq, e_int(7)), vec![
                                    s_assign("step", e_binop(e_binop(e_str("processing-instruction('"), BinOp::Concat, e_this_prop("nodeName")), BinOp::Concat, e_str("')"))),
                                ]),
                            ],
                                None,
                            ),
                            s_assign("same", e_int(0)),
                            s_assign("index", e_int(0)),
                            s_foreach(e_method_call(e_var("parent"), "__elephc_dom_kids", vec![]), None, "sibling", vec![
                                s_assign("siblingKind", e_prop(e_var("sibling"), "nodeType")),
                                s_assign("alike", e_binop(e_binop(e_var("siblingKind"), BinOp::StrictEq, e_var("kind")), BinOp::Or, e_binop(e_binop(e_binop(e_var("siblingKind"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("siblingKind"), BinOp::StrictEq, e_int(4))), BinOp::And, e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)))))),
                                s_if(
                                    e_binop(e_var("alike"), BinOp::And, e_binop(e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(1)), BinOp::Or, e_ternary(e_binop(e_var("step"), BinOp::StrictEq, e_str("*")), e_binop(e_binop(e_prop(e_var("sibling"), "namespaceURI"), BinOp::StrictEq, e_this_prop("namespaceURI")), BinOp::And, e_binop(e_prop(e_var("sibling"), "prefix"), BinOp::StrictEq, e_str(""))), e_binop(e_prop(e_var("sibling"), "nodeName"), BinOp::StrictEq, e_this_prop("nodeName"))))),
                                    vec![
                                        s_expr(e_post_inc("same")),
                                        s_if(
                                            e_binop(e_var("sibling"), BinOp::StrictEq, e_this()),
                                            vec![
                                                s_assign("index", e_var("same")),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_assign("suffix", e_ternary(e_binop(e_var("same"), BinOp::Gt, e_int(1)), e_binop(e_binop(e_str("["), BinOp::Concat, e_var("index")), BinOp::Concat, e_str("]")), e_str(""))),
                            s_if(
                                e_binop(e_var("parentPath"), BinOp::StrictEq, e_str("/")),
                                vec![
                                    s_return(e_binop(e_binop(e_str("/"), BinOp::Concat, e_var("step")), BinOp::Concat, e_var("suffix"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_binop(e_binop(e_binop(e_var("parentPath"), BinOp::Concat, e_str("/")), BinOp::Concat, e_var("step")), BinOp::Concat, e_var("suffix"))),
                        ]),
                )
                .method(
                    method("__elephc_dom_attr_pairs")
                        .returns(t_array())
                        .body(vec![
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_call("is_object", vec![e_var("bound")]),
                                vec![
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_attributes", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("pairs", e_this_prop("__attrs")),
                            s_if(
                                e_call("is_array", vec![e_var("pairs")]),
                                vec![
                                    s_return(e_var("pairs")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_array(vec![])),
                        ]),
                )
                .method(
                    method("__elephc_dom_shallow_copy")
                        .returns(t_class("DOMNode"))
                        .body(vec![
                            s_assign("kind", e_this_prop("nodeType")),
                            s_assign("document", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                                vec![
                                    s_assign("copy", e_new("DOMElement", vec![])),
                                    s_prop_assign(e_var("copy"), "nodeName", e_this_prop("nodeName")),
                                    s_prop_assign(e_var("copy"), "childNodes", e_new("DOMNodeList", vec![e_array(vec![])])),
                                    // The attribute slots keep their namespace URI (element package P5).
                                    s_assign("pairs", e_array(vec![])),
                                    s_foreach(e_method_call(e_this(), "__elephc_dom_attr_records", vec![e_bool(false)]), None, "record", vec![
                                        s_if(
                                            e_binop(e_index(e_var("record"), e_int(4)), BinOp::StrictEq, e_str("")),
                                            vec![s_array_push("pairs", e_array(vec![e_index(e_var("record"), e_int(1)), e_index(e_var("record"), e_int(5))]))],
                                            vec![],
                                            Some(vec![s_array_push("pairs", e_array(vec![e_index(e_var("record"), e_int(1)), e_index(e_var("record"), e_int(5)), e_index(e_var("record"), e_int(4))]))]),
                                        ),
                                    ]),
                                    s_prop_assign(e_var("copy"), "__attrs", e_var("pairs")),
                                    s_assign("declared", e_this_prop("__elephc_dom_xmlns")),
                                    s_if(
                                        e_binop(e_binop(e_var("declared"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_call("is_object", vec![e_this_prop("__elephc_simplexml")]))),
                                        vec![
                                            s_expr(e_method_call(e_var("copy"), "setAttribute", vec![e_str("xmlns"), e_var("declared")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_prop_assign(e_var("copy"), "__elephc_dom_ns", e_this_prop("namespaceURI")),
                                    s_return(e_var("copy")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)),
                                vec![
                                    s_return(e_new("DOMCdataSection", vec![e_cast(CastType::String, e_this_prop("nodeValue"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                                vec![
                                    s_return(e_new("DOMText", vec![e_cast(CastType::String, e_this_prop("nodeValue"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(8)),
                                vec![
                                    s_return(e_new("DOMComment", vec![e_cast(CastType::String, e_this_prop("nodeValue"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(7)),
                                vec![
                                    s_return(e_new("DOMProcessingInstruction", vec![e_this_prop("nodeName"), e_cast(CastType::String, e_this_prop("nodeValue"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(11)),
                                vec![
                                    s_return(e_new("DOMDocumentFragment", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new("DOMException", vec![e_str("Not Supported Error"), e_int(9)])),
                        ]),
                )
                .method(
                    method("cloneNode")
                        .param_default("deep", TypeExpr::Bool, e_bool(false))
                        .returns(t_class("DOMNode"))
                        .body(vec![
                            s_assign("copy", e_method_call(e_this(), "__elephc_dom_shallow_copy", vec![])),
                            s_assign("document", e_ternary(e_instance_of(e_this(), "DOMDocument"), e_this(), e_this_prop("ownerDocument"))),
                            s_if(
                                e_binop(e_var("document"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_prop_assign(e_var("copy"), "ownerDocument", e_var("document")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_var("deep"),
                                vec![
                                    s_foreach(e_method_call(e_this(), "__elephc_dom_kids", vec![]), None, "kid", vec![
                                        s_expr(e_method_call(e_var("copy"), "appendChild", vec![e_method_call(e_var("kid"), "cloneNode", vec![e_bool(true)])])),
                                    ]),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("copy")),
                        ]),
                )
                .method(
                    method("isEqualNode")
                        .param("otherNode", t_nullable(t_class("DOMNode")))
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_if(
                                e_binop(e_var("otherNode"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_this(), BinOp::StrictEq, e_var("otherNode")),
                                vec![
                                    s_return(e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("kind", e_this_prop("nodeType")),
                            s_if(
                                e_binop(e_binop(e_var("kind"), BinOp::StrictNotEq, e_prop(e_var("otherNode"), "nodeType")), BinOp::Or, e_binop(e_this_prop("nodeName"), BinOp::StrictNotEq, e_prop(e_var("otherNode"), "nodeName"))),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(9))), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(13))), BinOp::And, e_binop(e_var("kind"), BinOp::StrictNotEq, e_int(11))),
                                vec![
                                    s_return(e_binop(e_this_prop("nodeValue"), BinOp::StrictEq, e_prop(e_var("otherNode"), "nodeValue"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                                vec![
                                    s_if(
                                        e_binop(e_this_prop("namespaceURI"), BinOp::StrictNotEq, e_prop(e_var("otherNode"), "namespaceURI")),
                                        vec![
                                            s_return(e_bool(false)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("mine", e_method_call(e_this(), "__elephc_dom_attr_pairs", vec![])),
                                    s_assign("theirs", e_method_call(e_var("otherNode"), "__elephc_dom_attr_pairs", vec![])),
                                    s_if(
                                        e_binop(e_call("count", vec![e_var("mine")]), BinOp::StrictNotEq, e_call("count", vec![e_var("theirs")])),
                                        vec![
                                            s_return(e_bool(false)),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_foreach(e_var("mine"), None, "pair", vec![
                                        s_assign("found", e_bool(false)),
                                        s_foreach(e_var("theirs"), None, "other", vec![
                                            s_if(
                                                e_binop(e_binop(e_cast(CastType::String, e_index(e_var("other"), e_int(0))), BinOp::StrictEq, e_cast(CastType::String, e_index(e_var("pair"), e_int(0)))), BinOp::And, e_binop(e_cast(CastType::String, e_index(e_var("other"), e_int(1))), BinOp::StrictEq, e_cast(CastType::String, e_index(e_var("pair"), e_int(1))))),
                                                vec![
                                                    s_assign("found", e_bool(true)),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                        ]),
                                        s_if(
                                            e_not(e_var("found")),
                                            vec![
                                                s_return(e_bool(false)),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ]),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("left", e_method_call(e_this(), "__elephc_dom_kids", vec![])),
                            s_assign("right", e_method_call(e_var("otherNode"), "__elephc_dom_kids", vec![])),
                            s_if(
                                e_binop(e_call("count", vec![e_var("left")]), BinOp::StrictNotEq, e_call("count", vec![e_var("right")])),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            // Locals, not `$left[$i]->isEqualNode($right[$i])`: that spelling leaks the pair.
                            s_assign("i", e_int(0)),
                            s_foreach(e_var("left"), None, "one", vec![
                                s_assign("two", e_index(e_var("right"), e_var("i"))),
                                s_expr(e_post_inc("i")),
                                s_if(
                                    e_not(e_method_call(e_var("one"), "isEqualNode", vec![e_var("two")])),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_bool(true)),
                        ]),
                )
        .prop("nodeType", TypeExpr::Int, None)
        .hooked(true, false, false)
        .prop("firstChild", t_mixed(), None)
        .hooked(true, false, false)
        .prop("lastChild", t_mixed(), None)
        .hooked(true, false, false)
        .prop("localName", t_mixed(), None)
        .hooked(true, false, false)
        .prop("prefix", t_mixed(), None)
        .hooked(true, false, false)
        .prop("namespaceURI", t_mixed(), None)
        .hooked(true, false, false)
}

/// Registers this package's declarations.
pub(super) fn contribute(contributions: &mut Contributions) {
    contributions.members(Part::Walker, tree());
    contributions.members(
        Part::Walker,
        class("DOMElement").prop("__elephc_dom_ns", t_nullable(TypeExpr::Str), Some(e_null())),
    );
}
