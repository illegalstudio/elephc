//! Purpose:
//! Builds the `Uri` namespace of the URI prelude from the synthetic AST: the exception
//! hierarchy and `UriComparisonMode`.
//!
//! Called from:
//! - `crate::uri_prelude::uri_declarations`.
//!
//! Key details:
//! - Declarations are built in `SourceMode::Php`, not internal mode, so user-facing method
//!   and debug-info behavior is the ordinary class pipeline.

use crate::parser::ast::{BinOp, EnumCaseDecl, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::synthetic_class::*;

/// Builds `__elephc_uri_type_name(mixed $value): string`, PHP's value-name spelling.
///
/// The URI methods keep a wider internal `mixed` parameter contract so a `?Uri` value and a
/// runtime-checked argument stay callable; this helper supplies the `, <type> given` half of
/// php-src's `TypeError` message when the manual guard rejects one.
pub(super) fn uri_type_name_declaration() -> Stmt {
    function("__elephc_uri_type_name")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![s_return(e_call("get_class", vec![e_var("value")]))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![s_return(e_ternary(e_var("value"), e_str("true"), e_str("false")))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                vec![s_return(e_str("null"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![s_return(e_str("int"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("value")]),
                vec![s_return(e_str("float"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![s_return(e_str("string"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![s_return(e_str("array"))],
                vec![],
                None,
            ),
            s_return(e_str("resource")),
        ])
        .build()
}

/// Builds one URI prelude declaration.
pub(super) fn uri_types_declaration() -> Stmt {
    s_namespace("Uri", vec![
        class("UriException").span(Span::new(1, 1))
            .extends("\\Exception")
            .build(),
        class("UriError").span(Span::new(1, 1))
            .extends("\\Error")
            .build(),
        class("InvalidUriException").span(Span::new(1, 1))
            .extends("\\Uri\\UriException")
            .build(),
        Stmt::new(StmtKind::EnumDecl { name: "UriComparisonMode".to_string(), generics: None, backing_type: None, cases: vec![EnumCaseDecl { name: "IncludeFragment".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "ExcludeFragment".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }], implements: Vec::new(), trait_uses: Vec::new(), methods: Vec::new(), constants: Vec::new() }, Span::dummy())
    ])
}
