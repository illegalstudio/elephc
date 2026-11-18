//! Purpose:
//! Package P3 of the DOM migration: `DOMCharacterData` data methods, `DOMText::splitText`,
//! `DOMCdataSection`, the `DOMComment` / `DOMText` constructors, `DOMAttr`,
//! `DOMDocumentFragment` and `DOMDocumentType`.
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - Everything works through the `nodeValue` hook of `DOMNode`, which already reads and writes
//!   the native libxml2 node of a bound (SimpleXML-shared) node and a plain slot for an unbound
//!   one, so `data` and the offset methods behave the same on both without a bridge helper.
//! - Offsets and counts are in UTF-8 characters (`mb_substr` / `mb_strlen`), like php's.
//! - `nodeType` belongs to package P4 (it is a `DOMNode` member); nothing here reads or sets it.
//! - `DOMAttr` keeps its namespace in `__elephc_dom_ns` (null = none); the element package sets
//!   it and `ownerElement` when it materializes attribute nodes.

use super::extend::{Contributions, Part};
use crate::parser::ast::{BinOp, CastType, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

fn index_size_error() -> Stmt {
    s_throw(e_new("DOMException", vec![e_str("Index Size Error"), e_int(1)]))
}

fn no_children() -> Expr {
    e_new("DOMNodeList", vec![e_array(vec![])])
}

/// `(string) $this->nodeValue`.
fn text_of_this() -> Expr {
    e_cast(CastType::String, e_this_prop("nodeValue"))
}

fn char_length(text: Expr) -> Expr {
    e_call("mb_strlen", vec![text, e_str("UTF-8")])
}

fn char_slice(text: Expr, offset: Expr, count: Option<Expr>) -> Expr {
    e_call("mb_substr", vec![text, offset, count.unwrap_or_else(e_null), e_str("UTF-8")])
}

/// Throws the index-size error when `offset` is outside `0..=length` (or `count` is negative).
fn check_range(with_count: bool) -> Vec<Stmt> {
    let mut bad = e_binop(
        e_binop(e_var("offset"), BinOp::Lt, e_int(0)),
        BinOp::Or,
        e_binop(e_var("offset"), BinOp::Gt, e_var("_length")),
    );
    if with_count {
        bad = e_binop(bad, BinOp::Or, e_binop(e_var("count"), BinOp::Lt, e_int(0)));
    }
    vec![
        s_assign("_length", char_length(text_of_this())),
        s_if(bad, vec![index_size_error()], vec![], None),
    ]
}

fn character_data() -> ClassBuilder {
    // head . $data . tail, with `tail` starting after the replaced characters.
    let splice = |count: bool| {
        let tail_start = if count { e_binop(e_var("offset"), BinOp::Add, e_var("count")) } else { e_var("offset") };
        e_concat_all(vec![
            char_slice(text_of_this(), e_int(0), Some(e_var("offset"))),
            e_var("data"),
            char_slice(text_of_this(), tail_start, None),
        ])
    };
    class("DOMCharacterData")
        .prop("data", TypeExpr::Str, None)
        .hooked(true, true, false)
        .prop("length", TypeExpr::Int, None)
        .hooked(true, false, false)
        .method(method("__propget_data").returns(TypeExpr::Str).body(vec![s_return(text_of_this())]))
        .method(
            method("__propset_data")
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![s_prop_assign(e_this(), "nodeValue", e_var("value"))]),
        )
        .method(
            method("__propget_length")
                .returns(TypeExpr::Int)
                .body(vec![s_return(char_length(text_of_this()))]),
        )
        .method(
            method("appendData")
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "nodeValue", e_concat(text_of_this(), e_var("data"))),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("substringData")
                .param("offset", TypeExpr::Int)
                .param("count", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body({
                    let mut body = check_range(true);
                    body.push(s_return(char_slice(text_of_this(), e_var("offset"), Some(e_var("count")))));
                    body
                }),
        )
        .method(
            method("insertData")
                .param("offset", TypeExpr::Int)
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body({
                    let mut body = check_range(false);
                    body.push(s_prop_assign(e_this(), "nodeValue", splice(false)));
                    body.push(s_return(e_bool(true)));
                    body
                }),
        )
        .method(
            method("deleteData")
                .param("offset", TypeExpr::Int)
                .param("count", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body({
                    let mut body = check_range(true);
                    body.push(s_assign("data", e_str("")));
                    body.push(s_prop_assign(e_this(), "nodeValue", splice(true)));
                    body.push(s_return(e_bool(true)));
                    body
                }),
        )
        .method(
            method("replaceData")
                .param("offset", TypeExpr::Int)
                .param("count", TypeExpr::Int)
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body({
                    let mut body = check_range(true);
                    body.push(s_prop_assign(e_this(), "nodeValue", splice(true)));
                    body.push(s_return(e_bool(true)));
                    body
                }),
        )
}

fn text() -> ClassBuilder {
    class("DOMText")
        .prop("wholeText", TypeExpr::Str, None)
        .hooked(true, false, false)
        .method(
            method("__construct")
                .param_default("data", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_prop_assign(e_this(), "nodeName", e_str("#text")),
                    s_prop_assign(e_this(), "nodeValue", e_var("data")),
                    s_prop_assign(e_this(), "childNodes", no_children()),
                ]),
        )
        .method(
            // The text of the run of adjacent text and CDATA siblings holding `$this`, read from
            // the parent's child list (the sibling pointers of a node appended to a free-standing
            // parent are not maintained).
            method("__propget_wholeText").returns(TypeExpr::Str).body(vec![
                s_assign("_parent", e_this_prop("parentNode")),
                s_if(
                    e_not(e_instance_of(e_var("_parent"), "DOMNode")),
                    vec![s_return(text_of_this())],
                    vec![],
                    None,
                ),
                s_assign("_run", e_str("")),
                s_assign("_found", e_bool(false)),
                s_foreach(
                    e_prop(e_var("_parent"), "childNodes"),
                    None,
                    "_child",
                    vec![s_if(
                        e_instance_of(e_var("_child"), "DOMText"),
                        vec![
                            s_assign("_run", e_concat(e_var("_run"), e_cast(CastType::String, e_prop(e_var("_child"), "nodeValue")))),
                            s_if(
                                e_binop(e_var("_child"), BinOp::StrictEq, e_this()),
                                vec![s_assign("_found", e_bool(true))],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        Some(vec![
                            s_if(e_var("_found"), vec![s_return(e_var("_run"))], vec![], None),
                            s_assign("_run", e_str("")),
                        ]),
                    )],
                ),
                s_if(e_var("_found"), vec![s_return(e_var("_run"))], vec![], None),
                s_return(text_of_this()),
            ]),
        )
        .method(
            method("isWhitespaceInElementContent")
                .returns(TypeExpr::Bool)
                .body(vec![s_return(e_binop(
                    e_call("trim", vec![text_of_this(), e_str(" \t\r\n")]),
                    BinOp::StrictEq,
                    e_str(""),
                ))]),
        )
        .method(
            method("isElementContentWhitespace")
                .returns(TypeExpr::Bool)
                .body(vec![s_return(e_method_call(e_this(), "isWhitespaceInElementContent", vec![]))]),
        )
        .method(
            // php: a negative offset is a ValueError, an offset past the end returns false.
            method("splitText")
                .param("offset", TypeExpr::Int)
                .returns(t_union(vec![t_class("DOMText"), TypeExpr::Bool]))
                .body(vec![
                    s_if(
                        e_binop(e_var("offset"), BinOp::Lt, e_int(0)),
                        vec![s_throw(e_new(
                            "ValueError",
                            vec![e_str("DOMText::splitText(): Argument #1 ($offset) must be greater than or equal to 0")],
                        ))],
                        vec![],
                        None,
                    ),
                    s_assign("_length", char_length(text_of_this())),
                    s_if(e_binop(e_var("offset"), BinOp::Gt, e_var("_length")), vec![s_return(e_bool(false))], vec![], None),
                    s_assign("_tail", char_slice(text_of_this(), e_var("offset"), None)),
                    s_prop_assign(e_this(), "nodeValue", char_slice(text_of_this(), e_int(0), Some(e_var("offset")))),
                    s_assign("_new", e_new("DOMText", vec![e_var("_tail")])),
                    s_prop_assign(e_var("_new"), "ownerDocument", e_this_prop("ownerDocument")),
                    s_assign("_parent", e_this_prop("parentNode")),
                    s_if(
                        e_instance_of(e_var("_parent"), "DOMNode"),
                        vec![
                            // php links the new node right after this one without merging text.
                            s_expr(e_method_call(e_var("_parent"), "insertBefore", vec![e_var("_new"), e_this_prop("nextSibling")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_new")),
                ]),
        )
}

fn comment() -> ClassBuilder {
    class("DOMComment").method(
        method("__construct")
            .param_default("data", TypeExpr::Str, e_str(""))
            .body(vec![
                s_prop_assign(e_this(), "nodeName", e_str("#comment")),
                s_prop_assign(e_this(), "nodeValue", e_var("data")),
                s_prop_assign(e_this(), "childNodes", no_children()),
            ]),
    )
}

fn cdata_section() -> Stmt {
    class("DOMCdataSection")
        .extends("DOMText")
        .method(
            method("__construct")
                .param("data", TypeExpr::Str)
                .body(vec![
                    s_prop_assign(e_this(), "nodeName", e_str("#cdata-section")),
                    s_prop_assign(e_this(), "__elephc_raw_text", e_bool(true)),
                    s_prop_assign(e_this(), "nodeValue", e_var("data")),
                    s_prop_assign(e_this(), "childNodes", no_children()),
                ]),
        )
        .build()
}

fn attr() -> Stmt {
    class("DOMAttr")
        .extends("DOMNode")
        .prop("name", TypeExpr::Str, Some(e_str("")))
        .prop("specified", TypeExpr::Bool, Some(e_bool(true)))
        .prop("value", TypeExpr::Str, None)
        .hooked(true, true, false)
        .prop("ownerElement", t_nullable(t_class("DOMElement")), Some(e_null()))
        .prop("schemaTypeInfo", t_mixed(), Some(e_null()))
        .prop("__elephc_dom_ns", t_nullable(TypeExpr::Str), Some(e_null()))
        .method(method("__propget_value").returns(TypeExpr::Str).body(vec![s_return(text_of_this())]))
        .method(
            method("__propset_value")
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![s_prop_assign(e_this(), "nodeValue", e_var("value"))]),
        )
        .method(
            method("__construct")
                .param("name", TypeExpr::Str)
                .param_default("value", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_if(
                        e_not(e_call("__elephc_dom_valid_attribute_name", vec![e_var("name")])),
                        vec![s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)]))],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "name", e_var("name")),
                    s_prop_assign(e_this(), "nodeName", e_var("name")),
                    s_prop_assign(e_this(), "nodeValue", e_var("value")),
                    s_prop_assign(e_this(), "childNodes", no_children()),
                ]),
        )
        .method(
            // True only for an attribute declared as an ID (DTD or setIdAttribute()), which the
            // element package records; a free-standing attribute is never one.
            method("isId").returns(TypeExpr::Bool).body(vec![s_return(e_bool(false))]),
        )
        .build()
}

fn document_fragment() -> Stmt {
    class("DOMDocumentFragment")
        .extends("DOMNode")
        .method(method("__construct").body(vec![
            s_prop_assign(e_this(), "nodeName", e_str("#document-fragment")),
            s_prop_assign(e_this(), "childNodes", no_children()),
        ]))
        .method(
            // php parses the markup as the content of an element; a well-formedness error is a
            // warning plus `false`, and a fragment with no owner document cannot be modified.
            method("appendXML")
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_instance_of(e_this_prop("ownerDocument"), "DOMDocument")),
                        vec![s_throw(e_new("DOMException", vec![e_str("No Modification Allowed Error"), e_int(7)]))],
                        vec![],
                        None,
                    ),
                    s_if(e_binop(e_var("data"), BinOp::StrictEq, e_str("")), vec![s_return(e_bool(false))], vec![], None),
                    s_assign("_doc", e_new("DOMDocument", vec![])),
                    s_if(
                        e_not(e_error_suppress(e_method_call(
                            e_var("_doc"),
                            "loadXML",
                            vec![e_concat_all(vec![e_str("<elephc-fragment>"), e_var("data"), e_str("</elephc-fragment>")])],
                        ))),
                        vec![s_return(e_bool(false))],
                        vec![],
                        None,
                    ),
                    s_assign("_holder", e_prop(e_var("_doc"), "documentElement")),
                    s_assign("_moved", e_array(vec![])),
                    s_foreach(e_prop(e_var("_holder"), "childNodes"), None, "_child", vec![s_array_push("_moved", e_var("_child"))]),
                    s_foreach(
                        e_var("_moved"),
                        None,
                        "_child",
                        vec![s_expr(e_method_call(e_this(), "appendChild", vec![e_var("_child")]))],
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .build()
}

fn document_type() -> Stmt {
    class("DOMDocumentType")
        .extends("DOMNode")
        .prop("name", TypeExpr::Str, Some(e_str("")))
        .prop("publicId", TypeExpr::Str, Some(e_str("")))
        .prop("systemId", TypeExpr::Str, Some(e_str("")))
        .prop("internalSubset", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("entities", t_mixed(), Some(e_null()))
        .prop("notations", t_mixed(), Some(e_null()))
        .method(
            method("__construct")
                .param_default("name", TypeExpr::Str, e_str(""))
                .param_default("publicId", TypeExpr::Str, e_str(""))
                .param_default("systemId", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_prop_assign(e_this(), "name", e_var("name")),
                    s_prop_assign(e_this(), "nodeName", e_var("name")),
                    s_prop_assign(e_this(), "publicId", e_var("publicId")),
                    s_prop_assign(e_this(), "systemId", e_var("systemId")),
                    s_prop_assign(e_this(), "childNodes", no_children()),
                    s_prop_assign(e_this(), "entities", e_new("DOMNamedNodeMap", vec![e_array(vec![])])),
                    s_prop_assign(e_this(), "notations", e_new("DOMNamedNodeMap", vec![e_array(vec![])])),
                ]),
        )
        .build()
}

/// Registers this package's declarations.
pub(super) fn contribute(contributions: &mut Contributions) {
    contributions.members(Part::Walker, character_data());
    contributions.members(Part::Walker, text());
    contributions.members(Part::Walker, comment());
    contributions.add_class(Part::Walker, cdata_section());
    contributions.add_class(Part::Walker, attr());
    contributions.add_class(Part::Walker, document_fragment());
    contributions.add_class(Part::Walker, document_type());
}
