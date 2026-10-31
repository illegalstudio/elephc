//! Purpose:
//! Builds the shared PHP-side validators and coercion helpers used by the GMP class
//! adapters and the 51 procedural wrappers.
//!
//! Called from:
//! - `crate::gmp_prelude::declarations`.
//!
//! Key details:
//! - The packet prefixes are the bridge contract documented in `crates/elephc-gmp`.
//! - Error text and weak/strict coercion rules are php-src 8.5's own, kept in one place.
//! - Every statement is built as AST nodes; no PHP source is parsed.

use super::dsl::*;
use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Builds every shared validator helper.
pub(super) fn declarations() -> Vec<Stmt> {
    vec![
        value_unpacker(),
        type_name(),
        zval_type(),
        float_to_int(),
        operand(),
        init_operand(),
        operator_value(),
        shift_value(),
        int_parameter(),
        bool_parameter(),
        string_parameter(),
        arithmetic(),
        require_nonnegative(),
        base_parameters(),
        index_parameter(),
        div_parts(),
    ]
}

/// `__elephc_gmp_value(string $packet): string` — unwrap one success packet.
fn value_unpacker() -> Stmt {
    function("__elephc_gmp_value")
        .param("packet", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![s_switch(
            index(v("packet"), i(0)),
            vec![
                (
                    vec![s("O")],
                    vec![ret(call("__elephc_string_copy", vec![call("substr", vec![v("packet"), i(1)])]))],
                ),
                (
                    vec![s("V")],
                    vec![throw_new("ValueError", call("substr", vec![v("packet"), i(1)]))],
                ),
                (
                    vec![s("Z")],
                    vec![throw_new(
                        "DivisionByZeroError",
                        call("substr", vec![v("packet"), i(1)]),
                    )],
                ),
                (
                    vec![s("T")],
                    vec![throw_new("TypeError", call("substr", vec![v("packet"), i(1)]))],
                ),
                (
                    vec![s("F")],
                    vec![throw_new(
                        "Error",
                        s("GMP operation produced false in a string context"),
                    )],
                ),
            ],
            Some(vec![throw_new("Error", s("internal GMP bridge failure"))]),
        )])
        .build()
}

/// `__elephc_gmp_type_name(mixed $value): string` — PHP's value-name spelling.
fn type_name() -> Stmt {
    function("__elephc_gmp_type_name")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            if_(
                call("is_object", vec![v("value")]),
                vec![ret(call("get_class", vec![v("value")]))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_bool", vec![v("value")]),
                vec![ret(e_ternary(v("value"), s("true"), s("false")))],
                Vec::new(),
                None,
            ),
            if_(
                bin(v("value"), BinOp::StrictEq, e_null()),
                vec![ret(s("null"))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_int", vec![v("value")]),
                vec![ret(s("int"))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_float", vec![v("value")]),
                vec![ret(s("float"))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_string", vec![v("value")]),
                vec![ret(s("string"))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_array", vec![v("value")]),
                vec![ret(s("array"))],
                Vec::new(),
                None,
            ),
            ret(s("resource")),
        ])
        .build()
}

/// `__elephc_gmp_zval_type(mixed $value): string` — PHP's type-name spelling.
fn zval_type() -> Stmt {
    function("__elephc_gmp_zval_type")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            if_(
                call("is_object", vec![v("value")]),
                vec![ret(call("get_class", vec![v("value")]))],
                Vec::new(),
                None,
            ),
            assign("type", call("gettype", vec![v("value")])),
            s_switch(
                v("type"),
                vec![
                    (vec![s("boolean")], vec![ret(s("bool"))]),
                    (vec![s("integer")], vec![ret(s("int"))]),
                    (vec![s("double")], vec![ret(s("float"))]),
                    (vec![s("NULL")], vec![ret(s("null"))]),
                ],
                Some(vec![ret(v("type"))]),
            ),
        ])
        .build()
}

/// `__elephc_gmp_float_to_int(mixed $value, string $message): int`.
fn float_to_int() -> Stmt {
    function("__elephc_gmp_float_to_int")
        .param("value", t_mixed())
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            if_(
                or(
                    not(call("is_finite", vec![v("value")])),
                    or(
                        bin(v("value"), BinOp::Lt, e_neg(e_float(9223372036854775808.0))),
                        bin(v("value"), BinOp::GtEq, e_float(9223372036854775808.0)),
                    ),
                ),
                vec![throw_new("TypeError", v("message"))],
                Vec::new(),
                None,
            ),
            assign("integer", as_int(v("value"))),
            if_(
                bin(e_cast(crate::parser::ast::CastType::Float, v("integer")), BinOp::StrictNotEq, v("value")),
                vec![expr(call(
                    "trigger_error",
                    vec![
                        concat(
                            concat(
                                s("Implicit conversion from float "),
                                as_str(v("value")),
                            ),
                            s(" to int loses precision"),
                        ),
                        e_const("E_USER_DEPRECATED"),
                    ],
                ))],
                Vec::new(),
                None,
            ),
            ret(v("integer")),
        ])
        .build()
}

