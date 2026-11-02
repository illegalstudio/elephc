//! Purpose:
//! Builds the `filter_var()` filters the literal lowering has no runtime for: FILTER_VALIDATE_URL,
//! FILTER_VALIDATE_EMAIL, FILTER_VALIDATE_MAC, FILTER_VALIDATE_DOMAIN and FILTER_CALLBACK, as
//! AST-built internal helpers
//! behind `__elephc_filter_var_ext($value, $filter, $options)`.
//!
//! Called from:
//! - `crate::filter_var_prelude::inject_if_used`, which appends them whenever the program names
//!   `filter_var`, under the same reachability group as the dynamic-filter helpers.
//! - `crate::ir_lower::expr::filter::lower_static_filter_var`, which routes a literal call with
//!   one of those ids to [`FILTER_VAR_EXT_NAME`].
//!
//! Key details:
//! - Transcribed from php-src 8.5's `ext/filter` (`php_filter_validate_url` with the
//!   `php_filter_url` sanitize check and the parse_url parser, `php_filter_validate_domain_ex`,
//!   `php_filter_validate_email`'s two regexes, `php_filter_validate_mac` with its `separator`
//!   option, `php_filter_callback`'s recursive array walk) and
//!   checked against php with a differential oracle before transcription.
//! - `$options` follows php: an int is the flags; an array carries `flags`, and `options` is the
//!   callback (FILTER_CALLBACK) or holds `default` (the failure value). FILTER_NULL_ON_FAILURE
//!   turns the failure sentinel into null. A missing or invalid callback raises php's TypeError.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_const, e_index, e_int, e_neg, e_new, e_not, e_null, e_null_coalesce, e_post_inc, e_str, e_ternary, e_var, function, internal_declarations, s_array_assign, s_assign, s_expr, s_for, s_foreach, s_if, s_return, s_throw, s_while, t_mixed, t_nullable,
};

/// The helper a literal `filter_var()` with an extended filter id is lowered to.
pub(crate) const FILTER_VAR_EXT_NAME: &str = "__elephc_filter_var_ext";

/// Filter ids [`FILTER_VAR_EXT_NAME`] implements: VALIDATE_URL, VALIDATE_EMAIL, VALIDATE_MAC,
/// VALIDATE_DOMAIN and CALLBACK.
pub(crate) const EXTENDED_FILTER_IDS: &[i64] = &[273, 274, 276, 277, 1024];

/// FILTER_VALIDATE_EMAIL, the one helper that needs the regex runtime.
const FILTER_VALIDATE_EMAIL: i64 = 274;

/// Which extended filters `program` names as a literal `filter_var()` filter: `(any, email)`.
///
/// Only those calls are lowered to the helpers (a run-time filter id goes to the dynamic helper,
/// which fails closed for these ids), so a program naming none carries no helper, and the email
/// regex — the only `preg_match` — makes no program without FILTER_VALIDATE_EMAIL need pcre2.
pub(crate) fn extended_filters_used(program: &Program) -> (bool, bool) {
    use crate::parser::ast::visit::{walk_nodes, Node};
    use crate::parser::ast::ExprKind;
    let (mut any, mut email) = (false, false);
    walk_nodes(program, &mut |node| {
        let Node::Expr(expr) = node else { return };
        let ExprKind::FunctionCall { name, args } = &expr.kind else { return };
        if !name.as_str().trim_start_matches('\\').eq_ignore_ascii_case("filter_var") {
            return;
        }
        let Some(id) = args.get(1).and_then(crate::types::filter_constants::static_filter_int)
        else {
            return;
        };
        any |= EXTENDED_FILTER_IDS.contains(&id);
        email |= id == FILTER_VALIDATE_EMAIL;
    });
    (any, email)
}

