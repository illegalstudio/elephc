//! Purpose:
//! Builds `htmlspecialchars()`'s fourth parameter, `$double_encode`, as prelude declarations.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations when the
//!   program references `htmlspecialchars`.
//! - `crate::ir_lower::expr::compat_preludes`, which routes a four-argument call whose flag is not
//!   the literal `true` to [`HTMLSPECIALCHARS_DOUBLE_ENCODE_NAME`].
//!
//! Key details:
//! - The backend helper `__rt_htmlspecialchars` always double-encodes. With `$double_encode`
//!   true that is php's answer, so the helper delegates to it; with false, php leaves every VALID
//!   character reference alone and encodes the rest, which is this file's loop.
//! - Which references are valid is decided by `__elephc_html_entity_is_kept`, a transcription of
//!   `elephc_builtin_contract::html_entities::html_entity_is_kept` over the same name tables, so
//!   the compiled program and the interpreter answer identically.
//! - Laravel's `e()` — every Blade `{{ }}` — calls `htmlspecialchars($v, ENT_QUOTES |
//!   ENT_SUBSTITUTE, 'UTF-8', $doubleEncode)`, and the four-argument call stopped EIR lowering.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_int, e_str, e_var, function, php_semantics_declarations, s_assign,
    s_if, s_return, s_while, t_nullable,
};

/// Reserved helper backing a four-argument `htmlspecialchars()` call.
pub(crate) const HTMLSPECIALCHARS_DOUBLE_ENCODE_NAME: &str = "__elephc_htmlspecialchars_double_encode";

/// Reserved predicate deciding whether one `&...;` reference is kept.
const HTML_ENTITY_IS_KEPT_NAME: &str = "__elephc_html_entity_is_kept";

/// Returns the `htmlspecialchars()` double-encode declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![double_encode_fn(), entity_is_kept_fn()])
}

/// `$target = $target . value`.
fn append(target: &str, value: Expr) -> Stmt {
    s_assign(target, e_binop(e_var(target), BinOp::Concat, value))
}

/// `htmlspecialchars(value, $flags)`: the native helper, which ignores the encoding argument.
fn escape(value: Expr) -> Expr {
    e_call("htmlspecialchars", vec![value, e_var("flags")])
}

/// `$name + offset`.
fn plus(name: &str, offset: i64) -> Expr {
    e_binop(e_var(name), BinOp::Add, e_int(offset))
}

/// `$a - $b`.
fn minus(a: Expr, b: Expr) -> Expr {
    e_binop(a, BinOp::Sub, b)
}

/// `strspn(value, chars) === strlen(value)`: every byte of `value` is one of `chars`.
fn only_chars(value: Expr, chars: &str) -> Expr {
    e_binop(
        e_call("strspn", vec![value.clone(), e_str(chars)]),
        BinOp::StrictEq,
        e_call("strlen", vec![value]),
    )
}

/// `in_array($body, [names], true)`.
fn name_in(names: &[&str]) -> Expr {
    e_call(
        "in_array",
        vec![
            e_var("body"),
            crate::synthetic_class::e_array(names.iter().map(|name| e_str(name)).collect()),
            e_bool(true),
        ],
    )
}

/// Builds `__elephc_htmlspecialchars_double_encode(string $string, int $flags, ?string $encoding,
/// bool $double_encode): string`.
fn double_encode_fn() -> Stmt {
    let reference = e_call(
        "substr",
        vec![
            e_var("string"),
            plus("amp", 1),
            minus(minus(e_var("semi"), e_var("amp")), e_int(1)),
        ],
    );
    let kept = e_binop(
        e_binop(e_var("semi"), BinOp::StrictNotEq, e_bool(false)),
        BinOp::And,
        e_call(HTML_ENTITY_IS_KEPT_NAME, vec![reference, e_var("flags")]),
    );
    let body = vec![
        s_if(
            e_var("double_encode"),
            vec![s_return(escape(e_var("string")))],
            vec![],
            None,
        ),
        s_assign("out", e_str("")),
        s_assign("offset", e_int(0)),
        s_assign("length", e_call("strlen", vec![e_var("string")])),
        s_while(
            e_binop(e_var("offset"), BinOp::Lt, e_var("length")),
            vec![
                s_assign(
                    "amp",
                    e_call("strpos", vec![e_var("string"), e_str("&"), e_var("offset")]),
                ),
                s_if(
                    e_binop(e_var("amp"), BinOp::StrictEq, e_bool(false)),
                    vec![s_return(e_binop(
                        e_var("out"),
                        BinOp::Concat,
                        escape(e_call("substr", vec![e_var("string"), e_var("offset")])),
                    ))],
                    vec![],
                    None,
                ),
                append(
                    "out",
                    escape(e_call(
                        "substr",
                        vec![e_var("string"), e_var("offset"), minus(e_var("amp"), e_var("offset"))],
                    )),
                ),
                s_assign("semi", e_call("strpos", vec![e_var("string"), e_str(";"), plus("amp", 1)])),
                s_if(
                    kept,
                    vec![
                        append(
                            "out",
                            e_call(
                                "substr",
                                vec![
                                    e_var("string"),
                                    e_var("amp"),
                                    minus(plus("semi", 1), e_var("amp")),
                                ],
                            ),
                        ),
                        s_assign("offset", plus("semi", 1)),
                    ],
                    vec![],
                    Some(vec![append("out", e_str("&amp;")), s_assign("offset", plus("amp", 1))]),
                ),
            ],
        ),
        s_return(e_var("out")),
    ];
    function(HTMLSPECIALCHARS_DOUBLE_ENCODE_NAME)
        .param("string", TypeExpr::Str)
        .param("flags", TypeExpr::Int)
        .param("encoding", t_nullable(TypeExpr::Str))
        .param("double_encode", TypeExpr::Bool)
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}