/// `__elephc_gmp_operand(mixed $value, bool $strict, string $function, int $position, string $name): string`.
fn operand() -> Stmt {
    function("__elephc_gmp_operand")
        .param("value", t_mixed())
        .param("strict", TypeExpr::Bool)
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            if_(
                e_instance_of(v("value"), "GMP"),
                vec![ret(e_prop(v("value"), "__elephc_num"))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_int", vec![v("value")]),
                vec![ret(as_str(v("value")))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_string", vec![v("value")]),
                vec![
                    assign("packet", call("elephc_gmp_parse", vec![v("value"), i(0)])),
                    if_(
                        bin(index(v("packet"), i(0)), BinOp::StrictEq, s("O")),
                        vec![ret(call("__elephc_string_copy", vec![call("substr", vec![v("packet"), i(1)])]))],
                        Vec::new(),
                        None,
                    ),
                    throw_new(
                        "ValueError",
                        concat(
                            concat(
                                concat(
                                    concat(
                                        v("function"),
                                        s("(): Argument #"),
                                    ),
                                    v("position"),
                                ),
                                s(" ($"),
                            ),
                            concat(v("name"), s(") is not an integer string")),
                        ),
                    ),
                ],
                Vec::new(),
                None,
            ),
            if_(
                not(v("strict")),
                vec![
                    if_(
                        call("is_float", vec![v("value")]),
                        vec![ret(as_str(call(
                            "__elephc_gmp_float_to_int",
                            vec![
                                v("value"),
                                concat(
                                    concat(
                                        concat(
                                            concat(
                                                concat(
                                                    v("function"),
                                                    s("(): Argument #"),
                                                ),
                                                v("position"),
                                            ),
                                            s(" ($"),
                                        ),
                                        v("name"),
                                    ),
                                    s(") must be of type GMP|string|int, float given"),
                                ),
                            ],
                        )))],
                        Vec::new(),
                        None,
                    ),
                    if_(
                        call("is_bool", vec![v("value")]),
                        vec![ret(e_ternary(v("value"), s("1"), s("0")))],
                        Vec::new(),
                        None,
                    ),
                    if_(
                        bin(v("value"), BinOp::StrictEq, e_null()),
                        vec![
                            expr(call(
                                "trigger_error",
                                vec![
                                    concat(
                                        concat(
                                            concat(
                                                concat(
                                                    concat(
                                                        v("function"),
                                                        s("(): Passing null to parameter #"),
                                                    ),
                                                    v("position"),
                                                ),
                                                s(" ($"),
                                            ),
                                            v("name"),
                                        ),
                                        s(") of type GMP|string|int is deprecated"),
                                    ),
                                    e_const("E_USER_DEPRECATED"),
                                ],
                            )),
                            ret(s("0")),
                        ],
                        Vec::new(),
                        None,
                    ),
                ],
                Vec::new(),
                None,
            ),
            throw_new(
                "TypeError",
                concat(
                    concat(
                        concat(
                            concat(
                                concat(
                                    v("function"),
                                    s("(): Argument #"),
                                ),
                                v("position"),
                            ),
                            s(" ($"),
                        ),
                        v("name"),
                    ),
                    concat(
                        s(") must be of type GMP|string|int, "),
                        concat(call("__elephc_gmp_type_name", vec![v("value")]), s(" given")),
                    ),
                ),
            ),
        ])
        .build()
}

