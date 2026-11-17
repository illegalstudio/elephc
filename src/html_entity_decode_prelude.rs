//! Purpose:
//! Builds `html_entity_decode()` with its `$flags` and `$encoding` parameters as prelude declarations.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations when the
//!   program references `html_entity_decode`.
//! - `crate::ir_lower::expr::compat_preludes`, which routes every call to [`HTML_ENTITY_DECODE_NAME`].
//!
//! Key details:
//! - The backend helper `__rt_html_entity_decode` knows five references and no `$flags` or
//!   `$encoding`, so this helper replaces it: it walks the `&...;` references of the subject and
//!   decodes each against the caller's flags. A numeric reference must pass
//!   `__elephc_html_entity_is_kept` (the same php-src validity rule `htmlspecialchars` uses); a
//!   named one is looked up in php's HTML 4.01 table (`HTML401_ENTITY_CODE_POINTS`), narrowed to
//!   five names under `ENT_XML1` and widened by `&apos;` under XML 1.0, XHTML and HTML 5. HTML 5's
//!   own two-thousand-name table is approximated by the XHTML set.
//! - A `"` stays encoded unless `ENT_COMPAT` is set, a `'` unless `ENT_HTML_QUOTE_SINGLE` is set.
//! - A Latin-1 `$encoding` yields one byte per code point and leaves a reference whose code point
//!   does not fit as written, as php does. Other encodings decode as UTF-8.
//! - Built as AST, never parsed from PHP source.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_int, e_match, e_null, e_str, e_ternary, e_var, function,
    php_semantics_declarations, s_assign, s_break, s_continue, s_if, s_return, s_while, t_nullable,
};

/// Reserved helper backing every `html_entity_decode()` call.
pub(crate) const HTML_ENTITY_DECODE_NAME: &str = "__elephc_html_entity_decode";

/// Reserved helper mapping a named reference to its code point (`-1` when the table has none).
const HTML_ENTITY_CODE_POINT_NAME: &str = "__elephc_html_entity_code_point";

/// Reserved helper encoding one code point as UTF-8.
const UTF8_ENCODE_NAME: &str = "__elephc_html_entity_utf8";

/// Returns the `html_entity_decode()` declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| vec![code_point_fn(), utf8_fn(), decode_fn()])
}

/// `$target = $target . value`.
fn append(target: &str, value: Expr) -> Stmt {
    s_assign(target, e_binop(e_var(target), BinOp::Concat, value))
}

/// `$name + offset`.
fn plus(name: &str, offset: i64) -> Expr {
    e_binop(e_var(name), BinOp::Add, e_int(offset))
}

/// `$flags & mask`.
fn flag(mask: i64) -> Expr {
    e_binop(e_var("flags"), BinOp::BitAnd, e_int(mask))
}

/// `$cp < high`.
fn cp_below(high: i64) -> Expr {
    e_binop(e_var("cp"), BinOp::Lt, e_int(high))
}

/// `chr(base | (($cp >> shift) & mask))`, one lead or continuation byte of a UTF-8 sequence.
fn byte(base: i64, shift: i64, mask: i64) -> Expr {
    e_call(
        "chr",
        vec![e_binop(
            e_int(base),
            BinOp::BitOr,
            e_binop(e_binop(e_var("cp"), BinOp::ShiftRight, e_int(shift)), BinOp::BitAnd, e_int(mask)),
        )],
    )
}

/// `a . b . ...`.
fn concat(parts: Vec<Expr>) -> Expr {
    let mut parts = parts.into_iter();
    let first = parts.next().expect("concat needs a part");
    parts.fold(first, |left, right| e_binop(left, BinOp::Concat, right))
}

/// `$body === name`.
fn body_is(name: &str) -> Expr {
    e_binop(e_var("body"), BinOp::StrictEq, e_str(name))
}