/// Builds `__elephc_html_entity_is_kept(string $body, int $flags): bool`, php-src's
/// `process_numeric_entity` / `process_named_entity_html` over the doctype's name table.
fn entity_is_kept_fn() -> Stmt {
    use elephc_builtin_contract::html_entities::{HTML401_ENTITY_NAMES, XML1_ENTITY_NAMES};
    let doctype_is = |value: i64| {
        e_binop(
            e_binop(e_var("flags"), BinOp::BitAnd, e_int(48)),
            BinOp::StrictEq,
            e_int(value),
        )
    };
    let code_point_allowed_xml = e_binop(
        e_binop(
            e_binop(
                e_binop(e_var("cp"), BinOp::GtEq, e_int(0x20)),
                BinOp::And,
                e_binop(e_var("cp"), BinOp::LtEq, e_int(0xD7FF)),
            ),
            BinOp::Or,
            e_binop(
                e_binop(
                    e_binop(e_var("cp"), BinOp::StrictEq, e_int(0x09)),
                    BinOp::Or,
                    e_binop(e_var("cp"), BinOp::StrictEq, e_int(0x0A)),
                ),
                BinOp::Or,
                e_binop(e_var("cp"), BinOp::StrictEq, e_int(0x0D)),
            ),
        ),
        BinOp::Or,
        e_binop(
            e_binop(
                e_binop(e_var("cp"), BinOp::GtEq, e_int(0xE000)),
                BinOp::And,
                e_binop(e_var("cp"), BinOp::LtEq, e_int(0x10FFFF)),
            ),
            BinOp::And,
            e_binop(
                e_binop(e_var("cp"), BinOp::StrictNotEq, e_int(0xFFFE)),
                BinOp::And,
                e_binop(e_var("cp"), BinOp::StrictNotEq, e_int(0xFFFF)),
            ),
        ),
    );
    let numeric = vec![
        s_assign("digits", e_call("substr", vec![e_var("body"), e_int(1)])),
        s_assign(
            "hex",
            e_binop(
                e_binop(
                    e_call("substr", vec![e_var("digits"), e_int(0), e_int(1)]),
                    BinOp::StrictEq,
                    e_str("x"),
                ),
                BinOp::Or,
                e_binop(
                    e_call("substr", vec![e_var("digits"), e_int(0), e_int(1)]),
                    BinOp::StrictEq,
                    e_str("X"),
                ),
            ),
        ),
        s_if(
            e_var("hex"),
            vec![s_assign("digits", e_call("substr", vec![e_var("digits"), e_int(1)]))],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("digits"), BinOp::StrictEq, e_str("")),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_if(
            e_var("hex"),
            vec![s_if(
                crate::synthetic_class::e_not(only_chars(e_var("digits"), "0123456789abcdefABCDEF")),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            )],
            vec![],
            Some(vec![s_if(
                crate::synthetic_class::e_not(only_chars(e_var("digits"), "0123456789")),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            )]),
        ),
        s_assign("digits", e_call("ltrim", vec![e_var("digits"), e_str("0")])),
        s_if(
            e_binop(e_call("strlen", vec![e_var("digits")]), BinOp::Gt, e_int(8)),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_assign("cp", e_int(0)),
        s_if(
            e_binop(e_var("digits"), BinOp::StrictNotEq, e_str("")),
            // `intval($digits, 16)` rather than `hexdec()`: at most eight hex digits always fit an
            // int, and `hexdec()`'s `int|float` would make `$cp` a tagged scalar for no reason.
            vec![s_if(
                e_var("hex"),
                vec![s_assign("cp", e_call("intval", vec![e_var("digits"), e_int(16)]))],
                vec![],
                Some(vec![s_assign("cp", e_call("intval", vec![e_var("digits"), e_int(10)]))]),
            )],
            vec![],
            None,
        ),
        s_if(doctype_is(16), vec![s_return(code_point_allowed_xml)], vec![], None),
        s_return(e_binop(e_var("cp"), BinOp::LtEq, e_int(0x10FFFF))),
    ];
    let body = vec![
        s_if(
            e_binop(e_var("body"), BinOp::StrictEq, e_str("")),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("substr", vec![e_var("body"), e_int(0), e_int(1)]),
                BinOp::StrictEq,
                e_str("#"),
            ),
            numeric,
            vec![],
            None,
        ),
        s_if(
            crate::synthetic_class::e_not(only_chars(
                e_var("body"),
                "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
            )),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_if(doctype_is(16), vec![s_return(name_in(&XML1_ENTITY_NAMES))], vec![], None),
        s_if(
            e_binop(e_var("body"), BinOp::StrictEq, e_str("apos")),
            vec![s_return(crate::synthetic_class::e_not(doctype_is(0)))],
            vec![],
            None,
        ),
        s_return(name_in(&HTML401_ENTITY_NAMES)),
    ];
    function(HTML_ENTITY_IS_KEPT_NAME)
        .param("body", TypeExpr::Str)
        .param("flags", TypeExpr::Int)
        .returns(TypeExpr::Bool)
        .body(body)
        .build()
}
