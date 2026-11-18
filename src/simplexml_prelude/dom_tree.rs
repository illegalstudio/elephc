//! Purpose:
//! The SimpleXML-side bridge of DOM package P4: two externs of the `elephc_xml` bridge and the
//! two `SimpleXMLElement` methods the DOM prelude calls to relink, remove and describe nodes of
//! the shared libxml2 tree.
//!
//! Called from:
//! - `crate::simplexml_prelude::declarations()` (through `extend`).
//!
//! Key details:
//! - The DOM prelude reaches the bridge only through these methods, called on the bound
//!   `SimpleXMLElement` object. A program that never binds a node to the native tree (a
//!   free-standing DOM built with `createElement`) therefore never references the externs and
//!   does not link libxml2.
//! - `elephc_xml_sx_dom_tree` / `elephc_xml_sx_dom_ns` live in
//!   `crates/elephc-xml/src/simplexml/dom_tree.rs`.

use crate::parser::ast::{CType, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::*;

fn extern_dom_tree() -> Stmt {
    extern_fn("elephc_xml_sx_dom_tree", "elephc_xml")
        .param("handle", CType::Int)
        .param("operation", CType::Int)
        .param("node", CType::Int)
        .param("reference", CType::Int)
        .param("document_parent", CType::Int)
        .returns(CType::Int)
        .build()
}

fn extern_dom_ns() -> Stmt {
    extern_fn("elephc_xml_sx_dom_ns", "elephc_xml")
        .param("handle", CType::Int)
        .param("operation", CType::Int)
        .param("key", CType::Str)
        .returns(CType::Str)
        .build()
}

fn members() -> Stmt {
    class("SimpleXMLElement")
        .method(
            method("__elephc_dom_tree_op")
                .param("operation", TypeExpr::Int)
                .param("node", TypeExpr::Int)
                .param("reference", TypeExpr::Int)
                .param("documentParent", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![s_return(e_call(
                    "elephc_xml_sx_dom_tree",
                    vec![
                        e_this_prop("__elephc_handle"),
                        e_var("operation"),
                        e_var("node"),
                        e_var("reference"),
                        e_var("documentParent"),
                    ],
                ))]),
        )
        .method(
            method("__elephc_dom_ns_op")
                .param("operation", TypeExpr::Int)
                .param("key", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![s_return(e_call(
                    "elephc_xml_sx_dom_ns",
                    vec![e_this_prop("__elephc_handle"), e_var("operation"), e_var("key")],
                ))]),
        )
        .build()
}

/// Adds the externs and the two methods to the built SimpleXML prelude.
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
    for mut declaration in [extern_dom_tree(), extern_dom_ns()] {
        declaration.span = crate::span::Span::synthetic();
        program.push(declaration);
    }
}