/// Builds `__elephc_html_entity_code_point(string $body): int` over php's HTML 4.01 table.
fn code_point_fn() -> Stmt {
    use elephc_builtin_contract::html_entities::HTML401_ENTITY_CODE_POINTS;
    let arms = HTML401_ENTITY_CODE_POINTS
        .iter()
        .map(|(name, code_point)| (vec![e_str(name)], e_int(i64::from(*code_point))))
        .collect();
    function(HTML_ENTITY_CODE_POINT_NAME)
        .param("body", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![s_return(e_match(e_var("body"), arms, Some(e_int(-1))))])
        .build()
}

/// Builds `__elephc_html_entity_utf8(int $cp): string`, the UTF-8 bytes of one code point.
fn utf8_fn() -> Stmt {
    function(UTF8_ENCODE_NAME)
        .param("cp", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(cp_below(0x80), vec![s_return(e_call("chr", vec![e_var("cp")]))], vec![], None),
            s_if(
                cp_below(0x800),
                vec![s_return(concat(vec![byte(0xC0, 6, 0x1F), byte(0x80, 0, 0x3F)]))],
                vec![],
                None,
            ),
            s_if(
                cp_below(0x10000),
                vec![s_return(concat(vec![byte(0xE0, 12, 0x0F), byte(0x80, 6, 0x3F), byte(0x80, 0, 0x3F)]))],
                vec![],
                None,
            ),
            s_return(concat(vec![
                byte(0xF0, 18, 0x07),
                byte(0x80, 12, 0x3F),
                byte(0x80, 6, 0x3F),
                byte(0x80, 0, 0x3F),
            ])),
        ])
        .build()
}

