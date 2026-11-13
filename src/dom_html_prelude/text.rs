//! Purpose:
//! Reads unbound DOM document text without including comments or processing instructions.
//!
//! Called from:
//! - `crate::dom_html_prelude::declarations()` and the legacy DOMDocument textContent
//!   property hook.
//!
//! Key details:
//! - Bound documents use their shared native tree; HTML-only documents walk their node lists.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `__elephc_dom_descendant_text` — transcribed from the PHP form.
fn decl_fn_elephc_dom_descendant_text() -> Stmt {
    function("__elephc_dom_descendant_text")
        .param("node", t_class("DOMNode"))
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_instance_of(e_var("node"), "DOMText"),
                vec![
                    s_return(e_cast(CastType::String, e_prop(e_var("node"), "nodeValue"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_not(e_instance_of(e_var("node"), "DOMElement")), BinOp::And, e_not(e_instance_of(e_var("node"), "DOMDocument"))),
                vec![
                    s_return(e_str("")),
                ],
                vec![],
                None,
            ),
            s_assign("text", e_str("")),
            s_assign("children", e_prop(e_var("node"), "childNodes")),
            s_if(
                e_instance_of(e_var("children"), "DOMNodeList"),
                vec![
                    s_foreach(e_var("children"), None, "child", vec![
                        s_if(
                            e_instance_of(e_var("child"), "DOMNode"),
                            vec![
                                s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_call("__elephc_dom_descendant_text", vec![e_var("child")]))),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                ],
                vec![],
                None,
            ),
            s_return(e_var("text")),
        ])
        .build()
}

/// Builds this module's declarations, one helper per declaration above.
pub(super) fn declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_fn_elephc_dom_descendant_text(),
        ]
    })
}
