//! Purpose:
//! The classic DOM classes (`DOMDocument`, `DOMNode`, `DOMElement`, `DOMText`, `DOMNodeList`, ...),
//! the `LIBXML_*` / `XML_*_NODE` / `DOM_*_ERR` constants and the `libxml_*` error-state functions,
//! built as AST.
//!
//! Called from:
//! - `crate::dom_prelude::declarations`.
//!
//! Key details:
//! - TRANSCRIBED, not written: every declaration was generated from the parse of the PHP model in
//!   `fragments` (`synthetic_class::transcribe`), which was differential-tested against php's own
//!   DOM classes -- see the module documentation of `dom_prelude`. Edit the PHP model and
//!   re-transcribe rather than editing these builders by hand; `oracle` compares the two node by
//!   node.
//! - One helper per declaration, plus an aggregator: a prelude built as one nested builder
//!   expression overflows the stack.

#![allow(clippy::all)]

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// `LIBXML_VERSION` — transcribed from the PHP form.
fn decl_const_libxml_version() -> Stmt {
    s_const("LIBXML_VERSION", e_int(20913))
}

/// `LIBXML_DOTTED_VERSION` — transcribed from the PHP form.
fn decl_const_libxml_dotted_version() -> Stmt {
    s_const("LIBXML_DOTTED_VERSION", e_str("2.9.13"))
}

/// `LIBXML_LOADED_VERSION` — transcribed from the PHP form.
fn decl_const_libxml_loaded_version() -> Stmt {
    s_const("LIBXML_LOADED_VERSION", e_int(20913))
}

/// `LIBXML_RECOVER` — transcribed from the PHP form.
fn decl_const_libxml_recover() -> Stmt {
    s_const("LIBXML_RECOVER", e_int(1))
}

/// `LIBXML_NOENT` — transcribed from the PHP form.
fn decl_const_libxml_noent() -> Stmt {
    s_const("LIBXML_NOENT", e_int(2))
}

/// `LIBXML_DTDLOAD` — transcribed from the PHP form.
fn decl_const_libxml_dtdload() -> Stmt {
    s_const("LIBXML_DTDLOAD", e_int(4))
}

/// `LIBXML_DTDATTR` — transcribed from the PHP form.
fn decl_const_libxml_dtdattr() -> Stmt {
    s_const("LIBXML_DTDATTR", e_int(8))
}

/// `LIBXML_DTDVALID` — transcribed from the PHP form.
fn decl_const_libxml_dtdvalid() -> Stmt {
    s_const("LIBXML_DTDVALID", e_int(16))
}

/// `LIBXML_NOERROR` — transcribed from the PHP form.
fn decl_const_libxml_noerror() -> Stmt {
    s_const("LIBXML_NOERROR", e_int(32))
}

/// `LIBXML_NOWARNING` — transcribed from the PHP form.
fn decl_const_libxml_nowarning() -> Stmt {
    s_const("LIBXML_NOWARNING", e_int(64))
}

/// `LIBXML_PEDANTIC` — transcribed from the PHP form.
fn decl_const_libxml_pedantic() -> Stmt {
    s_const("LIBXML_PEDANTIC", e_int(128))
}

/// `LIBXML_NOBLANKS` — transcribed from the PHP form.
fn decl_const_libxml_noblanks() -> Stmt {
    s_const("LIBXML_NOBLANKS", e_int(256))
}

/// `LIBXML_XINCLUDE` — transcribed from the PHP form.
fn decl_const_libxml_xinclude() -> Stmt {
    s_const("LIBXML_XINCLUDE", e_int(1024))
}

/// `LIBXML_NSCLEAN` — transcribed from the PHP form.
fn decl_const_libxml_nsclean() -> Stmt {
    s_const("LIBXML_NSCLEAN", e_int(8192))
}

/// `LIBXML_NOCDATA` — transcribed from the PHP form.
fn decl_const_libxml_nocdata() -> Stmt {
    s_const("LIBXML_NOCDATA", e_int(16384))
}

/// `LIBXML_NOXMLDECL` — transcribed from the PHP form.
fn decl_const_libxml_noxmldecl() -> Stmt {
    s_const("LIBXML_NOXMLDECL", e_int(2))
}

/// `LIBXML_PARSEHUGE` — transcribed from the PHP form.
fn decl_const_libxml_parsehuge() -> Stmt {
    s_const("LIBXML_PARSEHUGE", e_int(524288))
}

/// `LIBXML_BIGLINES` — transcribed from the PHP form.
fn decl_const_libxml_biglines() -> Stmt {
    s_const("LIBXML_BIGLINES", e_int(4194304))
}

/// `LIBXML_NOEMPTYTAG` — transcribed from the PHP form.
fn decl_const_libxml_noemptytag() -> Stmt {
    s_const("LIBXML_NOEMPTYTAG", e_int(4))
}

/// `LIBXML_SCHEMA_CREATE` — transcribed from the PHP form.
fn decl_const_libxml_schema_create() -> Stmt {
    s_const("LIBXML_SCHEMA_CREATE", e_int(1))
}

/// `LIBXML_HTML_NOIMPLIED` — transcribed from the PHP form.
fn decl_const_libxml_html_noimplied() -> Stmt {
    s_const("LIBXML_HTML_NOIMPLIED", e_int(8192))
}

/// `LIBXML_HTML_NODEFDTD` — transcribed from the PHP form.
fn decl_const_libxml_html_nodefdtd() -> Stmt {
    s_const("LIBXML_HTML_NODEFDTD", e_int(4))
}

/// `XML_ELEMENT_NODE` — transcribed from the PHP form.
fn decl_const_xml_element_node() -> Stmt {
    s_const("XML_ELEMENT_NODE", e_int(1))
}

/// `XML_ATTRIBUTE_NODE` — transcribed from the PHP form.
fn decl_const_xml_attribute_node() -> Stmt {
    s_const("XML_ATTRIBUTE_NODE", e_int(2))
}

/// `XML_TEXT_NODE` — transcribed from the PHP form.
fn decl_const_xml_text_node() -> Stmt {
    s_const("XML_TEXT_NODE", e_int(3))
}

/// `XML_CDATA_SECTION_NODE` — transcribed from the PHP form.
fn decl_const_xml_cdata_section_node() -> Stmt {
    s_const("XML_CDATA_SECTION_NODE", e_int(4))
}

/// `XML_ENTITY_REF_NODE` — transcribed from the PHP form.
fn decl_const_xml_entity_ref_node() -> Stmt {
    s_const("XML_ENTITY_REF_NODE", e_int(5))
}

/// `XML_ENTITY_NODE` — transcribed from the PHP form.
fn decl_const_xml_entity_node() -> Stmt {
    s_const("XML_ENTITY_NODE", e_int(6))
}

/// `XML_PI_NODE` — transcribed from the PHP form.
fn decl_const_xml_pi_node() -> Stmt {
    s_const("XML_PI_NODE", e_int(7))
}

/// `XML_COMMENT_NODE` — transcribed from the PHP form.
fn decl_const_xml_comment_node() -> Stmt {
    s_const("XML_COMMENT_NODE", e_int(8))
}

/// `XML_DOCUMENT_NODE` — transcribed from the PHP form.
fn decl_const_xml_document_node() -> Stmt {
    s_const("XML_DOCUMENT_NODE", e_int(9))
}

/// `XML_DOCUMENT_TYPE_NODE` — transcribed from the PHP form.
fn decl_const_xml_document_type_node() -> Stmt {
    s_const("XML_DOCUMENT_TYPE_NODE", e_int(10))
}

/// `XML_DOCUMENT_FRAG_NODE` — transcribed from the PHP form.
fn decl_const_xml_document_frag_node() -> Stmt {
    s_const("XML_DOCUMENT_FRAG_NODE", e_int(11))
}

/// `XML_NOTATION_NODE` — transcribed from the PHP form.
fn decl_const_xml_notation_node() -> Stmt {
    s_const("XML_NOTATION_NODE", e_int(12))
}

/// `XML_HTML_DOCUMENT_NODE` — transcribed from the PHP form.
fn decl_const_xml_html_document_node() -> Stmt {
    s_const("XML_HTML_DOCUMENT_NODE", e_int(13))
}

/// `XML_DTD_NODE` — transcribed from the PHP form.
fn decl_const_xml_dtd_node() -> Stmt {
    s_const("XML_DTD_NODE", e_int(14))
}

/// `XML_ELEMENT_DECL_NODE` — transcribed from the PHP form.
fn decl_const_xml_element_decl_node() -> Stmt {
    s_const("XML_ELEMENT_DECL_NODE", e_int(15))
}

/// `XML_ATTRIBUTE_DECL_NODE` — transcribed from the PHP form.
fn decl_const_xml_attribute_decl_node() -> Stmt {
    s_const("XML_ATTRIBUTE_DECL_NODE", e_int(16))
}

/// `XML_ENTITY_DECL_NODE` — transcribed from the PHP form.
fn decl_const_xml_entity_decl_node() -> Stmt {
    s_const("XML_ENTITY_DECL_NODE", e_int(17))
}

/// `XML_NAMESPACE_DECL_NODE` — transcribed from the PHP form.
fn decl_const_xml_namespace_decl_node() -> Stmt {
    s_const("XML_NAMESPACE_DECL_NODE", e_int(18))
}

/// `XML_LOCAL_NAMESPACE` — transcribed from the PHP form.
fn decl_const_xml_local_namespace() -> Stmt {
    s_const("XML_LOCAL_NAMESPACE", e_int(18))
}

/// `XML_ATTRIBUTE_CDATA` — transcribed from the PHP form.
fn decl_const_xml_attribute_cdata() -> Stmt {
    s_const("XML_ATTRIBUTE_CDATA", e_int(1))
}

/// `XML_ATTRIBUTE_ID` — transcribed from the PHP form.
fn decl_const_xml_attribute_id() -> Stmt {
    s_const("XML_ATTRIBUTE_ID", e_int(2))
}

/// `XML_ATTRIBUTE_IDREF` — transcribed from the PHP form.
fn decl_const_xml_attribute_idref() -> Stmt {
    s_const("XML_ATTRIBUTE_IDREF", e_int(3))
}

/// `XML_ATTRIBUTE_IDREFS` — transcribed from the PHP form.
fn decl_const_xml_attribute_idrefs() -> Stmt {
    s_const("XML_ATTRIBUTE_IDREFS", e_int(4))
}

/// `XML_ATTRIBUTE_ENTITY` — transcribed from the PHP form.
fn decl_const_xml_attribute_entity() -> Stmt {
    s_const("XML_ATTRIBUTE_ENTITY", e_int(6))
}

/// `XML_ATTRIBUTE_NMTOKEN` — transcribed from the PHP form.
fn decl_const_xml_attribute_nmtoken() -> Stmt {
    s_const("XML_ATTRIBUTE_NMTOKEN", e_int(7))
}

/// `XML_ATTRIBUTE_NMTOKENS` — transcribed from the PHP form.
fn decl_const_xml_attribute_nmtokens() -> Stmt {
    s_const("XML_ATTRIBUTE_NMTOKENS", e_int(8))
}

/// `XML_ATTRIBUTE_ENUMERATION` — transcribed from the PHP form.
fn decl_const_xml_attribute_enumeration() -> Stmt {
    s_const("XML_ATTRIBUTE_ENUMERATION", e_int(9))
}

/// `XML_ATTRIBUTE_NOTATION` — transcribed from the PHP form.
fn decl_const_xml_attribute_notation() -> Stmt {
    s_const("XML_ATTRIBUTE_NOTATION", e_int(10))
}

/// `DOM_PHP_ERR` — transcribed from the PHP form.
fn decl_const_dom_php_err() -> Stmt {
    s_const("DOM_PHP_ERR", e_int(0))
}

/// `DOM_INDEX_SIZE_ERR` — transcribed from the PHP form.
fn decl_const_dom_index_size_err() -> Stmt {
    s_const("DOM_INDEX_SIZE_ERR", e_int(1))
}

/// `DOMSTRING_SIZE_ERR` — transcribed from the PHP form.
fn decl_const_domstring_size_err() -> Stmt {
    s_const("DOMSTRING_SIZE_ERR", e_int(2))
}

/// `DOM_HIERARCHY_REQUEST_ERR` — transcribed from the PHP form.
fn decl_const_dom_hierarchy_request_err() -> Stmt {
    s_const("DOM_HIERARCHY_REQUEST_ERR", e_int(3))
}

/// `DOM_WRONG_DOCUMENT_ERR` — transcribed from the PHP form.
fn decl_const_dom_wrong_document_err() -> Stmt {
    s_const("DOM_WRONG_DOCUMENT_ERR", e_int(4))
}

/// `DOM_INVALID_CHARACTER_ERR` — transcribed from the PHP form.
fn decl_const_dom_invalid_character_err() -> Stmt {
    s_const("DOM_INVALID_CHARACTER_ERR", e_int(5))
}

/// `DOM_NO_DATA_ALLOWED_ERR` — transcribed from the PHP form.
fn decl_const_dom_no_data_allowed_err() -> Stmt {
    s_const("DOM_NO_DATA_ALLOWED_ERR", e_int(6))
}

/// `DOM_NO_MODIFICATION_ALLOWED_ERR` — transcribed from the PHP form.
fn decl_const_dom_no_modification_allowed_err() -> Stmt {
    s_const("DOM_NO_MODIFICATION_ALLOWED_ERR", e_int(7))
}

/// `DOM_NOT_FOUND_ERR` — transcribed from the PHP form.
fn decl_const_dom_not_found_err() -> Stmt {
    s_const("DOM_NOT_FOUND_ERR", e_int(8))
}

/// `DOM_NOT_SUPPORTED_ERR` — transcribed from the PHP form.
fn decl_const_dom_not_supported_err() -> Stmt {
    s_const("DOM_NOT_SUPPORTED_ERR", e_int(9))
}

/// `DOM_INUSE_ATTRIBUTE_ERR` — transcribed from the PHP form.
fn decl_const_dom_inuse_attribute_err() -> Stmt {
    s_const("DOM_INUSE_ATTRIBUTE_ERR", e_int(10))
}

/// `DOM_INVALID_STATE_ERR` — transcribed from the PHP form.
fn decl_const_dom_invalid_state_err() -> Stmt {
    s_const("DOM_INVALID_STATE_ERR", e_int(11))
}

/// `DOM_SYNTAX_ERR` — transcribed from the PHP form.
fn decl_const_dom_syntax_err() -> Stmt {
    s_const("DOM_SYNTAX_ERR", e_int(12))
}

/// `DOM_INVALID_MODIFICATION_ERR` — transcribed from the PHP form.
fn decl_const_dom_invalid_modification_err() -> Stmt {
    s_const("DOM_INVALID_MODIFICATION_ERR", e_int(13))
}

/// `DOM_NAMESPACE_ERR` — transcribed from the PHP form.
fn decl_const_dom_namespace_err() -> Stmt {
    s_const("DOM_NAMESPACE_ERR", e_int(14))
}

/// `DOM_INVALID_ACCESS_ERR` — transcribed from the PHP form.
fn decl_const_dom_invalid_access_err() -> Stmt {
    s_const("DOM_INVALID_ACCESS_ERR", e_int(15))
}

/// `DOM_VALIDATION_ERR` — transcribed from the PHP form.
fn decl_const_dom_validation_err() -> Stmt {
    s_const("DOM_VALIDATION_ERR", e_int(16))
}

/// `DOMException` — transcribed from the PHP form.
fn decl_class_domexception() -> Stmt {
    class("DOMException")
        .extends("\\Exception")
        .build()
}

/// `DOMNodeList` — transcribed from the PHP form.
fn decl_class_domnodelist() -> Stmt {
    class("DOMNodeList")
        .implements("\\IteratorAggregate")
        .implements("\\Countable")
        .prop("length", TypeExpr::Int, Some(e_int(0)))
        .prop("parent", t_nullable(t_class("DOMNode")), Some(e_null()))
        .prop("items", t_nullable(t_array()), Some(e_null()))
        .method(
            method("__construct")
                .param_default("parent", t_nullable(t_class("DOMNode")), e_null())
                .param_default("items", t_array(), e_array(vec![]))
                .body(vec![
                    s_prop_assign(e_this(), "parent", e_var("parent")),
                    s_prop_assign(e_this(), "items", e_ternary(e_binop(e_var("items"), BinOp::StrictEq, e_array(vec![])), e_null(), e_var("items"))),
                    s_prop_assign(e_this(), "length", e_call("count", vec![e_var("items")])),
                ]),
        )
        .method(
            method("item")
                .param("index", TypeExpr::Int)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_if(
                        e_binop(e_var("index"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("parent"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null_coalesce(e_index(e_this_prop("items"), e_var("index")), e_null())),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("node", e_prop(e_this_prop("parent"), "firstChild")),
                    s_while(e_binop(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("index"), BinOp::Gt, e_int(0))), vec![
                        s_assign("node", e_prop(e_var("node"), "nextSibling")),
                        s_expr(e_post_dec("index")),
                    ]),
                    s_return(e_var("node")),
                ]),
        )
        .method(
            method("count")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("length")),
                ]),
        )
        .method(
            method("toArray")
                .returns(t_array())
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("parent"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null_coalesce(e_this_prop("items"), e_array(vec![]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("all", e_array(vec![])),
                    s_assign("node", e_prop(e_this_prop("parent"), "firstChild")),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_array_push("all", e_var("node")),
                        s_assign("node", e_prop(e_var("node"), "nextSibling")),
                    ]),
                    s_return(e_var("all")),
                ]),
        )
        .method(
            method("getIterator")
                .returns(t_class("\\Iterator"))
                .body(vec![
                    s_return(e_new_fq("ArrayIterator", vec![e_method_call(e_this(), "toArray", vec![])])),
                ]),
        )
        .build()
}