/// Builds `__elephc_html_entity_decode(string $string, int $flags, ?string $encoding): string`.
fn decode_fn() -> Stmt {
    let not_an_entity = e_binop(
        e_binop(e_var("semi"), BinOp::StrictEq, e_bool(false)),
        BinOp::Or,
        e_binop(e_binop(e_var("semi"), BinOp::Sub, e_var("amp")), BinOp::Gt, e_int(33)),
    );
    let is_numeric = e_binop(
        e_call("substr", vec![e_var("body"), e_int(0), e_int(1)]),
        BinOp::StrictEq,
        e_str("#"),
    );
    let hex_marker = e_binop(
        e_call("strtolower", vec![e_call("substr", vec![e_var("body"), e_int(1), e_int(1)])]),
        BinOp::StrictEq,
        e_str("x"),
    );
    let numeric_cp = e_ternary(
        hex_marker,
        e_call("hexdec", vec![e_call("substr", vec![e_var("body"), e_int(2)])]),
        e_call("intval", vec![e_call("substr", vec![e_var("body"), e_int(1)])]),
    );
    let doctype_is = |value: i64| e_binop(flag(48), BinOp::StrictEq, e_int(value));
    let xml_five = e_binop(
        e_binop(body_is("amp"), BinOp::Or, body_is("lt")),
        BinOp::Or,
        e_binop(body_is("gt"), BinOp::Or, body_is("quot")),
    );
    // Named reference: `apos` outside HTML 4.01, the table otherwise, five names under XML 1.0.
    let named = vec![s_if(
        body_is("apos"),
        vec![s_assign("cp", e_ternary(doctype_is(0), e_int(-1), e_int(39)))],
        vec![],
        Some(vec![s_if(
            e_binop(doctype_is(16), BinOp::And, e_binop(xml_five, BinOp::StrictEq, e_bool(false))),
            vec![s_assign("cp", e_int(-1))],
            vec![],
            Some(vec![s_assign("cp", e_call(HTML_ENTITY_CODE_POINT_NAME, vec![e_var("body")]))]),
        )]),
    )];
    let numeric = vec![s_if(
        e_call(
            crate::htmlspecialchars_prelude::HTML_ENTITY_IS_KEPT_NAME,
            vec![e_var("body"), e_var("flags")],
        ),
        vec![s_assign("cp", numeric_cp)],
        vec![],
        None,
    )];
    let latin1 = e_binop(
        e_binop(e_var("encoding"), BinOp::StrictNotEq, e_null()),
        BinOp::And,
        e_call(
            "in_array",
            vec![
                e_call("strtolower", vec![e_var("encoding")]),
                e_array(vec![e_str("iso-8859-1"), e_str("iso8859-1"), e_str("latin1")]),
                e_bool(true),
            ],
        ),
    );
    let render = vec![
        s_if(
            e_binop(
                e_binop(
                    e_binop(e_var("cp"), BinOp::StrictEq, e_int(34)),
                    BinOp::And,
                    e_binop(flag(2), BinOp::StrictEq, e_int(0)),
                ),
                BinOp::Or,
                e_binop(
                    e_binop(e_var("cp"), BinOp::StrictEq, e_int(39)),
                    BinOp::And,
                    e_binop(flag(1), BinOp::StrictEq, e_int(0)),
                ),
            ),
            vec![s_assign("cp", e_int(-1))],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("cp"), BinOp::GtEq, e_int(0)),
            vec![s_if(
                latin1,
                vec![s_if(
                    e_binop(e_var("cp"), BinOp::Lt, e_int(256)),
                    vec![s_assign("decoded", e_call("chr", vec![e_var("cp")]))],
                    vec![],
                    None,
                )],
                vec![],
                Some(vec![s_assign("decoded", e_call(UTF8_ENCODE_NAME, vec![e_var("cp")]))]),
            )],
            vec![],
            None,
        ),
    ];
    let mut loop_body = vec![
        s_assign("amp", e_call("strpos", vec![e_var("string"), e_str("&"), e_var("offset")])),
        s_if(
            e_binop(e_var("amp"), BinOp::StrictEq, e_bool(false)),
            vec![
                append("out", e_call("substr", vec![e_var("string"), e_var("offset")])),
                s_break(1),
            ],
            vec![],
            None,
        ),
        append(
            "out",
            e_call(
                "substr",
                vec![e_var("string"), e_var("offset"), e_binop(e_var("amp"), BinOp::Sub, e_var("offset"))],
            ),
        ),
        s_assign("semi", e_call("strpos", vec![e_var("string"), e_str(";"), plus("amp", 1)])),
        s_if(
            not_an_entity,
            vec![append("out", e_str("&")), s_assign("offset", plus("amp", 1)), s_continue(1)],
            vec![],
            None,
        ),
        s_assign(
            "token",
            e_call(
                "substr",
                vec![
                    e_var("string"),
                    e_var("amp"),
                    e_binop(plus("semi", 1), BinOp::Sub, e_var("amp")),
                ],
            ),
        ),
        // A second `&` inside the candidate means the first one is a bare ampersand.
        s_if(
            e_binop(
                e_call("strpos", vec![e_var("token"), e_str("&"), e_int(1)]),
                BinOp::StrictNotEq,
                e_bool(false),
            ),
            vec![append("out", e_str("&")), s_assign("offset", plus("amp", 1)), s_continue(1)],
            vec![],
            None,
        ),
        s_assign("body", e_call("substr", vec![e_var("token"), e_int(1), e_int(-1)])),
        s_assign("decoded", e_var("token")),
        s_assign("cp", e_int(-1)),
        s_if(is_numeric, numeric, vec![], Some(named)),
    ];
    loop_body.extend(render);
    loop_body.push(append("out", e_var("decoded")));
    loop_body.push(s_assign("offset", plus("semi", 1)));
    let body = vec![
        s_assign("out", e_str("")),
        s_assign("offset", e_int(0)),
        s_assign("length", e_call("strlen", vec![e_var("string")])),
        s_while(e_binop(e_var("offset"), BinOp::Lt, e_var("length")), loop_body),
        s_return(e_var("out")),
    ];
    function(HTML_ENTITY_DECODE_NAME)
        .param("string", TypeExpr::Str)
        // php's own defaults, so a call that leaves some of them out binds.
        .param_default("flags", TypeExpr::Int, e_int(11))
        .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}
