//! Purpose:
//! Package P7 of the DOM migration: the HTML serializer (`DOMDocument::saveHTML` /
//! `saveHTMLFile`).
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - Registered under `Part::Html`: a program that never mentions `saveHTML` /
//!   `saveHTMLFile` / `loadHTML` does not pay for it.
//! - The bytes are always written by libxml2's own HTML serializer
//!   (`htmlDocDumpMemoryFormat` / `htmlNodeDumpFormatOutput`, through
//!   `SimpleXMLElement::__elephc_dom_write`). A document or node that is not bound to the native
//!   tree yet (the HTML walker still builds free-standing nodes) is first imported into a native
//!   HTML document by `__elephc_dom_html_import` (`simplexml_prelude/dom_write.rs`), called by
//!   name so a program without the SimpleXML bridge does not need it.
//! - The `<!DOCTYPE ...>` line is written from the document's `DOMDocumentType` child, as the
//!   imported native document carries no DTD.
//! - The method bodies were written as PHP and converted with `synthetic_class::transcribe`.

use super::extend::{Contributions, Part};
use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::*;

fn document() -> ClassBuilder {
            class("DOMDocument")
                .method(
                    method("saveHTML")
                        .param_default("node", t_nullable(t_class("DOMNode")), e_null())
                        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                        .body(vec![
                            s_assign("format", e_this_prop("formatOutput")),
                            s_if(
                                e_binop(e_binop(e_binop(e_binop(e_var("node"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("node"), BinOp::StrictNotEq, e_this())), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMDocument"))), BinOp::And, e_binop(e_prop(e_var("node"), "ownerDocument"), BinOp::StrictNotEq, e_this())),
                                vec![
                                    s_throw(e_new("DOMException", vec![e_str("Wrong Document Error"), e_int(4)])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("whole", e_binop(e_binop(e_var("node"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("node"), BinOp::StrictEq, e_this()))),
                            s_assign("target", e_ternary(e_var("whole"), e_this(), e_var("node"))),
                            s_assign("bound", e_prop(e_var("target"), "__elephc_simplexml")),
                            s_assign("prefix", e_str("")),
                            s_if(
                                e_binop(e_not(e_call("is_object", vec![e_var("bound")])), BinOp::Or, e_not(e_method_call(e_var("bound"), "__elephc_dom_is_html", vec![]))),
                                vec![
                                    s_if(
                                        e_var("whole"),
                                        vec![
                                            s_foreach(e_this_prop("childNodes"), None, "child", vec![
                                                s_if(
                                                    e_instance_of(e_var("child"), "DOMDocumentType"),
                                                    vec![
                                                        s_assign("prefix", e_static_call("__ElephcDomHtmlWriter", "doctype", vec![e_var("child")])),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                            ]),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("bound", e_expr_call(e_str("__elephc_dom_html_import"), vec![e_var("target")])),
                                    s_if(
                                        e_not(e_call("is_object", vec![e_var("bound")])),
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
                            s_assign("out", e_method_call(e_var("bound"), "__elephc_dom_write", vec![e_int(1), e_ternary(e_var("whole"), e_int(0), e_int(1)), e_int(0), e_var("format")])),
                            s_return(e_binop(e_var("prefix"), BinOp::Concat, e_var("out"))),
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
}

fn writer() -> Stmt {
            class("__ElephcDomHtmlWriter")
                .final_()
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
                .build()
}

/// Registers this package's declarations.
pub(super) fn contribute(contributions: &mut Contributions) {
    contributions.members(Part::Html, document());
    contributions.add_class(Part::Html, writer());
}
