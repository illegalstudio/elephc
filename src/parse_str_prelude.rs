//! Purpose:
//! Builds PHP's `parse_str()` — the query-string grammar and its by-reference output array — as
//! prelude declarations, for programs that name it.
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends these declarations when the
//!   program references `parse_str`.
//!
//! Key details:
//! - The backend had NO `parse_str` at all: no runtime symbol, no lowering, and the contract was
//!   listed in `AOT_IMPLEMENTATION_PENDING`. Compiled code calling it fell through to the eval
//!   bridge, whose array ABI cannot bind a by-reference out-parameter, so Symfony's
//!   `%env(query_string:...)%` processor — which every Symfony 7 container resolves for
//!   `container.runtime_mode` — killed the process with `eval() fragment uses an unsupported
//!   construct`.
//! - The declarations are BUILT, never parsed: elephc does not parse PHP source outside tests.
//! - Written WITHOUT reference rebinding (`$node = &$node[$key]`), which the backend rejects for a
//!   non-integer index. The tree is rebuilt by a recursive value-returning insert instead, which
//!   costs copies on a path that parses at most `max_input_vars` fields.
//! - Every rule below is php's, each one covered by a case in
//!   `tests/codegen/parse_str.rs` that was measured against `php -n` 8.5:
//!   * A NUL byte terminates the WHOLE parse, not just the field it appears in.
//!   * `&` is the only separator modeled (php's default `arg_separator.input`).
//!   * Name and value are `+`/`%XX`-decoded ONCE, before the bracket grammar runs; a `%` with
//!     fewer than two following bytes, or with a non-hex digit, stays literal.
//!   * Leading `0x20` bytes are stripped from the decoded name — only that byte, only the front.
//!   * In the ROOT (before the first `[`), `' '` and `'.'` become `'_'`; inside a bracket segment
//!     neither is touched.
//!   * An empty root name drops the field.
//!   * An empty bracket segment appends, and so does a segment that is exactly one space or tab.
//!   * An unterminated `[` behaves differently depending on WHICH segment fails: a failure in the
//!     first flat-mangles the whole name (`' '`, `'.'` and `'['` all become `'_'`) into one key;
//!     a failure later drops the unparsed tail and keeps the path built so far.
//!   * `[]` appends at `max(existing int keys) + 1`, negative keys included, `0` with none, and is
//!     silently dropped once that max is already `PHP_INT_MAX`.
//!   * `max_input_vars` (1000) caps accepted fields and `max_input_nesting_level` (64) caps
//!     bracket depth per field, both at php's own defaults — there is no ini storage to read a
//!     configured value from, the same deliberate gap the interpreter documents.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_assign, e_binop, e_bool, e_call, e_const, e_index, e_int, e_not, e_str, e_var,
    function, php_semantics_declarations, t_array, s_array_assign, s_array_push, s_assign, s_break, s_continue,
    s_expr, s_foreach, s_if, s_return, s_return_void, s_while,
};

/// Percent/plus decoder shared by the name and the value of every field.
const DECODE_NAME: &str = "__elephc_parse_str_decode";

/// Single hex-digit value, or `-1` for a byte that is not one.
const HEX_NAME: &str = "__elephc_parse_str_hex";

/// `' '`/`'.'` (and optionally `'['`) to `'_'` mangling.
const MANGLE_NAME: &str = "__elephc_parse_str_mangle";

/// Decoded name to its list of path segments; an empty list means "drop this field".
const PATH_NAME: &str = "__elephc_parse_str_path";

/// Next `[]` append key as `[ok, key]`.
const APPEND_KEY_NAME: &str = "__elephc_parse_str_append_key";

/// Recursive value-returning insert of one decoded value at one path.
const INSERT_NAME: &str = "__elephc_parse_str_insert";

/// php's default `max_input_vars`.
const MAX_INPUT_VARS: i64 = 1000;

/// php's default `max_input_nesting_level`, plus the root segment the path also carries.
const MAX_PATH_SEGMENTS: i64 = 65;

