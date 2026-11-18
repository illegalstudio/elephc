//! Purpose:
//! Package P3 of the DOM migration: `DOMNamedNodeMap`. (`DOMNodeList` already has everything php
//! declares in `generated.rs`; php has no `toArray`, so none is added.)
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - The map is a snapshot built by whoever creates it (the element package for `attributes`,
//!   `DOMDocumentType` for `entities` / `notations`), keyed by qualified name, like
//!   `DOMNodeList::__items`. Attribute namespaces are read from `DOMAttr::__elephc_dom_ns`.

use super::extend::{Contributions, Part};
use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::synthetic_class::*;

fn named_node_map() -> Stmt {
    class("DOMNamedNodeMap")
        .implements("IteratorAggregate")
        .implements("Countable")
        .prop("length", TypeExpr::Int, Some(e_int(0)))
        .prop("__items", t_mixed(), Some(e_array(vec![])))
        .method(
            method("__construct").param_default("items", t_mixed(), e_array(vec![])).body(vec![
                s_assign("_list", e_array(vec![])),
                s_if(
                    e_call("is_array", vec![e_var("items")]),
                    vec![s_assign("_list", e_var("items"))],
                    vec![],
                    None,
                ),
                s_prop_assign(e_this(), "__items", e_var("_list")),
                s_prop_assign(e_this(), "length", e_call("count", vec![e_var("_list")])),
            ]),
        )
        .method(
            method("getNamedItem")
                .param("qualifiedName", TypeExpr::Str)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_foreach(
                        e_this_prop("__items"),
                        Some("_key"),
                        "_node",
                        vec![s_if(
                            e_binop(e_cast(CastType::String, e_var("_key")), BinOp::StrictEq, e_var("qualifiedName")),
                            vec![s_return(e_var("_node"))],
                            vec![],
                            None,
                        )],
                    ),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("getNamedItemNS")
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("localName", TypeExpr::Str)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_assign("_wanted", e_null_coalesce(e_var("namespace"), e_str(""))),
                    s_foreach(
                        e_this_prop("__items"),
                        Some("_key"),
                        "_node",
                        vec![s_if(
                            e_instance_of(e_var("_node"), "DOMAttr"),
                            vec![
                                s_assign("_name", e_cast(CastType::String, e_var("_key"))),
                                s_assign("_colon", e_call("strrpos", vec![e_var("_name"), e_str(":")])),
                                s_assign("_local", e_var("_name")),
                                s_if(
                                    e_binop(e_var("_colon"), BinOp::StrictNotEq, e_bool(false)),
                                    vec![s_assign(
                                        "_local",
                                        e_call("substr", vec![e_var("_name"), e_binop(e_var("_colon"), BinOp::Add, e_int(1))]),
                                    )],
                                    vec![],
                                    None,
                                ),
                                s_assign("_ns", e_null_coalesce(e_prop(e_var("_node"), "__elephc_dom_ns"), e_str(""))),
                                s_if(
                                    e_binop(
                                        e_binop(e_var("_local"), BinOp::StrictEq, e_var("localName")),
                                        BinOp::And,
                                        e_binop(e_var("_ns"), BinOp::StrictEq, e_var("_wanted")),
                                    ),
                                    vec![s_return(e_var("_node"))],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        )],
                    ),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("item")
                .param("index", TypeExpr::Int)
                .returns(t_nullable(t_class("DOMNode")))
                .body(vec![
                    s_if(
                        e_binop(
                            e_binop(e_var("index"), BinOp::Lt, e_int(0)),
                            BinOp::Or,
                            e_binop(e_var("index"), BinOp::GtEq, e_this_prop("length")),
                        ),
                        vec![s_return(e_null())],
                        vec![],
                        None,
                    ),
                    s_assign("_values", e_call("array_values", vec![e_this_prop("__items")])),
                    s_assign("_node", e_index(e_var("_values"), e_var("index"))),
                    s_if(e_instance_of(e_var("_node"), "DOMNode"), vec![s_return(e_var("_node"))], vec![], None),
                    s_return(e_null()),
                ]),
        )
        .method(method("count").returns(TypeExpr::Int).body(vec![s_return(e_this_prop("length"))]))
        .method(
            method("getIterator")
                .returns(t_class("Iterator"))
                .body(vec![s_return(e_new("ArrayIterator", vec![e_this_prop("__items")]))]),
        )
        .build()
}

/// Registers this package's declarations.
pub(super) fn contribute(contributions: &mut Contributions) {
    contributions.add_class(Part::Walker, named_node_map());
}