/// `__elephc_gmp_init_operand(...)` — the narrower `string|int` constructor/`gmp_init` form.
fn init_operand() -> Stmt {
    function("__elephc_gmp_init_operand")
        .param("value", t_mixed())
        .param("strict", TypeExpr::Bool)
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .param("base", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            if_(
                call("is_int", vec![v("value")]),
                vec![ret(as_str(v("value")))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_string", vec![v("value")]),
                vec![
                    assign(
                        "packet",
                        call("elephc_gmp_parse", vec![v("value"), v("base")]),
                    ),
                    if_(
                        bin(index(v("packet"), i(0)), BinOp::StrictEq, s("O")),
                        vec![ret(call("__elephc_string_copy", vec![call("substr", vec![v("packet"), i(1)])]))],
                        Vec::new(),
                        None,
                    ),
                    throw_new(
                        "ValueError",
                        concat(
                            concat(
                                concat(
                                    concat(v("function"), s("(): Argument #")),
                                    v("position"),
                                ),
                                s(" ($"),
                            ),
                            concat(v("name"), s(") is not an integer string")),
                        ),
                    ),
                ],
                Vec::new(),
                None,
            ),
            if_(
                not(v("strict")),
                vec![
                    if_(
                        call("is_float", vec![v("value")]),
                        vec![
                            if_(
                                or(
                                    not(call("is_finite", vec![v("value")])),
                                    or(
                                        bin(
                                            v("value"),
                                            BinOp::Lt,
                                            e_neg(e_float(9223372036854775808.0)),
                                        ),
                                        bin(
                                            v("value"),
                                            BinOp::GtEq,
                                            e_float(9223372036854775808.0),
                                        ),
                                    ),
                                ),
                                vec![throw_new(
                                    "ValueError",
                                    concat(
                                        concat(
                                            concat(
                                                concat(v("function"), s("(): Argument #")),
                                                v("position"),
                                            ),
                                            s(" ($"),
                                        ),
                                        concat(v("name"), s(") is not an integer string")),
                                    ),
                                )],
                                Vec::new(),
                                None,
                            ),
                            ret(as_str(call(
                                "__elephc_gmp_float_to_int",
                                vec![
                                    v("value"),
                                    concat(
                                        concat(
                                            concat(
                                                concat(v("function"), s("(): Argument #")),
                                                v("position"),
                                            ),
                                            s(" ($"),
                                        ),
                                        concat(v("name"), s(") must be of type string|int, float given")),
                                    ),
                                ],
                            ))),
                        ],
                        Vec::new(),
                        None,
                    ),
                    if_(
                        call("is_bool", vec![v("value")]),
                        vec![ret(e_ternary(v("value"), s("1"), s("0")))],
                        Vec::new(),
                        None,
                    ),
                    if_(
                        bin(v("value"), BinOp::StrictEq, e_null()),
                        vec![
                            expr(call(
                                "trigger_error",
                                vec![
                                    concat(
                                        concat(
                                            concat(
                                                concat(v("function"), s("(): Passing null to parameter #")),
                                                v("position"),
                                            ),
                                            s(" ($"),
                                        ),
                                        concat(v("name"), s(") of type string|int is deprecated")),
                                    ),
                                    e_const("E_USER_DEPRECATED"),
                                ],
                            )),
                            ret(s("0")),
                        ],
                        Vec::new(),
                        None,
                    ),
                ],
                Vec::new(),
                None,
            ),
            throw_new(
                "TypeError",
                concat(
                    concat(
                        concat(
                            concat(v("function"), s("(): Argument #")),
                            v("position"),
                        ),
                        s(" ($"),
                    ),
                    concat(
                        concat(v("name"), s(") must be of type string|int, ")),
                        concat(call("__elephc_gmp_type_name", vec![v("value")]), s(" given")),
                    ),
                ),
            ),
        ])
        .build()
}