/// `__elephc_filter_var_ext` — transcribed from the PHP form.
fn decl_fn_elephc_filter_var_ext() -> Stmt {
    function("__elephc_filter_var_ext")
        .param("value", t_mixed())
        .param("filter", TypeExpr::Int)
        .param_default("options", t_mixed(), e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_assign("flags", e_int(0)),
            s_assign("callback", e_null()),
            s_assign("has_default", e_bool(false)),
            s_assign("default", e_null()),
            s_assign("separator", e_null()),
            s_if(
                e_call("is_array", vec![e_var("options")]),
                vec![
                    s_if(
                        e_call("isset", vec![e_index(e_var("options"), e_str("flags"))]),
                        vec![
                            s_assign("flags", e_call("intval", vec![e_index(e_var("options"), e_str("flags"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("array_key_exists", vec![e_str("options"), e_var("options")]),
                        vec![
                            s_if(
                                e_binop(e_var("filter"), BinOp::StrictEq, e_int(1024)),
                                vec![
                                    s_assign("callback", e_index(e_var("options"), e_str("options"))),
                                ],
                                vec![
                                (e_call("is_array", vec![e_index(e_var("options"), e_str("options"))]), vec![
                                    s_if(
                                        e_call("array_key_exists", vec![e_str("default"), e_index(e_var("options"), e_str("options"))]),
                                        vec![
                                            s_assign("has_default", e_bool(true)),
                                            s_assign("default", e_index(e_index(e_var("options"), e_str("options")), e_str("default"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_binop(e_var("filter"), BinOp::StrictEq, e_int(276)), BinOp::And, e_call("array_key_exists", vec![e_str("separator"), e_index(e_var("options"), e_str("options"))])),
                                        vec![
                                            s_assign("separator", e_call("strval", vec![e_index(e_index(e_var("options"), e_str("options")), e_str("separator"))])),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                            ],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                Some(vec![
                s_assign("flags", e_call("intval", vec![e_var("options")])),
            ]),
            ),
            s_if(
                e_binop(e_var("filter"), BinOp::StrictEq, e_int(1024)),
                vec![
                    s_if(
                        e_binop(e_binop(e_var("callback"), BinOp::StrictEq, e_null()), BinOp::Or, e_not(e_call("is_callable", vec![e_var("callback")]))),
                        vec![
                            s_throw(e_new("TypeError", vec![e_str("filter_var(): Option must be a valid callback")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_filter_callback", vec![e_var("value"), e_var("callback")])),
                ],
                vec![],
                None,
            ),
            s_assign("failure", e_ternary(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(134217728)), BinOp::StrictNotEq, e_int(0)), e_null(), e_bool(false))),
            s_if(
                e_binop(e_binop(e_var("separator"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_call("strlen", vec![e_var("separator")]), BinOp::StrictNotEq, e_int(1))),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("filter_var(): \"separator\" option must be one character long")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("has_default"),
                vec![
                    s_assign("failure", e_var("default")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_call("is_object", vec![e_var("value")])), BinOp::Or, e_call("is_resource", vec![e_var("value")])),
                vec![
                    s_return(e_var("failure")),
                ],
                vec![],
                None,
            ),
            s_assign("string", e_ternary(e_call("is_bool", vec![e_var("value")]), e_ternary(e_var("value"), e_str("1"), e_str("")), e_call("strval", vec![e_var("value")]))),
            s_assign("ok", e_bool(false)),
            s_if(
                e_binop(e_var("filter"), BinOp::StrictEq, e_int(277)),
                vec![
                    s_assign("ok", e_call("__elephc_filter_domain", vec![e_var("string"), e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1048576)), BinOp::StrictNotEq, e_int(0))])),
                ],
                vec![
                (e_binop(e_var("filter"), BinOp::StrictEq, e_int(273)), vec![
                    s_assign("ok", e_call("__elephc_filter_url", vec![e_var("string"), e_var("flags")])),
                ]),
                (e_binop(e_var("filter"), BinOp::StrictEq, e_int(274)), vec![
                    s_assign("ok", e_call("__elephc_filter_email", vec![e_var("string"), e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1048576)), BinOp::StrictNotEq, e_int(0))])),
                ]),
                (e_binop(e_var("filter"), BinOp::StrictEq, e_int(276)), vec![
                    s_assign("ok", e_call("__elephc_filter_mac", vec![e_var("string"), e_var("separator")])),
                ]),
            ],
                None,
            ),
            s_return(e_ternary(e_var("ok"), e_var("string"), e_var("failure"))),
        ])
        .build()
}

/// `__elephc_filter_callback` — transcribed from the PHP form.
fn decl_fn_elephc_filter_callback() -> Stmt {
    function("__elephc_filter_callback")
        .param("value", t_mixed())
        .param("callback", t_mixed())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_assign("out", e_array(vec![])),
                    s_foreach(e_var("value"), Some("key"), "item", vec![
                        s_array_assign("out", e_var("key"), e_call("__elephc_filter_callback", vec![e_var("item"), e_var("callback")])),
                    ]),
                    s_return(e_var("out")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_call("is_object", vec![e_var("value")]), BinOp::Or, e_call("is_resource", vec![e_var("value")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("string", e_ternary(e_call("is_bool", vec![e_var("value")]), e_ternary(e_var("value"), e_str("1"), e_str("")), e_call("strval", vec![e_var("value")]))),
            s_return(e_call("call_user_func", vec![e_var("callback"), e_var("string")])),
        ])
        .build()
}

/// `__elephc_filter_alnum` — transcribed from the PHP form.
fn decl_fn_elephc_filter_alnum() -> Stmt {
    function("__elephc_filter_alnum")
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_var("c"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_call("strspn", vec![e_var("c"), e_str("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")]), BinOp::StrictEq, e_int(1)))),
        ])
        .build()
}

/// `__elephc_filter_domain` — transcribed from the PHP form.
fn decl_fn_elephc_filter_domain() -> Stmt {
    function("__elephc_filter_domain")
        .param("domain", TypeExpr::Str)
        .param("hostname", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("l", e_call("strlen", vec![e_var("domain")])),
            s_if(
                e_binop(e_binop(e_var("l"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_index(e_var("domain"), e_binop(e_var("l"), BinOp::Sub, e_int(1))), BinOp::StrictEq, e_str("."))),
                vec![
                    s_assign("l", e_binop(e_var("l"), BinOp::Sub, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("l"), BinOp::Gt, e_int(253)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("first", e_ternary(e_binop(e_var("l"), BinOp::Gt, e_int(0)), e_index(e_var("domain"), e_int(0)), e_str(""))),
            s_if(
                e_binop(e_binop(e_var("first"), BinOp::StrictEq, e_str(".")), BinOp::Or, e_binop(e_var("hostname"), BinOp::And, e_not(e_call("__elephc_filter_alnum", vec![e_var("first")])))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("label", e_int(1)),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("l"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("c", e_index(e_var("domain"), e_var("i"))),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str(".")),
                    vec![
                        s_assign("next", e_ternary(e_binop(e_binop(e_var("i"), BinOp::Add, e_int(1)), BinOp::Lt, e_call("strlen", vec![e_var("domain")])), e_index(e_var("domain"), e_binop(e_var("i"), BinOp::Add, e_int(1))), e_str(""))),
                        s_assign("prev", e_ternary(e_binop(e_var("i"), BinOp::Gt, e_int(0)), e_index(e_var("domain"), e_binop(e_var("i"), BinOp::Sub, e_int(1))), e_str(""))),
                        s_if(
                            e_binop(e_binop(e_var("next"), BinOp::StrictEq, e_str(".")), BinOp::Or, e_binop(e_var("hostname"), BinOp::And, e_binop(e_not(e_call("__elephc_filter_alnum", vec![e_var("prev")])), BinOp::Or, e_not(e_call("__elephc_filter_alnum", vec![e_var("next")]))))),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("label", e_int(1)),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("next", e_ternary(e_binop(e_binop(e_var("i"), BinOp::Add, e_int(1)), BinOp::Lt, e_call("strlen", vec![e_var("domain")])), e_index(e_var("domain"), e_binop(e_var("i"), BinOp::Add, e_int(1))), e_str(""))),
                    s_if(
                        e_binop(e_binop(e_var("label"), BinOp::Gt, e_int(63)), BinOp::Or, e_binop(e_binop(e_var("hostname"), BinOp::And, e_binop(e_binop(e_var("c"), BinOp::StrictNotEq, e_str("-")), BinOp::Or, e_binop(e_var("next"), BinOp::StrictEq, e_str("")))), BinOp::And, e_not(e_call("__elephc_filter_alnum", vec![e_var("c")])))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("label", e_binop(e_var("label"), BinOp::Add, e_int(1))),
                ]),
                ),
            ]),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_filter_userinfo` — transcribed from the PHP form.
fn decl_fn_elephc_filter_userinfo() -> Stmt {
    function("__elephc_filter_userinfo")
        .param("s", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("n", e_call("strlen", vec![e_var("s")])),
            s_assign("i", e_int(0)),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_var("n")), vec![
                s_assign("c", e_index(e_var("s"), e_var("i"))),
                s_if(
                    e_binop(e_call("__elephc_filter_alnum", vec![e_var("c")]), BinOp::Or, e_binop(e_call("strpos", vec![e_str("-._~!$&'()*+,;=:"), e_var("c")]), BinOp::StrictNotEq, e_bool(false))),
                    vec![
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                    ],
                    vec![
                    (e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("%")), BinOp::And, e_binop(e_var("i"), BinOp::LtEq, e_binop(e_var("n"), BinOp::Sub, e_int(3)))), BinOp::And, e_binop(e_call("strspn", vec![e_index(e_var("s"), e_binop(e_var("i"), BinOp::Add, e_int(1))), e_str("0123456789")]), BinOp::StrictEq, e_int(1))), BinOp::And, e_binop(e_call("strspn", vec![e_index(e_var("s"), e_binop(e_var("i"), BinOp::Add, e_int(2))), e_str("0123456789abcdefABCDEF")]), BinOp::StrictEq, e_int(1))), vec![
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(3))),
                    ]),
                ],
                    Some(vec![
                    s_return(e_bool(false)),
                ]),
                ),
            ]),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_filter_url` — transcribed from the PHP form.
fn decl_fn_elephc_filter_url() -> Stmt {
    function("__elephc_filter_url")
        .param("url", TypeExpr::Str)
        .param("flags", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("allowed", e_str("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789$-_.+!*'(),{}|\\^~[]`<>#%\";/?:@&=")),
            s_if(
                e_binop(e_call("strspn", vec![e_var("url"), e_var("allowed")]), BinOp::StrictNotEq, e_call("strlen", vec![e_var("url")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("parts", e_call("parse_url", vec![e_var("url")])),
            s_if(
                e_binop(e_var("parts"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("scheme", e_null_coalesce(e_index(e_var("parts"), e_str("scheme")), e_null())),
            s_assign("host", e_null_coalesce(e_index(e_var("parts"), e_str("host")), e_null())),
            s_if(
                e_binop(e_binop(e_var("scheme"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_binop(e_call("strcasecmp", vec![e_var("scheme"), e_str("http")]), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_call("strcasecmp", vec![e_var("scheme"), e_str("https")]), BinOp::StrictEq, e_int(0)))),
                vec![
                    s_if(
                        e_binop(e_var("host"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("ipv6", e_binop(e_binop(e_binop(e_binop(e_call("strlen", vec![e_var("host")]), BinOp::GtEq, e_int(2)), BinOp::And, e_binop(e_index(e_var("host"), e_int(0)), BinOp::StrictEq, e_str("["))), BinOp::And, e_binop(e_index(e_var("host"), e_binop(e_call("strlen", vec![e_var("host")]), BinOp::Sub, e_int(1))), BinOp::StrictEq, e_str("]"))), BinOp::And, e_binop(e_call("filter_var", vec![e_call("substr", vec![e_var("host"), e_int(1), e_neg(e_int(1))]), e_const("FILTER_VALIDATE_IP"), e_const("FILTER_FLAG_IPV6")]), BinOp::StrictNotEq, e_bool(false)))),
                    s_if(
                        e_binop(e_not(e_var("ipv6")), BinOp::And, e_not(e_call("__elephc_filter_domain", vec![e_var("host"), e_bool(true)]))),
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
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("scheme"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_binop(e_binop(e_binop(e_var("host"), BinOp::StrictEq, e_null()), BinOp::And, e_binop(e_var("scheme"), BinOp::StrictNotEq, e_str("mailto"))), BinOp::And, e_binop(e_var("scheme"), BinOp::StrictNotEq, e_str("news"))), BinOp::And, e_binop(e_var("scheme"), BinOp::StrictNotEq, e_str("file")))), BinOp::Or, e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(262144)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_not(e_call("isset", vec![e_index(e_var("parts"), e_str("path"))])))), BinOp::Or, e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(524288)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_not(e_call("isset", vec![e_index(e_var("parts"), e_str("query"))])))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("isset", vec![e_index(e_var("parts"), e_str("user"))]), BinOp::And, e_not(e_call("__elephc_filter_userinfo", vec![e_index(e_var("parts"), e_str("user"))]))), BinOp::Or, e_binop(e_call("isset", vec![e_index(e_var("parts"), e_str("pass"))]), BinOp::And, e_not(e_call("__elephc_filter_userinfo", vec![e_index(e_var("parts"), e_str("pass"))])))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_filter_email` — transcribed from the PHP form.
fn decl_fn_elephc_filter_email() -> Stmt {
    function("__elephc_filter_email")
        .param("email", TypeExpr::Str)
        .param("unicode", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_call("strlen", vec![e_var("email")]), BinOp::Gt, e_int(320)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("re", e_ternary(e_var("unicode"), e_str("/^(?!(?:(?:\\x22?\\x5C[\\x00-\\x7E]\\x22?)|(?:\\x22?[^\\x5C\\x22]\\x22?)){255,})(?!(?:(?:\\x22?\\x5C[\\x00-\\x7E]\\x22?)|(?:\\x22?[^\\x5C\\x22]\\x22?)){65,}@)(?:(?:[\\x21\\x23-\\x27\\x2A\\x2B\\x2D\\x2F-\\x39\\x3D\\x3F\\x5E-\\x7E\\pL\\pN]+)|(?:\\x22(?:[\\x01-\\x08\\x0B\\x0C\\x0E-\\x1F\\x21\\x23-\\x5B\\x5D-\\x7F\\pL\\pN]|(?:\\x5C[\\x00-\\x7F]))*\\x22))(?:\\.(?:(?:[\\x21\\x23-\\x27\\x2A\\x2B\\x2D\\x2F-\\x39\\x3D\\x3F\\x5E-\\x7E\\pL\\pN]+)|(?:\\x22(?:[\\x01-\\x08\\x0B\\x0C\\x0E-\\x1F\\x21\\x23-\\x5B\\x5D-\\x7F\\pL\\pN]|(?:\\x5C[\\x00-\\x7F]))*\\x22)))*@(?:(?:(?!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\\.){1,126}){1,}(?:(?:[a-z][a-z0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)|(?:\\[(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\\]]){7,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))|(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))?(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\\.(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))){3}))\\]))$/iDu"), e_str("/^(?!(?:(?:\\x22?\\x5C[\\x00-\\x7E]\\x22?)|(?:\\x22?[^\\x5C\\x22]\\x22?)){255,})(?!(?:(?:\\x22?\\x5C[\\x00-\\x7E]\\x22?)|(?:\\x22?[^\\x5C\\x22]\\x22?)){65,}@)(?:(?:[\\x21\\x23-\\x27\\x2A\\x2B\\x2D\\x2F-\\x39\\x3D\\x3F\\x5E-\\x7E]+)|(?:\\x22(?:[\\x01-\\x08\\x0B\\x0C\\x0E-\\x1F\\x21\\x23-\\x5B\\x5D-\\x7F]|(?:\\x5C[\\x00-\\x7F]))*\\x22))(?:\\.(?:(?:[\\x21\\x23-\\x27\\x2A\\x2B\\x2D\\x2F-\\x39\\x3D\\x3F\\x5E-\\x7E]+)|(?:\\x22(?:[\\x01-\\x08\\x0B\\x0C\\x0E-\\x1F\\x21\\x23-\\x5B\\x5D-\\x7F]|(?:\\x5C[\\x00-\\x7F]))*\\x22)))*@(?:(?:(?!.*[^.]{64,})(?:(?:(?:xn--)?[a-z0-9]+(?:-+[a-z0-9]+)*\\.){1,126}){1,}(?:(?:[a-z][a-z0-9]*)|(?:(?:xn--)[a-z0-9]+))(?:-+[a-z0-9]+)*)|(?:\\[(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){7})|(?:(?!(?:.*[a-f0-9][:\\]]){7,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,5})?)))|(?:(?:IPv6:(?:(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){5}:)|(?:(?!(?:.*[a-f0-9]:){5,})(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3})?::(?:[a-f0-9]{1,4}(?::[a-f0-9]{1,4}){0,3}:)?)))?(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))(?:\\.(?:(?:25[0-5])|(?:2[0-4][0-9])|(?:1[0-9]{2})|(?:[1-9]?[0-9]))){3}))\\]))$/iD"))),
            s_return(e_binop(e_call("preg_match", vec![e_var("re"), e_var("email")]), BinOp::StrictEq, e_int(1))),
        ])
        .build()
}

/// `__elephc_filter_mac` — transcribed from the PHP form.
fn decl_fn_elephc_filter_mac() -> Stmt {
    function("__elephc_filter_mac")
        .param("s", TypeExpr::Str)
        .param("separator", t_nullable(TypeExpr::Str))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("l", e_call("strlen", vec![e_var("s")])),
            s_if(
                e_binop(e_var("l"), BinOp::StrictEq, e_int(14)),
                vec![
                    s_assign("tokens", e_int(3)),
                    s_assign("len", e_int(4)),
                    s_assign("c", e_str(".")),
                ],
                vec![
                (e_binop(e_binop(e_var("l"), BinOp::StrictEq, e_int(17)), BinOp::And, e_binop(e_index(e_var("s"), e_int(2)), BinOp::StrictEq, e_str("-"))), vec![
                    s_assign("tokens", e_int(6)),
                    s_assign("len", e_int(2)),
                    s_assign("c", e_str("-")),
                ]),
                (e_binop(e_binop(e_var("l"), BinOp::StrictEq, e_int(17)), BinOp::And, e_binop(e_index(e_var("s"), e_int(2)), BinOp::StrictEq, e_str(":"))), vec![
                    s_assign("tokens", e_int(6)),
                    s_assign("len", e_int(2)),
                    s_assign("c", e_str(":")),
                ]),
            ],
                Some(vec![
                s_return(e_bool(false)),
            ]),
            ),
            s_if(
                e_binop(e_binop(e_var("separator"), BinOp::StrictNotEq, e_null()), BinOp::And, e_binop(e_var("separator"), BinOp::StrictNotEq, e_var("c"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("tokens"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("o", e_binop(e_var("i"), BinOp::Mul, e_binop(e_var("len"), BinOp::Add, e_int(1)))),
                s_if(
                    e_binop(e_binop(e_var("i"), BinOp::Lt, e_binop(e_var("tokens"), BinOp::Sub, e_int(1))), BinOp::And, e_binop(e_index(e_var("s"), e_binop(e_var("o"), BinOp::Add, e_var("len"))), BinOp::StrictNotEq, e_var("c"))),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_call("strspn", vec![e_var("s"), e_str("0123456789abcdefABCDEF"), e_var("o"), e_var("len")]), BinOp::StrictNotEq, e_var("len")),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Builds every helper.
pub(crate) fn declarations(with_email: bool) -> Program {
    internal_declarations(|| {
        let mut declarations = vec![
            decl_fn_elephc_filter_var_ext(),
            decl_fn_elephc_filter_callback(),
            decl_fn_elephc_filter_alnum(),
            decl_fn_elephc_filter_domain(),
            decl_fn_elephc_filter_userinfo(),
            decl_fn_elephc_filter_url(),
            decl_fn_elephc_filter_mac(),
        ];
        // Unreached when FILTER_VALIDATE_EMAIL never appears: `__elephc_filter_var_ext` only
        // calls it for that id, and only literal calls reach the helper.
        if with_email {
            declarations.push(decl_fn_elephc_filter_email());
        }
        declarations
    })
}