/// Builds `if (<condition>) { <then> } else { <otherwise> }` without the else-if chain
/// `s_if` also carries, which this prelude never needs.
fn when(condition: Expr, then_body: Vec<Stmt>, otherwise: Option<Vec<Stmt>>) -> Stmt {
    s_if(condition, then_body, Vec::new(), otherwise)
}

/// Returns the `parse_str()` declarations for injection.
pub(crate) fn declarations() -> Program {
    php_semantics_declarations(|| {
        vec![
            decl_fn_hex(),
            decl_fn_decode(),
            decl_fn_mangle(),
            decl_fn_path(),
            decl_fn_append_key(),
            decl_fn_insert(),
            decl_fn_parse_str(),
        ]
    })
}

/// Builds `$name === <text>`.
fn is_str(name: &str, text: &str) -> Expr {
    e_binop(e_var(name), BinOp::StrictEq, e_str(text))
}

/// Builds `<left> && <right>`.
fn and(left: Expr, right: Expr) -> Expr {
    e_binop(left, BinOp::And, right)
}

/// Builds `$name = $name + <amount>;`.
fn advance(name: &str, amount: i64) -> Stmt {
    s_assign(name, e_binop(e_var(name), BinOp::Add, e_int(amount)))
}

/// Builds `$out = $out . <part>;`.
fn append_str(out: &str, part: Expr) -> Stmt {
    s_assign(out, e_binop(e_var(out), BinOp::Concat, part))
}

/// `__elephc_parse_str_hex($byte)`: the value of one hex digit, or `-1`.
fn decl_fn_hex() -> Stmt {
    let code = e_var("code");
    let in_range = |low: i64, high: i64| {
        and(
            e_binop(code.clone(), BinOp::GtEq, e_int(low)),
            e_binop(code.clone(), BinOp::LtEq, e_int(high)),
        )
    };
    let digit = |offset: i64| s_return(e_binop(code.clone(), BinOp::Sub, e_int(offset)));
    function(HEX_NAME)
        .param("byte", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("code", e_call("ord", vec![e_var("byte")])),
            when(in_range(48, 57), vec![digit(48)], None),
            when(in_range(97, 102), vec![digit(87)], None),
            when(in_range(65, 70), vec![digit(55)], None),
            s_return(e_int(-1)),
        ])
        .build()
}

/// `__elephc_parse_str_decode($input)`: php's form decoding, applied once per component.
fn decl_fn_decode() -> Stmt {
    let byte_at = |offset: i64| {
        e_index(
            e_var("input"),
            e_binop(e_var("index"), BinOp::Add, e_int(offset)),
        )
    };
    // A `%` needs TWO bytes after it: `index + 2 < length` is the last position where both exist.
    let has_two_more = e_binop(
        e_binop(e_var("index"), BinOp::Add, e_int(2)),
        BinOp::Lt,
        e_var("length"),
    );
    let both_hex = and(
        e_binop(e_var("high"), BinOp::GtEq, e_int(0)),
        e_binop(e_var("low"), BinOp::GtEq, e_int(0)),
    );
    let decoded_byte = e_call(
        "chr",
        vec![e_binop(
            e_binop(e_var("high"), BinOp::Mul, e_int(16)),
            BinOp::Add,
            e_var("low"),
        )],
    );
    function(DECODE_NAME)
        .param("input", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("out", e_str("")),
            s_assign("length", e_call("strlen", vec![e_var("input")])),
            s_assign("index", e_int(0)),
            s_while(
                e_binop(e_var("index"), BinOp::Lt, e_var("length")),
                vec![
                    s_assign("current", e_index(e_var("input"), e_var("index"))),
                    when(
                        is_str("current", "+"),
                        vec![append_str("out", e_str(" ")), advance("index", 1), s_continue(1)],
                        None,
                    ),
                    when(
                        and(is_str("current", "%"), has_two_more.clone()),
                        vec![
                            s_assign("high", e_call(HEX_NAME, vec![byte_at(1)])),
                            s_assign("low", e_call(HEX_NAME, vec![byte_at(2)])),
                            when(
                                both_hex.clone(),
                                vec![
                                    append_str("out", decoded_byte.clone()),
                                    advance("index", 3),
                                    s_continue(1),
                                ],
                                None,
                            ),
                        ],
                        None,
                    ),
                    append_str("out", e_var("current")),
                    advance("index", 1),
                ],
            ),
            s_return(e_var("out")),
        ])
        .build()
}

