//! Purpose:
//! The libxml2-backed legacy DOM classes (`DOMNode`, `DOMDocument`, `DOMElement`, `DOMAttr`,
//! character data, fragments, `DOMNodeList`, `DOMNamedNodeMap`, `DOMException`) built as AST
//! over the `elephc_xml_dom_*` entry points of the native engine.
//!
//! Called from:
//! - `crate::dom_html_prelude::declarations` once the legacy surface replaces the termwind
//!   walker (bead elephc-8q7.10).
//!
//! Key details:
//! - TRANSCRIBED from the PHP form in `crate::dom_html_prelude::legacy_php` by
//!   `synthetic_class::transcribe` (`ELEPHC_TRANSCRIBE_SPLIT=declarations`); the oracle test
//!   compares both node by node. Edit the PHP form and re-transcribe, not this file.
//! - One helper per declaration: a single nested builder expression overflows the stack.

use crate::parser::ast::{BinOp, CType, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// `elephc_xml_dom_document_new` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_document_new() -> Stmt {
    extern_fn("elephc_xml_dom_document_new", "elephc_xml")
        .param("version", CType::Str)
        .param("encoding", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_load` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_load() -> Stmt {
    extern_fn("elephc_xml_dom_load", "elephc_xml")
        .param("document", CType::Int)
        .param("data", CType::Str)
        .param("length", CType::Int)
        .param("options", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_load_file` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_load_file() -> Stmt {
    extern_fn("elephc_xml_dom_load_file", "elephc_xml")
        .param("document", CType::Int)
        .param("filename", CType::Str)
        .param("options", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_load_html` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_load_html() -> Stmt {
    extern_fn("elephc_xml_dom_load_html", "elephc_xml")
        .param("document", CType::Int)
        .param("data", CType::Str)
        .param("length", CType::Int)
        .param("filename", CType::Str)
        .param("options", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_save` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_save() -> Stmt {
    extern_fn("elephc_xml_dom_save", "elephc_xml")
        .param("document", CType::Int)
        .param("node", CType::Int)
        .param("format", CType::Int)
        .param("noDecl", CType::Int)
        .param("noEmptyTags", CType::Int)
        .returns(CType::NullableStr)
        .build()
}

/// `elephc_xml_dom_save_html` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_save_html() -> Stmt {
    extern_fn("elephc_xml_dom_save_html", "elephc_xml")
        .param("document", CType::Int)
        .param("node", CType::Int)
        .param("format", CType::Int)
        .returns(CType::NullableStr)
        .build()
}

/// `elephc_xml_dom_type` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_type() -> Stmt {
    extern_fn("elephc_xml_dom_type", "elephc_xml")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_standalone` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_standalone() -> Stmt {
    extern_fn("elephc_xml_dom_standalone", "elephc_xml")
        .param("document", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_node` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_node() -> Stmt {
    extern_fn("elephc_xml_dom_node", "elephc_xml")
        .param("node", CType::Int)
        .param("relation", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_string` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_string() -> Stmt {
    extern_fn("elephc_xml_dom_string", "elephc_xml")
        .param("node", CType::Int)
        .param("field", CType::Int)
        .returns(CType::NullableStr)
        .build()
}

/// `elephc_xml_dom_set_text` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_set_text() -> Stmt {
    extern_fn("elephc_xml_dom_set_text", "elephc_xml")
        .param("node", CType::Int)
        .param("value", CType::Str)
        .param("literal", CType::Int)
        .returns(CType::Void)
        .build()
}

/// `elephc_xml_dom_child` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_child() -> Stmt {
    extern_fn("elephc_xml_dom_child", "elephc_xml")
        .param("node", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_child_count` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_child_count() -> Stmt {
    extern_fn("elephc_xml_dom_child_count", "elephc_xml")
        .param("node", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_attribute() -> Stmt {
    extern_fn("elephc_xml_dom_attribute", "elephc_xml")
        .param("node", CType::Int)
        .param("name", CType::Str)
        .returns(CType::NullableStr)
        .build()
}

/// `elephc_xml_dom_attribute_node` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_attribute_node() -> Stmt {
    extern_fn("elephc_xml_dom_attribute_node", "elephc_xml")
        .param("node", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_set_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_set_attribute() -> Stmt {
    extern_fn("elephc_xml_dom_set_attribute", "elephc_xml")
        .param("node", CType::Int)
        .param("name", CType::Str)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_remove_attribute` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_remove_attribute() -> Stmt {
    extern_fn("elephc_xml_dom_remove_attribute", "elephc_xml")
        .param("node", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_attribute_at` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_attribute_at() -> Stmt {
    extern_fn("elephc_xml_dom_attribute_at", "elephc_xml")
        .param("node", CType::Int)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_create` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_create() -> Stmt {
    extern_fn("elephc_xml_dom_create", "elephc_xml")
        .param("document", CType::Int)
        .param("kind", CType::Int)
        .param("name", CType::Str)
        .param("value", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_insert` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_insert() -> Stmt {
    extern_fn("elephc_xml_dom_insert", "elephc_xml")
        .param("parent", CType::Int)
        .param("child", CType::Int)
        .param("reference", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_remove_child` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_remove_child() -> Stmt {
    extern_fn("elephc_xml_dom_remove_child", "elephc_xml")
        .param("parent", CType::Int)
        .param("child", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_clone` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_clone() -> Stmt {
    extern_fn("elephc_xml_dom_clone", "elephc_xml")
        .param("node", CType::Int)
        .param("deep", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_tag_count` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_tag_count() -> Stmt {
    extern_fn("elephc_xml_dom_tag_count", "elephc_xml")
        .param("node", CType::Int)
        .param("name", CType::Str)
        .returns(CType::Int)
        .build()
}

/// `elephc_xml_dom_tag_item` — transcribed from the PHP form.
fn decl_extern_elephc_xml_dom_tag_item() -> Stmt {
    extern_fn("elephc_xml_dom_tag_item", "elephc_xml")
        .param("node", CType::Int)
        .param("name", CType::Str)
        .param("index", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `DOMException` — transcribed from the PHP form.
fn decl_class_domexception() -> Stmt {
    class("DOMException")
        .final_()
        .extends("Exception")
        .build()
}

/// `__ElephcDOM` — transcribed from the PHP form.
fn decl_class_elephcdom() -> Stmt {
    class("__ElephcDOM")
        .final_()
        .static_prop("objects", t_array(), Some(e_array(vec![])))
        .static_prop("minting", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("wrap")
                .static_()
                .param("handle", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("handle"), BinOp::LtEq, e_int(0)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("isset", vec![e_index(e_static_prop("__ElephcDOM", "objects"), e_var("handle"))]),
                        vec![
                            s_assign("cached", e_index(e_static_prop("__ElephcDOM", "objects"), e_var("handle"))),
                            s_if(
                                e_instance_of(e_var("cached"), "DOMNode"),
                                vec![
                                    s_return(e_var("cached")),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_static_prop_assign("__ElephcDOM", "minting", e_bool(true)),
                    s_assign("object", e_static_call("__ElephcDOM", "mint", vec![e_call("elephc_xml_dom_type", vec![e_var("handle")])])),
                    s_static_prop_assign("__ElephcDOM", "minting", e_bool(false)),
                    s_expr(e_method_call(e_var("object"), "__elephc_bind", vec![e_var("handle")])),
                    s_static_prop_array_assign("__ElephcDOM", "objects", e_var("handle"), e_var("object")),
                    s_return(e_var("object")),
                ]),
        )
        .method(
            method("mint")
                .static_()
                .param("type", TypeExpr::Int)
                .returns(t_class("DOMNode"))
                .body(vec![
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_return(e_new("DOMElement", vec![e_str("")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_return(e_new("DOMAttr", vec![e_str("")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_return(e_new("DOMText", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_return(e_new("DOMCdataSection", vec![e_str("")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(7)),
                        vec![
                            s_return(e_new("DOMProcessingInstruction", vec![e_str("")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(8)),
                        vec![
                            s_return(e_new("DOMComment", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("type"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_var("type"), BinOp::StrictEq, e_int(13))),
                        vec![
                            s_return(e_new("DOMDocument", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(10)),
                        vec![
                            s_return(e_new("DOMDocumentType", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(11)),
                        vec![
                            s_return(e_new("DOMDocumentFragment", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_new("DOMNode", vec![])),
                ]),
        )
        .method(
            method("fail")
                .static_()
                .param("code", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("messages", e_array_assoc(vec![(e_int(1), e_str("Index Size Error")), (e_int(2), e_str("DOM String Size Error")), (e_int(3), e_str("Hierarchy Request Error")), (e_int(4), e_str("Wrong Document Error")), (e_int(5), e_str("Invalid Character Error")), (e_int(6), e_str("No Data Allowed Error")), (e_int(7), e_str("No Modification Allowed Error")), (e_int(8), e_str("Not Found Error")), (e_int(9), e_str("Not Supported Error")), (e_int(10), e_str("Inuse Attribute Error")), (e_int(11), e_str("Invalid State Error")), (e_int(12), e_str("Syntax Error")), (e_int(13), e_str("Invalid Modification Error")), (e_int(14), e_str("Namespace Error")), (e_int(15), e_str("Invalid Access Error")), (e_int(16), e_str("Validation Error"))])),
                    s_throw(e_new("DOMException", vec![e_null_coalesce(e_index(e_var("messages"), e_var("code")), e_str("Invalid State Error")), e_var("code")])),
                ]),
        )
        .method(
            method("tree")
                .static_()
                .param("result", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("result"), BinOp::StrictEq, e_neg(e_int(100))),
                        vec![
                            s_expr(e_call("trigger_error", vec![e_str("Document Fragment is empty"), e_const("E_USER_WARNING")])),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("result"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_static_call("__ElephcDOM", "fail", vec![e_neg(e_var("result"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_var("result")])),
                    s_return(e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_bool(false))),
                ]),
        )
        .method(
            method("omit")
                .static_()
                .param("value", t_mixed())
                .returns(t_mixed())
                .body(vec![
                    s_return(e_ternary(e_call("is_object", vec![e_var("value")]), e_str("(object value omitted)"), e_var("value"))),
                ]),
        )
        .method(
            method("elementChild")
                .static_()
                .param("handle", TypeExpr::Int)
                .param("last", TypeExpr::Bool)
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_var("handle"), e_ternary(e_var("last"), e_int(2), e_int(1))])])),
                    s_while(e_binop(e_instance_of(e_var("node"), "DOMNode"), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMElement"))), vec![
                        s_assign("node", e_ternary(e_var("last"), e_prop(e_var("node"), "previousSibling"), e_prop(e_var("node"), "nextSibling"))),
                    ]),
                    s_return(e_ternary(e_instance_of(e_var("node"), "DOMElement"), e_var("node"), e_null())),
                ]),
        )
        .method(
            method("elementSibling")
                .static_()
                .param("handle", TypeExpr::Int)
                .param("next", TypeExpr::Bool)
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_var("handle"), e_ternary(e_var("next"), e_int(4), e_int(3))])])),
                    s_while(e_binop(e_instance_of(e_var("node"), "DOMNode"), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMElement"))), vec![
                        s_assign("node", e_ternary(e_var("next"), e_prop(e_var("node"), "nextSibling"), e_prop(e_var("node"), "previousSibling"))),
                    ]),
                    s_return(e_ternary(e_instance_of(e_var("node"), "DOMElement"), e_var("node"), e_null())),
                ]),
        )
        .method(
            method("elementCount")
                .static_()
                .param("handle", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("count", e_int(0)),
                    s_assign("node", e_static_call("__ElephcDOM", "elementChild", vec![e_var("handle"), e_bool(false)])),
                    s_while(e_instance_of(e_var("node"), "DOMElement"), vec![
                        s_expr(e_post_inc("count")),
                        s_assign("node", e_prop(e_var("node"), "nextElementSibling")),
                    ]),
                    s_return(e_var("count")),
                ]),
        )
        .method(
            method("created")
                .static_()
                .param("result", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("result"), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_static_call("__ElephcDOM", "fail", vec![e_neg(e_var("result"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("__ElephcDOM", "wrap", vec![e_var("result")])),
                ]),
        )
        .method(
            method("document")
                .static_()
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_xml_dom_document_new", vec![e_str("1.0"), e_str("")])),
                ]),
        )
        .method(
            method("argument")
                .static_()
                .param("value", t_mixed())
                .param("function", TypeExpr::Str)
                .param("position", TypeExpr::Int)
                .param("name", TypeExpr::Str)
                .param("nullable", TypeExpr::Bool)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_instance_of(e_var("value"), "DOMNode"),
                        vec![
                            s_return(e_method_call(e_var("value"), "__elephc_handle", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("nullable"), BinOp::And, e_binop(e_var("value"), BinOp::StrictEq, e_null())),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument #")), BinOp::Concat, e_var("position")), BinOp::Concat, e_str(" ($")), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(") must be of type ")), BinOp::Concat, e_ternary(e_var("nullable"), e_str("?"), e_str(""))), BinOp::Concat, e_str("DOMNode, ")), BinOp::Concat, e_static_call("__ElephcDOM", "given", vec![e_var("value")])), BinOp::Concat, e_str(" given"))])),
                ]),
        )
        .method(
            method("given")
                .static_()
                .param("value", t_mixed())
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_call("is_object", vec![e_var("value")]),
                        vec![
                            s_return(e_call("get_class", vec![e_var("value")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("value"), BinOp::StrictEq, e_bool(true)),
                        vec![
                            s_return(e_str("true")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("value"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_str("false")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("kind", e_call("gettype", vec![e_var("value")])),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_str("integer")),
                        vec![
                            s_return(e_str("int")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_str("double")),
                        vec![
                            s_return(e_str("float")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_str("NULL")),
                        vec![
                            s_return(e_str("null")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("kind")),
                ]),
        )
        .method(
            method("nodes")
                .static_()
                .param("context", t_class("DOMNode"))
                .param("nodes", t_array())
                .returns(t_array())
                .body(vec![
                    s_assign("document", e_ternary(e_instance_of(e_var("context"), "DOMDocument"), e_var("context"), e_prop(e_var("context"), "ownerDocument"))),
                    s_assign("converted", e_array(vec![])),
                    s_foreach(e_var("nodes"), None, "node", vec![
                        s_if(
                            e_instance_of(e_var("node"), "DOMNode"),
                            vec![
                                s_array_push("converted", e_var("node")),
                            ],
                            vec![
                            (e_instance_of(e_var("document"), "DOMDocument"), vec![
                                s_array_push("converted", e_method_call(e_var("document"), "createTextNode", vec![e_cast(CastType::String, e_var("node"))])),
                            ]),
                        ],
                            None,
                        ),
                    ]),
                    s_return(e_var("converted")),
                ]),
        )
        .build()
}

/// `DOMNode` — transcribed from the PHP form.
fn decl_class_domnode() -> Stmt {
    class("DOMNode")
        .private_prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .prop("nodeName", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("nodeValue", t_nullable(TypeExpr::Str), None)
        .hooked(true, true, false)
        .prop("nodeType", TypeExpr::Int, None)
        .hooked(true, false, false)
        .prop("parentNode", t_mixed(), None)
        .hooked(true, false, false)
        .prop("parentElement", t_mixed(), None)
        .hooked(true, false, false)
        .prop("childNodes", t_mixed(), None)
        .hooked(true, false, false)
        .prop("firstChild", t_mixed(), None)
        .hooked(true, false, false)
        .prop("lastChild", t_mixed(), None)
        .hooked(true, false, false)
        .prop("previousSibling", t_mixed(), None)
        .hooked(true, false, false)
        .prop("nextSibling", t_mixed(), None)
        .hooked(true, false, false)
        .prop("attributes", t_mixed(), None)
        .hooked(true, false, false)
        .prop("ownerDocument", t_mixed(), None)
        .hooked(true, false, false)
        .prop("namespaceURI", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("prefix", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("localName", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("textContent", TypeExpr::Str, None)
        .hooked(true, true, false)
        .prop("isConnected", TypeExpr::Bool, None)
        .hooked(true, false, false)
        .prop("baseURI", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .method(
            method("__propget_nodeName")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(0)]))),
                ]),
        )
        .method(
            method("__propget_nodeValue")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(1)])),
                ]),
        )
        .method(
            method("__propset_nodeValue")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_xml_dom_set_text", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_cast(CastType::String, e_var("value")), e_int(0)])),
                ]),
        )
        .method(
            method("__propget_nodeType")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_xml_dom_type", vec![e_method_call(e_this(), "__elephc_state", vec![])])),
                ]),
        )
        .method(
            method("__propget_parentNode")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(0)])])),
                ]),
        )
        .method(
            method("__propget_parentElement")
                .returns(t_mixed())
                .body(vec![
                    s_assign("parent", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(0)])])),
                    s_return(e_ternary(e_instance_of(e_var("parent"), "DOMElement"), e_var("parent"), e_null())),
                ]),
        )
        .method(
            method("__propget_childNodes")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_static_call("DOMNodeList", "__elephc_list", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_str("")])),
                ]),
        )
        .method(
            method("__propget_firstChild")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(1)])])),
                ]),
        )
        .method(
            method("__propget_lastChild")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(2)])])),
                ]),
        )
        .method(
            method("__propget_previousSibling")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(3)])])),
                ]),
        )
        .method(
            method("__propget_nextSibling")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(4)])])),
                ]),
        )
        .method(
            method("__propget_attributes")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_ternary(e_binop(e_call("elephc_xml_dom_type", vec![e_method_call(e_this(), "__elephc_state", vec![])]), BinOp::StrictEq, e_int(1)), e_static_call("DOMNamedNodeMap", "__elephc_map", vec![e_this_prop("__elephc_handle")]), e_null())),
                ]),
        )
        .method(
            method("__propget_ownerDocument")
                .returns(t_mixed())
                .body(vec![
                    s_assign("owner", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(5)])])),
                    s_return(e_ternary(e_instance_of(e_var("owner"), "DOMDocument"), e_var("owner"), e_null())),
                ]),
        )
        .method(
            method("__propget_namespaceURI")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(5)])),
                ]),
        )
        .method(
            method("__propget_prefix")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(4)]))),
                ]),
        )
        .method(
            method("__propget_localName")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(3)])),
                ]),
        )
        .method(
            method("__propget_textContent")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(2)]))),
                ]),
        )
        .method(
            method("__propset_textContent")
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_xml_dom_set_text", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_cast(CastType::String, e_var("value")), e_int(1)])),
                ]),
        )
        .method(
            method("__propget_isConnected")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("top", e_method_call(e_this(), "__elephc_state", vec![])),
                    s_assign("parent", e_call("elephc_xml_dom_node", vec![e_var("top"), e_int(0)])),
                    s_while(e_binop(e_var("parent"), BinOp::Gt, e_int(0)), vec![
                        s_assign("top", e_var("parent")),
                        s_assign("parent", e_call("elephc_xml_dom_node", vec![e_var("top"), e_int(0)])),
                    ]),
                    s_assign("type", e_call("elephc_xml_dom_type", vec![e_var("top")])),
                    s_return(e_binop(e_binop(e_var("type"), BinOp::StrictEq, e_int(9)), BinOp::Or, e_binop(e_var("type"), BinOp::StrictEq, e_int(13)))),
                ]),
        )
        .method(
            method("__propget_baseURI")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(9)])),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_node_debug", vec![])),
                ]),
        )
        .method(
            method("__elephc_node_debug")
                .returns(t_array())
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("nodeName"), e_this_prop("nodeName")), (e_str("nodeValue"), e_this_prop("nodeValue")), (e_str("nodeType"), e_this_prop("nodeType")), (e_str("parentNode"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("parentNode")])), (e_str("parentElement"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("parentElement")])), (e_str("childNodes"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("childNodes")])), (e_str("firstChild"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("firstChild")])), (e_str("lastChild"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("lastChild")])), (e_str("previousSibling"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("previousSibling")])), (e_str("nextSibling"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("nextSibling")])), (e_str("attributes"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("attributes")])), (e_str("isConnected"), e_this_prop("isConnected")), (e_str("ownerDocument"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("ownerDocument")])), (e_str("namespaceURI"), e_this_prop("namespaceURI")), (e_str("prefix"), e_this_prop("prefix")), (e_str("localName"), e_this_prop("localName")), (e_str("baseURI"), e_this_prop("baseURI")), (e_str("textContent"), e_this_prop("textContent"))])),
                ]),
        )
        .method(
            method("__elephc_bind")
                .param("handle", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "__elephc_handle", e_var("handle")),
                ]),
        )
        .method(
            method("__elephc_handle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("__elephc_handle"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_binop(e_str("Couldn't fetch "), BinOp::Concat, e_call("get_class", vec![e_this()]))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("__elephc_handle")),
                ]),
        )
        .method(
            method("__elephc_state")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("__elephc_handle"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid State Error"), e_int(11)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_this_prop("__elephc_handle")),
                ]),
        )
        .method(
            method("__elephc_raw")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("__elephc_handle")),
                ]),
        )
        .method(
            method("appendChild")
                .param_untyped("node")
                .body(vec![
                    s_assign("child", e_static_call("__ElephcDOM", "argument", vec![e_var("node"), e_str("DOMNode::appendChild"), e_int(1), e_str("node"), e_bool(false)])),
                    s_return(e_static_call("__ElephcDOM", "tree", vec![e_call("elephc_xml_dom_insert", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("child"), e_int(0)])])),
                ]),
        )
        .method(
            method("insertBefore")
                .param_untyped("node")
                .param_untyped_default("child", e_null())
                .body(vec![
                    s_assign("inserted", e_static_call("__ElephcDOM", "argument", vec![e_var("node"), e_str("DOMNode::insertBefore"), e_int(1), e_str("node"), e_bool(false)])),
                    s_assign("reference", e_static_call("__ElephcDOM", "argument", vec![e_var("child"), e_str("DOMNode::insertBefore"), e_int(2), e_str("child"), e_bool(true)])),
                    s_return(e_static_call("__ElephcDOM", "tree", vec![e_call("elephc_xml_dom_insert", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("inserted"), e_var("reference")])])),
                ]),
        )
        .method(
            method("removeChild")
                .param_untyped("child")
                .body(vec![
                    s_assign("removed", e_static_call("__ElephcDOM", "argument", vec![e_var("child"), e_str("DOMNode::removeChild"), e_int(1), e_str("child"), e_bool(false)])),
                    s_return(e_static_call("__ElephcDOM", "tree", vec![e_call("elephc_xml_dom_remove_child", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("removed")])])),
                ]),
        )
        .method(
            method("replaceChild")
                .param_untyped("node")
                .param_untyped("child")
                .body(vec![
                    s_assign("inserted", e_static_call("__ElephcDOM", "argument", vec![e_var("node"), e_str("DOMNode::replaceChild"), e_int(1), e_str("node"), e_bool(false)])),
                    s_assign("replaced", e_static_call("__ElephcDOM", "argument", vec![e_var("child"), e_str("DOMNode::replaceChild"), e_int(2), e_str("child"), e_bool(false)])),
                    s_if(
                        e_binop(e_static_call("__ElephcDOM", "tree", vec![e_call("elephc_xml_dom_insert", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("inserted"), e_var("replaced")])]), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("__ElephcDOM", "tree", vec![e_call("elephc_xml_dom_remove_child", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("replaced")])])),
                ]),
        )
        .method(
            method("cloneNode")
                .param_default("deep", TypeExpr::Bool, e_bool(false))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_clone", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_ternary(e_var("deep"), e_int(1), e_int(0))])])),
                ]),
        )
        .method(
            method("hasChildNodes")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_int(1)]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("hasAttributes")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_call("elephc_xml_dom_type", vec![e_method_call(e_this(), "__elephc_handle", vec![])]), BinOp::StrictEq, e_int(1)), BinOp::And, e_binop(e_call("elephc_xml_dom_attribute_at", vec![e_this_prop("__elephc_handle"), e_int(0)]), BinOp::StrictNotEq, e_int(0)))),
                ]),
        )
        .method(
            method("isSameNode")
                .param_untyped("otherNode")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_static_call("__ElephcDOM", "argument", vec![e_var("otherNode"), e_str("DOMNode::isSameNode"), e_int(1), e_str("otherNode"), e_bool(false)]), BinOp::StrictEq, e_method_call(e_this(), "__elephc_handle", vec![]))),
                ]),
        )
        .build()
}

/// `DOMNodeList` — transcribed from the PHP form.
fn decl_class_domnodelist() -> Stmt {
    class("DOMNodeList")
        .implements("IteratorAggregate")
        .implements("Countable")
        .private_prop("__elephc_root", TypeExpr::Int, Some(e_int(0)))
        .private_prop("__elephc_tag", TypeExpr::Str, Some(e_str("")))
        .prop("length", TypeExpr::Int, None)
        .hooked(true, false, false)
        .method(
            method("__propget_length")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_ternary(e_binop(e_this_prop("__elephc_tag"), BinOp::StrictEq, e_str("")), e_call("elephc_xml_dom_child_count", vec![e_this_prop("__elephc_root")]), e_call("elephc_xml_dom_tag_count", vec![e_this_prop("__elephc_root"), e_this_prop("__elephc_tag")]))),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("length"), e_this_prop("length"))])),
                ]),
        )
        .method(
            method("__elephc_list")
                .static_()
                .param("root", TypeExpr::Int)
                .param("tag", TypeExpr::Str)
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_assign("list", e_new("DOMNodeList", vec![])),
                    s_prop_assign(e_var("list"), "__elephc_root", e_var("root")),
                    s_prop_assign(e_var("list"), "__elephc_tag", e_var("tag")),
                    s_return(e_var("list")),
                ]),
        )
        .method(
            method("item")
                .param("index", TypeExpr::Int)
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
                        e_binop(e_this_prop("__elephc_tag"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return(e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_child", vec![e_this_prop("__elephc_root"), e_var("index")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_tag_item", vec![e_this_prop("__elephc_root"), e_this_prop("__elephc_tag"), e_var("index")])])),
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
                .returns(t_class("Iterator"))
                .body(vec![
                    s_assign("items", e_array(vec![])),
                    s_assign("count", e_this_prop("length")),
                    s_for(Some(s_assign("index", e_int(0))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("index"))), vec![
                        s_array_push("items", e_method_call(e_this(), "item", vec![e_var("index")])),
                    ]),
                    s_return(e_new("ArrayIterator", vec![e_var("items")])),
                ]),
        )
        .build()
}

/// `DOMNamedNodeMap` — transcribed from the PHP form.
fn decl_class_domnamednodemap() -> Stmt {
    class("DOMNamedNodeMap")
        .implements("IteratorAggregate")
        .implements("Countable")
        .private_prop("__elephc_element", TypeExpr::Int, Some(e_int(0)))
        .prop("length", TypeExpr::Int, None)
        .hooked(true, false, false)
        .method(
            method("__propget_length")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("count", e_int(0)),
                    s_while(e_binop(e_call("elephc_xml_dom_attribute_at", vec![e_this_prop("__elephc_element"), e_var("count")]), BinOp::StrictNotEq, e_int(0)), vec![
                        s_expr(e_post_inc("count")),
                    ]),
                    s_return(e_var("count")),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("length"), e_this_prop("length"))])),
                ]),
        )
        .method(
            method("__elephc_map")
                .static_()
                .param("element", TypeExpr::Int)
                .returns(t_class("DOMNamedNodeMap"))
                .body(vec![
                    s_assign("map", e_new("DOMNamedNodeMap", vec![])),
                    s_prop_assign(e_var("map"), "__elephc_element", e_var("element")),
                    s_return(e_var("map")),
                ]),
        )
        .method(
            method("getNamedItem")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_attribute_node", vec![e_this_prop("__elephc_element"), e_var("qualifiedName")])])),
                    s_return(e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_null())),
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
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_attribute_at", vec![e_this_prop("__elephc_element"), e_var("index")])])),
                    s_return(e_ternary(e_instance_of(e_var("node"), "DOMNode"), e_var("node"), e_null())),
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
                .returns(t_class("Iterator"))
                .body(vec![
                    s_assign("items", e_array(vec![])),
                    s_assign("count", e_this_prop("length")),
                    s_for(Some(s_assign("index", e_int(0))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("index"))), vec![
                        s_assign("node", e_method_call(e_this(), "item", vec![e_var("index")])),
                        s_array_assign("items", e_prop(e_var("node"), "nodeName"), e_var("node")),
                    ]),
                    s_return(e_new("ArrayIterator", vec![e_var("items")])),
                ]),
        )
        .build()
}

/// `DOMDocumentFragment` — transcribed from the PHP form.
fn decl_class_domdocumentfragment() -> Stmt {
    class("DOMDocumentFragment")
        .extends("DOMNode")
        .prop("firstElementChild", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("lastElementChild", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("childElementCount", TypeExpr::Int, None)
        .hooked(true, false, false)
        .method(
            method("__propget_firstElementChild")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementChild", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(false)])),
                ]),
        )
        .method(
            method("__propget_lastElementChild")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementChild", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(true)])),
                ]),
        )
        .method(
            method("__propget_childElementCount")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementCount", vec![e_method_call(e_this(), "__elephc_state", vec![])])),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_binop(e_array_assoc(vec![(e_str("firstElementChild"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("firstElementChild")])), (e_str("lastElementChild"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("lastElementChild")])), (e_str("childElementCount"), e_this_prop("childElementCount"))]), BinOp::Add, e_method_call(e_this(), "__elephc_node_debug", vec![]))),
                ]),
        )
        .method(
            method("__construct")
                .body(vec![
                    s_if(
                        e_not(e_static_prop("__ElephcDOM", "minting")),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_bind", vec![e_call("elephc_xml_dom_create", vec![e_static_call("__ElephcDOM", "document", vec![]), e_int(11), e_str(""), e_str("")])])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .build()
}

/// `DOMCharacterData` — transcribed from the PHP form.
fn decl_class_domcharacterdata() -> Stmt {
    class("DOMCharacterData")
        .extends("DOMNode")
        .prop("data", TypeExpr::Str, None)
        .hooked(true, true, false)
        .prop("length", TypeExpr::Int, None)
        .hooked(true, false, false)
        .prop("previousElementSibling", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("nextElementSibling", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .method(
            method("__propget_data")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(1)]))),
                ]),
        )
        .method(
            method("__propset_data")
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_xml_dom_set_text", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_var("value"), e_int(0)])),
                ]),
        )
        .method(
            method("__propget_length")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("mb_strlen", vec![e_cast(CastType::String, e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(1)])), e_str("UTF-8")])),
                ]),
        )
        .method(
            method("__propget_previousElementSibling")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementSibling", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(false)])),
                ]),
        )
        .method(
            method("__propget_nextElementSibling")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementSibling", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(true)])),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_binop(e_method_call(e_this(), "__elephc_character_debug", vec![]), BinOp::Add, e_method_call(e_this(), "__elephc_node_debug", vec![]))),
                ]),
        )
        .method(
            method("__elephc_character_debug")
                .returns(t_array())
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("data"), e_this_prop("data")), (e_str("length"), e_this_prop("length")), (e_str("previousElementSibling"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("previousElementSibling")])), (e_str("nextElementSibling"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("nextElementSibling")]))])),
                ]),
        )
        .method(
            method("appendData")
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "data", e_binop(e_this_prop("data"), BinOp::Concat, e_var("data"))),
                    s_return(e_bool(true)),
                ]),
        )
        .build()
}

/// `DOMText` — transcribed from the PHP form.
fn decl_class_domtext() -> Stmt {
    class("DOMText")
        .extends("DOMCharacterData")
        .prop("wholeText", TypeExpr::Str, None)
        .hooked(true, false, false)
        .method(
            method("__propget_wholeText")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("data")),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_binop(e_array_assoc(vec![(e_str("wholeText"), e_this_prop("wholeText"))]), BinOp::Add, e_parent_call("__elephcDebugInfo", vec![]))),
                ]),
        )
        .method(
            method("__construct")
                .param_default("data", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_if(
                        e_not(e_static_prop("__ElephcDOM", "minting")),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_bind", vec![e_call("elephc_xml_dom_create", vec![e_static_call("__ElephcDOM", "document", vec![]), e_int(3), e_str(""), e_var("data")])])),
                        ],
                        vec![],
                        None,
                    ),
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
                .param("data", TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_not(e_static_prop("__ElephcDOM", "minting")),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_bind", vec![e_call("elephc_xml_dom_create", vec![e_static_call("__ElephcDOM", "document", vec![]), e_int(4), e_str(""), e_var("data")])])),
                        ],
                        vec![],
                        None,
                    ),
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
                .param_default("data", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_if(
                        e_not(e_static_prop("__ElephcDOM", "minting")),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_bind", vec![e_call("elephc_xml_dom_create", vec![e_static_call("__ElephcDOM", "document", vec![]), e_int(8), e_str(""), e_var("data")])])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .build()
}

/// `DOMProcessingInstruction` — transcribed from the PHP form.
fn decl_class_domprocessinginstruction() -> Stmt {
    class("DOMProcessingInstruction")
        .extends("DOMNode")
        .prop("target", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("data", TypeExpr::Str, None)
        .hooked(true, true, false)
        .method(
            method("__propget_target")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("nodeName")),
                ]),
        )
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
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_binop(e_array_assoc(vec![(e_str("target"), e_this_prop("target")), (e_str("data"), e_this_prop("data"))]), BinOp::Add, e_method_call(e_this(), "__elephc_node_debug", vec![]))),
                ]),
        )
        .method(
            method("__construct")
                .param("name", TypeExpr::Str)
                .param_default("value", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_if(
                        e_not(e_static_prop("__ElephcDOM", "minting")),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_bind", vec![e_call("elephc_xml_dom_create", vec![e_static_call("__ElephcDOM", "document", vec![]), e_int(7), e_var("name"), e_var("value")])])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .build()
}

/// `DOMAttr` — transcribed from the PHP form.
fn decl_class_domattr() -> Stmt {
    class("DOMAttr")
        .extends("DOMNode")
        .prop("name", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("specified", TypeExpr::Bool, None)
        .hooked(true, false, false)
        .prop("value", TypeExpr::Str, None)
        .hooked(true, true, false)
        .prop("ownerElement", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("schemaTypeInfo", t_mixed(), None)
        .hooked(true, false, false)
        .method(
            method("__propget_name")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("nodeName")),
                ]),
        )
        .method(
            method("__propget_specified")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__propget_value")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_this_prop("nodeValue"))),
                ]),
        )
        .method(
            method("__propset_value")
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "nodeValue", e_var("value")),
                ]),
        )
        .method(
            method("__propget_ownerElement")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_assign("parent", e_this_prop("parentElement")),
                    s_return(e_ternary(e_instance_of(e_var("parent"), "DOMElement"), e_var("parent"), e_null())),
                ]),
        )
        .method(
            method("__propget_schemaTypeInfo")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_null()),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_binop(e_array_assoc(vec![(e_str("name"), e_this_prop("name")), (e_str("specified"), e_this_prop("specified")), (e_str("value"), e_this_prop("value")), (e_str("ownerElement"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("ownerElement")])), (e_str("schemaTypeInfo"), e_this_prop("schemaTypeInfo"))]), BinOp::Add, e_method_call(e_this(), "__elephc_node_debug", vec![]))),
                ]),
        )
        .method(
            method("__construct")
                .param("name", TypeExpr::Str)
                .param_default("value", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_if(
                        e_not(e_static_prop("__ElephcDOM", "minting")),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_bind", vec![e_call("elephc_xml_dom_create", vec![e_static_call("__ElephcDOM", "document", vec![]), e_int(2), e_var("name"), e_str("")])])),
                            s_prop_assign(e_this(), "nodeValue", e_var("value")),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .build()
}

/// `DOMElement` — transcribed from the PHP form.
fn decl_class_domelement() -> Stmt {
    class("DOMElement")
        .extends("DOMNode")
        .prop("tagName", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("className", TypeExpr::Str, None)
        .hooked(true, true, false)
        .prop("id", TypeExpr::Str, None)
        .hooked(true, true, false)
        .prop("schemaTypeInfo", t_mixed(), None)
        .hooked(true, false, false)
        .prop("firstElementChild", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("lastElementChild", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("childElementCount", TypeExpr::Int, None)
        .hooked(true, false, false)
        .prop("previousElementSibling", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("nextElementSibling", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .method(
            method("__propget_tagName")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("nodeName")),
                ]),
        )
        .method(
            method("__propget_className")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_attribute", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_str("class")]))),
                ]),
        )
        .method(
            method("__propset_className")
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_xml_dom_set_attribute", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_str("class"), e_var("value")])),
                ]),
        )
        .method(
            method("__propget_id")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_attribute", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_str("id")]))),
                ]),
        )
        .method(
            method("__propset_id")
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("elephc_xml_dom_set_attribute", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_str("id"), e_var("value")])),
                ]),
        )
        .method(
            method("__propget_schemaTypeInfo")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_null()),
                ]),
        )
        .method(
            method("__propget_firstElementChild")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementChild", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(false)])),
                ]),
        )
        .method(
            method("__propget_lastElementChild")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementChild", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(true)])),
                ]),
        )
        .method(
            method("__propget_childElementCount")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementCount", vec![e_method_call(e_this(), "__elephc_state", vec![])])),
                ]),
        )
        .method(
            method("__propget_previousElementSibling")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementSibling", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(false)])),
                ]),
        )
        .method(
            method("__propget_nextElementSibling")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementSibling", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(true)])),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_binop(e_array_assoc(vec![(e_str("tagName"), e_this_prop("tagName")), (e_str("className"), e_this_prop("className")), (e_str("id"), e_this_prop("id")), (e_str("schemaTypeInfo"), e_this_prop("schemaTypeInfo")), (e_str("firstElementChild"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("firstElementChild")])), (e_str("lastElementChild"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("lastElementChild")])), (e_str("childElementCount"), e_this_prop("childElementCount")), (e_str("previousElementSibling"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("previousElementSibling")])), (e_str("nextElementSibling"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("nextElementSibling")]))]), BinOp::Add, e_method_call(e_this(), "__elephc_node_debug", vec![]))),
                ]),
        )
        .method(
            method("__construct")
                .param("qualifiedName", TypeExpr::Str)
                .param_default("value", t_nullable(TypeExpr::Str), e_null())
                .param_default("namespace", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_assign("_unused", e_var("namespace")),
                    s_if(
                        e_not(e_static_prop("__ElephcDOM", "minting")),
                        vec![
                            s_assign("_created", e_call("elephc_xml_dom_create", vec![e_static_call("__ElephcDOM", "document", vec![]), e_int(1), e_var("qualifiedName"), e_cast(CastType::String, e_var("value"))])),
                            s_if(
                                e_binop(e_var("_created"), BinOp::Lt, e_int(0)),
                                vec![
                                    s_expr(e_static_call("__ElephcDOM", "fail", vec![e_neg(e_var("_created"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "__elephc_bind", vec![e_var("_created")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("getAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_attribute", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("qualifiedName")]))),
                ]),
        )
        .method(
            method("hasAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_xml_dom_attribute", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("qualifiedName")]), BinOp::StrictNotEq, e_null())),
                ]),
        )
        .method(
            method("setAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_var("qualifiedName"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("DOMElement::setAttribute(): Argument #1 ($qualifiedName) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_static_call("__ElephcDOM", "created", vec![e_call("elephc_xml_dom_set_attribute", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("qualifiedName"), e_var("value")])])),
                ]),
        )
        .method(
            method("removeAttribute")
                .param("qualifiedName", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_xml_dom_remove_attribute", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("qualifiedName")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("getAttributeNode")
                .param("qualifiedName", TypeExpr::Str)
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_attribute_node", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("qualifiedName")])])),
                    s_return(e_ternary(e_binop(e_var("node"), BinOp::StrictEq, e_null()), e_bool(false), e_var("node"))),
                ]),
        )
        .method(
            method("getElementsByTagName")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_return(e_static_call("DOMNodeList", "__elephc_list", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("qualifiedName")])),
                ]),
        )
        .method(
            method("append")
                .variadic("nodes", None)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_foreach(e_static_call("__ElephcDOM", "nodes", vec![e_this(), e_var("nodes")]), None, "node", vec![
                        s_expr(e_method_call(e_this(), "appendChild", vec![e_var("node")])),
                    ]),
                ]),
        )
        .method(
            method("prepend")
                .variadic("nodes", None)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("current", e_this_prop("firstChild")),
                    s_assign("first", e_ternary(e_instance_of(e_var("current"), "DOMNode"), e_var("current"), e_null())),
                    s_foreach(e_static_call("__ElephcDOM", "nodes", vec![e_this(), e_var("nodes")]), None, "node", vec![
                        s_expr(e_method_call(e_this(), "insertBefore", vec![e_var("node"), e_var("first")])),
                    ]),
                ]),
        )
        .method(
            method("replaceChildren")
                .variadic("nodes", None)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_instance_of(e_this_prop("firstChild"), "DOMNode"), vec![
                        s_expr(e_method_call(e_this(), "removeChild", vec![e_this_prop("firstChild")])),
                    ]),
                    s_expr(e_method_call(e_this(), "append", vec![e_spread(e_var("nodes"))])),
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
            method("before")
                .variadic("nodes", None)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("parent", e_this_prop("parentNode")),
                    s_if(
                        e_instance_of(e_var("parent"), "DOMNode"),
                        vec![
                            s_foreach(e_static_call("__ElephcDOM", "nodes", vec![e_this(), e_var("nodes")]), None, "node", vec![
                                s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_var("node"), e_this()])),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("after")
                .variadic("nodes", None)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("parent", e_this_prop("parentNode")),
                    s_if(
                        e_instance_of(e_var("parent"), "DOMNode"),
                        vec![
                            s_assign("sibling", e_this_prop("nextSibling")),
                            s_assign("next", e_ternary(e_instance_of(e_var("sibling"), "DOMNode"), e_var("sibling"), e_null())),
                            s_foreach(e_static_call("__ElephcDOM", "nodes", vec![e_this(), e_var("nodes")]), None, "node", vec![
                                s_expr(e_method_call(e_var("parent"), "insertBefore", vec![e_var("node"), e_var("next")])),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("replaceWith")
                .variadic("nodes", None)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "before", vec![e_spread(e_var("nodes"))])),
                    s_expr(e_method_call(e_this(), "remove", vec![])),
                ]),
        )
        .build()
}

/// `DOMDocumentType` — transcribed from the PHP form.
fn decl_class_domdocumenttype() -> Stmt {
    class("DOMDocumentType")
        .extends("DOMNode")
        .prop("name", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("entities", t_class("DOMNamedNodeMap"), None)
        .hooked(true, false, false)
        .prop("notations", t_class("DOMNamedNodeMap"), None)
        .hooked(true, false, false)
        .prop("publicId", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("systemId", TypeExpr::Str, None)
        .hooked(true, false, false)
        .prop("internalSubset", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .method(
            method("__propget_name")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_this_prop("nodeName")),
                ]),
        )
        .method(
            method("__propget_entities")
                .returns(t_class("DOMNamedNodeMap"))
                .body(vec![
                    s_assign("_unused", e_method_call(e_this(), "__elephc_state", vec![])),
                    s_return(e_static_call("DOMNamedNodeMap", "__elephc_map", vec![e_int(0)])),
                ]),
        )
        .method(
            method("__propget_notations")
                .returns(t_class("DOMNamedNodeMap"))
                .body(vec![
                    s_assign("_unused", e_method_call(e_this(), "__elephc_state", vec![])),
                    s_return(e_static_call("DOMNamedNodeMap", "__elephc_map", vec![e_int(0)])),
                ]),
        )
        .method(
            method("__propget_publicId")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(10)]))),
                ]),
        )
        .method(
            method("__propget_systemId")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_cast(CastType::String, e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(11)]))),
                ]),
        )
        .method(
            method("__propget_internalSubset")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(12)])),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_binop(e_array_assoc(vec![(e_str("name"), e_this_prop("name")), (e_str("entities"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("entities")])), (e_str("notations"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("notations")])), (e_str("publicId"), e_this_prop("publicId")), (e_str("systemId"), e_this_prop("systemId")), (e_str("internalSubset"), e_this_prop("internalSubset"))]), BinOp::Add, e_method_call(e_this(), "__elephc_node_debug", vec![]))),
                ]),
        )
        .build()
}

/// `DOMImplementation` — transcribed from the PHP form.
fn decl_class_domimplementation() -> Stmt {
    class("DOMImplementation")
        .method(
            method("hasFeature")
                .param("feature", TypeExpr::Str)
                .param("version", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_unused", e_array(vec![e_var("feature"), e_var("version")])),
                    s_return(e_bool(true)),
                ]),
        )
        .build()
}

/// `DOMDocument` — transcribed from the PHP form.
fn decl_class_domdocument() -> Stmt {
    class("DOMDocument")
        .extends("DOMNode")
        .prop("formatOutput", TypeExpr::Bool, Some(e_bool(false)))
        .prop("preserveWhiteSpace", TypeExpr::Bool, Some(e_bool(true)))
        .prop("validateOnParse", TypeExpr::Bool, Some(e_bool(false)))
        .prop("resolveExternals", TypeExpr::Bool, Some(e_bool(false)))
        .prop("recover", TypeExpr::Bool, Some(e_bool(false)))
        .prop("substituteEntities", TypeExpr::Bool, Some(e_bool(false)))
        .prop("strictErrorChecking", TypeExpr::Bool, Some(e_bool(true)))
        .prop("documentElement", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("xmlVersion", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("version", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("xmlEncoding", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("encoding", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("documentURI", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("doctype", t_nullable(t_class("DOMDocumentType")), None)
        .hooked(true, false, false)
        .prop("implementation", t_class("DOMImplementation"), None)
        .hooked(true, false, false)
        .prop("actualEncoding", t_nullable(TypeExpr::Str), None)
        .hooked(true, false, false)
        .prop("standalone", TypeExpr::Bool, None)
        .hooked(true, false, false)
        .prop("xmlStandalone", TypeExpr::Bool, None)
        .hooked(true, false, false)
        .prop("config", t_mixed(), None)
        .hooked(true, false, false)
        .prop("firstElementChild", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("lastElementChild", t_nullable(t_class("DOMElement")), None)
        .hooked(true, false, false)
        .prop("childElementCount", TypeExpr::Int, None)
        .hooked(true, false, false)
        .method(
            method("__propget_documentElement")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_assign("root", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(6)])])),
                    s_return(e_ternary(e_instance_of(e_var("root"), "DOMElement"), e_var("root"), e_null())),
                ]),
        )
        .method(
            method("__propget_xmlVersion")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(6)])),
                ]),
        )
        .method(
            method("__propget_version")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(6)])),
                ]),
        )
        .method(
            method("__propget_xmlEncoding")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(7)])),
                ]),
        )
        .method(
            method("__propget_encoding")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(7)])),
                ]),
        )
        .method(
            method("__propget_documentURI")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(8)])),
                ]),
        )
        .method(
            method("__propget_doctype")
                .returns(t_nullable(t_class("DOMDocumentType")))
                .body(vec![
                    s_assign("doctype", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_node", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(7)])])),
                    s_return(e_ternary(e_instance_of(e_var("doctype"), "DOMDocumentType"), e_var("doctype"), e_null())),
                ]),
        )
        .method(
            method("__propget_implementation")
                .returns(t_class("DOMImplementation"))
                .body(vec![
                    s_assign("_unused", e_method_call(e_this(), "__elephc_state", vec![])),
                    s_return(e_new("DOMImplementation", vec![])),
                ]),
        )
        .method(
            method("__propget_actualEncoding")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_return(e_call("elephc_xml_dom_string", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_int(7)])),
                ]),
        )
        .method(
            method("__propget_standalone")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_xml_dom_standalone", vec![e_method_call(e_this(), "__elephc_state", vec![])]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__propget_xmlStandalone")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_xml_dom_standalone", vec![e_method_call(e_this(), "__elephc_state", vec![])]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__propget_config")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_null()),
                ]),
        )
        .method(
            method("__propget_firstElementChild")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementChild", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(false)])),
                ]),
        )
        .method(
            method("__propget_lastElementChild")
                .returns(t_nullable(t_class("DOMElement")))
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementChild", vec![e_method_call(e_this(), "__elephc_state", vec![]), e_bool(true)])),
                ]),
        )
        .method(
            method("__propget_childElementCount")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_static_call("__ElephcDOM", "elementCount", vec![e_method_call(e_this(), "__elephc_state", vec![])])),
                ]),
        )
        .method(
            method("__elephcDebugInfo")
                .protected()
                .returns(t_array())
                .body(vec![
                    s_return(e_binop(e_array_assoc(vec![(e_str("doctype"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("doctype")])), (e_str("implementation"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("implementation")])), (e_str("documentElement"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("documentElement")])), (e_str("actualEncoding"), e_this_prop("actualEncoding")), (e_str("encoding"), e_this_prop("encoding")), (e_str("xmlEncoding"), e_this_prop("xmlEncoding")), (e_str("standalone"), e_this_prop("standalone")), (e_str("xmlStandalone"), e_this_prop("xmlStandalone")), (e_str("version"), e_this_prop("version")), (e_str("xmlVersion"), e_this_prop("xmlVersion")), (e_str("strictErrorChecking"), e_this_prop("strictErrorChecking")), (e_str("documentURI"), e_this_prop("documentURI")), (e_str("config"), e_this_prop("config")), (e_str("formatOutput"), e_this_prop("formatOutput")), (e_str("validateOnParse"), e_this_prop("validateOnParse")), (e_str("resolveExternals"), e_this_prop("resolveExternals")), (e_str("preserveWhiteSpace"), e_this_prop("preserveWhiteSpace")), (e_str("recover"), e_this_prop("recover")), (e_str("substituteEntities"), e_this_prop("substituteEntities")), (e_str("firstElementChild"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("firstElementChild")])), (e_str("lastElementChild"), e_static_call("__ElephcDOM", "omit", vec![e_this_prop("lastElementChild")])), (e_str("childElementCount"), e_this_prop("childElementCount"))]), BinOp::Add, e_method_call(e_this(), "__elephc_node_debug", vec![]))),
                ]),
        )
        .method(
            method("__construct")
                .param_default("version", TypeExpr::Str, e_str("1.0"))
                .param_default("encoding", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_if(
                        e_not(e_static_prop("__ElephcDOM", "minting")),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_bind", vec![e_call("elephc_xml_dom_document_new", vec![e_var("version"), e_var("encoding")])])),
                            s_static_prop_array_assign("__ElephcDOM", "objects", e_method_call(e_this(), "__elephc_handle", vec![]), e_this()),
                        ],
                        vec![],
                        None,
                    ),
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
                    s_assign("loaded", e_binop(e_call("elephc_xml_dom_load", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("source"), e_call("strlen", vec![e_var("source")]), e_var("options")]), BinOp::StrictNotEq, e_int(0))),
                    s_expr(e_call("__elephc_simplexml_report_text", vec![e_call("elephc_xml_sx_diagnostics", vec![]), e_str("DOMDocument::loadXML()"), e_bool(true)])),
                    s_return(e_var("loaded")),
                ]),
        )
        .method(
            method("load")
                .param("filename", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("filename"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("DOMDocument::load(): Argument #1 ($filename) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("loaded", e_binop(e_call("elephc_xml_dom_load_file", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("filename"), e_var("options")]), BinOp::StrictNotEq, e_int(0))),
                    s_expr(e_call("__elephc_simplexml_report_text", vec![e_call("elephc_xml_sx_diagnostics", vec![]), e_str("DOMDocument::load()"), e_bool(true)])),
                    s_return(e_var("loaded")),
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
                            s_throw(e_new("ValueError", vec![e_str("DOMDocument::loadHTML(): Argument #1 ($source) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("loaded", e_binop(e_call("elephc_xml_dom_load_html", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("source"), e_call("strlen", vec![e_var("source")]), e_str(""), e_var("options")]), BinOp::StrictNotEq, e_int(0))),
                    s_expr(e_call("__elephc_simplexml_report_text", vec![e_call("elephc_xml_sx_diagnostics", vec![]), e_str("DOMDocument::loadHTML()"), e_bool(true)])),
                    s_return(e_var("loaded")),
                ]),
        )
        .method(
            method("loadHTMLFile")
                .param("filename", TypeExpr::Str)
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("filename"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("DOMDocument::loadHTMLFile(): Argument #1 ($filename) must not be empty")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("loaded", e_binop(e_call("elephc_xml_dom_load_html", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_str(""), e_int(0), e_var("filename"), e_var("options")]), BinOp::StrictNotEq, e_int(0))),
                    s_expr(e_call("__elephc_simplexml_report_text", vec![e_call("elephc_xml_sx_diagnostics", vec![]), e_str("DOMDocument::loadHTMLFile()"), e_bool(true)])),
                    s_return(e_var("loaded")),
                ]),
        )
        .method(
            method("saveXML")
                .param_untyped_default("node", e_null())
                .param_default("options", TypeExpr::Int, e_int(0))
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_assign("target", e_static_call("__ElephcDOM", "argument", vec![e_var("node"), e_str("DOMDocument::saveXML"), e_int(1), e_str("node"), e_bool(true)])),
                    s_assign("xml", e_call("elephc_xml_dom_save", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("target"), e_ternary(e_this_prop("formatOutput"), e_int(1), e_int(0)), e_binop(e_var("options"), BinOp::BitAnd, e_int(2)), e_binop(e_var("options"), BinOp::BitAnd, e_int(4))])),
                    s_return(e_ternary(e_binop(e_var("xml"), BinOp::StrictEq, e_null()), e_bool(false), e_var("xml"))),
                ]),
        )
        .method(
            method("saveHTML")
                .param_untyped_default("node", e_null())
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_assign("target", e_static_call("__ElephcDOM", "argument", vec![e_var("node"), e_str("DOMDocument::saveHTML"), e_int(1), e_str("node"), e_bool(true)])),
                    s_assign("html", e_call("elephc_xml_dom_save_html", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("target"), e_ternary(e_this_prop("formatOutput"), e_int(1), e_int(0))])),
                    s_return(e_ternary(e_binop(e_var("html"), BinOp::StrictEq, e_null()), e_bool(false), e_var("html"))),
                ]),
        )
        .method(
            method("createElement")
                .param("localName", TypeExpr::Str)
                .param_default("value", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "created", vec![e_call("elephc_xml_dom_create", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_int(1), e_var("localName"), e_var("value")])])),
                    s_if(
                        e_instance_of(e_var("node"), "DOMElement"),
                        vec![
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("DOMException", vec![e_str("Invalid State Error"), e_int(11)])),
                ]),
        )
        .method(
            method("createTextNode")
                .param("data", TypeExpr::Str)
                .returns(t_class("DOMText"))
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_create", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_int(3), e_str(""), e_var("data")])])),
                    s_if(
                        e_not(e_instance_of(e_var("node"), "DOMText")),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid State Error"), e_int(11)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("node")),
                ]),
        )
        .method(
            method("createComment")
                .param("data", TypeExpr::Str)
                .returns(t_class("DOMComment"))
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_create", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_int(8), e_str(""), e_var("data")])])),
                    s_if(
                        e_not(e_instance_of(e_var("node"), "DOMComment")),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid State Error"), e_int(11)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("node")),
                ]),
        )
        .method(
            method("createCDATASection")
                .param("data", TypeExpr::Str)
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_create", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_int(4), e_str(""), e_var("data")])])),
                    s_if(
                        e_instance_of(e_var("node"), "DOMCdataSection"),
                        vec![
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("DOMException", vec![e_str("Invalid State Error"), e_int(11)])),
                ]),
        )
        .method(
            method("createProcessingInstruction")
                .param("target", TypeExpr::Str)
                .param_default("data", TypeExpr::Str, e_str(""))
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "created", vec![e_call("elephc_xml_dom_create", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_int(7), e_var("target"), e_var("data")])])),
                    s_if(
                        e_instance_of(e_var("node"), "DOMProcessingInstruction"),
                        vec![
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("DOMException", vec![e_str("Invalid State Error"), e_int(11)])),
                ]),
        )
        .method(
            method("createAttribute")
                .param("localName", TypeExpr::Str)
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "created", vec![e_call("elephc_xml_dom_create", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_int(2), e_var("localName"), e_str("")])])),
                    s_if(
                        e_instance_of(e_var("node"), "DOMAttr"),
                        vec![
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("DOMException", vec![e_str("Invalid State Error"), e_int(11)])),
                ]),
        )
        .method(
            method("createDocumentFragment")
                .returns(t_class("DOMDocumentFragment"))
                .body(vec![
                    s_assign("node", e_static_call("__ElephcDOM", "wrap", vec![e_call("elephc_xml_dom_create", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_int(11), e_str(""), e_str("")])])),
                    s_if(
                        e_not(e_instance_of(e_var("node"), "DOMDocumentFragment")),
                        vec![
                            s_throw(e_new("DOMException", vec![e_str("Invalid State Error"), e_int(11)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("node")),
                ]),
        )
        .method(
            method("getElementsByTagName")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_class("DOMNodeList"))
                .body(vec![
                    s_return(e_static_call("DOMNodeList", "__elephc_list", vec![e_method_call(e_this(), "__elephc_handle", vec![]), e_var("qualifiedName")])),
                ]),
        )
        .method(
            method("append")
                .variadic("nodes", None)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_foreach(e_static_call("__ElephcDOM", "nodes", vec![e_this(), e_var("nodes")]), None, "node", vec![
                        s_expr(e_method_call(e_this(), "appendChild", vec![e_var("node")])),
                    ]),
                ]),
        )
        .method(
            method("prepend")
                .variadic("nodes", None)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("current", e_this_prop("firstChild")),
                    s_assign("first", e_ternary(e_instance_of(e_var("current"), "DOMNode"), e_var("current"), e_null())),
                    s_foreach(e_static_call("__ElephcDOM", "nodes", vec![e_this(), e_var("nodes")]), None, "node", vec![
                        s_expr(e_method_call(e_this(), "insertBefore", vec![e_var("node"), e_var("first")])),
                    ]),
                ]),
        )
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_extern_elephc_xml_dom_document_new(),
            decl_extern_elephc_xml_dom_load(),
            decl_extern_elephc_xml_dom_load_file(),
            decl_extern_elephc_xml_dom_load_html(),
            decl_extern_elephc_xml_dom_save(),
            decl_extern_elephc_xml_dom_save_html(),
            decl_extern_elephc_xml_dom_type(),
            decl_extern_elephc_xml_dom_standalone(),
            decl_extern_elephc_xml_dom_node(),
            decl_extern_elephc_xml_dom_string(),
            decl_extern_elephc_xml_dom_set_text(),
            decl_extern_elephc_xml_dom_child(),
            decl_extern_elephc_xml_dom_child_count(),
            decl_extern_elephc_xml_dom_attribute(),
            decl_extern_elephc_xml_dom_attribute_node(),
            decl_extern_elephc_xml_dom_set_attribute(),
            decl_extern_elephc_xml_dom_remove_attribute(),
            decl_extern_elephc_xml_dom_attribute_at(),
            decl_extern_elephc_xml_dom_create(),
            decl_extern_elephc_xml_dom_insert(),
            decl_extern_elephc_xml_dom_remove_child(),
            decl_extern_elephc_xml_dom_clone(),
            decl_extern_elephc_xml_dom_tag_count(),
            decl_extern_elephc_xml_dom_tag_item(),
            decl_class_domexception(),
            decl_class_elephcdom(),
            decl_class_domnode(),
            decl_class_domnodelist(),
            decl_class_domnamednodemap(),
            decl_class_domdocumentfragment(),
            decl_class_domcharacterdata(),
            decl_class_domtext(),
            decl_class_domcdatasection(),
            decl_class_domcomment(),
            decl_class_domprocessinginstruction(),
            decl_class_domattr(),
            decl_class_domelement(),
            decl_class_domdocumenttype(),
            decl_class_domimplementation(),
            decl_class_domdocument(),
        ]
    })
}
