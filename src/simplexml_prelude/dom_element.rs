//! Purpose:
//! The SimpleXML-side bridge of DOM package P5: one extern of the `elephc_xml` bridge and the
//! `SimpleXMLElement` method the DOM prelude calls to read, write and list the attributes of an
//! element of the shared libxml2 tree.
//!
//! Called from:
//! - `crate::simplexml_prelude::declarations()` (through `extend`).
//!
//! Key details:
//! - The DOM prelude reaches the bridge only through this method, called on the bound
//!   `SimpleXMLElement` object, so a free-standing DOM never references the extern.
//! - `elephc_xml_sx_dom_element` lives in `crates/elephc-xml/src/simplexml/dom_element.rs`; it
//!   answers `1` + payload when the attribute exists / the write was done and "" otherwise.

use crate::parser::ast::{CType, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::*;

fn extern_dom_element() -> Stmt {
    extern_fn("elephc_xml_sx_dom_element", "elephc_xml")
        .param("handle", CType::Int)
        .param("operation", CType::Int)
        .param("uri", CType::Str)
        .param("name", CType::Str)
        .param("value", CType::Str)
        .returns(CType::Str)
        .build()
}

fn members() -> Stmt {
    class("SimpleXMLElement")
        .method(
            method("__elephc_dom_element_op")
                .param("operation", TypeExpr::Int)
                .param_default("uri", TypeExpr::Str, e_str(""))
                .param_default("name", TypeExpr::Str, e_str(""))
                .param_default("value", TypeExpr::Str, e_str(""))
                .returns(TypeExpr::Str)
                .body(vec![s_return(e_call(
                    "elephc_xml_sx_dom_element",
                    vec![
                        e_this_prop("__elephc_handle"),
                        e_var("operation"),
                        e_var("uri"),
                        e_var("name"),
                        e_var("value"),
                    ],
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
    let mut declaration = extern_dom_element();
    declaration.span = crate::span::Span::synthetic();
    program.push(declaration);
}
