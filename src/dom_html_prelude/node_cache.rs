//! Purpose:
//! Preserves PHP DOM wrapper identity without retaining otherwise unused native nodes.
//!
//! Called from:
//! - `crate::dom_html_prelude::declarations()` and SimpleXML's DOM wrapper creation and
//!   HTML import binding.
//!
//! Key details:
//! - Native node ids are compared only while their weak PHP wrappers remain alive.
//! - Geometric sweeps bound stale weak entries with amortized linear cleanup.
//! - A DOMDocument parameter lets EIR update the declared cache property directly.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `__elephc_dom_cached_node` — transcribed from the PHP form.
fn decl_fn_elephc_dom_cached_node() -> Stmt {
    function("__elephc_dom_cached_node")
        .param("document", t_mixed())
        .param("identity", TypeExpr::Int)
        .returns(t_nullable(t_class("DOMNode")))
        .body(vec![
            s_if(
                e_binop(e_not(e_instance_of(e_var("document"), "DOMDocument")), BinOp::Or, e_binop(e_var("identity"), BinOp::StrictEq, e_int(0))),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_assign("weak", e_null_coalesce(e_index(e_prop(e_var("document"), "__elephc_dom_nodes"), e_var("identity")), e_null())),
            s_if(
                e_instance_of(e_var("weak"), "WeakReference"),
                vec![
                    s_assign("node", e_method_call(e_var("weak"), "get", vec![])),
                    s_if(
                        e_instance_of(e_var("node"), "DOMNode"),
                        vec![
                            s_return(e_var("node")),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_null()),
        ])
        .build()
}

/// `__elephc_dom_remember_node` — transcribed from the PHP form.
fn decl_fn_elephc_dom_remember_node() -> Stmt {
    function("__elephc_dom_remember_node")
        .param("document", t_mixed())
        .param("identity", TypeExpr::Int)
        .param("node", t_class("DOMNode"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("identity"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_if(
                e_instance_of(e_var("document"), "DOMDocument"),
                vec![
                    s_expr(e_call("__elephc_dom_remember_in", vec![e_var("document"), e_var("identity"), e_var("node")])),
                ],
                vec![],
                None,
            ),
        ])
        .build()
}

/// `__elephc_dom_remember_in` — transcribed from the PHP form.
fn decl_fn_elephc_dom_remember_in() -> Stmt {
    function("__elephc_dom_remember_in")
        .param("document", t_class("DOMDocument"))
        .param("identity", TypeExpr::Int)
        .param("node", t_class("DOMNode"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_array_assign(e_var("document"), "__elephc_dom_nodes", e_var("identity"), e_static_call("WeakReference", "create", vec![e_var("node")])),
            s_if(
                e_binop(e_call("count", vec![e_prop(e_var("document"), "__elephc_dom_nodes")]), BinOp::Lt, e_prop(e_var("document"), "__elephc_dom_sweep")),
                vec![
                    s_return_void(),
                ],
                vec![],
                None,
            ),
            s_assign("live", e_array(vec![])),
            s_foreach(e_prop(e_var("document"), "__elephc_dom_nodes"), Some("key"), "weak", vec![
                s_if(
                    e_binop(e_instance_of(e_var("weak"), "WeakReference"), BinOp::And, e_binop(e_method_call(e_var("weak"), "get", vec![]), BinOp::StrictNotEq, e_null())),
                    vec![
                        s_array_assign("live", e_var("key"), e_var("weak")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_prop_assign(e_var("document"), "__elephc_dom_nodes", e_var("live")),
            s_assign("next", e_binop(e_call("count", vec![e_var("live")]), BinOp::Mul, e_int(2))),
            s_prop_assign(e_var("document"), "__elephc_dom_sweep", e_ternary(e_binop(e_var("next"), BinOp::Lt, e_int(64)), e_int(64), e_var("next"))),
        ])
        .build()
}

/// `__elephc_dom_drop_snapshot` — transcribed from the PHP form.
fn decl_fn_elephc_dom_drop_snapshot() -> Stmt {
    function("__elephc_dom_drop_snapshot")
        .param("node", t_class("DOMNode"))
        .returns(TypeExpr::Void)
        .body(vec![
            s_prop_assign(e_var("node"), "childNodes", e_null()),
            s_prop_assign(e_var("node"), "parentNode", e_null()),
            s_prop_assign(e_var("node"), "previousSibling", e_null()),
            s_prop_assign(e_var("node"), "nextSibling", e_null()),
        ])
        .build()
}

/// Builds this module's declarations, one helper per declaration above.
pub(super) fn declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_fn_elephc_dom_cached_node(),
            decl_fn_elephc_dom_remember_node(),
            decl_fn_elephc_dom_remember_in(),
            decl_fn_elephc_dom_drop_snapshot(),
        ]
    })
}