/// `__elephc_parse_str_mangle($text, $alsoBracket)`: `' '`/`'.'`, plus `'['` for the flat form.
fn decl_fn_mangle() -> Stmt {
    let is_mangled = e_binop(
        e_binop(is_str("current", " "), BinOp::Or, is_str("current", ".")),
        BinOp::Or,
        and(e_var("alsoBracket"), is_str("current", "[")),
    );
    function(MANGLE_NAME)
        .param("text", TypeExpr::Str)
        .param("alsoBracket", TypeExpr::Bool)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("out", e_str("")),
            s_assign("length", e_call("strlen", vec![e_var("text")])),
            s_assign("index", e_int(0)),
            s_while(
                e_binop(e_var("index"), BinOp::Lt, e_var("length")),
                vec![
                    s_assign("current", e_index(e_var("text"), e_var("index"))),
                    when(
                        is_mangled.clone(),
                        vec![append_str("out", e_str("_"))],
                        Some(vec![append_str("out", e_var("current"))]),
                    ),
                    advance("index", 1),
                ],
            ),
            s_return(e_var("out")),
        ])
        .build()
}

/// `__elephc_parse_str_path($name)`: the segment list, with `''` standing for the `[]` append.
///
/// A literal segment can never be the empty string — an empty `[]` IS the append — so the empty
/// string is free to carry that meaning and the whole path stays a plain list of strings.
fn decl_fn_path() -> Stmt {
    let mangled = |value: Expr, also_bracket: bool| {
        e_call(MANGLE_NAME, vec![value, e_bool(also_bracket)])
    };
    let first_open = e_call("strpos", vec![e_var("name"), e_str("[")]);
    let after_first_open = e_call(
        "substr",
        vec![
            e_var("name"),
            e_binop(e_var("open"), BinOp::Add, e_int(1)),
        ],
    );
    function(PATH_NAME)
        .param("name", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_assign("open", first_open),
            // A name that STARTS with `[` has an empty root: php drops the whole field.
            when(
                e_binop(e_var("open"), BinOp::StrictEq, e_int(0)),
                vec![s_return(e_array(Vec::new()))],
                None,
            ),
            when(
                e_binop(e_var("open"), BinOp::StrictEq, e_bool(false)),
                vec![s_return(e_array(vec![mangled(e_var("name"), false)]))],
                None,
            ),
            // The FIRST `[` never closing flat-mangles the whole name into one key.
            when(
                e_binop(
                    e_call("strpos", vec![after_first_open, e_str("]")]),
                    BinOp::StrictEq,
                    e_bool(false),
                ),
                vec![s_return(e_array(vec![mangled(e_var("name"), true)]))],
                None,
            ),
            s_assign(
                "path",
                e_array(vec![mangled(
                    e_call("substr", vec![e_var("name"), e_int(0), e_var("open")]),
                    false,
                )]),
            ),
            s_assign("rest", e_call("substr", vec![e_var("name"), e_var("open")])),
            s_while(
                and(
                    e_binop(e_var("rest"), BinOp::StrictNotEq, e_str("")),
                    e_binop(e_index(e_var("rest"), e_int(0)), BinOp::StrictEq, e_str("[")),
                ),
                vec![
                    s_assign("after", e_call("substr", vec![e_var("rest"), e_int(1)])),
                    s_assign("close", e_call("strpos", vec![e_var("after"), e_str("]")])),
                    // A LATER segment that never closes drops the tail and keeps what is built.
                    when(
                        e_binop(e_var("close"), BinOp::StrictEq, e_bool(false)),
                        vec![s_break(1)],
                        None,
                    ),
                    s_assign(
                        "segment",
                        e_call("substr", vec![e_var("after"), e_int(0), e_var("close")]),
                    ),
                    // One space or one tab is php's append marker too, not a literal key.
                    when(
                        e_binop(is_str("segment", " "), BinOp::Or, is_str("segment", "\t")),
                        vec![s_assign("segment", e_str(""))],
                        None,
                    ),
                    s_array_push("path", e_var("segment")),
                    s_assign(
                        "rest",
                        e_call(
                            "substr",
                            vec![
                                e_var("after"),
                                e_binop(e_var("close"), BinOp::Add, e_int(1)),
                            ],
                        ),
                    ),
                ],
            ),
            s_return(e_var("path")),
        ])
        .build()
}

