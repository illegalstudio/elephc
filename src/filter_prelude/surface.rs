//! Purpose:
//! Builds the reflection-visible filter surface that does not depend on a value: exceptions,
//! `filter_list()` and `filter_id()`.
//!
//! Called from:
//! - `crate::filter_prelude::filter_declarations`.
//!
//! Key details:
//! - The filter table order and names are copied from php-src's `filter_list[]`.
//! - `filter_id()` is case-sensitive and returns `false` for unknown names.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    class, e_array, e_binop, e_bool, e_call, e_int, e_str, e_var, function, s_assign, s_if,
    s_namespace, s_return, t_mixed, t_union,
};

/// The ordered filter table exactly as php-src exposes it through filter_list().
pub(crate) const FILTER_TABLE: &[(&str, i64)] = &[
    ("int", 257),
    ("boolean", 258),
    ("float", 259),
    ("validate_regexp", 272),
    ("validate_domain", 277),
    ("validate_url", 273),
    ("validate_email", 274),
    ("validate_ip", 275),
    ("validate_mac", 276),
    ("string", 513),
    ("stripped", 513),
    ("encoded", 514),
    ("special_chars", 515),
    ("full_special_chars", 522),
    ("unsafe_raw", 516),
    ("email", 517),
    ("url", 518),
    ("number_int", 519),
    ("number_float", 520),
    ("add_slashes", 523),
    ("callback", 1024),
];

/// Returns the exception classes, `filter_list()` and `filter_id()`.
pub(super) fn declarations() -> Program {
    let mut declarations = exceptions();
    declarations.push(filter_list_decl());
    declarations.push(filter_id_decl());
    declarations
}

/// Builds the `Filter\FilterException` and `Filter\FilterFailedException` hierarchy.
fn exceptions() -> Program {
    vec![s_namespace(
        "Filter",
        vec![
            class("FilterException")
                .extends("\\Exception")
                .build(),
            class("FilterFailedException")
                .extends("\\Filter\\FilterException")
                .build(),
        ],
    )]
}

/// Builds `filter_list(): array`.
fn filter_list_decl() -> Stmt {
    function("filter_list")
        .returns(crate::synthetic_class::t_array())
        .returning(e_array(
            FILTER_TABLE.iter().map(|(name, _)| e_str(name)).collect(),
        ))
        .build()
}

/// Builds `filter_id(mixed $name): mixed`.
fn filter_id_decl() -> Stmt {
    let mut body = vec![s_assign(
        "name",
        e_call(
            "__elephc_filter_string_arg",
            vec![e_var("name"), e_str("filter_id"), e_int(1), e_str("name")],
        ),
    )];
    for (name, id) in FILTER_TABLE {
        body.push(s_if(
            e_binop(e_var("name"), BinOp::StrictEq, e_str(name)),
            vec![s_return(e_int(*id))],
            vec![],
            None,
        ));
    }
    body.push(s_return(e_bool(false)));
    function("filter_id")
        .param("name", t_mixed())
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
        .body(body)
        .build()
}
