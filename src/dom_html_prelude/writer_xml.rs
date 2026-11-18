//! Purpose:
//! Package P7 of the DOM migration: the XML serializer (`DOMDocument::saveXML` / `save`,
//! `DOMNode::C14N` / `C14NFile`) and the `textContent` replacement helper.
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - A node bound to the shared libxml2 tree is written by libxml2 itself, through
//!   `SimpleXMLElement::__elephc_dom_write` (`crates/elephc-xml/src/simplexml/dom_write.rs`):
//!   the whole document with `xmlSaveToIO` (`formatOutput`, `LIBXML_NOEMPTYTAG`,
//!   `LIBXML_NOXMLDECL`), one node with `xmlSaveTree` in UTF-8 (as PHP's `saveXML($node)`),
//!   canonical XML with `xmlC14NExecute`.
//! - A free-standing (eager) tree is written by `__ElephcDomXmlWriter`, which follows libxml2's
//!   rules: the declaration of the document, `&#xE9;` references only where the document has no
//!   encoding, `formatOutput` indentation that stops under an element with a text child, empty
//!   elements as `<a/>` or `<a></a>`. Its XHTML special cases, XPath-restricted `C14N` and the
//!   encodings other than UTF-8 / ISO-8859-1 are not covered.
//! - The eager document keeps its constructor `version` / `encoding` in `__elephc_dom_version` /
//!   `__elephc_dom_encoding` (set in `generated.rs`); `formatOutput` is declared here
//!   (the serializers are its only readers).
//! - The method bodies were written as PHP and converted with `synthetic_class::transcribe`.

use super::extend::{Contributions, Part};
use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::synthetic_class::*;

