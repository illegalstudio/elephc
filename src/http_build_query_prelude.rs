//! Purpose:
//! Builds PHP's `http_build_query()` as prelude declarations, for programs that name it.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations when the
//!   program references `http_build_query`; `crate::ir_lower::expr::compat_preludes` routes the
//!   builtin call to `__elephc_http_build_query`.
//!
//! Key details:
//! - Transcribes php-src's `php_url_encode_hash_ex`: integer keys take the numeric prefix at the
//!   top level only, nested keys are spelled `outer[inner]`, `null` values are skipped, booleans
//!   are `1`/`0`, and `$encoding_type` 2 (`PHP_QUERY_RFC3986`) selects `rawurlencode()`.
//! - An OBJECT, as `$data` or as any value inside it, contributes its public properties as if it
//!   were an array (`get_object_vars()` from function scope), and nothing when it has none -- a
//!   `Stringable` object is not converted to its string. Symfony's `UrlGenerator` hands every
//!   request attribute that is not a route variable to `http_build_query()`, authenticator
//!   objects among them, so the string cast this helper used to apply killed the login redirect.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_cast, e_int, e_null, e_str, e_ternary, e_var, function,
    php_semantics_declarations, s_array_push, s_assign, s_foreach, s_if, s_return, t_array, t_mixed,
};

/// Name of the recursive pair builder.
const PAIRS_NAME: &str = "__elephc_http_build_query_pairs";

/// Name of the helper that reads an object value as its public properties.
const FIELDS_NAME: &str = "__elephc_http_build_query_fields";

/// Returns the `http_build_query()` declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            decl_fn_fields(),
            decl_fn_pairs(),
            decl_fn_build(crate::backend_gap_prelude::HTTP_BUILD_QUERY_NAME),
            decl_fn_wrapper(),
        ]
    })
}

/// `$encodingType === 2 ? rawurlencode($text) : urlencode($text)`.
fn encode(value: Expr) -> Expr {
    e_ternary(
        e_binop(e_var("encodingType"), BinOp::StrictEq, e_int(2)),
        e_call("rawurlencode", vec![value.clone()]),
        e_call("urlencode", vec![value]),
    )
}

/// `$name = __elephc_http_build_query_fields($name);`.
///
/// A call rather than an inline `get_object_vars()` assignment: the local stays `mixed` on both
/// sides, so the checker has no type change to warn about in every program that builds a query.
fn objects_as_public_properties(name: &str) -> Stmt {
    s_assign(name, e_call(FIELDS_NAME, vec![e_var(name)]))
}

/// `__elephc_http_build_query_fields(mixed $value): mixed`: an object's public properties (from
/// function scope, as `get_object_vars()` sees them), any other value unchanged.
fn decl_fn_fields() -> Stmt {
    function(FIELDS_NAME)
        .param("value", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![s_return(e_call("get_object_vars", vec![e_var("value")]))],
                vec![],
                None,
            ),
            s_return(e_var("value")),
        ])
        .build()
}

/// `__elephc_http_build_query_pairs(array $data, string $prefix, string $numericPrefix,
/// int $encodingType): array`, returning the encoded `key=value` pairs in order.
fn decl_fn_pairs() -> Stmt {
    let key_text = e_ternary(
        e_call("is_int", vec![e_var("key")]),
        e_binop(
            e_var("numericPrefix"),
            BinOp::Concat,
            e_cast(CastType::String, e_var("key")),
        ),
        e_cast(CastType::String, e_var("key")),
    );
    let full_key = e_ternary(
        e_binop(e_var("prefix"), BinOp::StrictEq, e_str("")),
        e_var("keyText"),
        e_binop(
            e_binop(
                e_binop(e_var("prefix"), BinOp::Concat, e_str("[")),
                BinOp::Concat,
                e_var("keyText"),
            ),
            BinOp::Concat,
            e_str("]"),
        ),
    );
    let text = e_ternary(
        e_call("is_bool", vec![e_var("value")]),
        e_ternary(e_var("value"), e_str("1"), e_str("0")),
        e_cast(CastType::String, e_var("value")),
    );
    let nested = s_foreach(
        e_call(
            PAIRS_NAME,
            vec![e_var("value"), e_var("fullKey"), e_str(""), e_var("encodingType")],
        ),
        None,
        "pair",
        vec![s_array_push("pairs", e_var("pair"))],
    );
    let scalar = vec![
        s_assign("text", text),
        s_array_push(
            "pairs",
            e_binop(
                e_binop(encode(e_var("fullKey")), BinOp::Concat, e_str("=")),
                BinOp::Concat,
                encode(e_var("text")),
            ),
        ),
    ];
    let body = vec![
        s_assign("pairs", e_array(vec![])),
        s_foreach(
            e_var("data"),
            Some("key"),
            "value",
            vec![
                s_assign("keyText", key_text),
                s_assign("fullKey", full_key),
                objects_as_public_properties("value"),
                s_if(
                    e_call("is_array", vec![e_var("value")]),
                    vec![nested],
                    vec![(
                        e_binop(e_var("value"), BinOp::StrictNotEq, e_null()),
                        scalar,
                    )],
                    None,
                ),
            ],
        ),
        s_return(e_var("pairs")),
    ];
    function(PAIRS_NAME)
        .param("data", t_array())
        .param("prefix", TypeExpr::Str)
        .param("numericPrefix", TypeExpr::Str)
        .param("encodingType", TypeExpr::Int)
        .returns(t_array())
        .body(body)
        .build()
}

/// The shared signature of the helper and its PHP-visible wrapper:
/// `(array|object $data, string $numeric_prefix = '', mixed $arg_separator = null,
/// int $encoding_type = 1): string`, with `$data` declared `mixed`. The names are php's own, so
/// named arguments (`http_build_query(data: $d, arg_separator: '&')`) bind.
fn with_signature(name: &str, body: Vec<Stmt>) -> Stmt {
    function(name)
        .param("data", t_mixed())
        .param_default("numeric_prefix", TypeExpr::Str, e_str(""))
        .param_default("arg_separator", t_mixed(), e_null())
        .param_default("encoding_type", TypeExpr::Int, e_int(1))
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}

/// `__elephc_http_build_query(...)`, the helper the builtin call lowers to.
fn decl_fn_build(name: &str) -> Stmt {
    let separator = e_ternary(
        e_binop(e_var("arg_separator"), BinOp::StrictEq, e_null()),
        e_str("&"),
        e_cast(CastType::String, e_var("arg_separator")),
    );
    with_signature(
        name,
        vec![
            objects_as_public_properties("data"),
            s_assign("separator", separator),
            s_return(e_call(
                "implode",
                vec![
                    e_var("separator"),
                    e_call(
                        PAIRS_NAME,
                        vec![e_var("data"), e_str(""), e_var("numeric_prefix"), e_var("encoding_type")],
                    ),
                ],
            )),
        ],
    )
}

/// The PHP-visible `http_build_query(...)`, for callable and `eval()` callers.
fn decl_fn_wrapper() -> Stmt {
    with_signature(
        "http_build_query",
        vec![s_return(e_call(
            crate::backend_gap_prelude::HTTP_BUILD_QUERY_NAME,
            vec![
                e_var("data"),
                e_var("numeric_prefix"),
                e_var("arg_separator"),
                e_var("encoding_type"),
            ],
        ))],
    )
}