/// `__elephc_parse_str_append_key($node)`: `[ok, key]` for the next `[]` position.
///
/// Returns `[false, 0]` once the existing integer keys already reach `PHP_INT_MAX`, which php
/// answers by silently dropping the append rather than wrapping or overwriting.
fn decl_fn_append_key() -> Stmt {
    function(APPEND_KEY_NAME)
        .param("node", t_array())
        .returns(t_array())
        .body(vec![
            s_assign("max", e_int(0)),
            s_assign("seen", e_bool(false)),
            s_foreach(
                e_var("node"),
                Some("key"),
                "ignored",
                vec![when(
                    and(
                        e_call("is_int", vec![e_var("key")]),
                        e_binop(
                            e_not(e_var("seen")),
                            BinOp::Or,
                            e_binop(e_var("key"), BinOp::Gt, e_var("max")),
                        ),
                    ),
                    vec![
                        s_assign("max", e_var("key")),
                        s_assign("seen", e_bool(true)),
                    ],
                    None,
                )],
            ),
            when(
                e_not(e_var("seen")),
                vec![s_return(e_array(vec![e_bool(true), e_int(0)]))],
                None,
            ),
            when(
                e_binop(e_var("max"), BinOp::StrictEq, e_const("PHP_INT_MAX")),
                vec![s_return(e_array(vec![e_bool(false), e_int(0)]))],
                None,
            ),
            s_return(e_array(vec![
                e_bool(true),
                e_binop(e_var("max"), BinOp::Add, e_int(1)),
            ])),
        ])
        .build()
}

/// `__elephc_parse_str_insert($node, $path, $index, $value)`: the rebuilt subtree.
///
/// Value-returning rather than reference-threading: `$node = &$node[$key]` is rejected by the
/// backend for a non-integer index, and a query string is bounded to `max_input_vars` fields, so
/// rebuilding costs nothing that matters.
fn decl_fn_insert() -> Stmt {
    let node_at_key = e_index(e_var("node"), e_var("key"));
    function(INSERT_NAME)
        .param("node", t_array())
        .param("path", t_array())
        .param("index", TypeExpr::Int)
        .param("value", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_assign("segment", e_index(e_var("path"), e_var("index"))),
            s_assign("key", e_var("segment")),
            when(
                is_str("segment", ""),
                vec![
                    s_assign("append", e_call(APPEND_KEY_NAME, vec![e_var("node")])),
                    when(
                        e_not(e_index(e_var("append"), e_int(0))),
                        vec![s_return(e_var("node"))],
                        None,
                    ),
                    s_assign("key", e_index(e_var("append"), e_int(1))),
                ],
                None,
            ),
            when(
                e_binop(
                    e_var("index"),
                    BinOp::StrictEq,
                    e_binop(
                        e_call("count", vec![e_var("path")]),
                        BinOp::Sub,
                        e_int(1),
                    ),
                ),
                vec![
                    s_array_assign("node", e_var("key"), e_var("value")),
                    s_return(e_var("node")),
                ],
                None,
            ),
            s_assign("child", e_array(Vec::new())),
            // php REUSES an existing array at this key and REPLACES anything else, in both
            // directions: `a=1&a[b]=2` ends as an array, `a[b]=1&a=2` ends as a scalar.
            when(
                and(
                    e_call("isset", vec![node_at_key.clone()]),
                    e_call("is_array", vec![node_at_key.clone()]),
                ),
                vec![s_assign("child", node_at_key)],
                None,
            ),
            s_array_assign(
                "node",
                e_var("key"),
                e_call(
                    INSERT_NAME,
                    vec![
                        e_var("child"),
                        e_var("path"),
                        e_binop(e_var("index"), BinOp::Add, e_int(1)),
                        e_var("value"),
                    ],
                ),
            ),
            s_return(e_var("node")),
        ])
        .build()
}