fn document() -> ClassBuilder {
            class("DOMDocument")
                .prop("formatOutput", TypeExpr::Bool, Some(e_bool(false)))
                .prop("__elephc_dom_version", TypeExpr::Str, Some(e_str("1.0")))
                .prop("__elephc_dom_encoding", t_nullable(TypeExpr::Str), Some(e_null()))
                .method(
                    method("saveXML")
                        .param_default("node", t_nullable(t_class("DOMNode")), e_null())
                        .param_default("options", TypeExpr::Int, e_int(0))
                        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                        .body(vec![
                            s_assign("format", e_this_prop("formatOutput")),
                            s_if(
                                e_binop(e_binop(e_binop(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("node"), BinOp::StrictNotEq, e_this())), BinOp::And, e_binop(e_prop(e_var("node"), "ownerDocument"), BinOp::StrictNotEq, e_this())), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMDocument"))),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("whole", e_binop(e_var("node"), BinOp::StrictEq, e_null())),
                            s_assign("asTree", e_binop(e_var("node"), BinOp::StrictEq, e_this())),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_binop(e_not(e_var("whole")), BinOp::And, e_not(e_var("asTree"))),
                                vec![
                                    s_assign("bound", e_prop(e_var("node"), "__elephc_simplexml")),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_call("is_object", vec![e_var("bound")]), BinOp::And, e_not(e_method_call(e_var("bound"), "__elephc_dom_is_html", vec![]))),
                                vec![
                                    s_if(
                                        e_var("whole"),
                                        vec![
                                            s_return(e_method_call(e_var("bound"), "__elephc_dom_write", vec![e_int(0), e_int(0), e_var("options"), e_var("format")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_var("asTree"),
                                        vec![
                                            s_return(e_method_call(e_var("bound"), "__elephc_dom_write", vec![e_int(3), e_int(0), e_var("options"), e_var("format")])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_return(e_method_call(e_var("bound"), "__elephc_dom_write", vec![e_int(0), e_int(1), e_var("options"), e_var("format")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_var("whole"),
                                vec![
                                    s_return(e_static_call("__ElephcDomXmlWriter", "document", vec![e_this(), e_var("format"), e_var("options"), e_bool(false)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_var("asTree"),
                                vec![
                                    s_return(e_static_call("__ElephcDomXmlWriter", "document", vec![e_this(), e_var("format"), e_var("options"), e_bool(true)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_static_call("__ElephcDomXmlWriter", "subtree", vec![e_var("node"), e_var("format"), e_var("options")])),
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
}

fn node() -> ClassBuilder {
            class("DOMNode")
                .method(
                    method("__elephc_dom_replace_text")
                        .param("text", TypeExpr::Str)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_while(e_binop(e_this_prop("firstChild"), BinOp::StrictNotEq, e_null()), vec![
                                s_expr(e_method_call(e_this(), "removeChild", vec![e_this_prop("firstChild")])),
                            ]),
                            s_expr(e_method_call(e_this(), "appendChild", vec![e_new("DOMText", vec![e_var("text")])])),
                        ]),
                )
                .method(
                    method("C14N")
                        .param_default("exclusive", TypeExpr::Bool, e_bool(false))
                        .param_default("withComments", TypeExpr::Bool, e_bool(false))
                        .param_default("xpath", t_nullable(t_array()), e_null())
                        .param_default("nsPrefixes", t_nullable(t_array()), e_null())
                        .keep_unread_params()
                        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                        .body(vec![
                            s_assign("prefixes", e_str("")),
                            s_if(
                                e_binop(e_var("nsPrefixes"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_assign("prefixes", e_call("implode", vec![e_str(","), e_var("nsPrefixes")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("bound", e_this_prop("__elephc_simplexml")),
                            s_if(
                                e_binop(e_call("is_object", vec![e_var("bound")]), BinOp::And, e_not(e_method_call(e_var("bound"), "__elephc_dom_is_html", vec![]))),
                                vec![
                                    s_assign("flags", e_int(0)),
                                    s_if(e_var("exclusive"), vec![s_assign("flags", e_int(1))], vec![], None),
                                    s_if(e_var("withComments"), vec![s_assign("flags", e_binop(e_var("flags"), BinOp::BitOr, e_int(2)))], vec![], None),
                                    s_assign("target", e_int(1)),
                                    s_if(e_instance_of(e_this(), "DOMDocument"), vec![s_assign("target", e_int(0))], vec![], None),
                                    s_assign("out", e_method_call(e_var("bound"), "__elephc_dom_write", vec![e_int(2), e_var("target"), e_var("flags"), e_bool(false), e_var("prefixes")])),
                                    s_return(e_var("out")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_static_call("__ElephcDomXmlWriter", "canonical", vec![e_this(), e_var("withComments")])),
                        ]),
                )
                .method(
                    method("C14NFile")
                        .param("uri", TypeExpr::Str)
                        .param_default("exclusive", TypeExpr::Bool, e_bool(false))
                        .param_default("withComments", TypeExpr::Bool, e_bool(false))
                        .param_default("xpath", t_nullable(t_array()), e_null())
                        .param_default("nsPrefixes", t_nullable(t_array()), e_null())
                        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
                        .body(vec![
                            s_assign("out", e_method_call(e_this(), "C14N", vec![e_var("exclusive"), e_var("withComments"), e_var("xpath"), e_var("nsPrefixes")])),
                            s_if(
                                e_binop(e_var("out"), BinOp::StrictEq, e_bool(false)),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("file_put_contents", vec![e_var("uri"), e_var("out")])),
                        ]),
                )
}

fn writer() -> Stmt {
            class("__ElephcDomXmlWriter")
                .final_()
                .prop("out", TypeExpr::Str, Some(e_str("")))
                .prop("fmt", TypeExpr::Bool, Some(e_bool(false)))
                .prop("level", TypeExpr::Int, Some(e_int(0)))
                .prop("ascii", TypeExpr::Bool, Some(e_bool(false)))
                .prop("attrAscii", TypeExpr::Bool, Some(e_bool(false)))
                .prop("options", TypeExpr::Int, Some(e_int(0)))
                .method(
                    method("text")
                        .static_()
                        .param("s", TypeExpr::Str)
                        .param("ascii", TypeExpr::Bool)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("s", e_call("str_replace", vec![e_str("&"), e_str("&amp;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("<"), e_str("&lt;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str(">"), e_str("&gt;"), e_var("s")])),
                            s_if(
                                e_var("ascii"),
                                vec![
                                    s_assign("s", e_call("str_replace", vec![e_str("\r"), e_str("&#xD;"), e_var("s")])),
                                    s_return(e_call("__elephc_dom_escape_attr_non_ascii", vec![e_var("s")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_call("str_replace", vec![e_str("\r"), e_str("&#13;"), e_var("s")])),
                        ]),
                )
                .method(
                    method("attribute")
                        .static_()
                        .param("s", TypeExpr::Str)
                        .param("ascii", TypeExpr::Bool)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("s", e_call("str_replace", vec![e_str("&"), e_str("&amp;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("<"), e_str("&lt;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str(">"), e_str("&gt;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("\""), e_str("&quot;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("\n"), e_str("&#10;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("\r"), e_str("&#13;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("\t"), e_str("&#9;"), e_var("s")])),
                            s_if(
                                e_var("ascii"),
                                vec![
                                    s_return(e_call("__elephc_dom_escape_attr_non_ascii", vec![e_var("s")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("s")),
                        ]),
                )
                .method(
                    method("header")
                        .static_()
                        .param("doc", t_class("DOMDocument"))
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("head", e_binop(e_binop(e_str("<?xml version=\""), BinOp::Concat, e_prop(e_var("doc"), "__elephc_dom_version")), BinOp::Concat, e_str("\""))),
                            s_if(
                                e_binop(e_prop(e_var("doc"), "__elephc_dom_encoding"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_assign("head", e_binop(e_var("head"), BinOp::Concat, e_binop(e_binop(e_str(" encoding=\""), BinOp::Concat, e_prop(e_var("doc"), "__elephc_dom_encoding")), BinOp::Concat, e_str("\"")))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_prop(e_var("doc"), "__elephc_dom_html"),
                                vec![
                                    s_assign("head", e_binop(e_var("head"), BinOp::Concat, e_str(" standalone=\"yes\""))),
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
                        .param("asTree", TypeExpr::Bool)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("writer", e_new("__ElephcDomXmlWriter", vec![])),
                            s_prop_assign(e_var("writer"), "fmt", e_var("format")),
                            s_prop_assign(e_var("writer"), "options", e_var("options")),
                            s_assign("encoding", e_prop(e_var("doc"), "__elephc_dom_encoding")),
                            s_prop_assign(e_var("writer"), "ascii", e_binop(e_binop(e_var("encoding"), BinOp::StrictEq, e_null()), BinOp::And, e_not(e_var("asTree")))),
                            s_prop_assign(e_var("writer"), "attrAscii", e_binop(e_binop(e_var("encoding"), BinOp::StrictEq, e_null()), BinOp::And, e_not(e_var("asTree")))),
                            s_assign("out", e_str("")),
                            s_if(
                                e_binop(e_binop(e_var("options"), BinOp::BitAnd, e_int(2)), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_if(
                                        e_binop(e_var("asTree"), BinOp::And, e_binop(e_var("encoding"), BinOp::StrictEq, e_null())),
                                        vec![
                                            s_assign("out", e_binop(e_binop(e_binop(e_str("<?xml version=\""), BinOp::Concat, e_prop(e_var("doc"), "__elephc_dom_version")), BinOp::Concat, e_str("\" encoding=\"UTF-8\"?>")), BinOp::Concat, e_str("\n"))),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_assign("out", e_self_call("header", vec![e_var("doc")])),
                                    ]),
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_prop(e_var("doc"), "childNodes"), None, "child", vec![
                                s_prop_assign(e_var("writer"), "out", e_str("")),
                                s_prop_assign(e_var("writer"), "level", e_int(0)),
                                s_prop_assign(e_var("writer"), "fmt", e_var("format")),
                                s_expr(e_method_call(e_var("writer"), "node", vec![e_var("child"), e_bool(true)])),
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_prop(e_var("writer"), "out"), BinOp::Concat, e_str("\n")))),
                            ]),
                            s_if(
                                e_binop(e_binop(e_var("encoding"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("strtoupper", vec![e_var("encoding")]), BinOp::StrictEq, e_str("ISO-8859-1"))),
                                vec![
                                    s_return(e_call("mb_convert_encoding", vec![e_var("out"), e_str("ISO-8859-1"), e_str("UTF-8")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("out")),
                        ]),
                )
                .method(
                    method("subtree")
                        .static_()
                        .param("node", t_class("DOMNode"))
                        .param("format", TypeExpr::Bool)
                        .param("options", TypeExpr::Int)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("writer", e_new("__ElephcDomXmlWriter", vec![])),
                            s_prop_assign(e_var("writer"), "fmt", e_var("format")),
                            s_prop_assign(e_var("writer"), "options", e_var("options")),
                            s_assign("owner", e_prop(e_var("node"), "ownerDocument")),
                            s_prop_assign(e_var("writer"), "attrAscii", e_binop(e_not(e_instance_of(e_var("owner"), "DOMDocument")), BinOp::Or, e_binop(e_prop(e_var("owner"), "__elephc_dom_encoding"), BinOp::StrictEq, e_null()))),
                            s_expr(e_method_call(e_var("writer"), "node", vec![e_var("node"), e_bool(true)])),
                            s_return(e_prop(e_var("writer"), "out")),
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
                    method("indent")
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_if(
                                e_this_prop("fmt"),
                                vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_call("str_repeat", vec![e_str("  "), e_ternary(e_binop(e_this_prop("level"), BinOp::Gt, e_int(30)), e_int(30), e_this_prop("level"))]))),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                )
                .method(
                    method("hasTextChild")
                        .static_()
                        .param("cur", t_class("DOMNode"))
                        .returns(TypeExpr::Bool)
                        .body(vec![
                            s_foreach(e_prop(e_var("cur"), "childNodes"), None, "child", vec![
                                s_assign("kind", e_prop(e_var("child"), "nodeType")),
                                s_if(
                                    e_binop(e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(4))), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(5))),
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
                    method("node")
                        .param("cur", t_class("DOMNode"))
                        .param("isRoot", TypeExpr::Bool)
                        .returns(TypeExpr::Void)
                        .body(vec![
                            s_if(
                                e_instance_of(e_var("cur"), "DOMElement"),
                                vec![
                                    s_expr(e_method_call(e_this(), "element", vec![e_var("cur"), e_var("isRoot")])),
                                ],
                                vec![
                                (e_instance_of(e_var("cur"), "DOMCdataSection"), vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("<![CDATA["), BinOp::Concat, e_call("str_replace", vec![e_str("]]>"), e_str("]]]]><![CDATA[>"), e_cast(CastType::String, e_prop(e_var("cur"), "nodeValue"))])), BinOp::Concat, e_str("]]>")))),
                                ]),
                                (e_instance_of(e_var("cur"), "DOMText"), vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_self_call("text", vec![e_cast(CastType::String, e_prop(e_var("cur"), "nodeValue")), e_this_prop("ascii")]))),
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
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("<!--"), BinOp::Concat, e_prop(e_var("cur"), "nodeValue")), BinOp::Concat, e_str("-->")))),
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
                                    s_assign("data", e_cast(CastType::String, e_prop(e_var("cur"), "nodeValue"))),
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_ternary(e_binop(e_var("data"), BinOp::StrictEq, e_str("")), e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str("?>")), e_binop(e_binop(e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("data")), BinOp::Concat, e_str("?>"))))),
                                ]),
                                (e_instance_of(e_var("cur"), "DOMDocumentType"), vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_self_call("doctype", vec![e_var("cur")]))),
                                ]),
                                (e_instance_of(e_var("cur"), "DOMDocumentFragment"), vec![
                                    s_foreach(e_prop(e_var("cur"), "childNodes"), None, "child", vec![
                                        s_expr(e_method_call(e_this(), "node", vec![e_var("child"), e_bool(false)])),
                                        s_if(
                                            e_this_prop("fmt"),
                                            vec![
                                                s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ]),
                                ]),
                                (e_instance_of(e_var("cur"), "DOMAttr"), vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_binop(e_binop(e_str(" "), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_self_call("attribute", vec![e_cast(CastType::String, e_prop(e_var("cur"), "nodeValue")), e_this_prop("attrAscii")])), BinOp::Concat, e_str("\"")))),
                                ]),
                                (e_instance_of(e_var("cur"), "DOMDocument"), vec![
                                    s_foreach(e_prop(e_var("cur"), "childNodes"), None, "child", vec![
                                        s_expr(e_method_call(e_this(), "node", vec![e_var("child"), e_bool(false)])),
                                    ]),
                                ]),
                            ],
                                None,
                            ),
                        ]),
                )
                .method(
                    method("element")
                        .param("cur", t_class("DOMElement"))
                        .param("isRoot", TypeExpr::Bool)
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
                            s_assign("name", e_prop(e_var("cur"), "nodeName")),
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_str("<"), BinOp::Concat, e_var("name")))),
                            s_assign("bound", e_prop(e_var("cur"), "__elephc_simplexml")),
                            s_assign("default", e_prop(e_var("cur"), "__elephc_dom_xmlns")),
                            s_if(
                                e_binop(e_var("default"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str(" xmlns=\""), BinOp::Concat, e_self_call("attribute", vec![e_var("default"), e_this_prop("attrAscii")])), BinOp::Concat, e_str("\"")))),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("uri", e_prop(e_var("cur"), "__elephc_dom_ns")),
                            s_if(
                                e_binop(e_binop(e_var("uri"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("default"), BinOp::StrictEq, e_null())),
                                vec![
                                    s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                                    s_if(
                                        e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)),
                                        vec![
                                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str(" xmlns=\""), BinOp::Concat, e_self_call("attribute", vec![e_var("uri"), e_this_prop("attrAscii")])), BinOp::Concat, e_str("\"")))),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_binop(e_binop(e_str(" xmlns:"), BinOp::Concat, e_call("substr", vec![e_var("name"), e_int(0), e_var("colon")])), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_self_call("attribute", vec![e_var("uri"), e_this_prop("attrAscii")])), BinOp::Concat, e_str("\"")))),
                                    ]),
                                    ),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("attrs", e_ternary(e_call("is_object", vec![e_var("bound")]), e_method_call(e_var("bound"), "__elephc_dom_attributes", vec![]), e_prop(e_var("cur"), "__attrs"))),
                            s_if(
                                e_call("is_array", vec![e_var("attrs")]),
                                vec![
                                    s_foreach(e_var("attrs"), None, "pair", vec![
                                        s_if(
                                            e_call("is_array", vec![e_var("pair")]),
                                            vec![
                                                s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_binop(e_binop(e_str(" "), BinOp::Concat, e_index(e_var("pair"), e_int(0))), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_self_call("attribute", vec![e_cast(CastType::String, e_index(e_var("pair"), e_int(1))), e_this_prop("attrAscii")])), BinOp::Concat, e_str("\"")))),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ]),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("kids", e_prop(e_var("cur"), "childNodes")),
                            s_if(
                                e_binop(e_binop(e_var("kids"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_prop(e_var("kids"), "length"), BinOp::StrictEq, e_int(0))),
                                vec![
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_ternary(e_binop(e_binop(e_this_prop("options"), BinOp::BitAnd, e_int(4)), BinOp::StrictNotEq, e_int(0)), e_binop(e_binop(e_str("></"), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(">")), e_str("/>")))),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("restore", e_this_prop("fmt")),
                            s_if(
                                e_binop(e_this_prop("fmt"), BinOp::And, e_self_call("hasTextChild", vec![e_var("cur")])),
                                vec![
                                    s_prop_assign(e_this(), "fmt", e_bool(false)),
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
                            s_foreach(e_var("kids"), None, "child", vec![
                                s_expr(e_method_call(e_this(), "node", vec![e_var("child"), e_bool(false)])),
                                s_if(
                                    e_this_prop("fmt"),
                                    vec![
                                        s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_str("\n"))),
                                    ],
                                    vec![],
                                    None,
                                ),
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
                                    s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_call("str_repeat", vec![e_str("  "), e_ternary(e_binop(e_this_prop("level"), BinOp::Gt, e_int(30)), e_int(30), e_this_prop("level"))]))),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "out", e_binop(e_this_prop("out"), BinOp::Concat, e_binop(e_binop(e_str("</"), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(">")))),
                            s_prop_assign(e_this(), "fmt", e_var("restore")),
                        ]),
                )
                .method(
                    method("canonical")
                        .static_()
                        .param("node", t_class("DOMNode"))
                        .param("withComments", TypeExpr::Bool)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("out", e_str("")),
                            s_if(
                                e_instance_of(e_var("node"), "DOMDocument"),
                                vec![
                                    s_foreach(e_prop(e_var("node"), "childNodes"), None, "child", vec![
                                        s_if(
                                            e_instance_of(e_var("child"), "DOMDocumentType"),
                                            vec![
                                                s_continue(1),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_self_call("canonicalNode", vec![e_var("child"), e_var("withComments")]))),
                                    ]),
                                    s_return(e_var("out")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_self_call("canonicalNode", vec![e_var("node"), e_var("withComments")])),
                        ]),
                )
                .method(
                    method("canonicalNode")
                        .static_()
                        .param("cur", t_class("DOMNode"))
                        .param("withComments", TypeExpr::Bool)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_if(
                                e_instance_of(e_var("cur"), "DOMElement"),
                                vec![
                                    s_assign("name", e_prop(e_var("cur"), "nodeName")),
                                    s_assign("out", e_binop(e_str("<"), BinOp::Concat, e_var("name"))),
                                    s_assign("bound", e_prop(e_var("cur"), "__elephc_simplexml")),
                                    s_assign("attrs", e_ternary(e_call("is_object", vec![e_var("bound")]), e_method_call(e_var("bound"), "__elephc_dom_attributes", vec![]), e_prop(e_var("cur"), "__attrs"))),
                                    s_assign("decls", e_array(vec![])),
                                    s_if(
                                        e_binop(e_prop(e_var("cur"), "__elephc_dom_xmlns"), BinOp::StrictNotEq, e_null()),
                                        vec![
                                            s_array_assign("decls", e_str("xmlns"), e_prop(e_var("cur"), "__elephc_dom_xmlns")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("uri", e_prop(e_var("cur"), "__elephc_dom_ns")),
                                    s_if(
                                        e_binop(e_binop(e_var("uri"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_prop(e_var("cur"), "__elephc_dom_xmlns"), BinOp::StrictEq, e_null())),
                                        vec![
                                            s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                                            s_array_assign("decls", e_ternary(e_binop(e_var("colon"), BinOp::StrictEq, e_bool(false)), e_str("xmlns"), e_binop(e_str("xmlns:"), BinOp::Concat, e_call("substr", vec![e_var("name"), e_int(0), e_var("colon")]))), e_var("uri")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_expr(e_call("ksort", vec![e_var("decls")])),
                                    s_foreach(e_var("decls"), Some("declared"), "value", vec![
                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_binop(e_binop(e_str(" "), BinOp::Concat, e_var("declared")), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_self_call("canonicalAttribute", vec![e_cast(CastType::String, e_var("value"))])), BinOp::Concat, e_str("\"")))),
                                    ]),
                                    s_assign("sorted", e_array(vec![])),
                                    s_if(
                                        e_call("is_array", vec![e_var("attrs")]),
                                        vec![
                                            s_foreach(e_var("attrs"), None, "pair", vec![
                                                s_if(
                                                    e_call("is_array", vec![e_var("pair")]),
                                                    vec![
                                                        s_array_assign("sorted", e_cast(CastType::String, e_index(e_var("pair"), e_int(0))), e_cast(CastType::String, e_index(e_var("pair"), e_int(1)))),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                            ]),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_expr(e_call("ksort", vec![e_var("sorted")])),
                                    s_foreach(e_var("sorted"), Some("attribute"), "value", vec![
                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_binop(e_binop(e_binop(e_str(" "), BinOp::Concat, e_var("attribute")), BinOp::Concat, e_str("=\"")), BinOp::Concat, e_self_call("canonicalAttribute", vec![e_var("value")])), BinOp::Concat, e_str("\"")))),
                                    ]),
                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str(">"))),
                                    s_foreach(e_prop(e_var("cur"), "childNodes"), None, "child", vec![
                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_self_call("canonicalNode", vec![e_var("child"), e_var("withComments")]))),
                                    ]),
                                    s_return(e_binop(e_binop(e_binop(e_var("out"), BinOp::Concat, e_str("</")), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(">"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_var("cur"), "DOMText"),
                                vec![
                                    s_assign("s", e_call("str_replace", vec![e_str("&"), e_str("&amp;"), e_cast(CastType::String, e_prop(e_var("cur"), "nodeValue"))])),
                                    s_assign("s", e_call("str_replace", vec![e_str("<"), e_str("&lt;"), e_var("s")])),
                                    s_assign("s", e_call("str_replace", vec![e_str(">"), e_str("&gt;"), e_var("s")])),
                                    s_return(e_call("str_replace", vec![e_str("\r"), e_str("&#xD;"), e_var("s")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_var("cur"), "DOMComment"),
                                vec![
                                    s_return(e_ternary(e_var("withComments"), e_binop(e_binop(e_str("<!--"), BinOp::Concat, e_prop(e_var("cur"), "nodeValue")), BinOp::Concat, e_str("-->")), e_str(""))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_var("cur"), "DOMProcessingInstruction"),
                                vec![
                                    s_assign("data", e_cast(CastType::String, e_prop(e_var("cur"), "nodeValue"))),
                                    s_return(e_ternary(e_binop(e_var("data"), BinOp::StrictEq, e_str("")), e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str("?>")), e_binop(e_binop(e_binop(e_binop(e_str("<?"), BinOp::Concat, e_prop(e_var("cur"), "nodeName")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_var("data")), BinOp::Concat, e_str("?>")))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_instance_of(e_var("cur"), "DOMDocumentFragment"),
                                vec![
                                    s_assign("out", e_str("")),
                                    s_foreach(e_prop(e_var("cur"), "childNodes"), None, "child", vec![
                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_self_call("canonicalNode", vec![e_var("child"), e_var("withComments")]))),
                                    ]),
                                    s_return(e_var("out")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_str("")),
                        ]),
                )
                .method(
                    method("canonicalAttribute")
                        .static_()
                        .param("s", TypeExpr::Str)
                        .returns(TypeExpr::Str)
                        .body(vec![
                            s_assign("s", e_call("str_replace", vec![e_str("&"), e_str("&amp;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("<"), e_str("&lt;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("\""), e_str("&quot;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("\t"), e_str("&#x9;"), e_var("s")])),
                            s_assign("s", e_call("str_replace", vec![e_str("\n"), e_str("&#xA;"), e_var("s")])),
                            s_return(e_call("str_replace", vec![e_str("\r"), e_str("&#xD;"), e_var("s")])),
                        ]),
                )
                .build()
}

/// Registers this package's declarations.
pub(super) fn contribute(contributions: &mut Contributions) {
    contributions.members(Part::Walker, document());
    contributions.members(Part::Walker, node());
    contributions.add_class(Part::Walker, writer());
}