/// `__elephc_gmp_operator_value(mixed $value): string`.
fn operator_value() -> Stmt {
    function("__elephc_gmp_operator_value")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            if_(
                e_instance_of(v("value"), "GMP"),
                vec![ret(e_prop(v("value"), "__elephc_num"))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_int", vec![v("value")]),
                vec![ret(as_str(v("value")))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_string", vec![v("value")]),
                vec![
                    assign("packet", call("elephc_gmp_parse", vec![v("value"), i(0)])),
                    if_(
                        bin(index(v("packet"), i(0)), BinOp::StrictEq, s("O")),
                        vec![ret(call("__elephc_string_copy", vec![call("substr", vec![v("packet"), i(1)])]))],
                        Vec::new(),
                        None,
                    ),
                    throw_new("ValueError", s("Number is not an integer string")),
                ],
                Vec::new(),
                None,
            ),
            if_(
                call("is_float", vec![v("value")]),
                vec![ret(as_str(call(
                    "__elephc_gmp_float_to_int",
                    vec![
                        v("value"),
                        s("Number must be of type GMP|string|int, float given"),
                    ],
                )))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_bool", vec![v("value")]),
                vec![ret(e_ternary(v("value"), s("1"), s("0")))],
                Vec::new(),
                None,
            ),
            if_(
                bin(v("value"), BinOp::StrictEq, e_null()),
                vec![throw_new(
                    "TypeError",
                    s("Number must be of type GMP|string|int, null given"),
                )],
                Vec::new(),
                None,
            ),
            throw_new(
                "TypeError",
                concat(
                    s("Number must be of type GMP|string|int, "),
                    concat(
                        call("__elephc_gmp_type_name", vec![v("value")]),
                        s(" given"),
                    ),
                ),
            ),
        ])
        .build()
}

/// `__elephc_gmp_shift_value(mixed $value, string $symbol): int`.
fn shift_value() -> Stmt {
    function("__elephc_gmp_shift_value")
        .param("value", t_mixed())
        .param("symbol", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            if_(
                call("is_int", vec![v("value")]),
                vec![ret(v("value"))],
                Vec::new(),
                None,
            ),
            if_(
                call("is_float", vec![v("value")]),
                vec![
                    if_(
                        or(
                            not(call("is_finite", vec![v("value")])),
                            or(
                                bin(v("value"), BinOp::Lt, e_neg(e_float(9223372036854775808.0))),
                                bin(v("value"), BinOp::GtEq, e_float(9223372036854775808.0)),
                            ),
                        ),
                        vec![throw_new(
                            "TypeError",
                            concat(
                                concat(s("Unsupported operand types: GMP "), v("symbol")),
                                s(" float"),
                            ),
                        )],
                        Vec::new(),
                        None,
                    ),
                    ret(as_int(v("value"))),
                ],
                Vec::new(),
                None,
            ),
            if_(
                call("is_string", vec![v("value")]),
                vec![
                    assign("trimmed", call("trim", vec![v("value")])),
                    assign("digits", call("ltrim", vec![v("trimmed"), s("+-")])),
                    assign("valid", bin(v("digits"), BinOp::StrictNotEq, s(""))),
                    for_(
                        Some(assign("index", i(0))),
                        Some(bin(
                            v("index"),
                            BinOp::Lt,
                            call("strlen", vec![v("digits")]),
                        )),
                        Some(expr(e_post_inc("index"))),
                        vec![if_(
                            or(
                                bin(index(v("digits"), v("index")), BinOp::Lt, s("0")),
                                bin(index(v("digits"), v("index")), BinOp::Gt, s("9")),
                            ),
                            vec![assign("valid", b(false)), break_()],
                            Vec::new(),
                            None,
                        )],
                    ),
                    if_(
                        not(v("valid")),
                        vec![throw_new("ValueError", s("Number is not an integer string"))],
                        Vec::new(),
                        None,
                    ),
                    ret(as_int(v("trimmed"))),
                ],
                Vec::new(),
                None,
            ),
            if_(
                e_instance_of(v("value"), "GMP"),
                vec![ret(instance_method(v("value"), "__elephc_to_int", Vec::new()))],
                Vec::new(),
                None,
            ),
            throw_new(
                "TypeError",
                concat(
                    concat(
                        s("Unsupported operand types: GMP "),
                        v("symbol"),
                    ),
                    concat(s(" "), call("__elephc_gmp_zval_type", vec![v("value")])),
                ),
            ),
        ])
        .build()
}

