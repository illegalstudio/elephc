//! Purpose:
//! The SimpleXML-side bridge of DOM package P7: one extern of the `elephc_xml` bridge and the
//! `SimpleXMLElement` method the DOM serializers call to write the shared libxml2 tree.
//!
//! Called from:
//! - `crate::simplexml_prelude::declarations()`.
//!
//! Key details:
//! - The DOM prelude reaches the bridge only through `__elephc_dom_write`, called on the bound
//!   `SimpleXMLElement` object, so a program that never binds a node to the native tree does not
//!   reference the extern. The bytes come back hex-encoded (`elephc_xml_sx_dom_write` in
//!   `crates/elephc-xml/src/simplexml/dom_write.rs`) and are decoded here.

use crate::parser::ast::{BinOp, CType, CastType, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::*;

fn extern_dom_write() -> Stmt {
    extern_fn("elephc_xml_sx_dom_write", "elephc_xml")
        .param("handle", CType::Int)
        .param("operation", CType::Int)
        .param("target", CType::Int)
        .param("options", CType::Int)
        .param("format", CType::Int)
        .param("prefixes", CType::Str)
        .returns(CType::Str)
        .build()
}

fn extern_html_first() -> Stmt {
    extern_fn("elephc_xml_sx_html_first", "elephc_xml")
        .param("handle", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `__elephc_dom_html_import`: imports a free-standing DOM document or node into a native HTML
/// document so `saveHTML` can use libxml2's serializer (written as PHP, then transcribed).
fn html_import() -> Stmt {
function("__elephc_dom_html_import")
    .param("node", t_class("DOMNode"))
    .returns(t_nullable(t_class("SimpleXMLElement")))
    .body(vec![
        s_if(
            e_instance_of(e_var("node"), "DOMDocument"),
            vec![
                s_assign("result", e_call("__elephc_simplexml_import_document", vec![e_var("node"), e_str("SimpleXMLElement")])),
                s_if(
                    e_instance_of(e_var("result"), "SimpleXMLElement"),
                    vec![
                        s_return(e_var("result")),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_null()),
            ],
            vec![],
            None,
        ),
        s_assign("builder", e_call("elephc_xml_sx_html_document", vec![])),
        s_if(
            e_binop(e_var("builder"), BinOp::StrictEq, e_int(0)),
            vec![
                s_return(e_null()),
            ],
            vec![],
            None,
        ),
        s_try(vec![
            s_expr(e_call("__elephc_simplexml_import_child", vec![e_var("builder"), e_var("node")])),
            s_assign("root", e_call("elephc_xml_sx_html_root", vec![e_var("builder")])),
            s_if(
                e_binop(e_var("root"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_assign("root", e_call("elephc_xml_sx_html_first", vec![e_var("builder")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("root"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_simplexml_wrap", vec![e_var("root"), e_str("SimpleXMLElement")])),
        ], vec![
        ], Some(vec![
            s_expr(e_call("elephc_xml_sx_free", vec![e_var("builder")])),
        ])),
    ])
    .build()
}

fn members() -> Stmt {
    class("SimpleXMLElement")
        .method(
            method("__elephc_dom_write")
                .param("operation", TypeExpr::Int)
                .param("target", TypeExpr::Int)
                .param("options", TypeExpr::Int)
                .param("format", TypeExpr::Bool)
                .param_default("prefixes", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Str)
                .body(vec![s_return(e_cast(
                    CastType::String,
                    e_call(
                        "hex2bin",
                        vec![e_call(
                            "elephc_xml_sx_dom_write",
                            vec![
                                e_this_prop("__elephc_handle"),
                                e_var("operation"),
                                e_var("target"),
                                e_var("options"),
                                e_ternary(e_var("format"), e_int(1), e_int(0)),
                                e_var("prefixes"),
                            ],
                        )],
                    ),
                ))]),
        )
        .build()
}

/// Adds the extern and the method to the built SimpleXML prelude.
pub(super) fn extend(program: &mut Program) {
    let extra = members();
    let StmtKind::ClassDecl { methods: extra_methods, .. } = &extra.kind else { unreachable!() };
    for declaration in program.iter_mut() {
        if let StmtKind::ClassDecl { name, methods, .. } = &mut declaration.kind {
            if name == "SimpleXMLElement" {
                methods.extend(extra_methods.iter().cloned());
            }
        }
    }
    for mut declaration in [extern_dom_write(), extern_html_first(), html_import()] {
        declaration.span = crate::span::Span::synthetic();
        program.push(declaration);
    }
}
