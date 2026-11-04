//! Purpose:
//! Builds the shared PHP-side validators, packet decoders and diagnostics used by every
//! synthetic sodium helper.
//!
//! Called from:
//! - `crate::sodium_prelude::declarations`.
//!
//! Key details:
//! - Weak/strict coercion, parameter names, and exception text are php-src 8.5's own,
//!   kept in one place so every generated helper stays byte-compatible.
//! - The bridge packet prefix selects success bytes, an integer result, `false`, or one
//!   of the crypto-failure exceptions; no helper re-implements that mapping.
//! - Every statement is built as AST nodes; no PHP source is parsed.

use super::dsl::*;
use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds every shared validator helper.
pub(super) fn declarations() -> Vec<Stmt> {
    vec![
        type_name(),
        arg_error(),
        same_length(),
        string_parameter(),
        int_parameter(),
        float_to_int(),
        string_result(),
        string_or_false_result(),
        bool_result(),
        int_result(),
        warning(),
    ]
}

/// `__elephc_sodium_type_name(mixed $value): string` — PHP's value-name spelling.
fn type_name() -> Stmt {
    function("__elephc_sodium_type_name")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            if_then(call("is_object", vec![v("value")]), vec![ret(call("get_class", vec![v("value")]))]),
            if_then(call("is_bool", vec![v("value")]), vec![ret(e_ternary(v("value"), s("true"), s("false")))]),
            if_then(bin(v("value"), BinOp::StrictEq, e_null()), vec![ret(s("null"))]),
            if_then(call("is_int", vec![v("value")]), vec![ret(s("int"))]),
            if_then(call("is_float", vec![v("value")]), vec![ret(s("float"))]),
            if_then(call("is_string", vec![v("value")]), vec![ret(s("string"))]),
            if_then(call("is_array", vec![v("value")]), vec![ret(s("array"))]),
            ret(s("resource")),
        ])
        .build()
}

/// `__elephc_sodium_arg_error(string $function, int $position, string $name, string $message): void`
fn arg_error() -> Stmt {
    function("__elephc_sodium_arg_error")
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![throw_new(
            "SodiumException",
            concat_all(vec![
                v("function"),
                s("(): Argument #"),
                v("position"),
                s(" ($"),
                v("name"),
                s(") "),
                v("message"),
            ]),
        )])
        .build()
}

/// `__elephc_sodium_same_length(string $function): void` — php-src's two-argument length error.
fn same_length() -> Stmt {
    function("__elephc_sodium_same_length")
        .param("function", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![throw_new(
            "SodiumException",
            concat(
                concat(v("function"), s("(): Argument #1 ($string1) and argument #2 ($string2)")),
                s(" must have the same length"),
            ),
        )])
        .build()
}

/// `__elephc_sodium_string_param(...)` — php-src's `string` coercion and TypeError.
fn string_parameter() -> Stmt {
    function("__elephc_sodium_string_param")
        .param("value", t_mixed())
        .param("strict", TypeExpr::Bool)
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            if_then(call("is_string", vec![v("value")]), vec![ret(as_str(v("value")))]),
            if_then(
                and(
                    call("is_object", vec![v("value")]),
                    call(
                        "in_array",
                        vec![
                            s("__toString"),
                            call("get_class_methods", vec![v("value")]),
                            b(true),
                        ],
                    ),
                ),
                vec![ret(as_str(v("value")))],
            ),
            if_then(
                not(v("strict")),
                vec![
                    if_then(
                        or(
                            call("is_int", vec![v("value")]),
                            or(call("is_float", vec![v("value")]), call("is_bool", vec![v("value")])),
                        ),
                        vec![ret(as_str(v("value")))],
                    ),
                    if_then(
                        bin(v("value"), BinOp::StrictEq, e_null()),
                        vec![
                            expr(call(
                                "__elephc_sodium_warning",
                                vec![
                                    v("function"),
                                    concat_all(vec![
                                        s("Passing null to parameter #"),
                                        v("position"),
                                        s(" ($"),
                                        v("name"),
                                        s(") of type string is deprecated"),
                                    ]),
                                ],
                            )),
                            ret(s("")),
                        ],
                    ),
                ],
            ),
            throw_new(
                "TypeError",
                concat_all(vec![
                    v("function"),
                    s("(): Argument #"),
                    v("position"),
                    s(" ($"),
                    v("name"),
                    s(") must be of type string, "),
                    call("__elephc_sodium_type_name", vec![v("value")]),
                    s(" given"),
                ]),
            ),
        ])
        .build()
}