/// `__elephc_gmp_int_parameter(...)`.
fn int_parameter() -> Stmt {
    function("__elephc_gmp_int_parameter")
        .param("value", t_mixed())
        .param("strict", TypeExpr::Bool)
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            if_(
                call("is_int", vec![v("value")]),
                vec![ret(v("value"))],
                Vec::new(),
                None,
            ),
            if_(
                not(v("strict")),
                vec![
                    if_(
                        call("is_bool", vec![v("value")]),
                        vec![ret(as_int(v("value")))],
                        Vec::new(),
                        None,
                    ),
                    if_(
                        call("is_float", vec![v("value")]),
                        vec![ret(call(
                            "__elephc_gmp_float_to_int",
                            vec![
                                v("value"),
                                concat(
                                    concat(
                                        concat(
                                            concat(v("function"), s("(): Argument #")),
                                            v("position"),
                                        ),
                                        s(" ($"),
                                    ),
                                    concat(v("name"), s(") must be of type int, float given")),
                                ),
                            ],
                        ))],
                        Vec::new(),
                        None,
                    ),
                    if_(
                        and(
                            call("is_string", vec![v("value")]),
                            call("is_numeric", vec![v("value")]),
                        ),
                        vec![ret(as_int(v("value")))],
                        Vec::new(),
                        None,
                    ),
                    if_(
                        bin(v("value"), BinOp::StrictEq, e_null()),
                        vec![
                            expr(call(
                                "trigger_error",
                                vec![
                                    concat(
                                        concat(
                                            concat(
                                                concat(v("function"), s("(): Passing null to parameter #")),
                                                v("position"),
                                            ),
                                            s(" ($"),
                                        ),
                                        concat(v("name"), s(") of type int is deprecated")),
                                    ),
                                    e_const("E_USER_DEPRECATED"),
                                ],
                            )),
                            ret(i(0)),
                        ],
                        Vec::new(),
                        None,
                    ),
                ],
                Vec::new(),
                None,
            ),
            throw_new(
                "TypeError",
                concat(
                    concat(
                        concat(
                            concat(v("function"), s("(): Argument #")),
                            v("position"),
                        ),
                        s(" ($"),
                    ),
                    concat(
                        concat(v("name"), s(") must be of type int, ")),
                        concat(call("__elephc_gmp_type_name", vec![v("value")]), s(" given")),
                    ),
                ),
            ),
        ])
        .build()
}

/// `__elephc_gmp_bool_parameter(...)`.
fn bool_parameter() -> Stmt {
    function("__elephc_gmp_bool_parameter")
        .param("value", t_mixed())
        .param("strict", TypeExpr::Bool)
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            if_(
                call("is_bool", vec![v("value")]),
                vec![ret(v("value"))],
                Vec::new(),
                None,
            ),
            if_(
                not(v("strict")),
                vec![ret(as_bool(v("value")))],
                Vec::new(),
                None,
            ),
            throw_new(
                "TypeError",
                concat(
                    concat(
                        concat(
                            concat(v("function"), s("(): Argument #")),
                            v("position"),
                        ),
                        s(" ($"),
                    ),
                    concat(
                        concat(v("name"), s(") must be of type bool, ")),
                        concat(call("__elephc_gmp_type_name", vec![v("value")]), s(" given")),
                    ),
                ),
            ),
        ])
        .build()
}

/// `__elephc_gmp_string_parameter(...)`.
fn string_parameter() -> Stmt {
    function("__elephc_gmp_string_parameter")
        .param("value", t_mixed())
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            if_(
                call("is_string", vec![v("value")]),
                vec![ret(v("value"))],
                Vec::new(),
                None,
            ),
            throw_new(
                "TypeError",
                concat(
                    concat(
                        concat(
                            concat(v("function"), s("(): Argument #")),
                            v("position"),
                        ),
                        s(" ($"),
                    ),
                    concat(
                        concat(v("name"), s(") must be of type string, ")),
                        concat(call("__elephc_gmp_type_name", vec![v("value")]), s(" given")),
                    ),
                ),
            ),
        ])
        .build()
}