/// `DOMNamedNodeMap` — transcribed from the PHP form.
fn decl_class_domnamednodemap() -> Stmt {
    class("DOMNamedNodeMap")
        .implements("\\IteratorAggregate")
        .implements("\\Countable")
        .prop("length", TypeExpr::Int, Some(e_int(0)))
        .prop("owner", t_nullable(t_class("DOMElement")), Some(e_null()))
        .method(
            method("__construct")
                .param_default("owner", t_nullable(t_class("DOMElement")), e_null())
                .body(vec![
                    s_prop_assign(e_this(), "owner", e_var("owner")),
                ]),
        )
        .method(
            method("getNamedItem")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("owner"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_foreach(e_prop(e_this_prop("owner"), "attrNodes"), None, "attr", vec![
                        s_if(
                            e_binop(e_prop(e_var("attr"), "name"), BinOp::StrictEq, e_var("qualifiedName")),
                            vec![
                                s_return(e_var("attr")),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("getNamedItemNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("owner"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this_prop("owner"), "getAttributeNodeNS", vec![e_var("namespace"), e_var("localName")])),
                ]),
        )
        .method(
            method("item")
                .param("index", TypeExpr::Int)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("owner"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("index"), BinOp::Lt, e_int(0))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("nodes", e_call("array_values", vec![e_prop(e_this_prop("owner"), "attrNodes")])),
                    s_return(e_null_coalesce(e_index(e_var("nodes"), e_var("index")), e_null())),
                ]),
        )
        .method(
            method("count")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("length")),
                ]),
        )
        .method(
            method("getIterator")
                .returns(t_class("\\Iterator"))
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("owner"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_new_fq("ArrayIterator", vec![e_array(vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("byName", e_array(vec![])),
                    s_foreach(e_prop(e_this_prop("owner"), "attrNodes"), None, "attr", vec![
                        s_array_assign("byName", e_prop(e_var("attr"), "name"), e_var("attr")),
                    ]),
                    s_return(e_new_fq("ArrayIterator", vec![e_var("byName")])),
                ]),
        )
        .build()
}

/// `DOMNode` — transcribed from the PHP form.
fn decl_class_domnode() -> Stmt {
    class("DOMNode")
        .prop("nodeName", TypeExpr::Str, Some(e_str("")))
        .prop("nodeValue", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("nodeType", TypeExpr::Int, Some(e_int(0)))
        .prop("parentNode", t_nullable(t_class("DOMNode")), Some(e_null()))
        .prop("childNodes", t_class("DOMNodeList"), None)
        .prop("firstChild", t_nullable(t_class("DOMNode")), Some(e_null()))
        .prop("lastChild", t_nullable(t_class("DOMNode")), Some(e_null()))
        .prop("previousSibling", t_nullable(t_class("DOMNode")), Some(e_null()))
        .prop("nextSibling", t_nullable(t_class("DOMNode")), Some(e_null()))
        .prop("attributes", t_nullable(t_class("DOMNamedNodeMap")), Some(e_null()))
        .prop("ownerDocument", t_nullable(t_class("DOMDocument")), Some(e_null()))
        .prop("namespaceURI", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("prefix", TypeExpr::Str, Some(e_str("")))
        .prop("localName", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("baseURI", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("textContent", t_nullable(TypeExpr::Str), Some(e_str("")))
        .prop("textCache", TypeExpr::Str, Some(e_str("")))
        .prop("tagName", TypeExpr::Str, Some(e_str("")))
        .prop("data", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("value", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("name", TypeExpr::Str, Some(e_str("")))
        .prop("target", TypeExpr::Str, Some(e_str("")))
        .prop("ownerElement", t_nullable(t_class("DOMElement")), Some(e_null()))
        .method(
            method("__construct")
                .body(vec![
                    s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_this()])),
                ]),
        )
        .method(
            method("__get")
                .param("name", TypeExpr::Str)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("firstElementChild")),
                        vec![
                            s_assign("node", e_this_prop("firstChild")),
                            s_while(e_binop(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictNotEq, e_int(1))), vec![
                                s_assign("node", e_prop(e_var("node"), "nextSibling")),
                            ]),
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("lastElementChild")),
                        vec![
                            s_assign("node", e_this_prop("lastChild")),
                            s_while(e_binop(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictNotEq, e_int(1))), vec![
                                s_assign("node", e_prop(e_var("node"), "previousSibling")),
                            ]),
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("childElementCount")),
                        vec![
                            s_assign("count", e_int(0)),
                            s_assign("node", e_this_prop("firstChild")),
                            s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                                s_if(
                                    e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(1)),
                                    vec![
                                        s_expr(e_post_inc("count")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("node", e_prop(e_var("node"), "nextSibling")),
                            ]),
                            s_return(e_var("count")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("previousElementSibling")),
                        vec![
                            s_assign("node", e_this_prop("previousSibling")),
                            s_while(e_binop(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictNotEq, e_int(1))), vec![
                                s_assign("node", e_prop(e_var("node"), "previousSibling")),
                            ]),
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("nextElementSibling")),
                        vec![
                            s_assign("node", e_this_prop("nextSibling")),
                            s_while(e_binop(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictNotEq, e_int(1))), vec![
                                s_assign("node", e_prop(e_var("node"), "nextSibling")),
                            ]),
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("__isset")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("firstElementChild")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("lastElementChild"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("previousElementSibling"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("nextElementSibling"))),
                        vec![
                            s_return(e_binop(e_method_call(e_this(), "__get", vec![e_var("name")]), BinOp::StrictNotEq, e_null())),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_var("name"), BinOp::StrictEq, e_str("childElementCount"))),
                ]),
        )
        .method(
            method("computeText")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("type", e_this_prop("nodeType")),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("type"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("type"), BinOp::StrictNotEq, e_int(9))), BinOp::And, e_binop(e_var("type"), BinOp::StrictNotEq, e_int(11))), BinOp::And, e_binop(e_var("type"), BinOp::StrictNotEq, e_int(13))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("text", e_str("")),
                    s_assign("child", e_this_prop("firstChild")),
                    s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                        s_assign("kind", e_prop(e_var("child"), "nodeType")),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4))), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(1))),
                            vec![
                                s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_prop(e_var("child"), "textContent"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("child", e_prop(e_var("child"), "nextSibling")),
                    ]),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_prop_assign(e_this(), "nodeValue", e_var("text")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "textContent", e_var("text")),
                    ]),
                    ),
                    s_prop_assign(e_this(), "textCache", e_var("text")),
                ]),
        )
        .method(
            method("refreshText")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("node", e_this()),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_expr(e_method_call(e_var("node"), "computeText", vec![])),
                        s_assign("node", e_prop(e_var("node"), "parentNode")),
                    ]),
                ]),
        )
        .method(
            method("isBuilding")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("doc", e_ternary(e_binop(e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(13))), e_this(), e_this_prop("ownerDocument"))),
                    s_return(e_binop(e_binop(e_var("doc"), BinOp::StrictNotEq, e_null()), BinOp::And, e_prop(e_var("doc"), "building"))),
                ]),
        )
        .method(
            method("syncText")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("type", e_this_prop("nodeType")),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("type"), BinOp::StrictEq, e_int(1)), BinOp::Or, e_binop(e_var("type"), BinOp::StrictEq, e_int(9))), BinOp::Or, e_binop(e_var("type"), BinOp::StrictEq, e_int(11))), BinOp::Or, e_binop(e_var("type"), BinOp::StrictEq, e_int(13))),
                        vec![
                            s_assign("current", e_ternary(e_binop(e_var("type"), BinOp::StrictEq, e_int(1)), e_this_prop("nodeValue"), e_this_prop("textContent"))),
                            s_if(
                                e_binop(e_var("current"), BinOp::StrictNotEq, e_this_prop("textCache")),
                                vec![
                                    s_expr(e_method_call(e_this(), "replaceWithText", vec![e_ternary(e_binop(e_var("current"), BinOp::StrictEq, e_null()), e_str(""), e_var("current"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("child", e_this_prop("firstChild")),
                            s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                                s_if(
                                    e_binop(e_prop(e_var("child"), "nodeType"), BinOp::StrictEq, e_int(1)),
                                    vec![
                                        s_expr(e_method_call(e_var("child"), "syncText", vec![])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("child", e_prop(e_var("child"), "nextSibling")),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("replaceWithText")
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("doc", e_method_call(e_this(), "owningDocument", vec![])),
                    s_if(
                        e_binop(e_var("doc"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("doc"), "building", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_while(e_binop(e_this_prop("firstChild"), BinOp::StrictNotEq, e_null()), vec![
                        s_expr(e_method_call(e_this_prop("firstChild"), "detach", vec![])),
                    ]),
                    s_if(
                        e_binop(e_var("value"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("text", e_new("DOMText", vec![e_var("value")])),
                            s_prop_assign(e_var("text"), "ownerDocument", e_var("doc")),
                            s_expr(e_method_call(e_this(), "linkBefore", vec![e_var("text"), e_null()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("doc"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("doc"), "building", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "refreshText", vec![])),
                ]),
        )
        .method(
            method("owningDocument")
                .returns(t_nullable(t_class("DOMDocument")))
                .body(vec![
                    s_if(
                        e_instance_of(e_this(), "DOMDocument"),
                        vec![
                            s_return(e_this()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("ownerDocument")),
                ]),
        )
        .method(
            method("setOwner")
                .param("document", t_nullable(t_class("DOMDocument")))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "ownerDocument", e_var("document")),
                    s_assign("node", e_this_prop("firstChild")),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_expr(e_method_call(e_var("node"), "setOwner", vec![e_var("document")])),
                        s_assign("node", e_prop(e_var("node"), "nextSibling")),
                    ]),
                    s_if(
                        e_instance_of(e_this(), "DOMElement"),
                        vec![
                            s_foreach(e_this_prop("attrNodes"), None, "attr", vec![
                                s_expr(e_method_call(e_var("attr"), "setOwner", vec![e_var("document")])),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("linkBefore")
                .param("node", t_class("DOMNode"))
                .param("ref", t_nullable(t_class("DOMNode")))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_var("node"), "parentNode", e_this()),
                    s_assign("prev", e_ternary(e_binop(e_var("ref"), BinOp::StrictEq, e_null()), e_this_prop("lastChild"), e_prop(e_var("ref"), "previousSibling"))),
                    s_prop_assign(e_var("node"), "previousSibling", e_var("prev")),
                    s_prop_assign(e_var("node"), "nextSibling", e_var("ref")),
                    s_if(
                        e_binop(e_var("prev"), BinOp::StrictEq, e_null()),
                        vec![
                            s_prop_assign(e_this(), "firstChild", e_var("node")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_var("prev"), "nextSibling", e_var("node")),
                    ]),
                    ),
                    s_if(
                        e_binop(e_var("ref"), BinOp::StrictEq, e_null()),
                        vec![
                            s_prop_assign(e_this(), "lastChild", e_var("node")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_var("ref"), "previousSibling", e_var("node")),
                    ]),
                    ),
                    s_expr(e_method_call(e_this(), "bumpCount", vec![e_int(1)])),
                    s_expr(e_method_call(e_this(), "childrenChanged", vec![])),
                    s_assign("kind", e_prop(e_var("node"), "nodeType")),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4))), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(1))), BinOp::And, e_binop(e_prop(e_var("node"), "textContent"), BinOp::StrictNotEq, e_str(""))), BinOp::And, e_not(e_method_call(e_this(), "isBuilding", vec![]))),
                        vec![
                            s_expr(e_method_call(e_this(), "refreshText", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("detach")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("parent", e_this_prop("parentNode")),
                    s_if(
                        e_binop(e_var("parent"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("prev", e_this_prop("previousSibling")),
                    s_assign("next", e_this_prop("nextSibling")),
                    s_if(
                        e_binop(e_var("prev"), BinOp::StrictEq, e_null()),
                        vec![
                            s_prop_assign(e_var("parent"), "firstChild", e_var("next")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_var("prev"), "nextSibling", e_var("next")),
                    ]),
                    ),
                    s_if(
                        e_binop(e_var("next"), BinOp::StrictEq, e_null()),
                        vec![
                            s_prop_assign(e_var("parent"), "lastChild", e_var("prev")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_var("next"), "previousSibling", e_var("prev")),
                    ]),
                    ),
                    s_prop_assign(e_this(), "parentNode", e_null()),
                    s_prop_assign(e_this(), "previousSibling", e_null()),
                    s_prop_assign(e_this(), "nextSibling", e_null()),
                    s_expr(e_method_call(e_var("parent"), "bumpCount", vec![e_neg(e_int(1))])),
                    s_expr(e_method_call(e_var("parent"), "childrenChanged", vec![])),
                    s_assign("kind", e_this_prop("nodeType")),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4))), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(1))), BinOp::And, e_binop(e_this_prop("textContent"), BinOp::StrictNotEq, e_str(""))), BinOp::And, e_not(e_method_call(e_var("parent"), "isBuilding", vec![]))),
                        vec![
                            s_expr(e_method_call(e_var("parent"), "refreshText", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("childrenChanged")
                .returns(TypeExpr::Void),
        )
        .method(
            method("bumpCount")
                .param("by", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("list", e_this_prop("childNodes")),
                    s_prop_assign(e_var("list"), "length", e_binop(e_prop(e_var("list"), "length"), BinOp::Add, e_var("by"))),
                ]),
        )
        .method(
            method("checkInsert")
                .param("node", t_class("DOMNode"))
                .param("child", t_nullable(t_class("DOMNode")))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(13))),
                        vec![
                            s_if(
                                e_binop(e_var("node"), BinOp::StrictEq, e_method_call(e_this(), "owningDocument", vec![])),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Hierarchy Request Error"), e_int(3)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
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
                    s_assign("mine", e_method_call(e_this(), "owningDocument", vec![])),
                    s_assign("theirs", e_method_call(e_var("node"), "owningDocument", vec![])),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("mine"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("theirs"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_var("mine"), BinOp::StrictNotEq, e_var("theirs"))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("child"), "parentNode"), BinOp::StrictNotEq, e_this())),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("appendChild")
                .param("node", t_class("DOMNode"))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_return(e_method_call(e_this(), "insertBefore", vec![e_var("node"), e_null()])),
                ]),
        )
        .method(
            method("insertBefore")
                .param("node", t_class("DOMNode"))
                .param_default("child", t_nullable(t_class("DOMNode")), e_null())
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_this_prop("nodeType"), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_this_prop("nodeType"), BinOp::StrictNotEq, e_int(9))), BinOp::And, e_binop(e_this_prop("nodeType"), BinOp::StrictNotEq, e_int(11))), BinOp::And, e_binop(e_this_prop("nodeType"), BinOp::StrictNotEq, e_int(13))),
                        vec![
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_instance_of(e_var("node"), "DOMAttr"), BinOp::And, e_instance_of(e_this(), "DOMElement")),
                        vec![
                            s_if(
                                e_binop(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("child"), "parentNode"), BinOp::StrictNotEq, e_this())),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "setAttributeNode", vec![e_var("node")])),
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "checkInsert", vec![e_var("node"), e_null()])),
                    s_if(
                        e_binop(e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(11)), BinOp::And, e_binop(e_prop(e_var("node"), "firstChild"), BinOp::StrictEq, e_null())),
                        vec![
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("child"), "parentNode"), BinOp::StrictNotEq, e_this())),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("child"), BinOp::StrictEq, e_var("node")),
                        vec![
                            s_expr(e_method_call(e_var("node"), "detach", vec![])),
                            s_throw(e_new_fq("Error", vec![e_str("Cannot add newnode as the previous sibling of refnode")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("mine", e_method_call(e_this(), "owningDocument", vec![])),
                    s_if(
                        e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(11)),
                        vec![
                            s_assign("first", e_prop(e_var("node"), "firstChild")),
                            s_while(e_binop(e_prop(e_var("node"), "firstChild"), BinOp::StrictNotEq, e_null()), vec![
                                s_assign("moved", e_prop(e_var("node"), "firstChild")),
                                s_expr(e_method_call(e_var("moved"), "detach", vec![])),
                                s_if(
                                    e_binop(e_var("mine"), BinOp::StrictNotEq, e_null()),
                                    vec![
                                        s_expr(e_method_call(e_var("moved"), "setOwner", vec![e_var("mine")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_this(), "linkBefore", vec![e_var("moved"), e_var("child")])),
                            ]),
                            s_return(e_var("first")),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_var("node"), "detach", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("mine"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("node"), "ownerDocument"), BinOp::StrictEq, e_null())),
                        vec![
                            s_expr(e_method_call(e_var("node"), "setOwner", vec![e_var("mine")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "linkBefore", vec![e_var("node"), e_var("child")])),
                    s_return(e_var("node")),
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
                    s_expr(e_method_call(e_var("child"), "detach", vec![])),
                    s_return(e_var("child")),
                ]),
        )
        .method(
            method("replaceChild")
                .param("node", t_class("DOMNode"))
                .param("child", t_class("DOMNode"))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("mine", e_method_call(e_this(), "owningDocument", vec![])),
                    s_assign("theirs", e_method_call(e_var("node"), "owningDocument", vec![])),
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
                            s_return(e_var("child")),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "checkInsert", vec![e_var("node"), e_null()])),
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
                    s_expr(e_method_call(e_this(), "insertBefore", vec![e_var("node"), e_var("child")])),
                    s_expr(e_method_call(e_var("child"), "detach", vec![])),
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
            method("hasAttributes")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(false)),
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
                .param("other", t_class("DOMNode"))
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
                .param_default("options", t_array(), e_array(vec![]))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("_unused", e_var("options")),
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
                    s_return(e_int(0)),
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
            method("lookupNamespaceURI")
                .param("prefix", t_nullable(TypeExpr::Str))
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_if(
                        e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xml")),
                        vec![
                            s_return(e_str("http://www.w3.org/XML/1998/namespace")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("node", e_this()),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_if(
                            e_instance_of(e_var("node"), "DOMElement"),
                            vec![
                                s_if(
                                    e_binop(e_prop(e_var("node"), "nsDecl"), BinOp::StrictNotEq, e_array(vec![])),
                                    vec![
                                        s_assign("key", e_ternary(e_binop(e_var("prefix"), BinOp::StrictEq, e_null()), e_str(""), e_var("prefix"))),
                                        s_if(
                                            e_call("isset", vec![e_index(e_prop(e_var("node"), "nsDecl"), e_var("key"))]),
                                            vec![
                                                s_return(e_index(e_prop(e_var("node"), "nsDecl"), e_var("key"))),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("node"), "prefix"), BinOp::StrictEq, e_ternary(e_binop(e_var("prefix"), BinOp::StrictEq, e_null()), e_str(""), e_var("prefix")))),
                                    vec![
                                        s_return(e_prop(e_var("node"), "namespaceURI")),
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
                    s_assign("node", e_this()),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_if(
                            e_instance_of(e_var("node"), "DOMElement"),
                            vec![
                                s_foreach(e_prop(e_var("node"), "nsDecl"), Some("declared"), "uri", vec![
                                    s_if(
                                        e_binop(e_var("uri"), BinOp::StrictEq, e_var("namespace")),
                                        vec![
                                            s_return(e_cast(CastType::String, e_var("declared"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                                s_if(
                                    e_binop(e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_var("namespace")), BinOp::And, e_binop(e_prop(e_var("node"), "prefix"), BinOp::StrictNotEq, e_str(""))),
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
            method("normalize")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("node", e_this_prop("firstChild")),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_assign("next", e_prop(e_var("node"), "nextSibling")),
                        s_if(
                            e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(3)),
                            vec![
                                s_while(e_binop(e_binop(e_var("next"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("next"), "nodeType"), BinOp::StrictEq, e_int(3))), vec![
                                    s_expr(e_method_call(e_var("node"), "appendData", vec![e_prop(e_var("next"), "data")])),
                                    s_assign("following", e_prop(e_var("next"), "nextSibling")),
                                    s_expr(e_method_call(e_var("next"), "detach", vec![])),
                                    s_assign("next", e_var("following")),
                                ]),
                                s_if(
                                    e_binop(e_prop(e_var("node"), "data"), BinOp::StrictEq, e_str("")),
                                    vec![
                                        s_expr(e_method_call(e_var("node"), "detach", vec![])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![
                            (e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(1)), vec![
                                s_expr(e_method_call(e_var("node"), "normalize", vec![])),
                            ]),
                        ],
                            None,
                        ),
                        s_assign("node", e_var("next")),
                    ]),
                ]),
        )
        .method(
            method("getNodePath")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("parentNode"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_ternary(e_instance_of(e_this(), "DOMDocument"), e_str("/"), e_null())),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("parentPath", e_method_call(e_this_prop("parentNode"), "getNodePath", vec![])),
                    s_assign("step", e_this_prop("nodeName")),
                    s_if(
                        e_binop(e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(4))),
                        vec![
                            s_assign("step", e_str("text()")),
                        ],
                        vec![
                        (e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(8)), vec![
                            s_assign("step", e_str("comment()")),
                        ]),
                        (e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(7)), vec![
                            s_assign("step", e_binop(e_binop(e_str("processing-instruction('"), BinOp::Concat, e_this_prop("nodeName")), BinOp::Concat, e_str("')"))),
                        ]),
                    ],
                        None,
                    ),
                    s_assign("same", e_int(0)),
                    s_assign("index", e_int(0)),
                    s_assign("sibling", e_prop(e_this_prop("parentNode"), "firstChild")),
                    s_while(e_binop(e_var("sibling"), BinOp::StrictNotEq, e_null()), vec![
                        s_assign("alike", e_binop(e_binop(e_prop(e_var("sibling"), "nodeType"), BinOp::StrictEq, e_this_prop("nodeType")), BinOp::Or, e_binop(e_binop(e_binop(e_prop(e_var("sibling"), "nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_prop(e_var("sibling"), "nodeType"), BinOp::StrictEq, e_int(4))), BinOp::And, e_binop(e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(4)))))),
                        s_if(
                            e_binop(e_var("alike"), BinOp::And, e_binop(e_binop(e_this_prop("nodeType"), BinOp::StrictNotEq, e_int(1)), BinOp::Or, e_binop(e_prop(e_var("sibling"), "nodeName"), BinOp::StrictEq, e_this_prop("nodeName")))),
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
                        s_assign("sibling", e_prop(e_var("sibling"), "nextSibling")),
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
            method("cloneNode")
                .param_default("deep", TypeExpr::Bool, e_bool(false))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_expr(e_method_call(e_this(), "syncText", vec![])),
                    s_assign("copy", e_method_call(e_this(), "shallowCopy", vec![])),
                    s_assign("home", e_this_prop("ownerDocument")),
                    s_expr(e_method_call(e_var("copy"), "setOwner", vec![e_var("home")])),
                    s_if(
                        e_var("deep"),
                        vec![
                            s_assign("doc", e_method_call(e_this(), "owningDocument", vec![])),
                            s_assign("was", e_binop(e_binop(e_var("doc"), BinOp::StrictNotEq, e_null()), BinOp::And, e_prop(e_var("doc"), "building"))),
                            s_if(
                                e_binop(e_var("doc"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_prop_assign(e_var("doc"), "building", e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("child", e_this_prop("firstChild")),
                            s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                                s_assign("part", e_method_call(e_var("child"), "cloneNode", vec![e_bool(true)])),
                                s_assign("last", e_prop(e_var("copy"), "lastChild")),
                                s_if(
                                    e_binop(e_binop(e_binop(e_prop(e_var("part"), "nodeType"), BinOp::StrictEq, e_int(3)), BinOp::And, e_binop(e_var("last"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(3))),
                                    vec![
                                        s_expr(e_method_call(e_var("last"), "appendData", vec![e_prop(e_var("part"), "data")])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_expr(e_method_call(e_var("copy"), "linkBefore", vec![e_var("part"), e_null()])),
                                ]),
                                ),
                                s_assign("child", e_prop(e_var("child"), "nextSibling")),
                            ]),
                            s_if(
                                e_binop(e_var("doc"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_prop_assign(e_var("doc"), "building", e_var("was")),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_var("copy"), "computeText", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("copy")),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_return(e_new("DOMNode", vec![])),
                ]),
        )
        .method(
            method("C14N")
                .param_default("exclusive", TypeExpr::Bool, e_bool(false))
                .param_default("withComments", TypeExpr::Bool, e_bool(false))
                .param_default("xpath", t_nullable(t_array()), e_null())
                .param_default("nsPrefixes", t_nullable(t_array()), e_null())
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_assign("_unused", e_array(vec![e_var("exclusive"), e_var("withComments"), e_var("xpath"), e_var("nsPrefixes")])),
                    s_return(e_static_call("__ElephcDomWriter", "xml", vec![e_this(), e_bool(false), e_bool(false), e_int(0)])),
                ]),
        )
        .build()
}

/// `DOMCharacterData` — transcribed from the PHP form.
fn decl_class_domcharacterdata() -> Stmt {
    class("DOMCharacterData")
        .extends("DOMNode")
        .method(
            method("__get")
                .param("name", TypeExpr::Str)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("length")),
                        vec![
                            s_return(e_static_call("__ElephcDom", "charLength", vec![e_this_prop("data")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_parent_call("__get", vec![e_var("name")])),
                ]),
        )
        .method(
            method("__isset")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("length")), BinOp::Or, e_parent_call("__isset", vec![e_var("name")]))),
                ]),
        )
        .method(
            method("bindData")
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "data", e_var("data")),
                    s_prop_ref_assign(e_this(), "nodeValue", e_this_prop("data")),
                    s_prop_ref_assign(e_this(), "textContent", e_this_prop("data")),
                ]),
        )
        .method(
            method("dataChanged")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("parentNode"), BinOp::StrictNotEq, e_null()), BinOp::And, e_not(e_method_call(e_this(), "isBuilding", vec![]))),
                        vec![
                            s_expr(e_method_call(e_this_prop("parentNode"), "refreshText", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("appendData")
                .param("data", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_prop_assign(e_this(), "data", e_binop(e_this_prop("data"), BinOp::Concat, e_var("data"))),
                    s_expr(e_method_call(e_this(), "dataChanged", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("substringData")
                .param("offset", TypeExpr::Int)
                .param("count", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("length", e_static_call("__ElephcDom", "charLength", vec![e_this_prop("data")])),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("offset"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("offset"), BinOp::Gt, e_var("length"))), BinOp::Or, e_binop(e_var("count"), BinOp::Lt, e_int(0))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Index Size Error"), e_int(1)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_var("offset"), e_var("count")])),
                ]),
        )
        .method(
            method("insertData")
                .param("offset", TypeExpr::Int)
                .param("data", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_assign("length", e_static_call("__ElephcDom", "charLength", vec![e_this_prop("data")])),
                    s_if(
                        e_binop(e_binop(e_var("offset"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("offset"), BinOp::Gt, e_var("length"))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Index Size Error"), e_int(1)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "data", e_binop(e_binop(e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_int(0), e_var("offset")]), BinOp::Concat, e_var("data")), BinOp::Concat, e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_var("offset"), e_var("length")]))),
                    s_expr(e_method_call(e_this(), "dataChanged", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("deleteData")
                .param("offset", TypeExpr::Int)
                .param("count", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("length", e_static_call("__ElephcDom", "charLength", vec![e_this_prop("data")])),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("offset"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("offset"), BinOp::Gt, e_var("length"))), BinOp::Or, e_binop(e_var("count"), BinOp::Lt, e_int(0))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Index Size Error"), e_int(1)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "data", e_binop(e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_int(0), e_var("offset")]), BinOp::Concat, e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_binop(e_var("offset"), BinOp::Add, e_var("count")), e_var("length")]))),
                    s_expr(e_method_call(e_this(), "dataChanged", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("replaceData")
                .param("offset", TypeExpr::Int)
                .param("count", TypeExpr::Int)
                .param("data", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_assign("length", e_static_call("__ElephcDom", "charLength", vec![e_this_prop("data")])),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("offset"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("offset"), BinOp::Gt, e_var("length"))), BinOp::Or, e_binop(e_var("count"), BinOp::Lt, e_int(0))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Index Size Error"), e_int(1)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "data", e_binop(e_binop(e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_int(0), e_var("offset")]), BinOp::Concat, e_var("data")), BinOp::Concat, e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_binop(e_var("offset"), BinOp::Add, e_var("count")), e_var("length")]))),
                    s_expr(e_method_call(e_this(), "dataChanged", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .build()
}

/// `DOMText` — transcribed from the PHP form.
fn decl_class_domtext() -> Stmt {
    class("DOMText")
        .extends("DOMCharacterData")
        .method(
            method("__construct")
                .param_default("data", t_mixed(), e_str(""))
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_expr(e_parent_call("__construct", vec![])),
                    s_prop_assign(e_this(), "nodeType", e_int(3)),
                    s_prop_assign(e_this(), "nodeName", e_str("#text")),
                    s_expr(e_method_call(e_this(), "bindData", vec![e_var("data")])),
                ]),
        )
        .method(
            method("__get")
                .param("name", TypeExpr::Str)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("wholeText")),
                        vec![
                            s_assign("start", e_this()),
                            s_while(e_binop(e_binop(e_prop(e_var("start"), "previousSibling"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_prop(e_prop(e_var("start"), "previousSibling"), "nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_prop(e_prop(e_var("start"), "previousSibling"), "nodeType"), BinOp::StrictEq, e_int(4)))), vec![
                                s_assign("start", e_prop(e_var("start"), "previousSibling")),
                            ]),
                            s_assign("text", e_str("")),
                            s_while(e_binop(e_binop(e_var("start"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_prop(e_var("start"), "nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_prop(e_var("start"), "nodeType"), BinOp::StrictEq, e_int(4)))), vec![
                                s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_prop(e_var("start"), "data"))),
                                s_assign("start", e_prop(e_var("start"), "nextSibling")),
                            ]),
                            s_return(e_var("text")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_parent_call("__get", vec![e_var("name")])),
                ]),
        )
        .method(
            method("isWhitespaceInElementContent")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("trim", vec![e_this_prop("data"), e_str(" \t\r\n")]), BinOp::StrictEq, e_str(""))),
                ]),
        )
        .method(
            method("isElementContentWhitespace")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("trim", vec![e_this_prop("data"), e_str(" \t\r\n")]), BinOp::StrictEq, e_str(""))),
                ]),
        )
        .method(
            method("splitText")
                .param("offset", TypeExpr::Int)
                .returns(t_class("DOMText"))
                .body(vec![
                    s_assign("length", e_static_call("__ElephcDom", "charLength", vec![e_this_prop("data")])),
                    s_if(
                        e_binop(e_binop(e_var("offset"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("offset"), BinOp::Gt, e_var("length"))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Index Size Error"), e_int(1)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("tail", e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_var("offset"), e_var("length")])),
                    s_prop_assign(e_this(), "data", e_static_call("__ElephcDom", "charSlice", vec![e_this_prop("data"), e_int(0), e_var("offset")])),
                    s_assign("new", e_ternary(e_binop(e_this_prop("nodeType"), BinOp::StrictEq, e_int(4)), e_new("DOMCdataSection", vec![e_var("tail")]), e_new("DOMText", vec![e_var("tail")]))),
                    s_prop_assign(e_var("new"), "ownerDocument", e_this_prop("ownerDocument")),
                    s_if(
                        e_binop(e_this_prop("parentNode"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("after", e_this_prop("nextSibling")),
                            s_expr(e_method_call(e_this_prop("parentNode"), "linkBefore", vec![e_var("new"), e_var("after")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("new")),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_return(e_new("DOMText", vec![e_this_prop("data")])),
                ]),
        )
        .build()
}

/// `DOMComment` — transcribed from the PHP form.
fn decl_class_domcomment() -> Stmt {
    class("DOMComment")
        .extends("DOMCharacterData")
        .method(
            method("__construct")
                .param_default("data", t_mixed(), e_str(""))
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_expr(e_parent_call("__construct", vec![])),
                    s_prop_assign(e_this(), "nodeType", e_int(8)),
                    s_prop_assign(e_this(), "nodeName", e_str("#comment")),
                    s_expr(e_method_call(e_this(), "bindData", vec![e_var("data")])),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_return(e_new("DOMComment", vec![e_this_prop("data")])),
                ]),
        )
        .build()
}

/// `DOMCdataSection` — transcribed from the PHP form.
fn decl_class_domcdatasection() -> Stmt {
    class("DOMCdataSection")
        .extends("DOMText")
        .method(
            method("__construct")
                .param_default("data", t_mixed(), e_str(""))
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_expr(e_parent_call("__construct", vec![e_var("data")])),
                    s_prop_assign(e_this(), "nodeType", e_int(4)),
                    s_prop_assign(e_this(), "nodeName", e_str("#cdata-section")),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_return(e_new("DOMCdataSection", vec![e_this_prop("data")])),
                ]),
        )
        .build()
}

/// `DOMProcessingInstruction` — transcribed from the PHP form.
fn decl_class_domprocessinginstruction() -> Stmt {
    class("DOMProcessingInstruction")
        .extends("DOMNode")
        .method(
            method("__construct")
                .param("name", TypeExpr::Str)
                .param_default("value", t_mixed(), e_str(""))
                .body(vec![
                    s_assign("value", e_cast(CastType::String, e_var("value"))),
                    s_expr(e_parent_call("__construct", vec![])),
                    s_prop_assign(e_this(), "nodeType", e_int(7)),
                    s_prop_assign(e_this(), "nodeName", e_var("name")),
                    s_prop_assign(e_this(), "target", e_var("name")),
                    s_prop_assign(e_this(), "data", e_var("value")),
                    s_prop_ref_assign(e_this(), "nodeValue", e_this_prop("data")),
                    s_prop_ref_assign(e_this(), "textContent", e_this_prop("data")),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_return(e_new("DOMProcessingInstruction", vec![e_this_prop("target"), e_this_prop("data")])),
                ]),
        )
        .build()
}

/// `DOMAttr` — transcribed from the PHP form.
fn decl_class_domattr() -> Stmt {
    class("DOMAttr")
        .extends("DOMNode")
        .prop("specified", TypeExpr::Bool, Some(e_bool(true)))
        .prop("valueless", TypeExpr::Bool, Some(e_bool(false)))
        .prop("schemaTypeInfo", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .param("name", TypeExpr::Str)
                .param_default("value", t_mixed(), e_str(""))
                .body(vec![
                    s_assign("value", e_cast(CastType::String, e_var("value"))),
                    s_expr(e_parent_call("__construct", vec![])),
                    s_prop_assign(e_this(), "nodeType", e_int(2)),
                    s_prop_assign(e_this(), "name", e_var("name")),
                    s_prop_assign(e_this(), "nodeName", e_var("name")),
                    s_prop_assign(e_this(), "value", e_var("value")),
                    s_prop_ref_assign(e_this(), "nodeValue", e_this_prop("value")),
                    s_prop_ref_assign(e_this(), "textContent", e_this_prop("value")),
                    s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                    s_if(
                        e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_prop_assign(e_this(), "prefix", e_call("substr", vec![e_var("name"), e_int(0), e_var("colon")])),
                            s_prop_assign(e_this(), "localName", e_call("substr", vec![e_var("name"), e_binop(e_var("colon"), BinOp::Add, e_int(1))])),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "localName", e_var("name")),
                    ]),
                    ),
                ]),
        )
        .method(
            method("applyNamespace")
                .param("uri", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "namespaceURI", e_var("uri")),
                    s_if(
                        e_binop(e_binop(e_this_prop("localName"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_this_prop("prefix"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_prop_assign(e_this(), "name", e_this_prop("localName")),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("isId")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_binop(e_this_prop("name"), BinOp::StrictEq, e_str("id")), BinOp::And, e_binop(e_this_prop("ownerDocument"), BinOp::StrictNotEq, e_null())), BinOp::And, e_prop(e_this_prop("ownerDocument"), "isHtml"))),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("copy", e_new("DOMAttr", vec![e_this_prop("nodeName"), e_this_prop("value")])),
                    s_prop_assign(e_var("copy"), "namespaceURI", e_this_prop("namespaceURI")),
                    s_prop_assign(e_var("copy"), "prefix", e_this_prop("prefix")),
                    s_prop_assign(e_var("copy"), "localName", e_this_prop("localName")),
                    s_prop_assign(e_var("copy"), "name", e_this_prop("name")),
                    s_return(e_var("copy")),
                ]),
        )
        .build()
}

/// `DOMElement` — transcribed from the PHP form.
fn decl_class_domelement() -> Stmt {
    class("DOMElement")
        .extends("DOMNode")
        .prop("attrNodes", t_array(), Some(e_array(vec![])))
        .prop("nsDecl", t_array(), Some(e_array(vec![])))
        .prop("nsAuto", t_array(), Some(e_array(vec![])))
        .prop("schemaTypeInfo", TypeExpr::Str, Some(e_str("")))
        .method(
            method("__construct")
                .param("qualifiedName", TypeExpr::Str)
                .param_default("value", t_mixed(), e_null())
                .param_default("namespace", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_assign("value", e_ternary(e_binop(e_var("value"), BinOp::StrictEq, e_null()), e_null(), e_cast(CastType::String, e_var("value")))),
                    s_expr(e_parent_call("__construct", vec![])),
                    s_if(
                        e_not(e_static_call("__ElephcDom", "validName", vec![e_var("qualifiedName")])),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "nodeType", e_int(1)),
                    s_prop_assign(e_this(), "nodeName", e_var("qualifiedName")),
                    s_prop_assign(e_this(), "tagName", e_var("qualifiedName")),
                    s_prop_assign(e_this(), "nodeValue", e_str("")),
                    s_prop_ref_assign(e_this(), "textContent", e_this_prop("nodeValue")),
                    s_prop_assign(e_this(), "attributes", e_new("DOMNamedNodeMap", vec![e_this()])),
                    s_assign("colon", e_call("strpos", vec![e_var("qualifiedName"), e_str(":")])),
                    s_if(
                        e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_prop_assign(e_this(), "prefix", e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("colon")])),
                            s_prop_assign(e_this(), "localName", e_call("substr", vec![e_var("qualifiedName"), e_binop(e_var("colon"), BinOp::Add, e_int(1))])),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "localName", e_var("qualifiedName")),
                    ]),
                    ),
                    s_if(
                        e_binop(e_var("namespace"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "namespaceURI", e_var("namespace")),
                            s_prop_array_assign(e_this(), "nsDecl", e_this_prop("prefix"), e_var("namespace")),
                            s_prop_array_assign(e_this(), "nsAuto", e_this_prop("prefix"), e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("value"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_method_call(e_this(), "linkBefore", vec![e_new("DOMText", vec![e_var("value")]), e_null()])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("hasAttributes")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_this_prop("attrNodes"), BinOp::StrictNotEq, e_array(vec![]))),
                ]),
        )
        .method(
            method("syncAttrCount")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("map", e_this_prop("attributes")),
                    s_if(
                        e_binop(e_var("map"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("map"), "length", e_call("count", vec![e_this_prop("attrNodes")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("lookupAttrKey")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_var("name")),
                ]),
        )
        .method(
            method("getAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("key", e_method_call(e_this(), "lookupAttrKey", vec![e_var("qualifiedName")])),
                    s_if(
                        e_call("isset", vec![e_index(e_this_prop("attrNodes"), e_var("key"))]),
                        vec![
                            s_return(e_prop(e_index(e_this_prop("attrNodes"), e_var("key")), "value")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_starts_with", vec![e_var("qualifiedName"), e_str("xmlns")]),
                        vec![
                            s_assign("declared", e_ternary(e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")), e_str(""), e_call("substr", vec![e_var("qualifiedName"), e_int(6)]))),
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("nsDecl"), e_var("declared"))]),
                                vec![
                                    s_return(e_index(e_this_prop("nsDecl"), e_var("declared"))),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_str("")),
                ]),
        )
        .method(
            method("getAttributeNode")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_union(vec![t_class("DOMAttr"), TypeExpr::False]))
                .body(vec![
                    s_assign("key", e_method_call(e_this(), "lookupAttrKey", vec![e_var("qualifiedName")])),
                    s_if(
                        e_call("isset", vec![e_index(e_this_prop("attrNodes"), e_var("key"))]),
                        vec![
                            s_return(e_index(e_this_prop("attrNodes"), e_var("key"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("setAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .param("value", t_mixed())
                .returns(t_class("DOMAttr"))
                .body(vec![
                    s_assign("value", e_cast(CastType::String, e_var("value"))),
                    s_if(
                        e_not(e_static_call("__ElephcDom", "validName", vec![e_var("qualifiedName")])),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("key", e_method_call(e_this(), "lookupAttrKey", vec![e_var("qualifiedName")])),
                    s_if(
                        e_call("isset", vec![e_index(e_this_prop("attrNodes"), e_var("key"))]),
                        vec![
                            s_assign("existing", e_index(e_this_prop("attrNodes"), e_var("key"))),
                            s_prop_assign(e_var("existing"), "value", e_var("value")),
                            s_prop_assign(e_var("existing"), "valueless", e_bool(false)),
                            s_return(e_var("existing")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("attr", e_new("DOMAttr", vec![e_var("key"), e_var("value")])),
                    s_if(
                        e_binop(e_prop(e_var("attr"), "prefix"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("bound", e_method_call(e_this(), "lookupNamespaceURI", vec![e_prop(e_var("attr"), "prefix")])),
                            s_if(
                                e_binop(e_var("bound"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_expr(e_method_call(e_var("attr"), "applyNamespace", vec![e_var("bound")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_var("attr"), "ownerElement", e_this()),
                    s_prop_assign(e_var("attr"), "ownerDocument", e_this_prop("ownerDocument")),
                    s_prop_assign(e_var("attr"), "parentNode", e_this()),
                    s_prop_array_assign(e_this(), "attrNodes", e_var("key"), e_var("attr")),
                    s_expr(e_method_call(e_this(), "syncAttrCount", vec![])),
                    s_return(e_var("attr")),
                ]),
        )
        .method(
            method("setAttributeNode")
                .param("attr", t_class("DOMAttr"))
                .returns(t_nullable(t_class("DOMAttr")))
                .body(vec![
                    s_assign("old", e_null_coalesce(e_index(e_this_prop("attrNodes"), e_prop(e_var("attr"), "nodeName")), e_null())),
                    s_if(
                        e_binop(e_var("old"), BinOp::StrictEq, e_var("attr")),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_prop(e_var("attr"), "ownerElement"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Inuse Attribute Error"), e_int(10)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("mine", e_method_call(e_this(), "owningDocument", vec![])),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("mine"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("attr"), "ownerDocument"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_prop(e_var("attr"), "ownerDocument"), BinOp::StrictNotEq, e_var("mine"))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("old"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("old"), "ownerElement", e_null()),
                            s_prop_assign(e_var("old"), "parentNode", e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_var("attr"), "ownerElement", e_this()),
                    s_prop_assign(e_var("attr"), "parentNode", e_this()),
                    s_prop_assign(e_var("attr"), "ownerDocument", e_var("mine")),
                    s_prop_array_assign(e_this(), "attrNodes", e_prop(e_var("attr"), "nodeName"), e_var("attr")),
                    s_expr(e_method_call(e_this(), "syncAttrCount", vec![])),
                    s_return(e_var("old")),
                ]),
        )
        .method(
            method("removeAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("key", e_method_call(e_this(), "lookupAttrKey", vec![e_var("qualifiedName")])),
                    s_if(
                        e_not(e_call("isset", vec![e_index(e_this_prop("attrNodes"), e_var("key"))])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("attr", e_index(e_this_prop("attrNodes"), e_var("key"))),
                    s_prop_assign(e_var("attr"), "ownerElement", e_null()),
                    s_prop_assign(e_var("attr"), "parentNode", e_null()),
                    s_assign("kept", e_array(vec![])),
                    s_foreach(e_this_prop("attrNodes"), Some("name"), "node", vec![
                        s_if(
                            e_binop(e_var("name"), BinOp::StrictNotEq, e_var("key")),
                            vec![
                                s_array_assign("kept", e_var("name"), e_var("node")),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_prop_assign(e_this(), "attrNodes", e_var("kept")),
                    s_expr(e_method_call(e_this(), "syncAttrCount", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("removeAttributeNode")
                .param("attr", t_class("DOMAttr"))
                .returns(t_union(vec![t_class("DOMAttr"), TypeExpr::False]))
                .body(vec![
                    s_if(
                        e_binop(e_prop(e_var("attr"), "ownerElement"), BinOp::StrictNotEq, e_this()),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Not Found Error"), e_int(8)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "removeAttribute", vec![e_prop(e_var("attr"), "nodeName")])),
                    s_return(e_var("attr")),
                ]),
        )
        .method(
            method("hasAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("key", e_method_call(e_this(), "lookupAttrKey", vec![e_var("qualifiedName")])),
                    s_if(
                        e_call("isset", vec![e_index(e_this_prop("attrNodes"), e_var("key"))]),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_starts_with", vec![e_var("qualifiedName"), e_str("xmlns")]),
                        vec![
                            s_assign("declared", e_ternary(e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")), e_str(""), e_call("substr", vec![e_var("qualifiedName"), e_int(6)]))),
                            s_return(e_call("isset", vec![e_index(e_this_prop("nsDecl"), e_var("declared"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("getAttributeNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("attr", e_method_call(e_this(), "getAttributeNodeNS", vec![e_var("namespace"), e_var("localName")])),
                    s_return(e_ternary(e_binop(e_var("attr"), BinOp::StrictEq, e_null()), e_str(""), e_prop(e_var("attr"), "value"))),
                ]),
        )
        .method(
            method("getAttributeNodeNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .returns(t_nullable(t_class("DOMAttr")))
                .body(vec![
                    s_assign("wanted", e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_str("")), e_null(), e_var("namespace"))),
                    s_foreach(e_this_prop("attrNodes"), None, "attr", vec![
                        s_if(
                            e_binop(e_binop(e_prop(e_var("attr"), "localName"), BinOp::StrictEq, e_var("localName")), BinOp::And, e_binop(e_prop(e_var("attr"), "namespaceURI"), BinOp::StrictEq, e_var("wanted"))),
                            vec![
                                s_return(e_var("attr")),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("hasAttributeNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_method_call(e_this(), "getAttributeNodeNS", vec![e_var("namespace"), e_var("localName")]), BinOp::StrictNotEq, e_null())),
                ]),
        )
        .method(
            method("setAttributeNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("qualifiedName", TypeExpr::Str)
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("value", e_cast(CastType::String, e_var("value"))),
                    s_assign("colon", e_call("strpos", vec![e_var("qualifiedName"), e_str(":")])),
                    s_assign("prefix", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_str(""), e_call("substr", vec![e_var("qualifiedName"), e_int(0), e_var("colon")]))),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("namespace"), BinOp::StrictEq, e_str("http://www.w3.org/2000/xmlns/")), BinOp::Or, e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xmlns"))), BinOp::Or, e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns"))),
                        vec![
                            s_assign("declared", e_var("prefix")),
                            s_if(
                                e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("xmlns")),
                                vec![
                                    s_assign("declared", e_str("")),
                                ],
                                vec![
                                (e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xmlns")), vec![
                                    s_assign("declared", e_call("substr", vec![e_var("qualifiedName"), e_int(6)])),
                                ]),
                            ],
                                None,
                            ),
                            s_prop_array_assign(e_this(), "nsDecl", e_var("declared"), e_var("value")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("attr", e_method_call(e_this(), "setAttribute", vec![e_var("qualifiedName"), e_var("value")])),
                    s_if(
                        e_binop(e_binop(e_var("namespace"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("namespace"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_method_call(e_var("attr"), "applyNamespace", vec![e_var("namespace")])),
                            s_if(
                                e_binop(e_binop(e_var("prefix"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_not(e_call("isset", vec![e_index(e_this_prop("nsDecl"), e_var("prefix"))]))),
                                vec![
                                    s_prop_array_assign(e_this(), "nsDecl", e_var("prefix"), e_var("namespace")),
                                    s_prop_array_assign(e_this(), "nsAuto", e_var("prefix"), e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("removeAttributeNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("attr", e_method_call(e_this(), "getAttributeNodeNS", vec![e_var("namespace"), e_var("localName")])),
                    s_if(
                        e_binop(e_var("attr"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "removeAttribute", vec![e_prop(e_var("attr"), "nodeName")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("setIdAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .param("isId", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_unused", e_array(vec![e_var("qualifiedName"), e_var("isId")])),
                ]),
        )
        .method(
            method("getElementsByTagName")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_assign("found", e_array(vec![])),
                    s_expr(e_static_call("__ElephcDom", "collectByTag", vec![e_this(), e_var("qualifiedName"), e_var("found")])),
                    s_return(e_new("DOMNodeList", vec![e_null(), e_var("found")])),
                ]),
        )
        .method(
            method("getElementsByTagNameNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_assign("found", e_array(vec![])),
                    s_expr(e_static_call("__ElephcDom", "collectByTagNS", vec![e_this(), e_var("namespace"), e_var("localName"), e_var("found")])),
                    s_return(e_new("DOMNodeList", vec![e_null(), e_var("found")])),
                ]),
        )
        .method(
            method("remove")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "detach", vec![])),
                ]),
        )
        .method(
            method("append")
                .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_foreach(e_var("nodes"), None, "node", vec![
                        s_expr(e_method_call(e_this(), "appendChild", vec![e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_new("DOMText", vec![e_var("node")]))])),
                    ]),
                ]),
        )
        .method(
            method("prepend")
                .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("first", e_this_prop("firstChild")),
                    s_foreach(e_var("nodes"), None, "node", vec![
                        s_expr(e_method_call(e_this(), "insertBefore", vec![e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_new("DOMText", vec![e_var("node")])), e_var("first")])),
                    ]),
                ]),
        )
        .method(
            method("before")
                .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("parentNode"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_foreach(e_var("nodes"), None, "node", vec![
                        s_expr(e_method_call(e_this_prop("parentNode"), "insertBefore", vec![e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_new("DOMText", vec![e_var("node")])), e_this()])),
                    ]),
                ]),
        )
        .method(
            method("after")
                .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("parentNode"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("anchor", e_this_prop("nextSibling")),
                    s_foreach(e_var("nodes"), None, "node", vec![
                        s_expr(e_method_call(e_this_prop("parentNode"), "insertBefore", vec![e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_new("DOMText", vec![e_var("node")])), e_var("anchor")])),
                    ]),
                ]),
        )
        .method(
            method("replaceWith")
                .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("parentNode"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("anchor", e_this_prop("nextSibling")),
                    s_assign("parent", e_this_prop("parentNode")),
                    s_expr(e_method_call(e_this(), "detach", vec![])),
                    s_foreach(e_var("nodes"), None, "node", vec![
                        s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_new("DOMText", vec![e_var("node")])), e_var("anchor")])),
                    ]),
                ]),
        )
        .method(
            method("replaceChildren")
                .variadic("nodes", Some(t_union(vec![t_class("DOMNode"), TypeExpr::Str])))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_this_prop("firstChild"), BinOp::StrictNotEq, e_null()), vec![
                        s_expr(e_method_call(e_this_prop("firstChild"), "detach", vec![])),
                    ]),
                    s_foreach(e_var("nodes"), None, "node", vec![
                        s_expr(e_method_call(e_this(), "appendChild", vec![e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_new("DOMText", vec![e_var("node")]))])),
                    ]),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("copy", e_new("DOMElement", vec![e_this_prop("nodeName")])),
                    s_prop_assign(e_var("copy"), "namespaceURI", e_this_prop("namespaceURI")),
                    s_prop_assign(e_var("copy"), "prefix", e_this_prop("prefix")),
                    s_prop_assign(e_var("copy"), "localName", e_this_prop("localName")),
                    s_prop_assign(e_var("copy"), "nsDecl", e_this_prop("nsDecl")),
                    s_prop_assign(e_var("copy"), "nsAuto", e_this_prop("nsAuto")),
                    s_foreach(e_this_prop("attrNodes"), Some("key"), "attr", vec![
                        s_assign("clone", e_method_call(e_var("attr"), "shallowCopy", vec![])),
                        s_prop_assign(e_var("clone"), "ownerElement", e_var("copy")),
                        s_prop_assign(e_var("clone"), "parentNode", e_var("copy")),
                        s_prop_array_assign(e_var("copy"), "attrNodes", e_var("key"), e_var("clone")),
                    ]),
                    s_expr(e_method_call(e_var("copy"), "syncAttrCount", vec![])),
                    s_return(e_var("copy")),
                ]),
        )
        .build()
}

/// `DOMDocumentFragment` — transcribed from the PHP form.
fn decl_class_domdocumentfragment() -> Stmt {
    class("DOMDocumentFragment")
        .extends("DOMNode")
        .method(
            method("__construct")
                .body(vec![
                    s_expr(e_parent_call("__construct", vec![])),
                    s_prop_assign(e_this(), "nodeType", e_int(11)),
                    s_prop_assign(e_this(), "nodeName", e_str("#document-fragment")),
                ]),
        )
        .method(
            method("appendXML")
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("doc", e_new("DOMDocument", vec![])),
                    s_if(
                        e_not(e_method_call(e_var("doc"), "loadXML", vec![e_binop(e_binop(e_str("<elephc-fragment>"), BinOp::Concat, e_var("data")), BinOp::Concat, e_str("</elephc-fragment>"))])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("owner", e_method_call(e_this(), "owningDocument", vec![])),
                    s_assign("holder", e_prop(e_var("doc"), "documentElement")),
                    s_while(e_binop(e_binop(e_var("holder"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("holder"), "firstChild"), BinOp::StrictNotEq, e_null())), vec![
                        s_assign("moved", e_prop(e_var("holder"), "firstChild")),
                        s_expr(e_method_call(e_var("moved"), "detach", vec![])),
                        s_expr(e_method_call(e_var("moved"), "setOwner", vec![e_var("owner")])),
                        s_expr(e_method_call(e_this(), "linkBefore", vec![e_var("moved"), e_null()])),
                    ]),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_return(e_new("DOMDocumentFragment", vec![])),
                ]),
        )
        .build()
}

/// `DOMDocumentType` — transcribed from the PHP form.
fn decl_class_domdocumenttype() -> Stmt {
    class("DOMDocumentType")
        .extends("DOMNode")
        .prop("publicId", TypeExpr::Str, Some(e_str("")))
        .prop("systemId", TypeExpr::Str, Some(e_str("")))
        .prop("internalSubset", t_nullable(TypeExpr::Str), Some(e_null()))
        .method(
            method("__construct")
                .param_default("name", TypeExpr::Str, e_str(""))
                .param_default("publicId", TypeExpr::Str, e_str(""))
                .param_default("systemId", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_expr(e_parent_call("__construct", vec![])),
                    s_prop_assign(e_this(), "nodeType", e_int(10)),
                    s_prop_assign(e_this(), "nodeName", e_var("name")),
                    s_prop_assign(e_this(), "name", e_var("name")),
                    s_prop_assign(e_this(), "publicId", e_var("publicId")),
                    s_prop_assign(e_this(), "systemId", e_var("systemId")),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("copy", e_new("DOMDocumentType", vec![e_this_prop("name"), e_this_prop("publicId"), e_this_prop("systemId")])),
                    s_prop_assign(e_var("copy"), "internalSubset", e_this_prop("internalSubset")),
                    s_return(e_var("copy")),
                ]),
        )
        .build()
}

/// `DOMDocument` — transcribed from the PHP form.
fn decl_class_domdocument() -> Stmt {
    class("DOMDocument")
        .extends("DOMNode")
        .prop("doctype", t_nullable(t_class("DOMDocumentType")), Some(e_null()))
        .prop("documentElement", t_nullable(t_class("DOMElement")), Some(e_null()))
        .prop("encoding", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("xmlEncoding", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("standalone", TypeExpr::Bool, Some(e_bool(false)))
        .prop("xmlStandalone", TypeExpr::Bool, Some(e_bool(false)))
        .prop("version", t_nullable(TypeExpr::Str), Some(e_str("1.0")))
        .prop("xmlVersion", t_nullable(TypeExpr::Str), Some(e_str("1.0")))
        .prop("strictErrorChecking", TypeExpr::Bool, Some(e_bool(true)))
        .prop("documentURI", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("formatOutput", TypeExpr::Bool, Some(e_bool(false)))
        .prop("validateOnParse", TypeExpr::Bool, Some(e_bool(false)))
        .prop("resolveExternals", TypeExpr::Bool, Some(e_bool(false)))
        .prop("preserveWhiteSpace", TypeExpr::Bool, Some(e_bool(true)))
        .prop("recover", TypeExpr::Bool, Some(e_bool(false)))
        .prop("substituteEntities", TypeExpr::Bool, Some(e_bool(false)))
        .prop("isHtml", TypeExpr::Bool, Some(e_bool(false)))
        .prop("building", TypeExpr::Bool, Some(e_bool(false)))
        .prop("standaloneSet", TypeExpr::Bool, Some(e_bool(false)))
        .prop("idIndex", t_array(), Some(e_array(vec![])))
        .method(
            method("__construct")
                .param_default("version", TypeExpr::Str, e_str("1.0"))
                .param_default("encoding", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_expr(e_parent_call("__construct", vec![])),
                    s_static_prop_assign("__ElephcDomState", "documents", e_binop(e_static_prop("__ElephcDomState", "documents"), BinOp::Add, e_int(1))),
                    s_if(
                        e_binop(e_binop(e_static_prop("__ElephcDomState", "documents"), BinOp::Mod, e_int(8)), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_expr(e_call("gc_collect_cycles", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "nodeType", e_int(9)),
                    s_prop_assign(e_this(), "nodeName", e_str("#document")),
                    s_prop_assign(e_this(), "version", e_var("version")),
                    s_prop_assign(e_this(), "xmlVersion", e_var("version")),
                    s_if(
                        e_binop(e_var("encoding"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "encoding", e_var("encoding")),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("childrenChanged")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "documentElement", e_null()),
                    s_prop_assign(e_this(), "doctype", e_null()),
                    s_assign("node", e_this_prop("firstChild")),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_if(
                            e_binop(e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(1)), BinOp::And, e_binop(e_this_prop("documentElement"), BinOp::StrictEq, e_null())),
                            vec![
                                s_prop_assign(e_this(), "documentElement", e_var("node")),
                            ],
                            vec![
                            (e_binop(e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(10)), BinOp::And, e_binop(e_this_prop("doctype"), BinOp::StrictEq, e_null())), vec![
                                s_prop_assign(e_this(), "doctype", e_var("node")),
                            ]),
                        ],
                            None,
                        ),
                        s_assign("node", e_prop(e_var("node"), "nextSibling")),
                    ]),
                ]),
        )
        .method(
            method("createElement")
                .param("localName", TypeExpr::Str)
                .param_default("value", t_mixed(), e_str(""))
                .returns(t_class("DOMElement"))
                .body(vec![
                    s_assign("value", e_cast(CastType::String, e_var("value"))),
                    s_assign("name", e_var("localName")),
                    s_assign("element", e_new("DOMElement", vec![e_var("name")])),
                    s_prop_assign(e_var("element"), "ownerDocument", e_this()),
                    s_if(
                        e_binop(e_var("value"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("text", e_static_call("__ElephcDom", "decodeValue", vec![e_var("value")])),
                            s_if(
                                e_binop(e_var("text"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_return(e_var("element")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("text"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_assign("node", e_new("DOMText", vec![e_var("text")])),
                                    s_prop_assign(e_var("node"), "ownerDocument", e_this()),
                                    s_expr(e_method_call(e_var("element"), "linkBefore", vec![e_var("node"), e_null()])),
                                ],
                                vec![],
                                None,
                            ),
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
                .param_default("value", t_mixed(), e_str(""))
                .returns(t_class("DOMElement"))
                .body(vec![
                    s_assign("value", e_cast(CastType::String, e_var("value"))),
                    s_assign("element", e_new("DOMElement", vec![e_var("qualifiedName"), e_null(), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_str(""), e_var("namespace"))])),
                    s_prop_assign(e_var("element"), "ownerDocument", e_this()),
                    s_if(
                        e_binop(e_var("value"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("text", e_static_call("__ElephcDom", "decodeValue", vec![e_var("value")])),
                            s_if(
                                e_binop(e_binop(e_var("text"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("text"), BinOp::StrictNotEq, e_str(""))),
                                vec![
                                    s_assign("node", e_new("DOMText", vec![e_var("text")])),
                                    s_prop_assign(e_var("node"), "ownerDocument", e_this()),
                                    s_expr(e_method_call(e_var("element"), "linkBefore", vec![e_var("node"), e_null()])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("element")),
                ]),
        )
        .method(
            method("createTextNode")
                .param("data", t_mixed())
                .returns(t_class("DOMText"))
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_assign("text", e_new("DOMText", vec![e_var("data")])),
                    s_prop_assign(e_var("text"), "ownerDocument", e_this()),
                    s_return(e_var("text")),
                ]),
        )
        .method(
            method("createComment")
                .param("data", t_mixed())
                .returns(t_class("DOMComment"))
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_assign("comment", e_new("DOMComment", vec![e_var("data")])),
                    s_prop_assign(e_var("comment"), "ownerDocument", e_this()),
                    s_return(e_var("comment")),
                ]),
        )
        .method(
            method("createCDATASection")
                .param("data", t_mixed())
                .returns(t_class("DOMCdataSection"))
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_assign("cdata", e_new("DOMCdataSection", vec![e_var("data")])),
                    s_prop_assign(e_var("cdata"), "ownerDocument", e_this()),
                    s_return(e_var("cdata")),
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
                        e_not(e_static_call("__ElephcDom", "validName", vec![e_var("localName")])),
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
                .returns(t_class("DOMAttr"))
                .body(vec![
                    s_assign("attr", e_method_call(e_this(), "createAttribute", vec![e_var("qualifiedName")])),
                    s_if(
                        e_binop(e_binop(e_var("namespace"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("namespace"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_expr(e_method_call(e_var("attr"), "applyNamespace", vec![e_var("namespace")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("attr")),
                ]),
        )
        .method(
            method("createProcessingInstruction")
                .param("target", TypeExpr::Str)
                .param_default("data", t_mixed(), e_str(""))
                .returns(t_class("DOMProcessingInstruction"))
                .body(vec![
                    s_assign("data", e_cast(CastType::String, e_var("data"))),
                    s_if(
                        e_not(e_static_call("__ElephcDom", "validName", vec![e_var("target")])),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid Character Error"), e_int(5)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("pi", e_new("DOMProcessingInstruction", vec![e_var("target"), e_var("data")])),
                    s_prop_assign(e_var("pi"), "ownerDocument", e_this()),
                    s_return(e_var("pi")),
                ]),
        )
        .method(
            method("importNode")
                .param("node", t_class("DOMNode"))
                .param_default("deep", TypeExpr::Bool, e_bool(false))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(13))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Not Supported Error"), e_int(9)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_var("node"), "owningDocument", vec![]), BinOp::StrictEq, e_this()),
                        vec![
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("copy", e_method_call(e_var("node"), "cloneNode", vec![e_var("deep")])),
                    s_expr(e_method_call(e_var("copy"), "setOwner", vec![e_this()])),
                    s_return(e_var("copy")),
                ]),
        )
        .method(
            method("adoptNode")
                .param("node", t_class("DOMNode"))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(13))), BinOp::Or, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(10))),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Not Supported Error"), e_int(9)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_instance_of(e_var("node"), "DOMAttr"), BinOp::And, e_binop(e_prop(e_var("node"), "ownerElement"), BinOp::StrictNotEq, e_null())),
                        vec![
                            s_expr(e_method_call(e_prop(e_var("node"), "ownerElement"), "removeAttribute", vec![e_prop(e_var("node"), "nodeName")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_var("node"), "detach", vec![])),
                    s_expr(e_method_call(e_var("node"), "setOwner", vec![e_this()])),
                    s_return(e_var("node")),
                ]),
        )
        .method(
            method("getElementsByTagName")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_assign("found", e_array(vec![])),
                    s_expr(e_static_call("__ElephcDom", "collectByTag", vec![e_this(), e_var("qualifiedName"), e_var("found")])),
                    s_return(e_new("DOMNodeList", vec![e_null(), e_var("found")])),
                ]),
        )
        .method(
            method("getElementsByTagNameNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_assign("found", e_array(vec![])),
                    s_expr(e_static_call("__ElephcDom", "collectByTagNS", vec![e_this(), e_var("namespace"), e_var("localName"), e_var("found")])),
                    s_return(e_new("DOMNodeList", vec![e_null(), e_var("found")])),
                ]),
        )
        .method(
            method("getElementById")
                .param("elementId", TypeExpr::Str)
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDom", "findById", vec![e_this(), e_var("elementId"), e_this_prop("isHtml")])),
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
            method("saveXML")
                .param_default("node", t_nullable(t_class("DOMNode")), e_null())
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "syncText", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("node"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("node"), BinOp::StrictEq, e_this())),
                        vec![
                            s_return(e_static_call("__ElephcDomWriter", "document", vec![e_this(), e_this_prop("formatOutput"), e_var("options")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_var("node"), "owningDocument", vec![]), BinOp::StrictNotEq, e_this()),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("__ElephcDomWriter", "xml", vec![e_var("node"), e_this_prop("formatOutput"), e_binop(e_this_prop("encoding"), BinOp::StrictEq, e_null()), e_var("options")])),
                ]),
        )
        .method(
            method("saveHTML")
                .param_default("node", t_nullable(t_class("DOMNode")), e_null())
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_expr(e_method_call(e_this(), "syncText", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("node"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("node"), BinOp::StrictEq, e_this())),
                        vec![
                            s_return(e_static_call("__ElephcDomHtmlWriter", "document", vec![e_this(), e_this_prop("formatOutput")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_var("node"), "owningDocument", vec![]), BinOp::StrictNotEq, e_this()),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("__ElephcDomHtmlWriter", "dumpNode", vec![e_this(), e_var("node"), e_this_prop("formatOutput")])),
                ]),
        )
        .method(
            method("save")
                .param("filename", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
                .body(vec![
                    s_assign("xml", e_method_call(e_this(), "saveXML", vec![e_null(), e_var("options")])),
                    s_if(
                        e_binop(e_var("xml"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("file_put_contents", vec![e_var("filename"), e_var("xml")])),
                ]),
        )
        .method(
            method("saveHTMLFile")
                .param("filename", TypeExpr::Str)
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
                .body(vec![
                    s_assign("html", e_method_call(e_this(), "saveHTML", vec![])),
                    s_if(
                        e_binop(e_var("html"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("file_put_contents", vec![e_var("filename"), e_var("html")])),
                ]),
        )
        .method(
            method("load")
                .param("filename", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_call("is_readable", vec![e_var("filename")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("source", e_call("file_get_contents", vec![e_var("filename")])),
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
            method("loadHTMLFile")
                .param("filename", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_call("is_readable", vec![e_var("filename")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("source", e_call("file_get_contents", vec![e_var("filename")])),
                    s_if(
                        e_binop(e_var("source"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "loadHTML", vec![e_var("source"), e_var("options")])),
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
                            s_throw(e_new_fq("ValueError", vec![e_str("DOMDocument::loadXML(): Argument #1 ($source) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearForLoad", vec![])),
                    s_assign("parser", e_new("__ElephcXmlParser", vec![e_this(), e_var("source"), e_var("options")])),
                    s_prop_assign(e_this(), "building", e_bool(true)),
                    s_assign("parsed", e_method_call(e_var("parser"), "parse", vec![])),
                    s_prop_assign(e_this(), "building", e_bool(false)),
                    s_if(
                        e_not(e_var("parsed")),
                        vec![
                            s_expr(e_method_call(e_this(), "clearForLoad", vec![])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_static_call("__ElephcDom", "rebuildText", vec![e_this()])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("loadHTML")
                .param("source", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("source"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new_fq("ValueError", vec![e_str("DOMDocument::loadHTML(): Argument #1 ($source) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "clearForLoad", vec![])),
                    s_assign("parser", e_new("__ElephcHtmlParser", vec![e_this(), e_var("source"), e_var("options")])),
                    s_prop_assign(e_this(), "building", e_bool(true)),
                    s_expr(e_method_call(e_var("parser"), "parse", vec![])),
                    s_prop_assign(e_this(), "building", e_bool(false)),
                    s_expr(e_static_call("__ElephcDom", "rebuildText", vec![e_this()])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("clearForLoad")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "building", e_bool(true)),
                    s_while(e_binop(e_this_prop("firstChild"), BinOp::StrictNotEq, e_null()), vec![
                        s_expr(e_method_call(e_this_prop("firstChild"), "detach", vec![])),
                    ]),
                    s_prop_assign(e_this(), "textContent", e_str("")),
                    s_prop_assign(e_this(), "textCache", e_str("")),
                    s_prop_assign(e_this(), "building", e_bool(false)),
                    s_prop_assign(e_this(), "idIndex", e_array(vec![])),
                    s_prop_assign(e_this(), "isHtml", e_bool(false)),
                    s_prop_assign(e_this(), "nodeType", e_int(9)),
                    s_prop_assign(e_this(), "nodeName", e_str("#document")),
                ]),
        )
        .method(
            method("schemaValidate")
                .param("filename", TypeExpr::Str)
                .param_default("flags", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_unused", e_array(vec![e_var("filename"), e_var("flags")])),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("schemaValidateSource")
                .param("source", TypeExpr::Str)
                .param_default("flags", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_unused", e_array(vec![e_var("source"), e_var("flags")])),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("relaxNGValidate")
                .param("filename", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_unused", e_var("filename")),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("relaxNGValidateSource")
                .param("source", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_unused", e_var("source")),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("validate")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("xinclude")
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
                .body(vec![
                    s_assign("_unused", e_var("options")),
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("shallowCopy")
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("copy", e_new("DOMDocument", vec![e_ternary(e_binop(e_this_prop("version"), BinOp::StrictEq, e_null()), e_str("1.0"), e_this_prop("version")), e_ternary(e_binop(e_this_prop("encoding"), BinOp::StrictEq, e_null()), e_str(""), e_this_prop("encoding"))])),
                    s_prop_assign(e_var("copy"), "isHtml", e_this_prop("isHtml")),
                    s_prop_assign(e_var("copy"), "nodeType", e_this_prop("nodeType")),
                    s_prop_assign(e_var("copy"), "formatOutput", e_this_prop("formatOutput")),
                    s_return(e_var("copy")),
                ]),
        )
        .method(
            method("cloneNode")
                .param_default("deep", TypeExpr::Bool, e_bool(false))
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_assign("copy", e_method_call(e_this(), "shallowCopy", vec![])),
                    s_if(
                        e_var("deep"),
                        vec![
                            s_assign("child", e_this_prop("firstChild")),
                            s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                                s_assign("clone", e_method_call(e_var("child"), "cloneNode", vec![e_bool(true)])),
                                s_expr(e_method_call(e_var("clone"), "setOwner", vec![e_var("copy")])),
                                s_expr(e_method_call(e_var("copy"), "linkBefore", vec![e_var("clone"), e_null()])),
                                s_assign("child", e_prop(e_var("child"), "nextSibling")),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("copy")),
                ]),
        )
        .build()
}

/// `__ElephcDom` — transcribed from the PHP form.
fn decl_class_elephcdom() -> Stmt {
    class("__ElephcDom")
        .final_()
        .method(
            method("rebuildText")
                .static_()
                .param("node", t_class("DOMNode"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("child", e_prop(e_var("node"), "firstChild")),
                    s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                        s_if(
                            e_binop(e_prop(e_var("child"), "nodeType"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_expr(e_self_call("rebuildText", vec![e_var("child")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("child", e_prop(e_var("child"), "nextSibling")),
                    ]),
                    s_expr(e_method_call(e_var("node"), "computeText", vec![])),
                ]),
        )
        .method(
            method("validName")
                .static_()
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("name")])),
                    s_if(
                        e_binop(e_var("length"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("code", e_call("ord", vec![e_index(e_var("name"), e_var("i"))])),
                        s_assign("letter", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(122)))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(95))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(58))), BinOp::Or, e_binop(e_var("code"), BinOp::GtEq, e_int(128)))),
                        s_if(
                            e_binop(e_var("i"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_if(
                                    e_not(e_var("letter")),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![
                            (e_binop(e_binop(e_binop(e_not(e_var("letter")), BinOp::And, e_not(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(57))))), BinOp::And, e_binop(e_var("code"), BinOp::StrictNotEq, e_int(45))), BinOp::And, e_binop(e_var("code"), BinOp::StrictNotEq, e_int(46))), vec![
                                s_return(e_bool(false)),
                            ]),
                        ],
                            None,
                        ),
                    ]),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("charLength")
                .static_()
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_assign("count", e_int(0)),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::BitAnd, e_int(192)), BinOp::StrictNotEq, e_int(128)),
                            vec![
                                s_expr(e_post_inc("count")),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_var("count")),
                ]),
        )
        .method(
            method("charSlice")
                .static_()
                .param("text", TypeExpr::Str)
                .param("offset", TypeExpr::Int)
                .param("count", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_assign("chars", e_int(0)),
                    s_assign("start", e_var("length")),
                    s_assign("end", e_var("length")),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::BitAnd, e_int(192)), BinOp::StrictNotEq, e_int(128)),
                            vec![
                                s_if(
                                    e_binop(e_var("chars"), BinOp::StrictEq, e_var("offset")),
                                    vec![
                                        s_assign("start", e_var("i")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_var("chars"), BinOp::StrictEq, e_binop(e_var("offset"), BinOp::Add, e_var("count"))),
                                    vec![
                                        s_assign("end", e_var("i")),
                                        s_break(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_post_inc("chars")),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_var("start"), BinOp::Gt, e_var("end")),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("substr", vec![e_var("text"), e_var("start"), e_binop(e_var("end"), BinOp::Sub, e_var("start"))])),
                ]),
        )
        .method(
            method("decodeValue")
                .static_()
                .param("value", TypeExpr::Str)
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_if(
                        e_binop(e_call("strpos", vec![e_var("value"), e_str("&")]), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_var("value")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("out", e_str("")),
                    s_assign("length", e_call("strlen", vec![e_var("value")])),
                    s_assign("i", e_int(0)),
                    s_while(e_binop(e_var("i"), BinOp::Lt, e_var("length")), vec![
                        s_assign("char", e_index(e_var("value"), e_var("i"))),
                        s_if(
                            e_binop(e_var("char"), BinOp::StrictNotEq, e_str("&")),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("char"))),
                                s_expr(e_post_inc("i")),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("semi", e_call("strpos", vec![e_var("value"), e_str(";"), e_var("i")])),
                        s_if(
                            e_binop(e_var("semi"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("decoded", e_self_call("decodeReference", vec![e_call("substr", vec![e_var("value"), e_binop(e_var("i"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("semi"), BinOp::Sub, e_var("i")), BinOp::Sub, e_int(1))])])),
                        s_if(
                            e_binop(e_var("decoded"), BinOp::StrictEq, e_null()),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("decoded"))),
                        s_assign("i", e_binop(e_var("semi"), BinOp::Add, e_int(1))),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .method(
            method("decodeReference")
                .static_()
                .param("name", TypeExpr::Str)
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("amp")),
                        vec![
                            s_return(e_str("&")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("lt")),
                        vec![
                            s_return(e_str("<")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("gt")),
                        vec![
                            s_return(e_str(">")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("quot")),
                        vec![
                            s_return(e_str("\"")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("apos")),
                        vec![
                            s_return(e_str("'")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_call("strlen", vec![e_var("name")]), BinOp::Gt, e_int(1)), BinOp::And, e_binop(e_index(e_var("name"), e_int(0)), BinOp::StrictEq, e_str("#"))),
                        vec![
                            s_assign("number", e_ternary(e_binop(e_index(e_var("name"), e_int(1)), BinOp::StrictEq, e_str("x")), e_call("hexdec", vec![e_call("substr", vec![e_var("name"), e_int(2)])]), e_cast(CastType::Int, e_call("substr", vec![e_var("name"), e_int(1)])))),
                            s_if(
                                e_binop(e_binop(e_var("number"), BinOp::LtEq, e_int(0)), BinOp::Or, e_binop(e_var("number"), BinOp::Gt, e_int(1114111))),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_self_call("utf8", vec![e_var("number")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("utf8")
                .static_()
                .param("code", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_var("code"), BinOp::Lt, e_int(128)),
                        vec![
                            s_return(e_call("chr", vec![e_var("code")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("code"), BinOp::Lt, e_int(2048)),
                        vec![
                            s_return(e_binop(e_call("chr", vec![e_binop(e_int(192), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("code"), BinOp::Lt, e_int(65536)),
                        vec![
                            s_return(e_binop(e_binop(e_call("chr", vec![e_binop(e_int(224), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(12)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_binop(e_binop(e_call("chr", vec![e_binop(e_int(240), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(18)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(12)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
                ]),
        )
        .method(
            method("collectByTag")
                .static_()
                .param("root", t_class("DOMNode"))
                .param("name", TypeExpr::Str)
                .param_by_ref("found", Some(t_array()))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("wanted", e_var("name")),
                    s_assign("node", e_prop(e_var("root"), "firstChild")),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_if(
                            e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_if(
                                    e_binop(e_binop(e_var("wanted"), BinOp::StrictEq, e_str("*")), BinOp::Or, e_binop(e_prop(e_var("node"), "nodeName"), BinOp::StrictEq, e_var("wanted"))),
                                    vec![
                                        s_array_push("found", e_var("node")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_self_call("collectByTag", vec![e_var("node"), e_var("name"), e_var("found")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("node", e_prop(e_var("node"), "nextSibling")),
                    ]),
                ]),
        )
        .method(
            method("collectByTagNS")
                .static_()
                .param("root", t_class("DOMNode"))
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .param_by_ref("found", Some(t_array()))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("node", e_prop(e_var("root"), "firstChild")),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_if(
                            e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_assign("nsMatch", e_binop(e_binop(e_var("namespace"), BinOp::StrictEq, e_str("*")), BinOp::Or, e_binop(e_prop(e_var("node"), "namespaceURI"), BinOp::StrictEq, e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_str("")), e_null(), e_var("namespace"))))),
                                s_if(
                                    e_binop(e_var("nsMatch"), BinOp::And, e_binop(e_binop(e_var("localName"), BinOp::StrictEq, e_str("*")), BinOp::Or, e_binop(e_prop(e_var("node"), "localName"), BinOp::StrictEq, e_var("localName")))),
                                    vec![
                                        s_array_push("found", e_var("node")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_self_call("collectByTagNS", vec![e_var("node"), e_var("namespace"), e_var("localName"), e_var("found")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("node", e_prop(e_var("node"), "nextSibling")),
                    ]),
                ]),
        )
        .method(
            method("findById")
                .static_()
                .param("root", t_class("DOMNode"))
                .param("id", TypeExpr::Str)
                .param("html", TypeExpr::Bool)
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_assign("node", e_prop(e_var("root"), "firstChild")),
                    s_while(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), vec![
                        s_if(
                            e_instance_of(e_var("node"), "DOMElement"),
                            vec![
                                s_if(
                                    e_binop(e_binop(e_var("html"), BinOp::And, e_call("isset", vec![e_index(e_prop(e_var("node"), "attrNodes"), e_str("id"))])), BinOp::And, e_binop(e_prop(e_index(e_prop(e_var("node"), "attrNodes"), e_str("id")), "value"), BinOp::StrictEq, e_var("id"))),
                                    vec![
                                        s_return(e_var("node")),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("inner", e_self_call("findById", vec![e_var("node"), e_var("id"), e_var("html")])),
                                s_if(
                                    e_binop(e_var("inner"), BinOp::StrictNotEq, e_null()),
                                    vec![
                                        s_return(e_var("inner")),
                                    ],
                                    vec![],
                                    None,
                                ),
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
            method("escapeText")
                .static_()
                .param("text", TypeExpr::Str)
                .param("ascii", TypeExpr::Bool)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("text", e_call("str_replace", vec![e_array(vec![e_str("&"), e_str("<"), e_str(">"), e_str("\r")]), e_array(vec![e_str("&amp;"), e_str("&lt;"), e_str("&gt;"), e_ternary(e_var("ascii"), e_str("&#xD;"), e_str("&#13;"))]), e_var("text")])),
                    s_return(e_ternary(e_var("ascii"), e_self_call("asciiRefs", vec![e_var("text")]), e_var("text"))),
                ]),
        )
        .method(
            method("escapeAttr")
                .static_()
                .param("text", TypeExpr::Str)
                .param("ascii", TypeExpr::Bool)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("text", e_call("str_replace", vec![e_array(vec![e_str("&"), e_str("<"), e_str(">"), e_str("\""), e_str("\n"), e_str("\r"), e_str("\t")]), e_array(vec![e_str("&amp;"), e_str("&lt;"), e_str("&gt;"), e_str("&quot;"), e_str("&#10;"), e_str("&#13;"), e_str("&#9;")]), e_var("text")])),
                    s_return(e_ternary(e_var("ascii"), e_self_call("asciiRefs", vec![e_var("text")]), e_var("text"))),
                ]),
        )
        .method(
            method("asciiRefs")
                .static_()
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_assign("clean", e_bool(true)),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::GtEq, e_int(128)),
                            vec![
                                s_assign("clean", e_bool(false)),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_var("clean"),
                        vec![
                            s_return(e_var("text")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("out", e_str("")),
                    s_assign("i", e_int(0)),
                    s_while(e_binop(e_var("i"), BinOp::Lt, e_var("length")), vec![
                        s_assign("code", e_call("ord", vec![e_index(e_var("text"), e_var("i"))])),
                        s_if(
                            e_binop(e_var("code"), BinOp::Lt, e_int(128)),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_index(e_var("text"), e_var("i")))),
                                s_expr(e_post_inc("i")),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(240)), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(3)), BinOp::Lt, e_var("length"))), vec![
                                s_assign("point", e_binop(e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::BitAnd, e_int(7)), BinOp::ShiftLeft, e_int(18)), BinOp::BitOr, e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(63)), BinOp::ShiftLeft, e_int(12))), BinOp::BitOr, e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(2)))]), BinOp::BitAnd, e_int(63)), BinOp::ShiftLeft, e_int(6))), BinOp::BitOr, e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(3)))]), BinOp::BitAnd, e_int(63)))),
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_str("&#x"), BinOp::Concat, e_call("strtoupper", vec![e_call("dechex", vec![e_var("point")])])), BinOp::Concat, e_str(";")))),
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(4))),
                            ]),
                            (e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(224)), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(2)), BinOp::Lt, e_var("length"))), vec![
                                s_assign("point", e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::BitAnd, e_int(15)), BinOp::ShiftLeft, e_int(12)), BinOp::BitOr, e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(63)), BinOp::ShiftLeft, e_int(6))), BinOp::BitOr, e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(2)))]), BinOp::BitAnd, e_int(63)))),
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_str("&#x"), BinOp::Concat, e_call("strtoupper", vec![e_call("dechex", vec![e_var("point")])])), BinOp::Concat, e_str(";")))),
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(3))),
                            ]),
                            (e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(192)), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("length"))), vec![
                                s_assign("point", e_binop(e_binop(e_binop(e_var("code"), BinOp::BitAnd, e_int(31)), BinOp::ShiftLeft, e_int(6)), BinOp::BitOr, e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(63)))),
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_str("&#x"), BinOp::Concat, e_call("strtoupper", vec![e_call("dechex", vec![e_var("point")])])), BinOp::Concat, e_str(";")))),
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(2))),
                            ]),
                        ],
                            Some(vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_str("&#x"), BinOp::Concat, e_call("strtoupper", vec![e_call("dechex", vec![e_var("code")])])), BinOp::Concat, e_str(";")))),
                            s_expr(e_post_inc("i")),
                        ]),
                        ),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .build()
}

/// `__ElephcDomWriter` — transcribed from the PHP form.
fn decl_class_elephcdomwriter() -> Stmt {
    class("__ElephcDomWriter")
        .final_()
        .prop("out", TypeExpr::Str, Some(e_str("")))
        .prop("level", TypeExpr::Int, Some(e_int(0)))
        .prop("fmt", TypeExpr::Bool, Some(e_bool(false)))
        .prop("ascii", TypeExpr::Bool, Some(e_bool(false)))
        .prop("options", TypeExpr::Int, Some(e_int(0)))
        .prop("xhtml", TypeExpr::Bool, Some(e_bool(false)))
        .prop("rawText", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("isXhtml")
                .static_()
                .param("dtd", t_nullable(t_class("DOMDocumentType")))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("dtd"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("ids", e_array(vec![e_str("-//W3C//DTD XHTML 1.0 Strict//EN"), e_str("-//W3C//DTD XHTML 1.0 Frameset//EN"), e_str("-//W3C//DTD XHTML 1.0 Transitional//EN"), e_str("http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd"), e_str("http://www.w3.org/TR/xhtml1/DTD/xhtml1-frameset.dtd"), e_str("http://www.w3.org/TR/xhtml1/DTD/xhtml1-transitional.dtd")])),
                    s_return(e_binop(e_call("in_array", vec![e_prop(e_var("dtd"), "publicId"), e_var("ids"), e_bool(true)]), BinOp::Or, e_call("in_array", vec![e_prop(e_var("dtd"), "systemId"), e_var("ids"), e_bool(true)]))),
                ]),
        )
        .method(
            method("header")
                .static_()
                .param("doc", t_class("DOMDocument"))
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("head", e_binop(e_binop(e_str("<?xml version=\""), BinOp::Concat, e_ternary(e_binop(e_prop(e_var("doc"), "version"), BinOp::StrictEq, e_null()), e_str("1.0"), e_prop(e_var("doc"), "version"))), BinOp::Concat, e_str("\""))),
                    s_if(
                        e_binop(e_prop(e_var("doc"), "encoding"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("head", e_binop(e_var("head"), BinOp::Concat, e_binop(e_binop(e_str(" encoding=\""), BinOp::Concat, e_prop(e_var("doc"), "encoding")), BinOp::Concat, e_str("\"")))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_prop(e_var("doc"), "standaloneSet"),
                        vec![
                            s_assign("head", e_binop(e_var("head"), BinOp::Concat, e_ternary(e_prop(e_var("doc"), "xmlStandalone"), e_str(" standalone=\"yes\""), e_str(" standalone=\"no\"")))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_var("head"), BinOp::Concat, e_str("?>\n"))),
                ]),
        )
        .method(
            method("document")
                .static_()
                .param("doc", t_class("DOMDocument"))
                .param("format", TypeExpr::Bool)
                .param("options", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("writer", e_new("__ElephcDomWriter", vec![])),
                    s_prop_assign(e_var("writer"), "fmt", e_var("format")),
                    s_prop_assign(e_var("writer"), "ascii", e_binop(e_prop(e_var("doc"), "encoding"), BinOp::StrictEq, e_null())),
                    s_prop_assign(e_var("writer"), "options", e_var("options")),
                    s_assign("dtd", e_prop(e_var("doc"), "doctype")),
                    s_prop_assign(e_var("writer"), "xhtml", e_self_call("isXhtml", vec![e_var("dtd")])),
                    s_assign("out", e_ternary(e_binop(e_binop(e_var("options"), BinOp::BitAnd, e_int(2)), BinOp::StrictNotEq, e_int(0)), e_str(""), e_self_call("header", vec![e_var("doc")]))),
                    s_assign("child", e_prop(e_var("doc"), "firstChild")),
                    s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                        s_prop_assign(e_var("writer"), "out", e_str("")),
                        s_prop_assign(e_var("writer"), "level", e_int(0)),
                        s_prop_assign(e_var("writer"), "fmt", e_var("format")),
                        s_expr(e_method_call(e_var("writer"), "node", vec![e_var("child"), e_bool(true), e_var("format"), e_array(vec![])])),
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_prop(e_var("writer"), "out"), BinOp::Concat, e_str("\n")))),
                        s_assign("child", e_prop(e_var("child"), "nextSibling")),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .method(
            method("xml")
                .static_()
                .param("node", t_class("DOMNode"))
                .param("format", TypeExpr::Bool)
                .param("ascii", TypeExpr::Bool)
                .param("options", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("writer", e_new("__ElephcDomWriter", vec![])),
                    s_prop_assign(e_var("writer"), "fmt", e_var("format")),
                    s_prop_assign(e_var("writer"), "ascii", e_var("ascii")),
                    s_prop_assign(e_var("writer"), "options", e_var("options")),
                    s_assign("owner", e_method_call(e_var("node"), "owningDocument", vec![])),
                    s_assign("ownerDtd", e_ternary(e_binop(e_var("owner"), BinOp::StrictEq, e_null()), e_null(), e_prop(e_var("owner"), "doctype"))),
                    s_prop_assign(e_var("writer"), "xhtml", e_binop(e_binop(e_var("owner"), BinOp::StrictNotEq, e_null()), BinOp::And, e_self_call("isXhtml", vec![e_var("ownerDtd")]))),
                    s_prop_assign(e_var("writer"), "rawText", e_bool(true)),
                    s_if(
                        e_instance_of(e_var("node"), "DOMDocument"),
                        vec![
                            s_return(e_self_call("document", vec![e_var("node"), e_var("format"), e_var("options")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_var("writer"), "node", vec![e_var("node"), e_bool(true), e_var("format"), e_array(vec![])])),
                    s_return(e_prop(e_var("writer"), "out")),
                ]),
        )
        .method(
            method("indent")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_this_prop("fmt"),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_call("str_repeat", vec![e_str("  "), e_this_prop("level")]))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("node")
                .param("cur", t_class("DOMNode"))
                .param("isRoot", TypeExpr::Bool)
                .param("origFormat", TypeExpr::Bool)
                .param("scope", t_array())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("type", e_prop(e_var("cur"), "nodeType")),
                    s_if(
                        e_instance_of(e_var("cur"), "DOMElement"),
                        vec![
                            s_expr(e_method_call(e_this(), "element", vec![e_var("cur"), e_var("isRoot"), e_var("origFormat"), e_var("scope")])),
                        ],
                        vec![
                        (e_instance_of(e_var("cur"), "DOMCdataSection"), vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("<![CDATA["), BinOp::Concat, e_call("str_replace", vec![e_str("]]>"), e_str("]]]]><![CDATA[>"), e_prop(e_var("cur"), "data")])), BinOp::Concat, e_str("]]>")))),
                        ]),
                        (e_instance_of(e_var("cur"), "DOMText"), vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_static_call("__ElephcDom", "escapeText", vec![e_prop(e_var("cur"), "data"), e_binop(e_this_prop("ascii"), BinOp::And, e_not(e_this_prop("rawText")))]))),
                        ]),
                        (e_instance_of(e_var("cur"), "DOMComment"), vec![
                            s_if(
                                e_not(e_var("isRoot")),
                                vec![
                                    s_expr(e_method_call(e_this(), "indent", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("<!--"), BinOp::Concat, e_prop(e_var("cur"), "data")), BinOp::Concat, e_str("-->")))),
                        ]),
                        (e_instance_of(e_var("cur"), "DOMProcessingInstruction"), vec![
                            s_if(
                                e_not(e_var("isRoot")),
                                vec![
                                    s_expr(e_method_call(e_this(), "indent", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_ternary(e_binop(e_prop(e_var("cur"), "data"), BinOp::StrictEq, e_str("")), e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("cur"), "target")), BinOp::Concat, e_str("?>")), e_binop(e_binop(e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("cur"), "target")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_prop(e_var("cur"), "data")), BinOp::Concat, e_str("?>"))))),
                        ]),
                        (e_instance_of(e_var("cur"), "DOMDocumentType"), vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_self_call("doctype", vec![e_var("cur")]))),
                        ]),
                        (e_binop(e_var("type"), BinOp::StrictEq, e_int(11)), vec![
                            s_assign("child", e_prop(e_var("cur"), "firstChild")),
                            s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                                s_expr(e_method_call(e_this(), "node", vec![e_var("child"), e_bool(false), e_var("origFormat"), e_var("scope")])),
                                s_assign("child", e_prop(e_var("child"), "nextSibling")),
                            ]),
                        ]),
                        (e_instance_of(e_var("cur"), "DOMAttr"), vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_static_call("__ElephcDom", "escapeText", vec![e_prop(e_var("cur"), "value"), e_this_prop("ascii")]))),
                        ]),
                    ],
                        None,
                    ),
                ]),
        )
        .method(
            method("doctype")
                .static_()
                .param("dtd", t_class("DOMDocumentType"))
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("out", e_binop(e_str("<!DOCTYPE "), BinOp::Concat, e_prop(e_var("dtd"), "name"))),
                    s_if(
                        e_binop(e_prop(e_var("dtd"), "publicId"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_binop(e_binop(e_str(" PUBLIC \""), BinOp::Concat, e_prop(e_var("dtd"), "publicId")), BinOp::Concat, e_str("\" \"")), BinOp::Concat, e_prop(e_var("dtd"), "systemId")), BinOp::Concat, e_str("\"")))),
                        ],
                        vec![
                        (e_binop(e_prop(e_var("dtd"), "systemId"), BinOp::StrictNotEq, e_str("")), vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_str(" SYSTEM \""), BinOp::Concat, e_prop(e_var("dtd"), "systemId")), BinOp::Concat, e_str("\"")))),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_prop(e_var("dtd"), "internalSubset"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("dtd"), "internalSubset"), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_str(" ["), BinOp::Concat, e_prop(e_var("dtd"), "internalSubset")), BinOp::Concat, e_str("]")))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_var("out"), BinOp::Concat, e_str(">"))),
                ]),
        )
        .method(
            method("hasTextChild")
                .static_()
                .param("cur", t_class("DOMNode"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("child", e_prop(e_var("cur"), "firstChild")),
                    s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                        s_assign("kind", e_prop(e_var("child"), "nodeType")),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4))), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(5))),
                            vec![
                                s_return(e_bool(true)),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("child", e_prop(e_var("child"), "nextSibling")),
                    ]),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("element")
                .param("cur", t_class("DOMElement"))
                .param("isRoot", TypeExpr::Bool)
                .param("origFormat", TypeExpr::Bool)
                .param("scope", t_array())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_not(e_var("isRoot")),
                        vec![
                            s_expr(e_method_call(e_this(), "indent", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_str("<"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")))),
                    s_if(
                        e_binop(e_binop(e_binop(e_this_prop("xhtml"), BinOp::And, e_binop(e_prop(e_var("cur"), "nodeName"), BinOp::StrictEq, e_str("html"))), BinOp::And, e_binop(e_prop(e_var("cur"), "namespaceURI"), BinOp::StrictEq, e_null())), BinOp::And, e_not(e_call("isset", vec![e_index(e_prop(e_var("cur"), "nsDecl"), e_str(""))]))),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str(" xmlns=\"http://www.w3.org/1999/xhtml\""))),
                        ],
                        vec![],
                        None,
                    ),
                    s_foreach(e_prop(e_var("cur"), "nsDecl"), Some("declared"), "uri", vec![
                        s_if(
                            e_binop(e_call("isset", vec![e_index(e_prop(e_var("cur"), "nsAuto"), e_var("declared"))]), BinOp::And, e_binop(e_null_coalesce(e_index(e_var("scope"), e_cast(CastType::String, e_var("declared"))), e_null()), BinOp::StrictEq, e_var("uri"))),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_ternary(e_binop(e_var("declared"), BinOp::StrictEq, e_str("")), e_binop(e_binop(e_str(" xmlns=\""), BinOp::Concat, e_static_call("__ElephcDom", "escapeAttr", vec![e_var("uri"), e_this_prop("ascii")])), BinOp::Concat, e_str("\"")), e_binop(e_binop(e_binop(e_binop(e_str(" xmlns:"), BinOp::Concat, e_var("declared")), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_static_call("__ElephcDom", "escapeAttr", vec![e_var("uri"), e_this_prop("ascii")])), BinOp::Concat, e_str("\""))))),
                        s_array_assign("scope", e_cast(CastType::String, e_var("declared")), e_var("uri")),
                    ]),
                    s_if(
                        e_binop(e_binop(e_prop(e_var("cur"), "namespaceURI"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_null_coalesce(e_index(e_var("scope"), e_prop(e_var("cur"), "prefix")), e_null()), BinOp::StrictNotEq, e_prop(e_var("cur"), "namespaceURI"))),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_ternary(e_binop(e_prop(e_var("cur"), "prefix"), BinOp::StrictEq, e_str("")), e_binop(e_binop(e_str(" xmlns=\""), BinOp::Concat, e_static_call("__ElephcDom", "escapeAttr", vec![e_prop(e_var("cur"), "namespaceURI"), e_this_prop("ascii")])), BinOp::Concat, e_str("\"")), e_binop(e_binop(e_binop(e_binop(e_str(" xmlns:"), BinOp::Concat, e_prop(e_var("cur"), "prefix")), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_static_call("__ElephcDom", "escapeAttr", vec![e_prop(e_var("cur"), "namespaceURI"), e_this_prop("ascii")])), BinOp::Concat, e_str("\""))))),
                            s_array_assign("scope", e_prop(e_var("cur"), "prefix"), e_prop(e_var("cur"), "namespaceURI")),
                        ],
                        vec![],
                        None,
                    ),
                    s_foreach(e_prop(e_var("cur"), "attrNodes"), None, "attr", vec![
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_prop(e_var("attr"), "namespaceURI"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("attr"), "prefix"), BinOp::StrictNotEq, e_str(""))), BinOp::And, e_binop(e_prop(e_var("attr"), "prefix"), BinOp::StrictNotEq, e_str("xml"))), BinOp::And, e_binop(e_null_coalesce(e_index(e_var("scope"), e_prop(e_var("attr"), "prefix")), e_null()), BinOp::StrictNotEq, e_prop(e_var("attr"), "namespaceURI"))),
                            vec![
                                s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_binop(e_binop(e_str(" xmlns:"), BinOp::Concat, e_prop(e_var("attr"), "prefix")), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_static_call("__ElephcDom", "escapeAttr", vec![e_prop(e_var("attr"), "namespaceURI"), e_this_prop("ascii")])), BinOp::Concat, e_str("\"")))),
                                s_array_assign("scope", e_prop(e_var("attr"), "prefix"), e_prop(e_var("attr"), "namespaceURI")),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_binop(e_binop(e_str(" "), BinOp::Concat, e_prop(e_var("attr"), "nodeName")), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_static_call("__ElephcDom", "escapeAttr", vec![e_prop(e_var("attr"), "value"), e_this_prop("ascii")])), BinOp::Concat, e_str("\"")))),
                    ]),
                    s_if(
                        e_binop(e_prop(e_var("cur"), "firstChild"), BinOp::StrictEq, e_null()),
                        vec![
                            s_if(
                                e_this_prop("xhtml"),
                                vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_ternary(e_binop(e_binop(e_prop(e_var("cur"), "namespaceURI"), BinOp::StrictEq, e_null()), BinOp::And, e_call("in_array", vec![e_prop(e_var("cur"), "nodeName"), e_array(vec![e_str("area"), e_str("base"), e_str("basefont"), e_str("br"), e_str("col"), e_str("frame"), e_str("hr"), e_str("img"), e_str("input"), e_str("isindex"), e_str("link"), e_str("meta"), e_str("param")]), e_bool(true)])), e_str(" />"), e_binop(e_binop(e_str("></"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str(">"))))),
                                ],
                                vec![],
                                Some(vec![
                                s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_ternary(e_binop(e_binop(e_this_prop("options"), BinOp::BitAnd, e_int(4)), BinOp::StrictNotEq, e_int(0)), e_binop(e_binop(e_str("></"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str(">")), e_str("/>")))),
                            ]),
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("unformatted", e_bool(false)),
                    s_if(
                        e_binop(e_this_prop("fmt"), BinOp::And, e_self_call("hasTextChild", vec![e_var("cur")])),
                        vec![
                            s_prop_assign(e_this(), "fmt", e_bool(false)),
                            s_assign("unformatted", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str(">"))),
                    s_if(
                        e_this_prop("fmt"),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "level", e_binop(e_this_prop("level"), BinOp::Add, e_int(1))),
                    s_assign("child", e_prop(e_var("cur"), "firstChild")),
                    s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                        s_expr(e_method_call(e_this(), "node", vec![e_var("child"), e_bool(false), e_var("origFormat"), e_var("scope")])),
                        s_if(
                            e_this_prop("fmt"),
                            vec![
                                s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("child", e_prop(e_var("child"), "nextSibling")),
                    ]),
                    s_if(
                        e_binop(e_this_prop("level"), BinOp::Gt, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "level", e_binop(e_this_prop("level"), BinOp::Sub, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_this_prop("fmt"),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_call("str_repeat", vec![e_str("  "), e_this_prop("level")]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("</"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str(">")))),
                    s_if(
                        e_var("unformatted"),
                        vec![
                            s_prop_assign(e_this(), "fmt", e_var("origFormat")),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .build()
}

/// `__ElephcXmlParser` — transcribed from the PHP form.
fn decl_class_elephcxmlparser() -> Stmt {
    class("__ElephcXmlParser")
        .final_()
        .prop("doc", t_class("DOMDocument"), None)
        .prop("src", TypeExpr::Str, Some(e_str("")))
        .prop("pos", TypeExpr::Int, Some(e_int(0)))
        .prop("len", TypeExpr::Int, Some(e_int(0)))
        .prop("options", TypeExpr::Int, Some(e_int(0)))
        .prop("scopes", t_array(), Some(e_array(vec![])))
        .method(
            method("__construct")
                .param("doc", t_class("DOMDocument"))
                .param("src", TypeExpr::Str)
                .param("options", TypeExpr::Int)
                .body(vec![
                    s_prop_assign(e_this(), "doc", e_var("doc")),
                    s_prop_assign(e_this(), "src", e_var("src")),
                    s_prop_assign(e_this(), "len", e_call("strlen", vec![e_var("src")])),
                    s_prop_assign(e_this(), "options", e_var("options")),
                ]),
        )
        .method(
            method("startsWith")
                .param("token", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("substr", vec![e_this_prop("src"), e_this_prop("pos"), e_call("strlen", vec![e_var("token")])]), BinOp::StrictEq, e_var("token"))),
                ]),
        )
        .method(
            method("skipSpace")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("src"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\n"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\r"))),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            Some(vec![
                            s_break(1),
                        ]),
                        ),
                    ]),
                ]),
        )
        .method(
            method("parse")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("substr", vec![e_this_prop("src"), e_int(0), e_int(3)]), BinOp::StrictEq, e_str("\u{e0ef}\u{e0bb}\u{e0bf}")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_int(3)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_method_call(e_this(), "startsWith", vec![e_str("<?xml")]), BinOp::And, e_binop(e_binop(e_this_prop("pos"), BinOp::Add, e_int(5)), BinOp::Lt, e_this_prop("len"))), BinOp::And, e_binop(e_call("strpos", vec![e_str(" \t\r\n"), e_index(e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(5)))]), BinOp::StrictNotEq, e_bool(false))),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "parseDeclaration", vec![])),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("sawRoot", e_bool(false)),
                    s_while(e_bool(true), vec![
                        s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                        s_if(
                            e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_method_call(e_this(), "startsWith", vec![e_str("<!--")]),
                            vec![
                                s_assign("comment", e_method_call(e_this(), "parseComment", vec![])),
                                s_if(
                                    e_binop(e_var("comment"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_this_prop("doc"), "linkBefore", vec![e_var("comment"), e_null()])),
                            ],
                            vec![
                            (e_method_call(e_this(), "startsWith", vec![e_str("<?")]), vec![
                                s_assign("pi", e_method_call(e_this(), "parseProcessingInstruction", vec![])),
                                s_if(
                                    e_binop(e_var("pi"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_this_prop("doc"), "linkBefore", vec![e_var("pi"), e_null()])),
                            ]),
                            (e_method_call(e_this(), "startsWith", vec![e_str("<!DOCTYPE")]), vec![
                                s_if(
                                    e_binop(e_var("sawRoot"), BinOp::Or, e_binop(e_prop(e_this_prop("doc"), "doctype"), BinOp::StrictNotEq, e_null())),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("dtd", e_method_call(e_this(), "parseDoctype", vec![])),
                                s_if(
                                    e_binop(e_var("dtd"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_this_prop("doc"), "linkBefore", vec![e_var("dtd"), e_null()])),
                            ]),
                            (e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictEq, e_str("<")), vec![
                                s_if(
                                    e_var("sawRoot"),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("root", e_method_call(e_this(), "parseElement", vec![])),
                                s_if(
                                    e_binop(e_var("root"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_this_prop("doc"), "linkBefore", vec![e_var("root"), e_null()])),
                                s_assign("sawRoot", e_bool(true)),
                            ]),
                        ],
                            Some(vec![
                            s_return(e_bool(false)),
                        ]),
                        ),
                    ]),
                    s_return(e_var("sawRoot")),
                ]),
        )
        .method(
            method("parseDeclaration")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("end", e_call("strpos", vec![e_this_prop("src"), e_str("?>"), e_this_prop("pos")])),
                    s_if(
                        e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("body", e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(5)), e_binop(e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(5))])),
                    s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(2))),
                    s_assign("version", e_method_call(e_this(), "pseudoAttribute", vec![e_var("body"), e_str("version")])),
                    s_if(
                        e_binop(e_var("version"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("doc", e_this_prop("doc")),
                    s_prop_assign(e_var("doc"), "version", e_var("version")),
                    s_prop_assign(e_var("doc"), "xmlVersion", e_var("version")),
                    s_assign("encoding", e_method_call(e_this(), "pseudoAttribute", vec![e_var("body"), e_str("encoding")])),
                    s_if(
                        e_binop(e_var("encoding"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("doc"), "encoding", e_var("encoding")),
                            s_prop_assign(e_var("doc"), "xmlEncoding", e_var("encoding")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("standalone", e_method_call(e_this(), "pseudoAttribute", vec![e_var("body"), e_str("standalone")])),
                    s_if(
                        e_binop(e_var("standalone"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_prop_assign(e_var("doc"), "standaloneSet", e_bool(true)),
                            s_prop_assign(e_var("doc"), "xmlStandalone", e_binop(e_var("standalone"), BinOp::StrictEq, e_str("yes"))),
                            s_prop_assign(e_var("doc"), "standalone", e_binop(e_var("standalone"), BinOp::StrictEq, e_str("yes"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("pseudoAttribute")
                .param("body", TypeExpr::Str)
                .param("name", TypeExpr::Str)
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("at", e_call("strpos", vec![e_var("body"), e_var("name")])),
                    s_if(
                        e_binop(e_var("at"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("rest", e_call("ltrim", vec![e_call("substr", vec![e_var("body"), e_binop(e_var("at"), BinOp::Add, e_call("strlen", vec![e_var("name")]))]), e_str(" \t\r\n")])),
                    s_if(
                        e_binop(e_binop(e_var("rest"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_index(e_var("rest"), e_int(0)), BinOp::StrictNotEq, e_str("="))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("rest", e_call("ltrim", vec![e_call("substr", vec![e_var("rest"), e_int(1)]), e_str(" \t\r\n")])),
                    s_if(
                        e_binop(e_binop(e_var("rest"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_binop(e_index(e_var("rest"), e_int(0)), BinOp::StrictNotEq, e_str("\"")), BinOp::And, e_binop(e_index(e_var("rest"), e_int(0)), BinOp::StrictNotEq, e_str("'")))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("close", e_call("strpos", vec![e_var("rest"), e_index(e_var("rest"), e_int(0)), e_int(1)])),
                    s_if(
                        e_binop(e_var("close"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("substr", vec![e_var("rest"), e_int(1), e_binop(e_var("close"), BinOp::Sub, e_int(1))])),
                ]),
        )
        .method(
            method("parseComment")
                .returns(t_nullable(t_class("DOMComment")))
                .body(vec![
                    s_assign("end", e_call("strpos", vec![e_this_prop("src"), e_str("-->"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(4))])),
                    s_if(
                        e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("comment", e_new("DOMComment", vec![e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(4)), e_binop(e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(4))])])),
                    s_prop_assign(e_var("comment"), "ownerDocument", e_this_prop("doc")),
                    s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(3))),
                    s_return(e_var("comment")),
                ]),
        )
        .method(
            method("parseProcessingInstruction")
                .returns(t_nullable(t_class("DOMProcessingInstruction")))
                .body(vec![
                    s_assign("end", e_call("strpos", vec![e_this_prop("src"), e_str("?>"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))])),
                    s_if(
                        e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("body", e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(2)), e_binop(e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(2))])),
                    s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(2))),
                    s_assign("space", e_call("strcspn", vec![e_var("body"), e_str(" \t\r\n")])),
                    s_assign("target", e_call("substr", vec![e_var("body"), e_int(0), e_var("space")])),
                    s_if(
                        e_binop(e_not(e_static_call("__ElephcDom", "validName", vec![e_var("target")])), BinOp::Or, e_binop(e_call("strtolower", vec![e_var("target")]), BinOp::StrictEq, e_str("xml"))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("pi", e_new("DOMProcessingInstruction", vec![e_var("target"), e_call("ltrim", vec![e_call("substr", vec![e_var("body"), e_var("space")]), e_str(" \t\r\n")])])),
                    s_prop_assign(e_var("pi"), "ownerDocument", e_this_prop("doc")),
                    s_return(e_var("pi")),
                ]),
        )
        .method(
            method("parseDoctype")
                .returns(t_nullable(t_class("DOMDocumentType")))
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(9))),
                    s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                    s_assign("start", e_this_prop("pos")),
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_call("strcspn", vec![e_this_prop("src"), e_str(" \t\r\n>["), e_this_prop("pos")]))),
                    s_assign("name", e_call("substr", vec![e_this_prop("src"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])),
                    s_assign("public", e_str("")),
                    s_assign("system", e_str("")),
                    s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                    s_if(
                        e_method_call(e_this(), "startsWith", vec![e_str("PUBLIC")]),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(6))),
                            s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                            s_assign("public", e_method_call(e_this(), "quoted", vec![])),
                            s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                            s_if(
                                e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictEq, e_str("\"")), BinOp::Or, e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictEq, e_str("'")))),
                                vec![
                                    s_assign("system", e_method_call(e_this(), "quoted", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![
                        (e_method_call(e_this(), "startsWith", vec![e_str("SYSTEM")]), vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(6))),
                            s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                            s_assign("system", e_method_call(e_this(), "quoted", vec![])),
                        ]),
                    ],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                    s_assign("subset", e_null()),
                    s_if(
                        e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictEq, e_str("["))),
                        vec![
                            s_assign("close", e_call("strpos", vec![e_this_prop("src"), e_str("]"), e_this_prop("pos")])),
                            s_if(
                                e_binop(e_var("close"), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("subset", e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("close"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(1))])),
                            s_prop_assign(e_this(), "pos", e_binop(e_var("close"), BinOp::Add, e_int(1))),
                            s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")), BinOp::Or, e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_assign("dtd", e_new("DOMDocumentType", vec![e_var("name"), e_var("public"), e_var("system")])),
                    s_prop_assign(e_var("dtd"), "internalSubset", e_var("subset")),
                    s_prop_assign(e_var("dtd"), "ownerDocument", e_this_prop("doc")),
                    s_return(e_var("dtd")),
                ]),
        )
        .method(
            method("quoted")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("quote", e_index(e_this_prop("src"), e_this_prop("pos"))),
                    s_assign("close", e_call("strpos", vec![e_this_prop("src"), e_var("quote"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))])),
                    s_if(
                        e_binop(e_var("close"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_prop_assign(e_this(), "pos", e_this_prop("len")),
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("value", e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("close"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(1))])),
                    s_prop_assign(e_this(), "pos", e_binop(e_var("close"), BinOp::Add, e_int(1))),
                    s_return(e_var("value")),
                ]),
        )
        .method(
            method("parseName")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("src"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_call("strpos", vec![e_str(" \t\r\n/>=<'\""), e_var("c")]), BinOp::StrictNotEq, e_bool(false)),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_return(e_call("substr", vec![e_this_prop("src"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])),
                ]),
        )
        .method(
            method("lookupNamespace")
                .param("prefix", TypeExpr::Str)
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_if(
                        e_binop(e_var("prefix"), BinOp::StrictEq, e_str("xml")),
                        vec![
                            s_return(e_str("http://www.w3.org/XML/1998/namespace")),
                        ],
                        vec![],
                        None,
                    ),
                    s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("scopes")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                        s_if(
                            e_call("isset", vec![e_index(e_index(e_this_prop("scopes"), e_var("i")), e_var("prefix"))]),
                            vec![
                                s_return(e_index(e_index(e_this_prop("scopes"), e_var("i")), e_var("prefix"))),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("parseElement")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_assign("name", e_method_call(e_this(), "parseName", vec![])),
                    s_if(
                        e_not(e_static_call("__ElephcDom", "validName", vec![e_var("name")])),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("rawAttrs", e_array(vec![])),
                    s_assign("declared", e_array(vec![])),
                    s_while(e_bool(true), vec![
                        s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                        s_if(
                            e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("c", e_index(e_this_prop("src"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(">")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("/"))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("attrName", e_method_call(e_this(), "parseName", vec![])),
                        s_if(
                            e_not(e_static_call("__ElephcDom", "validName", vec![e_var("attrName")])),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                        s_if(
                            e_binop(e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")), BinOp::Or, e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictNotEq, e_str("="))),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                        s_if(
                            e_binop(e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")), BinOp::Or, e_binop(e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictNotEq, e_str("\"")), BinOp::And, e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictNotEq, e_str("'")))),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("quote", e_index(e_this_prop("src"), e_this_prop("pos"))),
                        s_assign("close", e_call("strpos", vec![e_this_prop("src"), e_var("quote"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))])),
                        s_if(
                            e_binop(e_var("close"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("raw", e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("close"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(1))])),
                        s_if(
                            e_binop(e_call("strpos", vec![e_var("raw"), e_str("<")]), BinOp::StrictNotEq, e_bool(false)),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_var("close"), BinOp::Add, e_int(1))),
                        s_assign("value", e_method_call(e_this(), "decodeText", vec![e_var("raw"), e_bool(true)])),
                        s_if(
                            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_call("isset", vec![e_index(e_var("rawAttrs"), e_var("attrName"))]), BinOp::Or, e_call("isset", vec![e_index(e_var("declared"), e_var("attrName"))])),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("attrName"), BinOp::StrictEq, e_str("xmlns")),
                            vec![
                                s_array_assign("declared", e_str(""), e_var("value")),
                                s_array_assign("rawAttrs", e_var("attrName"), e_null()),
                            ],
                            vec![
                            (e_call("str_starts_with", vec![e_var("attrName"), e_str("xmlns:")]), vec![
                                s_array_assign("declared", e_call("substr", vec![e_var("attrName"), e_int(6)]), e_var("value")),
                                s_array_assign("rawAttrs", e_var("attrName"), e_null()),
                            ]),
                        ],
                            Some(vec![
                            s_array_assign("rawAttrs", e_var("attrName"), e_var("value")),
                        ]),
                        ),
                        s_if(
                            e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_call("strpos", vec![e_str(" \t\r\n/>"), e_index(e_this_prop("src"), e_this_prop("pos"))]), BinOp::StrictEq, e_bool(false))),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_prop_array_push(e_this(), "scopes", e_var("declared")),
                    s_assign("element", e_new("DOMElement", vec![e_var("name")])),
                    s_prop_assign(e_var("element"), "ownerDocument", e_this_prop("doc")),
                    s_prop_assign(e_var("element"), "nsDecl", e_var("declared")),
                    s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                    s_if(
                        e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_assign("uri", e_method_call(e_this(), "lookupNamespace", vec![e_call("substr", vec![e_var("name"), e_int(0), e_var("colon")])])),
                            s_if(
                                e_binop(e_var("uri"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_prop_assign(e_var("element"), "namespaceURI", e_var("uri")),
                                ],
                                vec![],
                                Some(vec![
                                s_prop_assign(e_var("element"), "prefix", e_str("")),
                                s_prop_assign(e_var("element"), "localName", e_var("name")),
                            ]),
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("uri", e_method_call(e_this(), "lookupNamespace", vec![e_str("")])),
                        s_if(
                            e_binop(e_binop(e_var("uri"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("uri"), BinOp::StrictNotEq, e_str(""))),
                            vec![
                                s_prop_assign(e_var("element"), "namespaceURI", e_var("uri")),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    ),
                    s_foreach(e_var("rawAttrs"), Some("attrName"), "value", vec![
                        s_if(
                            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("attr", e_new("DOMAttr", vec![e_cast(CastType::String, e_var("attrName")), e_var("value")])),
                        s_prop_assign(e_var("attr"), "ownerDocument", e_this_prop("doc")),
                        s_prop_assign(e_var("attr"), "ownerElement", e_var("element")),
                        s_prop_assign(e_var("attr"), "parentNode", e_var("element")),
                        s_assign("attrColon", e_call("strpos", vec![e_cast(CastType::String, e_var("attrName")), e_str(":")])),
                        s_if(
                            e_binop(e_var("attrColon"), BinOp::StrictNotEq, e_bool(false)),
                            vec![
                                s_assign("attrUri", e_method_call(e_this(), "lookupNamespace", vec![e_call("substr", vec![e_cast(CastType::String, e_var("attrName")), e_int(0), e_var("attrColon")])])),
                                s_if(
                                    e_binop(e_var("attrUri"), BinOp::StrictNotEq, e_null()),
                                    vec![
                                        s_expr(e_method_call(e_var("attr"), "applyNamespace", vec![e_var("attrUri")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_array_assign(e_var("element"), "attrNodes", e_cast(CastType::String, e_var("attrName")), e_var("attr")),
                    ]),
                    s_expr(e_method_call(e_var("element"), "syncAttrCount", vec![])),
                    s_if(
                        e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictEq, e_str("/")),
                        vec![
                            s_if(
                                e_not(e_method_call(e_this(), "startsWith", vec![e_str("/>")])),
                                vec![
                                    s_return(e_null()),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                            s_expr(e_call("array_pop", vec![e_this_prop("scopes")])),
                            s_return(e_var("element")),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_if(
                        e_not(e_method_call(e_this(), "parseContent", vec![e_var("element"), e_var("name")])),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("array_pop", vec![e_this_prop("scopes")])),
                    s_return(e_var("element")),
                ]),
        )
        .method(
            method("parseContent")
                .param("element", t_class("DOMElement"))
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_while(e_bool(true), vec![
                        s_if(
                            e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictNotEq, e_str("<")),
                            vec![
                                s_assign("next", e_call("strpos", vec![e_this_prop("src"), e_str("<"), e_this_prop("pos")])),
                                s_assign("stop", e_ternary(e_binop(e_var("next"), BinOp::StrictEq, e_bool(false)), e_this_prop("len"), e_var("next"))),
                                s_assign("raw", e_call("substr", vec![e_this_prop("src"), e_this_prop("pos"), e_binop(e_var("stop"), BinOp::Sub, e_this_prop("pos"))])),
                                s_prop_assign(e_this(), "pos", e_var("stop")),
                                s_assign("text", e_method_call(e_this(), "decodeText", vec![e_var("raw"), e_bool(false)])),
                                s_if(
                                    e_binop(e_var("text"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_binop(e_binop(e_binop(e_this_prop("options"), BinOp::BitAnd, e_int(256)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_call("trim", vec![e_var("text"), e_str(" \t\r\n")]), BinOp::StrictEq, e_str(""))), BinOp::And, e_method_call(e_this(), "blankIgnorable", vec![e_var("element")])),
                                    vec![
                                        s_continue(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_this(), "appendText", vec![e_var("element"), e_var("text")])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_method_call(e_this(), "startsWith", vec![e_str("</")]),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                                s_assign("closing", e_method_call(e_this(), "parseName", vec![])),
                                s_expr(e_method_call(e_this(), "skipSpace", vec![])),
                                s_if(
                                    e_binop(e_binop(e_binop(e_var("closing"), BinOp::StrictNotEq, e_var("name")), BinOp::Or, e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len"))), BinOp::Or, e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                s_return(e_bool(true)),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_method_call(e_this(), "startsWith", vec![e_str("<!--")]),
                            vec![
                                s_assign("comment", e_method_call(e_this(), "parseComment", vec![])),
                                s_if(
                                    e_binop(e_var("comment"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_var("element"), "linkBefore", vec![e_var("comment"), e_null()])),
                            ],
                            vec![
                            (e_method_call(e_this(), "startsWith", vec![e_str("<![CDATA[")]), vec![
                                s_assign("end", e_call("strpos", vec![e_this_prop("src"), e_str("]]>"), e_this_prop("pos")])),
                                s_if(
                                    e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("data", e_call("substr", vec![e_this_prop("src"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(9)), e_binop(e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(9))])),
                                s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(3))),
                                s_if(
                                    e_binop(e_binop(e_this_prop("options"), BinOp::BitAnd, e_int(16384)), BinOp::StrictNotEq, e_int(0)),
                                    vec![
                                        s_expr(e_method_call(e_this(), "appendText", vec![e_var("element"), e_var("data")])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("cdata", e_new("DOMCdataSection", vec![e_var("data")])),
                                    s_prop_assign(e_var("cdata"), "ownerDocument", e_this_prop("doc")),
                                    s_expr(e_method_call(e_var("element"), "linkBefore", vec![e_var("cdata"), e_null()])),
                                ]),
                                ),
                            ]),
                            (e_method_call(e_this(), "startsWith", vec![e_str("<?")]), vec![
                                s_assign("pi", e_method_call(e_this(), "parseProcessingInstruction", vec![])),
                                s_if(
                                    e_binop(e_var("pi"), BinOp::StrictEq, e_null()),
                                    vec![
                                        s_return(e_bool(false)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_expr(e_method_call(e_var("element"), "linkBefore", vec![e_var("pi"), e_null()])),
                            ]),
                        ],
                            Some(vec![
                            s_assign("child", e_method_call(e_this(), "parseElement", vec![])),
                            s_if(
                                e_binop(e_var("child"), BinOp::StrictEq, e_null()),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_var("element"), "linkBefore", vec![e_var("child"), e_null()])),
                        ]),
                        ),
                    ]),
                ]),
        )
        .method(
            method("blankIgnorable")
                .param("element", t_class("DOMElement"))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("src"), e_this_prop("pos")), BinOp::StrictNotEq, e_str("<"))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_prop(e_var("element"), "firstChild"), BinOp::StrictEq, e_null()), BinOp::And, e_method_call(e_this(), "startsWith", vec![e_str("</")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("last", e_prop(e_var("element"), "lastChild")),
                    s_if(
                        e_binop(e_binop(e_var("last"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(4)))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("last"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("element"), "firstChild"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_binop(e_prop(e_prop(e_var("element"), "firstChild"), "nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_prop(e_prop(e_var("element"), "firstChild"), "nodeType"), BinOp::StrictEq, e_int(4)))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("appendText")
                .param("element", t_class("DOMElement"))
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("last", e_prop(e_var("element"), "lastChild")),
                    s_if(
                        e_binop(e_binop(e_var("last"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(3))),
                        vec![
                            s_expr(e_method_call(e_var("last"), "appendData", vec![e_var("text")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("node", e_new("DOMText", vec![e_var("text")])),
                    s_prop_assign(e_var("node"), "ownerDocument", e_this_prop("doc")),
                    s_expr(e_method_call(e_var("element"), "linkBefore", vec![e_var("node"), e_null()])),
                ]),
        )
        .method(
            method("decodeText")
                .param("raw", TypeExpr::Str)
                .param("attribute", TypeExpr::Bool)
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_if(
                        e_var("attribute"),
                        vec![
                            s_assign("raw", e_call("str_replace", vec![e_array(vec![e_str("\t"), e_str("\n"), e_str("\r")]), e_str(" "), e_var("raw")])),
                        ],
                        vec![
                        (e_binop(e_call("strpos", vec![e_var("raw"), e_str("\r")]), BinOp::StrictNotEq, e_bool(false)), vec![
                            s_assign("raw", e_call("str_replace", vec![e_str("\r\n"), e_str("\n"), e_var("raw")])),
                            s_assign("raw", e_call("str_replace", vec![e_str("\r"), e_str("\n"), e_var("raw")])),
                        ]),
                    ],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("strpos", vec![e_var("raw"), e_str("&")]), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_var("raw")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("out", e_str("")),
                    s_assign("length", e_call("strlen", vec![e_var("raw")])),
                    s_assign("i", e_int(0)),
                    s_while(e_binop(e_var("i"), BinOp::Lt, e_var("length")), vec![
                        s_assign("amp", e_call("strpos", vec![e_var("raw"), e_str("&"), e_var("i")])),
                        s_if(
                            e_binop(e_var("amp"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("substr", vec![e_var("raw"), e_var("i")]))),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("substr", vec![e_var("raw"), e_var("i"), e_binop(e_var("amp"), BinOp::Sub, e_var("i"))]))),
                        s_assign("semi", e_call("strpos", vec![e_var("raw"), e_str(";"), e_var("amp")])),
                        s_if(
                            e_binop(e_var("semi"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("decoded", e_static_call("__ElephcDom", "decodeReference", vec![e_call("substr", vec![e_var("raw"), e_binop(e_var("amp"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("semi"), BinOp::Sub, e_var("amp")), BinOp::Sub, e_int(1))])])),
                        s_if(
                            e_binop(e_var("decoded"), BinOp::StrictEq, e_null()),
                            vec![
                                s_return(e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("decoded"))),
                        s_assign("i", e_binop(e_var("semi"), BinOp::Add, e_int(1))),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .build()
}

/// `__ElephcHtmlTables` — transcribed from the PHP form.
fn decl_class_elephchtmltables() -> Stmt {
    class("__ElephcHtmlTables")
        .final_()
        .method(
            method("entityByName")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_assign("map", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str(","), e_str("quot:34,amp:38,lt:60,gt:62,nbsp:160,iexcl:161,cent:162,pound:163,curren:164,yen:165,brvbar:166,sect:167,uml:168,copy:169,ordf:170,laquo:171,not:172,shy:173,reg:174,macr:175,deg:176,plusmn:177,sup2:178,sup3:179,acute:180,micro:181,para:182,middot:183,cedil:184,sup1:185,ordm:186,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,Agrave:192,Aacute:193,Acirc:194,Atilde:195,Auml:196,Aring:197,AElig:198,Ccedil:199,Egrave:200,Eacute:201,Ecirc:202,Euml:203,Igrave:204,Iacute:205,Icirc:206,Iuml:207,ETH:208,Ntilde:209,Ograve:210,Oacute:211,Ocirc:212,Otilde:213,Ouml:214,times:215,Oslash:216,Ugrave:217,Uacute:218,Ucirc:219,Uuml:220,Yacute:221,THORN:222,szlig:223,agrave:224,aacute:225,acirc:226,atilde:227,auml:228,aring:229,aelig:230,ccedil:231,egrave:232,eacute:233,ecirc:234,euml:235,igrave:236,iacute:237,icirc:238,iuml:239,eth:240,ntilde:241,ograve:242,oacute:243,ocirc:244,otilde:245,ouml:246,divide:247,oslash:248,ugrave:249,uacute:250,ucirc:251,uuml:252,yacute:253,thorn:254,yuml:255,OElig:338,oelig:339,Scaron:352,scaron:353,Yuml:376,fnof:402,circ:710,tilde:732,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969,thetasym:977,upsih:978,piv:982,ensp:8194,emsp:8195,thinsp:8201,zwnj:8204,zwj:8205,lrm:8206,rlm:8207,ndash:8211,mdash:8212,lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,oline:8254,frasl:8260,euro:8364,image:8465,weierp:8472,real:8476,trade:8482,alefsym:8501,larr:8592,uarr:8593,rarr:8594,darr:8595,harr:8596,crarr:8629,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,lowast:8727,radic:8730,prop:8733,infin:8734,ang:8736,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,nsub:8836,sube:8838,supe:8839,oplus:8853,otimes:8855,perp:8869,sdot:8901,lceil:8968,rceil:8969,lfloor:8970,rfloor:8971,lang:9001,rang:9002,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,apos:39")]), None, "pair", vec![
                        s_assign("parts", e_call("explode", vec![e_str(":"), e_var("pair")])),
                        s_array_assign("map", e_index(e_var("parts"), e_int(0)), e_cast(CastType::Int, e_index(e_var("parts"), e_int(1)))),
                    ]),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("entityByValue")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_assign("map", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str(","), e_str("quot:34,amp:38,lt:60,gt:62,nbsp:160,iexcl:161,cent:162,pound:163,curren:164,yen:165,brvbar:166,sect:167,uml:168,copy:169,ordf:170,laquo:171,not:172,shy:173,reg:174,macr:175,deg:176,plusmn:177,sup2:178,sup3:179,acute:180,micro:181,para:182,middot:183,cedil:184,sup1:185,ordm:186,raquo:187,frac14:188,frac12:189,frac34:190,iquest:191,Agrave:192,Aacute:193,Acirc:194,Atilde:195,Auml:196,Aring:197,AElig:198,Ccedil:199,Egrave:200,Eacute:201,Ecirc:202,Euml:203,Igrave:204,Iacute:205,Icirc:206,Iuml:207,ETH:208,Ntilde:209,Ograve:210,Oacute:211,Ocirc:212,Otilde:213,Ouml:214,times:215,Oslash:216,Ugrave:217,Uacute:218,Ucirc:219,Uuml:220,Yacute:221,THORN:222,szlig:223,agrave:224,aacute:225,acirc:226,atilde:227,auml:228,aring:229,aelig:230,ccedil:231,egrave:232,eacute:233,ecirc:234,euml:235,igrave:236,iacute:237,icirc:238,iuml:239,eth:240,ntilde:241,ograve:242,oacute:243,ocirc:244,otilde:245,ouml:246,divide:247,oslash:248,ugrave:249,uacute:250,ucirc:251,uuml:252,yacute:253,thorn:254,yuml:255,OElig:338,oelig:339,Scaron:352,scaron:353,Yuml:376,fnof:402,circ:710,tilde:732,Alpha:913,Beta:914,Gamma:915,Delta:916,Epsilon:917,Zeta:918,Eta:919,Theta:920,Iota:921,Kappa:922,Lambda:923,Mu:924,Nu:925,Xi:926,Omicron:927,Pi:928,Rho:929,Sigma:931,Tau:932,Upsilon:933,Phi:934,Chi:935,Psi:936,Omega:937,alpha:945,beta:946,gamma:947,delta:948,epsilon:949,zeta:950,eta:951,theta:952,iota:953,kappa:954,lambda:955,mu:956,nu:957,xi:958,omicron:959,pi:960,rho:961,sigmaf:962,sigma:963,tau:964,upsilon:965,phi:966,chi:967,psi:968,omega:969,thetasym:977,upsih:978,piv:982,ensp:8194,emsp:8195,thinsp:8201,zwnj:8204,zwj:8205,lrm:8206,rlm:8207,ndash:8211,mdash:8212,lsquo:8216,rsquo:8217,sbquo:8218,ldquo:8220,rdquo:8221,bdquo:8222,dagger:8224,Dagger:8225,bull:8226,hellip:8230,permil:8240,prime:8242,Prime:8243,lsaquo:8249,rsaquo:8250,oline:8254,frasl:8260,euro:8364,image:8465,weierp:8472,real:8476,trade:8482,alefsym:8501,larr:8592,uarr:8593,rarr:8594,darr:8595,harr:8596,crarr:8629,lArr:8656,uArr:8657,rArr:8658,dArr:8659,hArr:8660,forall:8704,part:8706,exist:8707,empty:8709,nabla:8711,isin:8712,notin:8713,ni:8715,prod:8719,sum:8721,minus:8722,lowast:8727,radic:8730,prop:8733,infin:8734,ang:8736,and:8743,or:8744,cap:8745,cup:8746,int:8747,there4:8756,sim:8764,cong:8773,asymp:8776,ne:8800,equiv:8801,le:8804,ge:8805,sub:8834,sup:8835,nsub:8836,sube:8838,supe:8839,oplus:8853,otimes:8855,perp:8869,sdot:8901,lceil:8968,rceil:8969,lfloor:8970,rfloor:8971,lang:9001,rang:9002,loz:9674,spades:9824,clubs:9827,hearts:9829,diams:9830,apos:39")]), None, "pair", vec![
                        s_assign("parts", e_call("explode", vec![e_str(":"), e_var("pair")])),
                        s_array_assign("map", e_cast(CastType::Int, e_index(e_var("parts"), e_int(1))), e_index(e_var("parts"), e_int(0))),
                    ]),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("closeMap")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_assign("map", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str(" "), e_str("a>a a>fieldset a>table a>td a>th address>dd address>dl address>dt address>form address>li address>ul b>center b>p b>td b>th big>p caption>col caption>colgroup caption>tbody caption>tfoot caption>thead caption>tr colgroup>colgroup colgroup>tbody colgroup>tfoot colgroup>thead colgroup>tr dd>dt dir>dd dir>dl dir>dt dir>form dir>ul dl>form dl>li dt>dd dt>dl font>center font>td font>th form>form h1>fieldset h1>form h1>li h1>p h1>table h2>fieldset h2>form h2>li h2>p h2>table h3>fieldset h3>form h3>li h3>p h3>table h4>fieldset h4>form h4>li h4>p h4>table h5>fieldset h5>form h5>li h5>p h5>table h6>fieldset h6>form h6>li h6>p h6>table head>a head>abbr head>acronym head>address head>b head>bdo head>big head>blockquote head>body head>br head>center head>cite head>code head>dd head>dfn head>dir head>div head>dl head>dt head>em head>fieldset head>font head>form head>frameset head>h1 head>h2 head>h3 head>h4 head>h5 head>h6 head>hr head>i head>iframe head>img head>kbd head>li head>map head>menu head>ol head>p head>pre head>q head>s head>samp head>small head>span head>strike head>strong head>sub head>sup head>table head>tt head>u head>ul head>var i>center i>p i>td i>th legend>fieldset li>li menu>dd menu>dl menu>dt menu>form menu>ul ol>form ol>ul option>optgroup option>option p>address p>blockquote p>body p>caption p>center p>col p>colgroup p>dd p>dir p>div p>dl p>dt p>fieldset p>form p>frameset p>h1 p>h2 p>h3 p>h4 p>h5 p>h6 p>head p>hr p>li p>menu p>ol p>p p>pre p>table p>tbody p>td p>tfoot p>th p>title p>tr p>ul pre>dd pre>dl pre>dt pre>fieldset pre>form pre>li pre>table pre>ul s>p small>p span>td span>th strike>p tbody>tbody tbody>tfoot td>tbody td>td td>tfoot td>th td>tr tfoot>tbody th>tbody th>td th>tfoot th>th th>tr thead>tbody thead>tfoot title>body title>frameset tr>tbody tr>tfoot tr>tr tt>p u>p u>td u>th ul>address ul>form ul>menu ul>ol ul>pre script>noscript style>body style>frameset")]), None, "pair", vec![
                        s_array_assign("map", e_var("pair"), e_bool(true)),
                    ]),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("words")
                .static_()
                .param("list", TypeExpr::Str)
                .returns(t_array())
                .body(vec![
                    s_assign("map", e_array(vec![])),
                    s_foreach(e_call("explode", vec![e_str(" "), e_var("list")]), None, "word", vec![
                        s_array_assign("map", e_var("word"), e_bool(true)),
                    ]),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("voidTags")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("area base basefont br col frame hr img input isindex link meta param")])),
                ]),
        )
        .method(
            method("knownTags")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("a abbr acronym address applet area b base basefont bdo big blockquote body br button caption center cite code col colgroup dd del dfn dir div dl dt em embed fieldset font form frame frameset h1 h2 h3 h4 h5 h6 head hr html i iframe img input ins isindex kbd label legend li link map menu meta noframes noscript object ol optgroup option p param pre q s samp script select small span strike strong style sub sup table tbody td textarea tfoot th thead title tr tt u ul var")])),
                ]),
        )
        .method(
            method("blockTags")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("address area base blockquote body caption center col colgroup dd dir div dl dt fieldset form frame frameset h1 h2 h3 h4 h5 h6 head hr html isindex legend li link menu meta noframes noscript ol optgroup option p param pre style table tbody td tfoot th thead title tr ul")])),
                ]),
        )
        .method(
            method("booleanAttrs")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("checked compact declare defer disabled ismap multiple nohref noresize noshade nowrap readonly selected")])),
                ]),
        )
        .method(
            method("allowPcdata")
                .static_()
                .returns(t_array())
                .body(vec![
                    s_return(e_self_call("words", vec![e_str("a abbr acronym address applet b bdo big blockquote body button caption center cite code dd del dfn div dt em font form h1 h2 h3 h4 h5 h6 i iframe ins kbd label legend li noscript object p pre q s samp small span strike strong td th tt u var")])),
                ]),
        )
        .method(
            method("endPriority")
                .static_()
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("div")),
                        vec![
                            s_return(e_int(150)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("td")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("th"))),
                        vec![
                            s_return(e_int(160)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("tr")),
                        vec![
                            s_return(e_int(170)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("thead")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("tbody"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("tfoot"))),
                        vec![
                            s_return(e_int(180)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("table")),
                        vec![
                            s_return(e_int(220)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("head")),
                        vec![
                            s_return(e_int(230)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("body")),
                        vec![
                            s_return(e_int(240)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_return(e_int(250)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_int(100)),
                ]),
        )
        .build()
}

/// `__ElephcHtmlParser` — transcribed from the PHP form.
fn decl_class_elephchtmlparser() -> Stmt {
    class("__ElephcHtmlParser")
        .final_()
        .prop("doc", t_class("DOMDocument"), None)
        .prop("s", TypeExpr::Str, Some(e_str("")))
        .prop("pos", TypeExpr::Int, Some(e_int(0)))
        .prop("len", TypeExpr::Int, Some(e_int(0)))
        .prop("options", TypeExpr::Int, Some(e_int(0)))
        .prop("names", t_array(), Some(e_array(vec![])))
        .prop("nodes", t_array(), Some(e_array(vec![])))
        .prop("nodeTop", TypeExpr::Int, Some(e_int(0)))
        .prop("depth", TypeExpr::Int, Some(e_int(0)))
        .prop("seen", TypeExpr::Int, Some(e_int(0)))
        .prop("closeMap", t_array(), Some(e_array(vec![])))
        .prop("voidTags", t_array(), Some(e_array(vec![])))
        .prop("entities", t_array(), Some(e_array(vec![])))
        .prop("booleanAttrs", t_array(), Some(e_array(vec![])))
        .prop("allowPcdata", t_array(), Some(e_array(vec![])))
        .prop("keepBlanks", TypeExpr::Bool, Some(e_bool(true)))
        .method(
            method("__construct")
                .param("doc", t_class("DOMDocument"))
                .param("source", TypeExpr::Str)
                .param("options", TypeExpr::Int)
                .body(vec![
                    s_prop_assign(e_this(), "doc", e_var("doc")),
                    s_prop_assign(e_this(), "options", e_var("options")),
                    s_prop_assign(e_this(), "keepBlanks", e_binop(e_binop(e_var("options"), BinOp::BitAnd, e_int(256)), BinOp::StrictEq, e_int(0))),
                    s_prop_assign(e_this(), "s", e_self_call("decodeSource", vec![e_var("source")])),
                    s_prop_assign(e_this(), "len", e_call("strlen", vec![e_this_prop("s")])),
                ]),
        )
        .method(
            method("decodeSource")
                .static_()
                .param("source", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_not(e_self_call("hasHighByte", vec![e_var("source")])),
                        vec![
                            s_return(e_var("source")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_starts_with", vec![e_var("source"), e_str("<?xml")]),
                        vec![
                            s_return(e_var("source")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("cut", e_self_call("metaUtf8End", vec![e_var("source")])),
                    s_if(
                        e_binop(e_var("cut"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_self_call("latin1ToUtf8", vec![e_var("source")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_self_call("latin1ToUtf8", vec![e_call("substr", vec![e_var("source"), e_int(0), e_var("cut")])]), BinOp::Concat, e_call("substr", vec![e_var("source"), e_var("cut")]))),
                ]),
        )
        .method(
            method("hasHighByte")
                .static_()
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::GtEq, e_int(128)),
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
            method("metaUtf8End")
                .static_()
                .param("source", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("lower", e_call("strtolower", vec![e_var("source")])),
                    s_assign("from", e_int(0)),
                    s_while(e_bool(true), vec![
                        s_assign("at", e_call("strpos", vec![e_var("lower"), e_str("<meta"), e_var("from")])),
                        s_if(
                            e_binop(e_var("at"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_return(e_neg(e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("end", e_call("strpos", vec![e_var("lower"), e_str(">"), e_var("at")])),
                        s_if(
                            e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_assign("end", e_call("strlen", vec![e_var("lower")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("next", e_call("substr", vec![e_var("lower"), e_binop(e_var("at"), BinOp::Add, e_int(5)), e_int(1)])),
                        s_assign("tag", e_call("substr", vec![e_var("lower"), e_var("at"), e_binop(e_var("end"), BinOp::Sub, e_var("at"))])),
                        s_assign("charset", e_call("strpos", vec![e_var("tag"), e_str("charset")])),
                        s_if(
                            e_binop(e_binop(e_var("charset"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_binop(e_var("next"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_binop(e_binop(e_binop(e_binop(e_var("next"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("next"), BinOp::LtEq, e_str("9"))), BinOp::Or, e_binop(e_binop(e_var("next"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("next"), BinOp::LtEq, e_str("z")))), BinOp::Or, e_binop(e_var("next"), BinOp::StrictEq, e_str("_")))))),
                            vec![
                                s_assign("rest", e_call("ltrim", vec![e_call("substr", vec![e_var("tag"), e_binop(e_var("charset"), BinOp::Add, e_int(7))]), e_str(" \t\r\n")])),
                                s_if(
                                    e_binop(e_binop(e_var("rest"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_index(e_var("rest"), e_int(0)), BinOp::StrictEq, e_str("="))),
                                    vec![
                                        s_assign("rest", e_call("ltrim", vec![e_call("substr", vec![e_var("rest"), e_int(1)]), e_str(" \t\r\n\"'")])),
                                        s_if(
                                            e_binop(e_call("str_starts_with", vec![e_var("rest"), e_str("utf-8")]), BinOp::Or, e_call("str_starts_with", vec![e_var("rest"), e_str("utf8")])),
                                            vec![
                                                s_return(e_binop(e_var("end"), BinOp::Add, e_int(1))),
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
                        s_assign("from", e_binop(e_var("at"), BinOp::Add, e_int(5))),
                    ]),
                ]),
        )
        .method(
            method("latin1ToUtf8")
                .static_()
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("out", e_str("")),
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("code", e_call("ord", vec![e_index(e_var("text"), e_var("i"))])),
                        s_if(
                            e_binop(e_var("code"), BinOp::Lt, e_int(128)),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_index(e_var("text"), e_var("i")))),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_call("chr", vec![e_binop(e_int(192), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))])))),
                        ]),
                        ),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .method(
            method("cur")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_ternary(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("s"), e_this_prop("pos")), e_str(""))),
                ]),
        )
        .method(
            method("nxt")
                .param("n", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_ternary(e_binop(e_binop(e_this_prop("pos"), BinOp::Add, e_var("n")), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("s"), e_binop(e_this_prop("pos"), BinOp::Add, e_var("n"))), e_str(""))),
                ]),
        )
        .method(
            method("skipBlanks")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\n"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\r"))),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            Some(vec![
                            s_break(1),
                        ]),
                        ),
                    ]),
                ]),
        )
        .method(
            method("isBlankChar")
                .param("c", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\n"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\r")))),
                ]),
        )
        .method(
            method("isLetter")
                .param("c", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("z"))), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("A")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("Z"))))),
                ]),
        )
        .method(
            method("startsWithNoCase")
                .param("word", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("strtolower", vec![e_call("substr", vec![e_this_prop("s"), e_this_prop("pos"), e_call("strlen", vec![e_var("word")])])]), BinOp::StrictEq, e_var("word"))),
                ]),
        )
        .method(
            method("topName")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("n", e_call("count", vec![e_this_prop("names")])),
                    s_return(e_ternary(e_binop(e_var("n"), BinOp::StrictEq, e_int(0)), e_null(), e_index(e_this_prop("names"), e_binop(e_var("n"), BinOp::Sub, e_int(1))))),
                ]),
        )
        .method(
            method("topNode")
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_return(e_ternary(e_binop(e_this_prop("nodeTop"), BinOp::StrictEq, e_int(0)), e_null(), e_index(e_this_prop("nodes"), e_binop(e_this_prop("nodeTop"), BinOp::Sub, e_int(1))))),
                ]),
        )
        .method(
            method("parse")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "closeMap", e_static_call("__ElephcHtmlTables", "closeMap", vec![])),
                    s_prop_assign(e_this(), "voidTags", e_static_call("__ElephcHtmlTables", "voidTags", vec![])),
                    s_prop_assign(e_this(), "entities", e_static_call("__ElephcHtmlTables", "entityByName", vec![])),
                    s_prop_assign(e_this(), "booleanAttrs", e_static_call("__ElephcHtmlTables", "booleanAttrs", vec![])),
                    s_prop_assign(e_this(), "allowPcdata", e_static_call("__ElephcHtmlTables", "allowPcdata", vec![])),
                    s_assign("doc", e_this_prop("doc")),
                    s_prop_assign(e_var("doc"), "isHtml", e_bool(true)),
                    s_prop_assign(e_var("doc"), "nodeType", e_int(13)),
                    s_prop_assign(e_var("doc"), "standaloneSet", e_bool(true)),
                    s_prop_assign(e_var("doc"), "xmlStandalone", e_bool(true)),
                    s_prop_assign(e_var("doc"), "standalone", e_bool(true)),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_while(e_binop(e_binop(e_binop(e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("-"))), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(3)]), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("?")))), vec![
                        s_if(
                            e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))),
                            vec![
                                s_expr(e_method_call(e_this(), "parseComment", vec![])),
                            ],
                            vec![],
                            Some(vec![
                            s_expr(e_method_call(e_this(), "parsePI", vec![])),
                        ]),
                        ),
                        s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    ]),
                    s_if(
                        e_binop(e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))), BinOp::And, e_method_call(e_this(), "startsWithNoCaseAt", vec![e_int(2), e_str("doctype")])),
                        vec![
                            s_expr(e_method_call(e_this(), "parseDoctype", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_while(e_binop(e_binop(e_binop(e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("-"))), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(3)]), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("?")))), vec![
                        s_if(
                            e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))),
                            vec![
                                s_expr(e_method_call(e_this(), "parseComment", vec![])),
                            ],
                            vec![],
                            Some(vec![
                            s_expr(e_method_call(e_this(), "parsePI", vec![])),
                        ]),
                        ),
                        s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    ]),
                    s_expr(e_method_call(e_this(), "parseContent", vec![])),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_expr(e_method_call(e_this(), "autoCloseOnEnd", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_this_prop("options"), BinOp::BitAnd, e_int(4)), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_prop(e_var("doc"), "doctype"), BinOp::StrictEq, e_null())),
                        vec![
                            s_assign("dtd", e_new("DOMDocumentType", vec![e_str("html"), e_str("-//W3C//DTD HTML 4.0 Transitional//EN"), e_str("http://www.w3.org/TR/REC-html40/loose.dtd")])),
                            s_prop_assign(e_var("dtd"), "ownerDocument", e_var("doc")),
                            s_assign("before", e_prop(e_var("doc"), "firstChild")),
                            s_expr(e_method_call(e_var("doc"), "linkBefore", vec![e_var("dtd"), e_var("before")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("startsWithNoCaseAt")
                .param("offset", TypeExpr::Int)
                .param("word", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("strtolower", vec![e_call("substr", vec![e_this_prop("s"), e_binop(e_this_prop("pos"), BinOp::Add, e_var("offset")), e_call("strlen", vec![e_var("word")])])]), BinOp::StrictEq, e_var("word"))),
                ]),
        )
        .method(
            method("saxStartElement")
                .param("name", TypeExpr::Str)
                .param("attrs", t_array())
                .param("valueless", t_array())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("local", e_var("name")),
                    s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                    s_if(
                        e_binop(e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_var("colon"), BinOp::Gt, e_int(0))),
                        vec![
                            s_assign("local", e_call("substr", vec![e_var("name"), e_binop(e_var("colon"), BinOp::Add, e_int(1))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("local", e_ternary(e_binop(e_var("local"), BinOp::StrictEq, e_str("")), e_var("name"), e_var("local"))),
                    s_assign("element", e_new("DOMElement", vec![e_str("x")])),
                    s_prop_assign(e_var("element"), "nodeName", e_var("local")),
                    s_prop_assign(e_var("element"), "tagName", e_var("local")),
                    s_prop_assign(e_var("element"), "localName", e_var("local")),
                    s_prop_assign(e_var("element"), "ownerDocument", e_this_prop("doc")),
                    s_foreach(e_var("attrs"), Some("attrName"), "value", vec![
                        s_assign("attr", e_new("DOMAttr", vec![e_cast(CastType::String, e_var("attrName")), e_var("value")])),
                        s_prop_assign(e_var("attr"), "ownerDocument", e_this_prop("doc")),
                        s_prop_assign(e_var("attr"), "ownerElement", e_var("element")),
                        s_prop_assign(e_var("attr"), "parentNode", e_var("element")),
                        s_if(
                            e_call("isset", vec![e_index(e_var("valueless"), e_cast(CastType::String, e_var("attrName")))]),
                            vec![
                                s_prop_assign(e_var("attr"), "valueless", e_bool(true)),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_array_assign(e_var("element"), "attrNodes", e_cast(CastType::String, e_var("attrName")), e_var("attr")),
                    ]),
                    s_expr(e_method_call(e_var("element"), "syncAttrCount", vec![])),
                    s_assign("parent", e_method_call(e_this(), "topNode", vec![])),
                    s_if(
                        e_binop(e_prop(e_this_prop("doc"), "firstChild"), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this_prop("doc"), "linkBefore", vec![e_var("element"), e_null()])),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("parent"), BinOp::StrictEq, e_null()),
                            vec![
                                s_assign("parent", e_prop(e_this_prop("doc"), "firstChild")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_prop(e_var("parent"), "nodeType"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_expr(e_method_call(e_var("parent"), "linkBefore", vec![e_var("element"), e_null()])),
                            ],
                            vec![],
                            Some(vec![
                            s_expr(e_method_call(e_this_prop("doc"), "linkBefore", vec![e_var("element"), e_null()])),
                        ]),
                        ),
                    ]),
                    ),
                    s_if(
                        e_binop(e_call("count", vec![e_this_prop("nodes")]), BinOp::Gt, e_this_prop("nodeTop")),
                        vec![
                            s_prop_array_assign(e_this(), "nodes", e_this_prop("nodeTop"), e_var("element")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_array_push(e_this(), "nodes", e_var("element")),
                    ]),
                    ),
                    s_prop_assign(e_this(), "nodeTop", e_binop(e_this_prop("nodeTop"), BinOp::Add, e_int(1))),
                ]),
        )
        .method(
            method("saxEndElement")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "nodeTop", e_binop(e_this_prop("nodeTop"), BinOp::Sub, e_int(1))),
                    s_prop_array_assign(e_this(), "nodes", e_this_prop("nodeTop"), e_null()),
                ]),
        )
        .method(
            method("saxCharacters")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("node", e_method_call(e_this(), "topNode", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("node"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("text"), BinOp::StrictEq, e_str(""))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("last", e_prop(e_var("node"), "lastChild")),
                    s_if(
                        e_binop(e_binop(e_var("last"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(3))),
                        vec![
                            s_expr(e_method_call(e_var("last"), "appendData", vec![e_var("text")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("created", e_new("DOMText", vec![e_var("text")])),
                    s_prop_assign(e_var("created"), "ownerDocument", e_this_prop("doc")),
                    s_expr(e_method_call(e_var("node"), "linkBefore", vec![e_var("created"), e_null()])),
                ]),
        )
        .method(
            method("saxCdata")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("node", e_method_call(e_this(), "topNode", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("node"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("text"), BinOp::StrictEq, e_str(""))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("last", e_prop(e_var("node"), "lastChild")),
                    s_if(
                        e_binop(e_binop(e_var("last"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(4))),
                        vec![
                            s_expr(e_method_call(e_var("last"), "appendData", vec![e_var("text")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("created", e_new("DOMCdataSection", vec![e_var("text")])),
                    s_prop_assign(e_var("created"), "ownerDocument", e_this_prop("doc")),
                    s_expr(e_method_call(e_var("node"), "linkBefore", vec![e_var("created"), e_null()])),
                ]),
        )
        .method(
            method("saxAddMisc")
                .param("node", t_class("DOMNode"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("parent", e_method_call(e_this(), "topNode", vec![])),
                    s_prop_assign(e_var("node"), "ownerDocument", e_this_prop("doc")),
                    s_if(
                        e_binop(e_binop(e_var("parent"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_prop(e_var("parent"), "nodeType"), BinOp::StrictNotEq, e_int(1))),
                        vec![
                            s_expr(e_method_call(e_this_prop("doc"), "linkBefore", vec![e_var("node"), e_null()])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_method_call(e_var("parent"), "linkBefore", vec![e_var("node"), e_null()])),
                    ]),
                    ),
                ]),
        )
        .method(
            method("namePush")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("seen"), BinOp::Lt, e_int(3)), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_prop_assign(e_this(), "seen", e_int(3)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("seen"), BinOp::Lt, e_int(10)), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("body"))),
                        vec![
                            s_prop_assign(e_this(), "seen", e_int(10)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_array_push(e_this(), "names", e_var("name")),
                ]),
        )
        .method(
            method("namePop")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("array_pop", vec![e_this_prop("names")])),
                ]),
        )
        .method(
            method("checkAutoClose")
                .param("newTag", TypeExpr::Str)
                .param("oldTag", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_call("isset", vec![e_index(e_this_prop("closeMap"), e_binop(e_binop(e_var("oldTag"), BinOp::Concat, e_str(">")), BinOp::Concat, e_var("newTag")))])),
                ]),
        )
        .method(
            method("autoClose")
                .param("newTag", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Gt, e_int(0)), BinOp::And, e_method_call(e_this(), "checkAutoClose", vec![e_var("newTag"), e_index(e_this_prop("names"), e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Sub, e_int(1)))])), vec![
                        s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                        s_expr(e_method_call(e_this(), "namePop", vec![])),
                    ]),
                ]),
        )
        .method(
            method("autoCloseOnEnd")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Gt, e_int(0)), vec![
                        s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                        s_expr(e_method_call(e_this(), "namePop", vec![])),
                    ]),
                ]),
        )
        .method(
            method("autoCloseOnClose")
                .param("newTag", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("priority", e_static_call("__ElephcHtmlTables", "endPriority", vec![e_var("newTag")])),
                    s_assign("i", e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Sub, e_int(1))),
                    s_while(e_binop(e_var("i"), BinOp::GtEq, e_int(0)), vec![
                        s_if(
                            e_binop(e_index(e_this_prop("names"), e_var("i")), BinOp::StrictEq, e_var("newTag")),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_static_call("__ElephcHtmlTables", "endPriority", vec![e_index(e_this_prop("names"), e_var("i"))]), BinOp::Gt, e_var("priority")),
                            vec![
                                s_return_void(),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_post_dec("i")),
                    ]),
                    s_if(
                        e_binop(e_var("i"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_while(e_binop(e_method_call(e_this(), "topName", vec![]), BinOp::StrictNotEq, e_var("newTag")), vec![
                        s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                        s_expr(e_method_call(e_this(), "namePop", vec![])),
                    ]),
                ]),
        )
        .method(
            method("checkImplied")
                .param("newTag", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("options"), BinOp::BitAnd, e_int(8192)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("newTag"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::LtEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("html")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("html"), e_array(vec![]), e_array(vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("newTag"), BinOp::StrictEq, e_str("body")), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::LtEq, e_int(1)), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("newTag"), BinOp::StrictEq, e_str("script")), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("style"))), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("meta"))), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("link"))), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("title"))), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("base")))),
                        vec![
                            s_if(
                                e_binop(e_this_prop("seen"), BinOp::GtEq, e_int(3)),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("head")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("head"), e_array(vec![]), e_array(vec![])])),
                        ],
                        vec![
                        (e_binop(e_binop(e_binop(e_var("newTag"), BinOp::StrictNotEq, e_str("noframes")), BinOp::And, e_binop(e_var("newTag"), BinOp::StrictNotEq, e_str("frame"))), BinOp::And, e_binop(e_var("newTag"), BinOp::StrictNotEq, e_str("frameset"))), vec![
                            s_if(
                                e_binop(e_this_prop("seen"), BinOp::GtEq, e_int(10)),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_this_prop("names"), None, "open", vec![
                                s_if(
                                    e_binop(e_binop(e_var("open"), BinOp::StrictEq, e_str("body")), BinOp::Or, e_binop(e_var("open"), BinOp::StrictEq, e_str("head"))),
                                    vec![
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("body")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("body"), e_array(vec![]), e_array(vec![])])),
                        ]),
                    ],
                        None,
                    ),
                ]),
        )
        .method(
            method("checkParagraph")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("tag", e_method_call(e_this(), "topName", vec![])),
                    s_if(
                        e_binop(e_var("tag"), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "autoClose", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "checkImplied", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("p"), e_array(vec![]), e_array(vec![])])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("tag"), BinOp::StrictEq, e_str("html")), BinOp::Or, e_binop(e_var("tag"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_expr(e_method_call(e_this(), "autoClose", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "checkImplied", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("p"), e_array(vec![]), e_array(vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("parseContent")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("currentNode", e_method_call(e_this(), "topName", vec![])),
                    s_assign("depth", e_call("count", vec![e_this_prop("names")])),
                    s_while(e_bool(true), vec![
                        s_assign("c", e_ternary(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("s"), e_this_prop("pos")), e_str(""))),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("/"))),
                            vec![
                                s_assign("ended", e_method_call(e_this(), "parseEndTag", vec![])),
                                s_if(
                                    e_binop(e_var("ended"), BinOp::And, e_binop(e_binop(e_var("currentNode"), BinOp::StrictNotEq, e_null()), BinOp::Or, e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::StrictEq, e_int(0)))),
                                    vec![
                                        s_assign("currentNode", e_method_call(e_this(), "topName", vec![])),
                                        s_assign("depth", e_call("count", vec![e_this_prop("names")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("top", e_method_call(e_this(), "topName", vec![])),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_var("top"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_method_call(e_this(), "nxt", vec![e_int(1)])]), BinOp::Or, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str(":")))),
                            vec![
                                s_assign("save", e_this_prop("pos")),
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                s_assign("peek", e_method_call(e_this(), "parseHtmlName", vec![])),
                                s_prop_assign(e_this(), "pos", e_var("save")),
                                s_if(
                                    e_binop(e_binop(e_var("peek"), BinOp::StrictNotEq, e_null()), BinOp::And, e_method_call(e_this(), "checkAutoClose", vec![e_var("peek"), e_var("top")])),
                                    vec![
                                        s_expr(e_method_call(e_this(), "autoClose", vec![e_var("peek")])),
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
                            e_binop(e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_var("depth"), BinOp::GtEq, e_call("count", vec![e_this_prop("names")]))), BinOp::And, e_binop(e_var("currentNode"), BinOp::StrictNotEq, e_var("top"))),
                            vec![
                                s_assign("currentNode", e_var("top")),
                                s_assign("depth", e_call("count", vec![e_this_prop("names")])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_binop(e_var("currentNode"), BinOp::StrictEq, e_str("script")), BinOp::Or, e_binop(e_var("currentNode"), BinOp::StrictEq, e_str("style")))),
                            vec![
                                s_expr(e_method_call(e_this(), "parseScript", vec![])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("c"), BinOp::StrictEq, e_str("")),
                            vec![
                                s_expr(e_method_call(e_this(), "autoCloseOnEnd", vec![])),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))),
                            vec![
                                s_if(
                                    e_method_call(e_this(), "startsWithNoCaseAt", vec![e_int(2), e_str("doctype")]),
                                    vec![
                                        s_expr(e_method_call(e_this(), "parseDoctype", vec![])),
                                    ],
                                    vec![
                                    (e_binop(e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("-")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(3)]), BinOp::StrictEq, e_str("-"))), vec![
                                        s_expr(e_method_call(e_this(), "parseComment", vec![])),
                                    ]),
                                ],
                                    Some(vec![
                                    s_expr(e_method_call(e_this(), "skipBogusComment", vec![])),
                                ]),
                                ),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("?"))), vec![
                                s_expr(e_method_call(e_this(), "parsePI", vec![])),
                            ]),
                            (e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), vec![
                                s_expr(e_method_call(e_this(), "parseElement", vec![])),
                                s_assign("currentNode", e_method_call(e_this(), "topName", vec![])),
                                s_assign("depth", e_call("count", vec![e_this_prop("names")])),
                            ]),
                            (e_binop(e_var("c"), BinOp::StrictEq, e_str("&")), vec![
                                s_expr(e_method_call(e_this(), "parseReference", vec![])),
                            ]),
                        ],
                            Some(vec![
                            s_expr(e_method_call(e_this(), "parseCharData", vec![])),
                        ]),
                        ),
                    ]),
                ]),
        )
        .method(
            method("skipBogusComment")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("parseScript")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_if(
                            e_binop(e_binop(e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("/"))), BinOp::And, e_method_call(e_this(), "isLetter", vec![e_method_call(e_this(), "nxt", vec![e_int(2)])])),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_expr(e_method_call(e_this(), "saxCdata", vec![e_call("substr", vec![e_this_prop("s"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])])),
                ]),
        )
        .method(
            method("parseComment")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("end", e_call("strpos", vec![e_this_prop("s"), e_str("-->"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(4))])),
                    s_if(
                        e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_prop_assign(e_this(), "pos", e_this_prop("len")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("comment", e_new("DOMComment", vec![e_call("substr", vec![e_this_prop("s"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(4)), e_binop(e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(4))])])),
                    s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(3))),
                    s_expr(e_method_call(e_this(), "saxAddMisc", vec![e_var("comment")])),
                ]),
        )
        .method(
            method("parseName")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("c", e_method_call(e_this(), "cur", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_binop(e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_var("c")]), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":"))), BinOp::Or, e_binop(e_call("ord", vec![e_var("c")]), BinOp::GtEq, e_int(128))))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_var("c")]), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9")))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("."))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":"))), BinOp::Or, e_binop(e_call("ord", vec![e_var("c")]), BinOp::GtEq, e_int(128))),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            Some(vec![
                            s_break(1),
                        ]),
                        ),
                    ]),
                    s_return(e_call("substr", vec![e_this_prop("s"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])),
                ]),
        )
        .method(
            method("parsePI")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                    s_assign("target", e_method_call(e_this(), "parseName", vec![])),
                    s_if(
                        e_binop(e_var("target"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                            s_expr(e_method_call(e_this(), "saxCharacters", vec![e_var("target")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            s_expr(e_method_call(e_this(), "saxAddMisc", vec![e_new("DOMProcessingInstruction", vec![e_var("target"), e_str("")])])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("data", e_call("substr", vec![e_this_prop("s"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])),
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_expr(e_method_call(e_this(), "saxAddMisc", vec![e_new("DOMProcessingInstruction", vec![e_var("target"), e_var("data")])])),
                ]),
        )
        .method(
            method("parseLiteral")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("quote", e_method_call(e_this(), "cur", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("quote"), BinOp::StrictNotEq, e_str("\"")), BinOp::And, e_binop(e_var("quote"), BinOp::StrictNotEq, e_str("'"))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("end", e_call("strpos", vec![e_this_prop("s"), e_var("quote"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))])),
                    s_if(
                        e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_prop_assign(e_this(), "pos", e_this_prop("len")),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("value", e_call("substr", vec![e_this_prop("s"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(1))])),
                    s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(1))),
                    s_return(e_var("value")),
                ]),
        )
        .method(
            method("parseDoctype")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(9))),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_assign("name", e_method_call(e_this(), "parseName", vec![])),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_assign("public", e_null()),
                    s_assign("system", e_null()),
                    s_if(
                        e_method_call(e_this(), "startsWithNoCase", vec![e_str("system")]),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(6))),
                            s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                            s_assign("system", e_method_call(e_this(), "parseLiteral", vec![])),
                        ],
                        vec![
                        (e_method_call(e_this(), "startsWithNoCase", vec![e_str("public")]), vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(6))),
                            s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                            s_assign("public", e_method_call(e_this(), "parseLiteral", vec![])),
                            s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                            s_if(
                                e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("\"")), BinOp::Or, e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("'"))),
                                vec![
                                    s_assign("system", e_method_call(e_this(), "parseLiteral", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                    ],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_prop(e_this_prop("doc"), "doctype"), BinOp::StrictNotEq, e_null())),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("dtd", e_new("DOMDocumentType", vec![e_var("name"), e_ternary(e_binop(e_var("public"), BinOp::StrictEq, e_null()), e_str(""), e_var("public")), e_ternary(e_binop(e_var("system"), BinOp::StrictEq, e_null()), e_str(""), e_var("system"))])),
                    s_prop_assign(e_var("dtd"), "ownerDocument", e_this_prop("doc")),
                    s_assign("before", e_prop(e_this_prop("doc"), "firstChild")),
                    s_expr(e_method_call(e_this_prop("doc"), "linkBefore", vec![e_var("dtd"), e_var("before")])),
                ]),
        )
        .method(
            method("areBlanks")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_not(e_method_call(e_this(), "isBlankChar", vec![e_index(e_var("text"), e_var("i"))])),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str("<")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("name", e_method_call(e_this(), "topName", vec![])),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("html"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("body")),
                        vec![
                            s_assign("dtd", e_prop(e_this_prop("doc"), "doctype")),
                            s_if(
                                e_binop(e_binop(e_var("dtd"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("dtd"), "publicId"), BinOp::StrictNotEq, e_str(""))),
                                vec![
                                    s_assign("id", e_call("strtolower", vec![e_prop(e_var("dtd"), "publicId")])),
                                    s_if(
                                        e_binop(e_binop(e_var("id"), BinOp::StrictEq, e_str("-//w3c//dtd html 4.01//en")), BinOp::Or, e_binop(e_var("id"), BinOp::StrictEq, e_str("-//w3c//dtd html 4//en"))),
                                        vec![
                                            s_return(e_bool(true)),
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
                    s_assign("node", e_method_call(e_this(), "topNode", vec![])),
                    s_if(
                        e_binop(e_var("node"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("last", e_prop(e_var("node"), "lastChild")),
                    s_while(e_binop(e_binop(e_var("last"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(8))), vec![
                        s_assign("last", e_prop(e_var("last"), "previousSibling")),
                    ]),
                    s_if(
                        e_binop(e_var("last"), BinOp::StrictEq, e_null()),
                        vec![
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("allowPcdata"), e_var("name"))]),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![
                        (e_binop(e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_prop(e_var("last"), "nodeType"), BinOp::StrictEq, e_int(4))), vec![
                            s_return(e_bool(false)),
                        ]),
                        (e_call("isset", vec![e_index(e_this_prop("allowPcdata"), e_prop(e_var("last"), "nodeName"))]), vec![
                            s_return(e_bool(false)),
                        ]),
                    ],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("parseCharData")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("buf", e_str("")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("&"))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("code", e_call("ord", vec![e_var("c")])),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(32)), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(9))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(10))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(13))),
                            vec![
                                s_assign("buf", e_binop(e_var("buf"), BinOp::Concat, e_var("c"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_if(
                        e_binop(e_var("buf"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_method_call(e_this(), "areBlanks", vec![e_var("buf")]),
                        vec![
                            s_if(
                                e_this_prop("keepBlanks"),
                                vec![
                                    s_expr(e_method_call(e_this(), "saxCharacters", vec![e_var("buf")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                        s_expr(e_method_call(e_this(), "saxCharacters", vec![e_var("buf")])),
                    ]),
                    ),
                ]),
        )
        .method(
            method("parseCharRef")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("value", e_int(0)),
                    s_if(
                        e_binop(e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("x")), BinOp::Or, e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("X"))),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(3))),
                            s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(";"))), vec![
                                s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                                s_if(
                                    e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9"))),
                                    vec![
                                        s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(16)), BinOp::Add, e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(48)))),
                                    ],
                                    vec![
                                    (e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("f"))), vec![
                                        s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(16)), BinOp::Add, e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(87)))),
                                    ]),
                                    (e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("A")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("F"))), vec![
                                        s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(16)), BinOp::Add, e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(55)))),
                                    ]),
                                ],
                                    Some(vec![
                                    s_break(1),
                                ]),
                                ),
                                s_if(
                                    e_binop(e_var("value"), BinOp::Gt, e_int(1114112)),
                                    vec![
                                        s_assign("value", e_int(1114112)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ]),
                            s_if(
                                e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(";")),
                                vec![
                                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                        s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(";"))), vec![
                            s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                            s_if(
                                e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9"))),
                                vec![
                                    s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(10)), BinOp::Add, e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(48)))),
                                ],
                                vec![],
                                Some(vec![
                                s_break(1),
                            ]),
                            ),
                            s_if(
                                e_binop(e_var("value"), BinOp::Gt, e_int(1114112)),
                                vec![
                                    s_assign("value", e_int(1114112)),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ]),
                        s_if(
                            e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(";")),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("value"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("value"), BinOp::GtEq, e_int(1114112))), BinOp::Or, e_binop(e_binop(e_binop(e_binop(e_var("value"), BinOp::Lt, e_int(32)), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_int(9))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_int(10))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_int(13)))), BinOp::Or, e_binop(e_binop(e_var("value"), BinOp::GtEq, e_int(55296)), BinOp::And, e_binop(e_var("value"), BinOp::LtEq, e_int(57343)))), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_int(65534))), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_int(65535))),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("value")),
                ]),
        )
        .method(
            method("parseEntityRef")
                .returns(t_array())
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_assign("name", e_method_call(e_this(), "parseName", vec![])),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_array(vec![e_null(), e_null()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(";")),
                        vec![
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("entities"), e_var("name"))]),
                                vec![
                                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                    s_return(e_array(vec![e_var("name"), e_index(e_this_prop("entities"), e_var("name"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_array(vec![e_var("name"), e_null()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array(vec![e_var("name"), e_null()])),
                ]),
        )
        .method(
            method("parseReference")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("#")),
                        vec![
                            s_assign("value", e_method_call(e_this(), "parseCharRef", vec![])),
                            s_if(
                                e_binop(e_var("value"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                            s_expr(e_method_call(e_this(), "saxCharacters", vec![e_static_call("__ElephcDom", "utf8", vec![e_var("value")])])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("ref", e_method_call(e_this(), "parseEntityRef", vec![])),
                    s_if(
                        e_binop(e_index(e_var("ref"), e_int(0)), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                            s_expr(e_method_call(e_this(), "saxCharacters", vec![e_str("&")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_index(e_var("ref"), e_int(1)), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                            s_expr(e_method_call(e_this(), "saxCharacters", vec![e_binop(e_str("&"), BinOp::Concat, e_index(e_var("ref"), e_int(0)))])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                    s_expr(e_method_call(e_this(), "saxCharacters", vec![e_static_call("__ElephcDom", "utf8", vec![e_index(e_var("ref"), e_int(1))])])),
                ]),
        )
        .method(
            method("parseHtmlName")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("c", e_method_call(e_this(), "cur", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_binop(e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_var("c")]), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("."))))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_var("c")]), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9")))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("."))),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            Some(vec![
                            s_break(1),
                        ]),
                        ),
                    ]),
                    s_return(e_call("strtolower", vec![e_call("substr", vec![e_this_prop("s"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])])),
                ]),
        )
        .method(
            method("parseEndTag")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                    s_assign("name", e_method_call(e_this(), "parseHtmlName", vec![])),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictNotEq, e_str(">")),
                        vec![
                            s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("depth"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("html")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("body"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("head")))),
                        vec![
                            s_prop_assign(e_this(), "depth", e_binop(e_this_prop("depth"), BinOp::Sub, e_int(1))),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("found", e_bool(false)),
                    s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                        s_if(
                            e_binop(e_index(e_this_prop("names"), e_var("i")), BinOp::StrictEq, e_var("name")),
                            vec![
                                s_assign("found", e_bool(true)),
                                s_break(1),
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
                    s_expr(e_method_call(e_this(), "autoCloseOnClose", vec![e_var("name")])),
                    s_if(
                        e_binop(e_method_call(e_this(), "topName", vec![]), BinOp::StrictEq, e_var("name")),
                        vec![
                            s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                            s_expr(e_method_call(e_this(), "namePop", vec![])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("parseElement")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("failed", e_method_call(e_this(), "parseStartTag", vec![])),
                    s_assign("name", e_method_call(e_this(), "topName", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("failed"), BinOp::StrictEq, e_neg(e_int(1))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_null())),
                        vec![
                            s_if(
                                e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                                vec![
                                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
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
                        e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("/")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str(">"))),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                            s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                            s_expr(e_method_call(e_this(), "namePop", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                        s_expr(e_method_call(e_this(), "namePop", vec![])),
                        s_return_void(),
                    ]),
                    ),
                    s_if(
                        e_call("isset", vec![e_index(e_this_prop("voidTags"), e_var("name"))]),
                        vec![
                            s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                            s_expr(e_method_call(e_this(), "namePop", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("parseStartTag")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_assign("name", e_method_call(e_this(), "parseHtmlName", vec![])),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_null()),
                        vec![
                            s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ]),
                            s_return(e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "autoClose", vec![e_var("name")])),
                    s_expr(e_method_call(e_this(), "checkImplied", vec![e_var("name")])),
                    s_assign("discard", e_bool(false)),
                    s_if(
                        e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("html"))),
                        vec![
                            s_assign("discard", e_bool(true)),
                            s_prop_assign(e_this(), "depth", e_binop(e_this_prop("depth"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_assign("discard", e_bool(true)),
                            s_prop_assign(e_this(), "depth", e_binop(e_this_prop("depth"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("body")),
                        vec![
                            s_foreach(e_this_prop("names"), None, "open", vec![
                                s_if(
                                    e_binop(e_var("open"), BinOp::StrictEq, e_str("body")),
                                    vec![
                                        s_assign("discard", e_bool(true)),
                                        s_prop_assign(e_this(), "depth", e_binop(e_this_prop("depth"), BinOp::Add, e_int(1))),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_assign("attrs", e_array(vec![])),
                    s_assign("valueless", e_array(vec![])),
                    s_while(e_binop(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), BinOp::And, e_not(e_binop(e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictEq, e_str("/")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str(">"))))), vec![
                        s_assign("attrName", e_method_call(e_this(), "parseHtmlName", vec![])),
                        s_if(
                            e_binop(e_var("attrName"), BinOp::StrictNotEq, e_null()),
                            vec![
                                s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                                s_assign("value", e_null()),
                                s_if(
                                    e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("=")),
                                    vec![
                                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                        s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                                        s_assign("value", e_method_call(e_this(), "parseAttValue", vec![])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_not(e_call("isset", vec![e_index(e_var("attrs"), e_var("attrName"))])),
                                    vec![
                                        s_if(
                                            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                                            vec![
                                                s_if(
                                                    e_call("isset", vec![e_index(e_this_prop("booleanAttrs"), e_var("attrName"))]),
                                                    vec![
                                                        s_array_assign("attrs", e_var("attrName"), e_var("attrName")),
                                                    ],
                                                    vec![],
                                                    Some(vec![
                                                    s_array_assign("attrs", e_var("attrName"), e_str("")),
                                                    s_array_assign("valueless", e_var("attrName"), e_bool(true)),
                                                ]),
                                                ),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_array_assign("attrs", e_var("attrName"), e_var("value")),
                                        ]),
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            Some(vec![
                            s_while(e_binop(e_binop(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_not(e_method_call(e_this(), "isBlankChar", vec![e_index(e_this_prop("s"), e_this_prop("pos"))]))), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), BinOp::And, e_not(e_binop(e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictEq, e_str("/")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str(">"))))), vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ]),
                        ]),
                        ),
                        s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    ]),
                    s_if(
                        e_not(e_var("discard")),
                        vec![
                            s_expr(e_method_call(e_this(), "namePush", vec![e_var("name")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_var("name"), e_var("attrs"), e_var("valueless")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_ternary(e_var("discard"), e_int(1), e_int(0))),
                ]),
        )
        .method(
            method("parseAttValue")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("c", e_method_call(e_this(), "cur", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("\"")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("'"))),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            s_assign("value", e_method_call(e_this(), "parseAttribute", vec![e_var("c")])),
                            s_if(
                                e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_var("c")),
                                vec![
                                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("value")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "parseAttribute", vec![e_str("")])),
                ]),
        )
        .method(
            method("parseAttribute")
                .param("stop", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("out", e_str("")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_var("stop"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_var("c"), BinOp::StrictEq, e_var("stop"))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("stop"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(">")), BinOp::Or, e_method_call(e_this(), "isBlankChar", vec![e_var("c")]))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("c"), BinOp::StrictEq, e_str("&")),
                            vec![
                                s_if(
                                    e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("#")),
                                    vec![
                                        s_assign("value", e_method_call(e_this(), "parseCharRef", vec![])),
                                        s_if(
                                            e_binop(e_var("value"), BinOp::StrictNotEq, e_int(0)),
                                            vec![
                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_static_call("__ElephcDom", "utf8", vec![e_var("value")]))),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("ref", e_method_call(e_this(), "parseEntityRef", vec![])),
                                    s_if(
                                        e_binop(e_index(e_var("ref"), e_int(0)), BinOp::StrictEq, e_null()),
                                        vec![
                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("&"))),
                                        ],
                                        vec![
                                        (e_binop(e_index(e_var("ref"), e_int(1)), BinOp::StrictEq, e_null()), vec![
                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_str("&"), BinOp::Concat, e_index(e_var("ref"), e_int(0))))),
                                        ]),
                                    ],
                                        Some(vec![
                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_static_call("__ElephcDom", "utf8", vec![e_index(e_var("ref"), e_int(1))]))),
                                    ]),
                                    ),
                                ]),
                                ),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("code", e_call("ord", vec![e_var("c")])),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(32)), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(9))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(10))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(13))),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("c"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .build()
}

/// `__ElephcDomHtmlWriter` — transcribed from the PHP form.
fn decl_class_elephcdomhtmlwriter() -> Stmt {
    class("__ElephcDomHtmlWriter")
        .final_()
        .prop("out", TypeExpr::Str, Some(e_str("")))
        .prop("format", TypeExpr::Bool, Some(e_bool(false)))
        .prop("mode", TypeExpr::Int, Some(e_int(0)))
        .prop("voidTags", t_array(), Some(e_array(vec![])))
        .prop("known", t_array(), Some(e_array(vec![])))
        .prop("block", t_array(), Some(e_array(vec![])))
        .prop("booleans", t_array(), Some(e_array(vec![])))
        .prop("byValue", t_array(), Some(e_array(vec![])))
        .method(
            method("make")
                .static_()
                .param("format", TypeExpr::Bool)
                .param("mode", TypeExpr::Int)
                .returns(t_class("__ElephcDomHtmlWriter"))
                .body(vec![
                    s_assign("writer", e_new("__ElephcDomHtmlWriter", vec![])),
                    s_prop_assign(e_var("writer"), "format", e_var("format")),
                    s_prop_assign(e_var("writer"), "mode", e_var("mode")),
                    s_prop_assign(e_var("writer"), "voidTags", e_static_call("__ElephcHtmlTables", "voidTags", vec![])),
                    s_prop_assign(e_var("writer"), "known", e_static_call("__ElephcHtmlTables", "knownTags", vec![])),
                    s_prop_assign(e_var("writer"), "block", e_static_call("__ElephcHtmlTables", "blockTags", vec![])),
                    s_prop_assign(e_var("writer"), "booleans", e_static_call("__ElephcHtmlTables", "booleanAttrs", vec![])),
                    s_if(
                        e_binop(e_var("mode"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_prop_assign(e_var("writer"), "byValue", e_static_call("__ElephcHtmlTables", "entityByValue", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("writer")),
                ]),
        )
        .method(
            method("document")
                .static_()
                .param("doc", t_class("DOMDocument"))
                .param("format", TypeExpr::Bool)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("writer", e_self_call("make", vec![e_var("format"), e_int(0)])),
                    s_assign("child", e_prop(e_var("doc"), "firstChild")),
                    s_if(
                        e_binop(e_prop(e_var("doc"), "doctype"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("dtd", e_prop(e_var("doc"), "doctype")),
                            s_prop_assign(e_var("writer"), "out", e_binop(e_prop(e_var("writer"), "out"), BinOp::Concat, e_self_call("doctype", vec![e_var("dtd")]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                        s_expr(e_method_call(e_var("writer"), "node", vec![e_var("child")])),
                        s_assign("child", e_prop(e_var("child"), "nextSibling")),
                    ]),
                    s_return(e_method_call(e_var("writer"), "finish", vec![e_binop(e_prop(e_var("writer"), "out"), BinOp::Concat, e_str("\n"))])),
                ]),
        )
        .method(
            method("dumpNode")
                .static_()
                .param("doc", t_class("DOMDocument"))
                .param("node", t_class("DOMNode"))
                .param("format", TypeExpr::Bool)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("mode", e_int(1)),
                    s_if(
                        e_binop(e_binop(e_prop(e_var("doc"), "encoding"), BinOp::StrictEq, e_null()), BinOp::And, e_binop(e_prop(e_var("doc"), "nodeType"), BinOp::StrictNotEq, e_int(13))),
                        vec![
                            s_assign("mode", e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("writer", e_self_call("make", vec![e_var("format"), e_var("mode")])),
                    s_expr(e_method_call(e_var("writer"), "node", vec![e_var("node")])),
                    s_return(e_method_call(e_var("writer"), "finish", vec![e_prop(e_var("writer"), "out")])),
                ]),
        )
        .method(
            method("finish")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("mode"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_return(e_var("text")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("mode"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_return(e_static_call("__ElephcDom", "asciiRefs", vec![e_var("text")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "entityRefs", vec![e_var("text")])),
                ]),
        )
        .method(
            method("doctype")
                .static_()
                .param("dtd", t_class("DOMDocumentType"))
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("out", e_binop(e_str("<!DOCTYPE "), BinOp::Concat, e_prop(e_var("dtd"), "name"))),
                    s_if(
                        e_binop(e_prop(e_var("dtd"), "publicId"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_str(" PUBLIC "), BinOp::Concat, e_self_call("quoted", vec![e_prop(e_var("dtd"), "publicId")])))),
                            s_if(
                                e_binop(e_prop(e_var("dtd"), "systemId"), BinOp::StrictNotEq, e_str("")),
                                vec![
                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_str(" "), BinOp::Concat, e_self_call("quoted", vec![e_prop(e_var("dtd"), "systemId")])))),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![
                        (e_binop(e_prop(e_var("dtd"), "systemId"), BinOp::StrictNotEq, e_str("")), vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_str(" SYSTEM "), BinOp::Concat, e_self_call("quoted", vec![e_prop(e_var("dtd"), "systemId")])))),
                        ]),
                    ],
                        None,
                    ),
                    s_return(e_binop(e_var("out"), BinOp::Concat, e_str(">\n"))),
                ]),
        )
        .method(
            method("quoted")
                .static_()
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_call("strpos", vec![e_var("value"), e_str("\"")]), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_binop(e_binop(e_str("\""), BinOp::Concat, e_var("value")), BinOp::Concat, e_str("\""))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("strpos", vec![e_var("value"), e_str("'")]), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_binop(e_binop(e_str("'"), BinOp::Concat, e_var("value")), BinOp::Concat, e_str("'"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_binop(e_str("\""), BinOp::Concat, e_call("str_replace", vec![e_str("\""), e_str("&quot;"), e_var("value")])), BinOp::Concat, e_str("\""))),
                ]),
        )
        .method(
            method("escape")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("str_replace", vec![e_array(vec![e_str("&"), e_str("<"), e_str(">"), e_str("\r")]), e_array(vec![e_str("&amp;"), e_str("&lt;"), e_str("&gt;"), e_str("&#13;")]), e_var("text")])),
                ]),
        )
        .method(
            method("entityRefs")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_assign("clean", e_bool(true)),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::GtEq, e_int(128)),
                            vec![
                                s_assign("clean", e_bool(false)),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_var("clean"),
                        vec![
                            s_return(e_var("text")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("out", e_str("")),
                    s_assign("i", e_int(0)),
                    s_while(e_binop(e_var("i"), BinOp::Lt, e_var("length")), vec![
                        s_assign("code", e_call("ord", vec![e_index(e_var("text"), e_var("i"))])),
                        s_assign("size", e_int(1)),
                        s_assign("point", e_var("code")),
                        s_if(
                            e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(240)), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(3)), BinOp::Lt, e_var("length"))),
                            vec![
                                s_assign("point", e_binop(e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::BitAnd, e_int(7)), BinOp::ShiftLeft, e_int(18)), BinOp::BitOr, e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(63)), BinOp::ShiftLeft, e_int(12))), BinOp::BitOr, e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(2)))]), BinOp::BitAnd, e_int(63)), BinOp::ShiftLeft, e_int(6))), BinOp::BitOr, e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(3)))]), BinOp::BitAnd, e_int(63)))),
                                s_assign("size", e_int(4)),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(224)), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(2)), BinOp::Lt, e_var("length"))), vec![
                                s_assign("point", e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::BitAnd, e_int(15)), BinOp::ShiftLeft, e_int(12)), BinOp::BitOr, e_binop(e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(63)), BinOp::ShiftLeft, e_int(6))), BinOp::BitOr, e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(2)))]), BinOp::BitAnd, e_int(63)))),
                                s_assign("size", e_int(3)),
                            ]),
                            (e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(192)), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("length"))), vec![
                                s_assign("point", e_binop(e_binop(e_binop(e_var("code"), BinOp::BitAnd, e_int(31)), BinOp::ShiftLeft, e_int(6)), BinOp::BitOr, e_binop(e_call("ord", vec![e_index(e_var("text"), e_binop(e_var("i"), BinOp::Add, e_int(1)))]), BinOp::BitAnd, e_int(63)))),
                                s_assign("size", e_int(2)),
                            ]),
                        ],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("point"), BinOp::Lt, e_int(128)),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_index(e_var("text"), e_var("i")))),
                            ],
                            vec![
                            (e_call("isset", vec![e_index(e_this_prop("byValue"), e_var("point"))]), vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_str("&"), BinOp::Concat, e_index(e_this_prop("byValue"), e_var("point"))), BinOp::Concat, e_str(";")))),
                            ]),
                        ],
                            Some(vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_str("&#"), BinOp::Concat, e_var("point")), BinOp::Concat, e_str(";")))),
                        ]),
                        ),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_var("size"))),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .method(
            method("uriEscape")
                .static_()
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("out", e_str("")),
                    s_assign("length", e_call("strlen", vec![e_var("value")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("c", e_index(e_var("value"), e_var("i"))),
                        s_assign("code", e_call("ord", vec![e_var("c")])),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(57))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(90)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(122)))), BinOp::Or, e_binop(e_call("strpos", vec![e_str("-_.!~*'()@/:=?;#%&,+"), e_var("c")]), BinOp::StrictNotEq, e_bool(false))),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("c"))),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_str("%"), BinOp::Concat, e_call("strtoupper", vec![e_call("substr", vec![e_binop(e_str("0"), BinOp::Concat, e_call("dechex", vec![e_var("code")])), e_neg(e_int(2))])])))),
                        ]),
                        ),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .method(
            method("attribute")
                .param("attr", t_class("DOMAttr"))
                .param("owner", t_class("DOMElement"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_str(" "), BinOp::Concat, e_prop(e_var("attr"), "nodeName")))),
                    s_if(
                        e_binop(e_call("isset", vec![e_index(e_this_prop("booleans"), e_call("strtolower", vec![e_prop(e_var("attr"), "name")]))]), BinOp::Or, e_prop(e_var("attr"), "valueless")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("value", e_method_call(e_this(), "escape", vec![e_prop(e_var("attr"), "value")])),
                    s_assign("lower", e_call("strtolower", vec![e_prop(e_var("attr"), "name")])),
                    s_if(
                        e_binop(e_binop(e_binop(e_prop(e_var("owner"), "namespaceURI"), BinOp::StrictEq, e_null()), BinOp::And, e_binop(e_prop(e_var("attr"), "namespaceURI"), BinOp::StrictEq, e_null())), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_var("lower"), BinOp::StrictEq, e_str("href")), BinOp::Or, e_binop(e_var("lower"), BinOp::StrictEq, e_str("action"))), BinOp::Or, e_binop(e_var("lower"), BinOp::StrictEq, e_str("src"))), BinOp::Or, e_binop(e_binop(e_var("lower"), BinOp::StrictEq, e_str("name")), BinOp::And, e_binop(e_call("strtolower", vec![e_prop(e_var("owner"), "nodeName")]), BinOp::StrictEq, e_str("a"))))),
                        vec![
                            s_assign("value", e_self_call("uriEscape", vec![e_call("ltrim", vec![e_var("value"), e_str(" \t\r\n")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_str("="), BinOp::Concat, e_self_call("quoted", vec![e_var("value")])))),
                ]),
        )
        .method(
            method("isText")
                .param("node", t_nullable(t_class("DOMNode")))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_prop(e_var("node"), "nodeType"), BinOp::StrictEq, e_int(5))))),
                ]),
        )
        .method(
            method("node")
                .param("cur", t_class("DOMNode"))
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_instance_of(e_var("cur"), "DOMDocumentType"),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_instance_of(e_var("cur"), "DOMDocument"), BinOp::Or, e_instance_of(e_var("cur"), "DOMDocumentFragment")),
                        vec![
                            s_assign("child", e_prop(e_var("cur"), "firstChild")),
                            s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                                s_expr(e_method_call(e_this(), "node", vec![e_var("child")])),
                                s_assign("child", e_prop(e_var("child"), "nextSibling")),
                            ]),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("cur"), "DOMCdataSection"),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_prop(e_var("cur"), "data"))),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("cur"), "DOMText"),
                        vec![
                            s_assign("parent", e_prop(e_var("cur"), "parentNode")),
                            s_if(
                                e_binop(e_binop(e_var("parent"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_prop(e_var("parent"), "nodeName"), BinOp::StrictEq, e_str("script")), BinOp::Or, e_binop(e_prop(e_var("parent"), "nodeName"), BinOp::StrictEq, e_str("style")))),
                                vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_prop(e_var("cur"), "data"))),
                                ],
                                vec![],
                                Some(vec![
                                s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_method_call(e_this(), "escape", vec![e_prop(e_var("cur"), "data")]))),
                            ]),
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("cur"), "DOMComment"),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("<!--"), BinOp::Concat, e_prop(e_var("cur"), "data")), BinOp::Concat, e_str("-->")))),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_instance_of(e_var("cur"), "DOMProcessingInstruction"),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_ternary(e_binop(e_prop(e_var("cur"), "data"), BinOp::StrictEq, e_str("")), e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("cur"), "target")), BinOp::Concat, e_str(">")), e_binop(e_binop(e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("cur"), "target")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_prop(e_var("cur"), "data")), BinOp::Concat, e_str(">"))))),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_instance_of(e_var("cur"), "DOMElement")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("lower", e_call("strtolower", vec![e_prop(e_var("cur"), "nodeName")])),
                    s_assign("known", e_binop(e_binop(e_prop(e_var("cur"), "namespaceURI"), BinOp::StrictEq, e_null()), BinOp::And, e_call("isset", vec![e_index(e_this_prop("known"), e_var("lower"))]))),
                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_str("<"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")))),
                    s_foreach(e_prop(e_var("cur"), "attrNodes"), None, "attr", vec![
                        s_expr(e_method_call(e_this(), "attribute", vec![e_var("attr"), e_var("cur")])),
                    ]),
                    s_assign("next", e_prop(e_var("cur"), "nextSibling")),
                    s_assign("parent", e_prop(e_var("cur"), "parentNode")),
                    s_assign("afterNewline", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_this_prop("format"), BinOp::And, e_var("known")), BinOp::And, e_call("isset", vec![e_index(e_this_prop("block"), e_var("lower"))])), BinOp::And, e_binop(e_var("next"), BinOp::StrictNotEq, e_null())), BinOp::And, e_not(e_method_call(e_this(), "isText", vec![e_var("next")]))), BinOp::And, e_binop(e_var("parent"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_prop(e_var("parent"), "nodeType"), BinOp::StrictEq, e_int(1))), BinOp::And, e_binop(e_prop(e_var("parent"), "nodeName"), BinOp::StrictNotEq, e_str(""))), BinOp::And, e_binop(e_index(e_prop(e_var("parent"), "nodeName"), e_int(0)), BinOp::StrictNotEq, e_str("p")))),
                    s_if(
                        e_binop(e_var("known"), BinOp::And, e_call("isset", vec![e_index(e_this_prop("voidTags"), e_var("lower"))])),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str(">"))),
                            s_if(
                                e_var("afterNewline"),
                                vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
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
                        e_binop(e_prop(e_var("cur"), "firstChild"), BinOp::StrictEq, e_null()),
                        vec![
                            s_if(
                                e_binop(e_var("known"), BinOp::And, e_binop(e_var("lower"), BinOp::StrictEq, e_str("li"))),
                                vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str(">"))),
                                ],
                                vec![],
                                Some(vec![
                                s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("></"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str(">")))),
                            ]),
                            ),
                            s_if(
                                e_var("afterNewline"),
                                vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str(">"))),
                    s_assign("first", e_prop(e_var("cur"), "firstChild")),
                    s_assign("last", e_prop(e_var("cur"), "lastChild")),
                    s_assign("nameOk", e_binop(e_binop(e_prop(e_var("cur"), "nodeName"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_index(e_prop(e_var("cur"), "nodeName"), e_int(0)), BinOp::StrictNotEq, e_str("p")))),
                    s_assign("blockish", e_binop(e_binop(e_this_prop("format"), BinOp::And, e_var("known")), BinOp::And, e_call("isset", vec![e_index(e_this_prop("block"), e_var("lower"))]))),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("blockish"), BinOp::And, e_not(e_method_call(e_this(), "isText", vec![e_var("first")]))), BinOp::And, e_binop(e_var("first"), BinOp::StrictNotEq, e_var("last"))), BinOp::And, e_var("nameOk")),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("child", e_var("first")),
                    s_while(e_binop(e_var("child"), BinOp::StrictNotEq, e_null()), vec![
                        s_expr(e_method_call(e_this(), "node", vec![e_var("child")])),
                        s_assign("child", e_prop(e_var("child"), "nextSibling")),
                    ]),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("blockish"), BinOp::And, e_not(e_method_call(e_this(), "isText", vec![e_var("last")]))), BinOp::And, e_binop(e_var("first"), BinOp::StrictNotEq, e_var("last"))), BinOp::And, e_var("nameOk")),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("</"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str(">")))),
                    s_if(
                        e_var("afterNewline"),
                        vec![
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .build()
}

/// `__ElephcDomState` — transcribed from the PHP form.
fn decl_class_elephcdomstate() -> Stmt {
    class("__ElephcDomState")
        .final_()
        .static_prop("internalErrors", TypeExpr::Bool, Some(e_bool(false)))
        .static_prop("documents", TypeExpr::Int, Some(e_int(0)))
        .build()
}

/// `libxml_use_internal_errors` — transcribed from the PHP form.
fn decl_fn_libxml_use_internal_errors() -> Stmt {
    function("libxml_use_internal_errors")
        .param_default("use_errors", t_nullable(TypeExpr::Bool), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("previous", e_static_prop("__ElephcDomState", "internalErrors")),
            s_if(
                e_binop(e_var("use_errors"), BinOp::StrictNotEq, e_null()),
                vec![
                    s_static_prop_assign("__ElephcDomState", "internalErrors", e_var("use_errors")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("previous")),
        ])
        .build()
}

/// `libxml_clear_errors` — transcribed from the PHP form.
fn decl_fn_libxml_clear_errors() -> Stmt {
    function("libxml_clear_errors")
        .returns(TypeExpr::Void)
        .build()
}

/// `LibXMLError` — transcribed from the PHP form.
fn decl_class_libxmlerror() -> Stmt {
    class("LibXMLError")
        .prop("level", TypeExpr::Int, None)
        .prop("code", TypeExpr::Int, None)
        .prop("column", TypeExpr::Int, None)
        .prop("message", TypeExpr::Str, None)
        .prop("file", TypeExpr::Str, None)
        .prop("line", TypeExpr::Int, None)
        .build()
}

/// `libxml_get_errors` — transcribed from the PHP form.
fn decl_fn_libxml_get_errors() -> Stmt {
    function("libxml_get_errors")
        .returns(t_array())
        .body(vec![
            s_assign("errors", e_array(vec![])),
            s_if(
                e_binop(e_call("count", vec![e_var("errors")]), BinOp::Gt, e_int(0)),
                vec![
                    s_array_push("errors", e_new("LibXMLError", vec![])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("errors")),
        ])
        .build()
}

/// `libxml_disable_entity_loader` — transcribed from the PHP form.
fn decl_fn_libxml_disable_entity_loader() -> Stmt {
    function("libxml_disable_entity_loader")
        .param_default("disable", TypeExpr::Bool, e_bool(true))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("_unused", e_var("disable")),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn dom_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_const_libxml_version(),
            decl_const_libxml_dotted_version(),
            decl_const_libxml_loaded_version(),
            decl_const_libxml_recover(),
            decl_const_libxml_noent(),
            decl_const_libxml_dtdload(),
            decl_const_libxml_dtdattr(),
            decl_const_libxml_dtdvalid(),
            decl_const_libxml_noerror(),
            decl_const_libxml_nowarning(),
            decl_const_libxml_pedantic(),
            decl_const_libxml_noblanks(),
            decl_const_libxml_xinclude(),
            decl_const_libxml_nsclean(),
            decl_const_libxml_nocdata(),
            decl_const_libxml_noxmldecl(),
            decl_const_libxml_parsehuge(),
            decl_const_libxml_biglines(),
            decl_const_libxml_noemptytag(),
            decl_const_libxml_schema_create(),
            decl_const_libxml_html_noimplied(),
            decl_const_libxml_html_nodefdtd(),
            decl_const_xml_element_node(),
            decl_const_xml_attribute_node(),
            decl_const_xml_text_node(),
            decl_const_xml_cdata_section_node(),
            decl_const_xml_entity_ref_node(),
            decl_const_xml_entity_node(),
            decl_const_xml_pi_node(),
            decl_const_xml_comment_node(),
            decl_const_xml_document_node(),
            decl_const_xml_document_type_node(),
            decl_const_xml_document_frag_node(),
            decl_const_xml_notation_node(),
            decl_const_xml_html_document_node(),
            decl_const_xml_dtd_node(),
            decl_const_xml_element_decl_node(),
            decl_const_xml_attribute_decl_node(),
            decl_const_xml_entity_decl_node(),
            decl_const_xml_namespace_decl_node(),
            decl_const_xml_local_namespace(),
            decl_const_xml_attribute_cdata(),
            decl_const_xml_attribute_id(),
            decl_const_xml_attribute_idref(),
            decl_const_xml_attribute_idrefs(),
            decl_const_xml_attribute_entity(),
            decl_const_xml_attribute_nmtoken(),
            decl_const_xml_attribute_nmtokens(),
            decl_const_xml_attribute_enumeration(),
            decl_const_xml_attribute_notation(),
            decl_const_dom_php_err(),
            decl_const_dom_index_size_err(),
            decl_const_domstring_size_err(),
            decl_const_dom_hierarchy_request_err(),
            decl_const_dom_wrong_document_err(),
            decl_const_dom_invalid_character_err(),
            decl_const_dom_no_data_allowed_err(),
            decl_const_dom_no_modification_allowed_err(),
            decl_const_dom_not_found_err(),
            decl_const_dom_not_supported_err(),
            decl_const_dom_inuse_attribute_err(),
            decl_const_dom_invalid_state_err(),
            decl_const_dom_syntax_err(),
            decl_const_dom_invalid_modification_err(),
            decl_const_dom_namespace_err(),
            decl_const_dom_invalid_access_err(),
            decl_const_dom_validation_err(),
            decl_class_domexception(),
            decl_class_domnodelist(),
            decl_class_domnamednodemap(),
            decl_class_domnode(),
            decl_class_domcharacterdata(),
            decl_class_domtext(),
            decl_class_domcomment(),
            decl_class_domcdatasection(),
            decl_class_domprocessinginstruction(),
            decl_class_domattr(),
            decl_class_domelement(),
            decl_class_domdocumentfragment(),
            decl_class_domdocumenttype(),
            decl_class_domdocument(),
            decl_class_elephcdom(),
            decl_class_elephcdomwriter(),
            decl_class_elephcxmlparser(),
            decl_class_elephchtmltables(),
            decl_class_elephchtmlparser(),
            decl_class_elephcdomhtmlwriter(),
            decl_class_elephcdomstate(),
            decl_fn_libxml_use_internal_errors(),
            decl_fn_libxml_clear_errors(),
            decl_class_libxmlerror(),
            decl_fn_libxml_get_errors(),
            decl_fn_libxml_disable_entity_loader(),
        ]
    })
}