/// `__elephc_sodium_int_param(...)` — php-src's `int` coercion and TypeError.
fn int_parameter() -> Stmt {
    function("__elephc_sodium_int_param")
        .param("value", t_mixed())
        .param("strict", TypeExpr::Bool)
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            if_then(call("is_int", vec![v("value")]), vec![ret(as_int(v("value")))]),
            if_then(
                not(v("strict")),
                vec![
                    if_then(call("is_bool", vec![v("value")]), vec![ret(as_int(v("value")))]),
                    if_then(
                        call("is_float", vec![v("value")]),
                        vec![ret(call(
                            "__elephc_sodium_float_to_int",
                            vec![
                                v("value"),
                                concat_all(vec![
                                    v("function"),
                                    s("(): Argument #"),
                                    v("position"),
                                    s(" ($"),
                                    v("name"),
                                    s(") must be of type int, float given"),
                                ]),
                            ],
                        ))],
                    ),
                    if_then(
                        and(call("is_string", vec![v("value")]), call("is_numeric", vec![v("value")])),
                        vec![ret(as_int(v("value")))],
                    ),
                    if_then(
                        bin(v("value"), BinOp::StrictEq, e_null()),
                        vec![
                            expr(call(
                                "__elephc_sodium_warning",
                                vec![
                                    v("function"),
                                    concat_all(vec![
                                        s("Passing null to parameter #"),
                                        v("position"),
                                        s(" ($"),
                                        v("name"),
                                        s(") of type int is deprecated"),
                                    ]),
                                ],
                            )),
                            ret(i(0)),
                        ],
                    ),
                ],
            ),
            throw_new(
                "TypeError",
                concat_all(vec![
                    v("function"),
                    s("(): Argument #"),
                    v("position"),
                    s(" ($"),
                    v("name"),
                    s(") must be of type int, "),
                    call("__elephc_sodium_type_name", vec![v("value")]),
                    s(" given"),
                ]),
            ),
        ])
        .build()
}

/// `__elephc_sodium_float_to_int(mixed $value, string $message): int`.
fn float_to_int() -> Stmt {
    function("__elephc_sodium_float_to_int")
        .param("value", t_mixed())
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            if_then(
                or(
                    not(call("is_finite", vec![v("value")])),
                    or(
                        bin(v("value"), BinOp::Lt, e_neg(e_float(9223372036854775808.0))),
                        bin(v("value"), BinOp::GtEq, e_float(9223372036854775808.0)),
                    ),
                ),
                vec![throw_new("TypeError", v("message"))],
            ),
            assign("integer", as_int(v("value"))),
            if_then(
                bin(as_str(v("value")), BinOp::StrictNotEq, as_str(v("integer"))),
                vec![expr(call(
                    "trigger_error",
                    vec![
                        concat(
                            concat(s("Implicit conversion from float "), as_str(v("value"))),
                            s(" to int loses precision"),
                        ),
                        e_const("E_USER_DEPRECATED"),
                    ],
                ))],
            ),
            ret(v("integer")),
        ])
        .build()
}

/// `__elephc_sodium_string_result(string $packet, string $function): string`.
fn string_result() -> Stmt {
    function("__elephc_sodium_string_result")
        .param("packet", TypeExpr::Str)
        .param("function", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            assign("kind", index(v("packet"), i(0))),
            if_then(bin(v("kind"), BinOp::StrictEq, s("O")), vec![ret(call("hex2bin", vec![call("substr", vec![v("packet"), i(1)])]))]),
            if_then(bin(v("kind"), BinOp::StrictEq, s("C")), vec![throw_new("SodiumException", s("conversion failed"))]),
            if_then(bin(v("kind"), BinOp::StrictEq, s("I")), vec![throw_new("SodiumException", s("Result is identity element"))]),
            if_then(
                bin(v("kind"), BinOp::StrictEq, s("Z")),
                vec![throw_new(
                    "SodiumException",
                    concat(v("function"), s("(): Argument #1 ($n) must not be zero")),
                )],
            ),
            throw_new("SodiumException", s("internal error")),
        ])
        .build()
}

/// `__elephc_sodium_string_or_false_result(string $packet, string $function): string|false`.
fn string_or_false_result() -> Stmt {
    function("__elephc_sodium_string_or_false_result")
        .param("packet", TypeExpr::Str)
        .param("function", TypeExpr::Str)
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Bool]))
        .body(vec![
            if_then(
                bin(index(v("packet"), i(0)), BinOp::StrictEq, s("F")),
                vec![ret(e_bool(false))],
            ),
            ret(call("__elephc_sodium_string_result", vec![v("packet"), v("function")])),
        ])
        .build()
}

/// `__elephc_sodium_bool_result(string $packet): bool`.
fn bool_result() -> Stmt {
    function("__elephc_sodium_bool_result")
        .param("packet", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            if_then(
                bin(index(v("packet"), i(0)), BinOp::StrictEq, s("F")),
                vec![ret(e_bool(false))],
            ),
            if_then(
                bin(index(v("packet"), i(0)), BinOp::StrictEq, s("M")),
                vec![ret(bin(as_int(call("substr", vec![v("packet"), i(1)])), BinOp::StrictNotEq, i(0)))],
            ),
            throw_new("SodiumException", s("internal error")),
        ])
        .build()
}

/// `__elephc_sodium_int_result(string $packet): int`.
fn int_result() -> Stmt {
    function("__elephc_sodium_int_result")
        .param("packet", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            if_then(
                bin(index(v("packet"), i(0)), BinOp::StrictEq, s("M")),
                vec![ret(as_int(call("substr", vec![v("packet"), i(1)])))],
            ),
            throw_new("SodiumException", s("internal error")),
        ])
        .build()
}

/// `__elephc_sodium_warning(string $function, string $message): void`.
fn warning() -> Stmt {
    function("__elephc_sodium_warning")
        .param("function", TypeExpr::Str)
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![expr(call(
            "trigger_error",
            vec![
                e_ternary(
                    bin(v("function"), BinOp::StrictEq, s("__global__")),
                    v("message"),
                    concat_all(vec![v("function"), s("(): "), v("message")]),
                ),
                e_const("E_USER_WARNING"),
            ],
        ))])
        .build()
}