/// `__elephc_gmp_arithmetic(int $op, mixed $left, mixed $right, string $zero): GMP`.
fn arithmetic() -> Stmt {
    function("__elephc_gmp_arithmetic")
        .param("op", TypeExpr::Int)
        .param("left", t_mixed())
        .param("right", t_mixed())
        .param("zero_message", TypeExpr::Str)
        .returns(t_class("GMP"))
        .body(vec![
            assign("first", call("__elephc_gmp_operator_value", vec![v("left")])),
            assign("second", call("__elephc_gmp_operator_value", vec![v("right")])),
            assign(
                "packet",
                call(
                    "elephc_gmp_binary",
                    vec![v("op"), v("first"), v("second")],
                ),
            ),
            if_(
                bin(index(v("packet"), i(0)), BinOp::StrictEq, s("Z")),
                vec![throw_new("DivisionByZeroError", v("zero_message"))],
                Vec::new(),
                None,
            ),
            ret(new(
                "GMP",
                vec![call("__elephc_gmp_value", vec![v("packet")])],
            )),
        ])
        .build()
}

/// `__elephc_gmp_require_nonnegative(...)`.
fn require_nonnegative() -> Stmt {
    function("__elephc_gmp_require_nonnegative")
        .param("value", TypeExpr::Str)
        .param("function", TypeExpr::Str)
        .param("position", TypeExpr::Int)
        .param("name", TypeExpr::Str)
        .param("message", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![if_(
            and(
                bin(v("value"), BinOp::StrictNotEq, s("")),
                bin(index(v("value"), i(0)), BinOp::StrictEq, s("-")),
            ),
            vec![throw_new(
                "ValueError",
                concat(
                    concat(
                        concat(
                            concat(
                                concat(
                                    concat(v("function"), s("(): Argument #")),
                                    v("position"),
                                ),
                                s(" ($"),
                            ),
                            v("name"),
                        ),
                        s(") "),
                    ),
                    v("message"),
                ),
            )],
            Vec::new(),
            None,
        )])
        .build()
}

/// `__elephc_gmp_base_parameters(int $word_size, int $flags, string $function): array`.
fn base_parameters() -> Stmt {
    function("__elephc_gmp_base_parameters")
        .param("word_size", TypeExpr::Int)
        .param("flags", TypeExpr::Int)
        .param("function", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            if_(
                bin(v("word_size"), BinOp::Lt, i(1)),
                vec![throw_new(
                    "ValueError",
                    concat(
                        v("function"),
                        s("(): Argument #2 ($word_size) must be greater than or equal to 1"),
                    ),
                )],
                Vec::new(),
                None,
            ),
            assign(
                "order_bits",
                bin(
                    v("flags"),
                    BinOp::BitAnd,
                    bin(e_const("GMP_LSW_FIRST"), BinOp::BitOr, e_const("GMP_MSW_FIRST")),
                ),
            ),
            if_(
                bin(
                    v("order_bits"),
                    BinOp::StrictEq,
                    bin(e_const("GMP_LSW_FIRST"), BinOp::BitOr, e_const("GMP_MSW_FIRST")),
                ),
                vec![throw_new(
                    "ValueError",
                    concat(
                        v("function"),
                        s("(): Argument #3 ($flags) cannot use multiple word order options"),
                    ),
                )],
                Vec::new(),
                None,
            ),
            assign(
                "order",
                e_ternary(
                    bin(v("order_bits"), BinOp::StrictEq, e_const("GMP_LSW_FIRST")),
                    i(-1),
                    i(1),
                ),
            ),
            assign(
                "endian_bits",
                bin(
                    v("flags"),
                    BinOp::BitAnd,
                    bin(
                        bin(e_const("GMP_LITTLE_ENDIAN"), BinOp::BitOr, e_const("GMP_BIG_ENDIAN")),
                        BinOp::BitOr,
                        e_const("GMP_NATIVE_ENDIAN"),
                    ),
                ),
            ),
            if_(
                or(
                    bin(v("endian_bits"), BinOp::StrictEq, i(0)),
                    bin(v("endian_bits"), BinOp::StrictEq, e_const("GMP_NATIVE_ENDIAN")),
                ),
                vec![assign("endian", i(0))],
                Vec::new(),
                Some(vec![if_(
                    bin(v("endian_bits"), BinOp::StrictEq, e_const("GMP_LITTLE_ENDIAN")),
                    vec![assign("endian", i(-1))],
                    Vec::new(),
                    Some(vec![if_(
                        bin(v("endian_bits"), BinOp::StrictEq, e_const("GMP_BIG_ENDIAN")),
                        vec![assign("endian", i(1))],
                        Vec::new(),
                        Some(vec![throw_new(
                            "ValueError",
                            concat(
                                v("function"),
                                s("(): Argument #3 ($flags) cannot use multiple endian options"),
                            ),
                        )]),
                    )]),
                )]),
            ),
            ret(array(vec![v("order"), v("endian")])),
        ])
        .build()
}