/// PHP-visible `parse_str(string $string, &$result): void`.
fn decl_fn_parse_str() -> Stmt {
    let path_length = e_call("count", vec![e_var("path")]);
    function("parse_str")
        .param("string", TypeExpr::Str)
        .param_by_ref("result", None)
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_assign(e_var("result"), e_array(Vec::new()))),
            // A NUL ends the WHOLE parse, not just the field carrying it.
            s_assign("nul", e_call("strpos", vec![e_var("string"), e_str("\0")])),
            when(
                e_binop(e_var("nul"), BinOp::StrictNotEq, e_bool(false)),
                vec![s_assign(
                    "string",
                    e_call("substr", vec![e_var("string"), e_int(0), e_var("nul")]),
                )],
                None,
            ),
            s_assign("accepted", e_int(0)),
            s_foreach(
                e_call("explode", vec![e_str("&"), e_var("string")]),
                None,
                "field",
                vec![
                    when(is_str("field", ""), vec![s_continue(1)], None),
                    s_assign("eq", e_call("strpos", vec![e_var("field"), e_str("=")])),
                    s_assign("rawName", e_var("field")),
                    s_assign("rawValue", e_str("")),
                    when(
                        e_binop(e_var("eq"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_assign(
                                "rawName",
                                e_call("substr", vec![e_var("field"), e_int(0), e_var("eq")]),
                            ),
                            s_assign(
                                "rawValue",
                                e_call(
                                    "substr",
                                    vec![
                                        e_var("field"),
                                        e_binop(e_var("eq"), BinOp::Add, e_int(1)),
                                    ],
                                ),
                            ),
                        ],
                        None,
                    ),
                    s_assign("name", e_call(DECODE_NAME, vec![e_var("rawName")])),
                    // Only `0x20`, and only at the front: a leading tab stays part of the name.
                    s_assign("start", e_int(0)),
                    s_assign("length", e_call("strlen", vec![e_var("name")])),
                    s_while(
                        and(
                            e_binop(e_var("start"), BinOp::Lt, e_var("length")),
                            e_binop(
                                e_index(e_var("name"), e_var("start")),
                                BinOp::StrictEq,
                                e_str(" "),
                            ),
                        ),
                        vec![advance("start", 1)],
                    ),
                    s_assign(
                        "name",
                        e_call("substr", vec![e_var("name"), e_var("start")]),
                    ),
                    when(is_str("name", ""), vec![s_continue(1)], None),
                    when(
                        e_binop(e_var("accepted"), BinOp::GtEq, e_int(MAX_INPUT_VARS)),
                        vec![s_break(1)],
                        None,
                    ),
                    s_assign("path", e_call(PATH_NAME, vec![e_var("name")])),
                    // An empty path is the dropped-field marker; an over-deep one php drops too,
                    // silently, while every other field in the same query still lands.
                    when(
                        e_binop(
                            e_binop(path_length.clone(), BinOp::StrictEq, e_int(0)),
                            BinOp::Or,
                            e_binop(path_length.clone(), BinOp::Gt, e_int(MAX_PATH_SEGMENTS)),
                        ),
                        vec![s_continue(1)],
                        None,
                    ),
                    s_expr(e_assign(
                        e_var("result"),
                        e_call(
                            INSERT_NAME,
                            vec![
                                e_var("result"),
                                e_var("path"),
                                e_int(0),
                                e_call(DECODE_NAME, vec![e_var("rawValue")]),
                            ],
                        ),
                    )),
                    advance("accepted", 1),
                ],
            ),
            s_return_void(),
        ])
        .build()
}