/// `__elephc_gmp_index_parameter(int $index, string $function): void`.
fn index_parameter() -> Stmt {
    function("__elephc_gmp_index_parameter")
        .param("index", TypeExpr::Int)
        .param("function", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![if_(
            or(
                bin(v("index"), BinOp::Lt, i(0)),
                bin(
                    call("intdiv", vec![v("index"), i(64)]),
                    BinOp::GtEq,
                    i(2147483647),
                ),
            ),
            vec![throw_new(
                "ValueError",
                concat(
                    v("function"),
                    s("(): Argument #2 ($index) must be between 0 and 2147483647 * 64"),
                ),
            )],
            Vec::new(),
            None,
        )])
        .build()
}

/// `__elephc_gmp_div_parts(bool $strict, mixed $num1, mixed $num2, mixed $mode, string $function): array`.
fn div_parts() -> Stmt {
    function("__elephc_gmp_div_parts")
        .param("strict", TypeExpr::Bool)
        .param("num1", t_mixed())
        .param("num2", t_mixed())
        .param("rounding_mode", t_mixed())
        .param("function", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            assign(
                "first",
                call(
                    "__elephc_gmp_operand",
                    vec![v("num1"), v("strict"), v("function"), i(1), s("num1")],
                ),
            ),
            assign(
                "second",
                call(
                    "__elephc_gmp_operand",
                    vec![v("num2"), v("strict"), v("function"), i(2), s("num2")],
                ),
            ),
            assign(
                "rounding_mode",
                call(
                    "__elephc_gmp_int_parameter",
                    vec![
                        v("rounding_mode"),
                        v("strict"),
                        v("function"),
                        i(3),
                        s("rounding_mode"),
                    ],
                ),
            ),
            if_(
                bin(v("second"), BinOp::StrictEq, s("0")),
                vec![throw_new(
                    "DivisionByZeroError",
                    concat(
                        concat(v("function"), s("(): Argument #2 ($num2) Division by zero")),
                        s(""),
                    ),
                )],
                Vec::new(),
                None,
            ),
            if_(
                and(
                    bin(v("rounding_mode"), BinOp::StrictNotEq, i(0)),
                    and(
                        bin(v("rounding_mode"), BinOp::StrictNotEq, i(1)),
                        bin(v("rounding_mode"), BinOp::StrictNotEq, i(2)),
                    ),
                ),
                vec![throw_new(
                    "ValueError",
                    concat(
                        v("function"),
                        s("(): Argument #3 ($rounding_mode) must be one of GMP_ROUND_ZERO, GMP_ROUND_PLUSINF, or GMP_ROUND_MINUSINF"),
                    ),
                )],
                Vec::new(),
                None,
            ),
            assign(
                "op",
                e_ternary(
                    bin(v("rounding_mode"), BinOp::StrictEq, i(1)),
                    i(BIN_DIV_QR_CDIV),
                    e_ternary(
                        bin(v("rounding_mode"), BinOp::StrictEq, i(2)),
                        i(BIN_DIV_QR_FDIV),
                        i(BIN_DIV_QR_TDIV),
                    ),
                ),
            ),
            assign(
                "packet",
                call(
                    "elephc_gmp_binary",
                    vec![v("op"), v("first"), v("second")],
                ),
            ),
            if_(
                bin(index(v("packet"), i(0)), BinOp::StrictEq, s("Z")),
                vec![throw_new(
                    "DivisionByZeroError",
                    concat(v("function"), s("(): Argument #2 ($num2) Division by zero")),
                )],
                Vec::new(),
                None,
            ),
            ret(call(
                "explode",
                vec![s("|"), call("__elephc_gmp_value", vec![v("packet")])],
            )),
        ])
        .build()
}
