//! Purpose:
//! Declares `Random\Randomizer`, whose methods implement php-src's integer, float, byte,
//! shuffle and key-pick algorithms on top of any `Random\Engine` implementation.
//!
//! Called from:
//! - `crate::random_prelude::random_declarations`.
//!
//! Key details:
//! - Every draw goes through `$this->engine->generate()`, so a user-defined engine is just
//!   another implementation and state sharing with the engine object is automatic.
//! - `getInt`/`getFloat`/`getBytesFromString`/`shuffle*`/`pickArrayKeys` transcribe
//!   php-src's draw accounting (byte-wise accumulation, 50-attempt rejection, inclusive
//!   versus exclusive ranges) rather than approximating it.
//! - Unsigned 64-bit arithmetic is expressed over `(low32, high32)` limb pairs so the
//!   rejection threshold and modulus stay exact on a signed 64-bit PHP integer.

use crate::parser::ast::{BinOp, CastType, Expr, Stmt, TypeExpr};
use crate::synthetic_class::*;

use super::{
    add_assign, append, assign, bin, bridge, call, cast, count, expr_stmt, if_chain,
    float, if_then, index, int, int_array, len, limb, low_byte, method_call, not, ord, push, ret,
    self_call,
    set_property, throw_, throw_broken, throw_empty_engine, unique_new, var,
};

/// Builds the `Random\Randomizer` class inside the `Random` namespace.
pub(super) fn declaration() -> Stmt {
    s_namespace("Random", vec![randomizer_class()])
}

/// Builds the final `Randomizer` declaration.
fn randomizer_class() -> Stmt {
    class("Randomizer")
        .final_()
        .readonly_prop("engine", t_class("\\Random\\Engine"))
        .method(constructor())
        .method(serialize_method())
        .method(unserialize_method())
        .method(clone_method())
        .method(next_int_method())
        .method(next_float_method())
        .method(get_float_method())
        .method(get_int_method())
        .method(get_bytes_method())
        .method(get_bytes_from_string_method())
        .method(shuffle_array_method())
        .method(shuffle_bytes_method())
        .method(pick_array_keys_method())
        .method(next_bits_method())
        .method(range_plain_method())
        .method(range32_method())
        .method(range64_method())
        .method(range64_small_method())
        .method(type_name_method())
        .method(logical_shift_right_method())
        .method(split_method())
        .method(join_method())
        .method(add_method())
        .method(subtract_method())
        .method(compare_method())
        .method(modulus_method())
        .method(gamma_max_method())
        .method(ceilint_method())
        .method(gamma_closed_open_method())
        .method(gamma_closed_closed_method())
        .method(gamma_open_closed_method())
        .method(gamma_open_open_method())
        .build()
}

/// Builds `__construct`.
fn constructor() -> MethodBuilder {
    method("__construct")
        .param_default("engine", t_mixed(), e_null())
        .body(vec![
            if_then(
                bin(
                    bin(var("engine"), BinOp::StrictNotEq, e_null()),
                    BinOp::And,
                    not(e_instance_of(var("engine"), "\\Random\\Engine")),
                ),
                vec![throw_(unique_new(
                    "\\TypeError",
                    vec![e_concat_all(vec![
                        e_str(
                            "Random\\Randomizer::__construct(): Argument #1 ($engine) must be of type ?Random\\Engine, ",
                        ),
                        self_call("typeName", vec![var("engine")]),
                        e_str(" given"),
                    ])],
                ))],
            ),
            if_then(
                call("isset", vec![e_this_prop("engine")]),
                vec![throw_(unique_new(
                    "\\Error",
                    vec![e_str(
                        "Cannot modify readonly property Random\\Randomizer::$engine",
                    )],
                ))],
            ),
            set_property(
                "engine",
                e_null_coalesce(
                    var("engine"),
                    unique_new("\\Random\\Engine\\Secure", vec![]),
                ),
            ),
        ])
}

/// Builds `__serialize`.
fn serialize_method() -> MethodBuilder {
    method("__serialize").returns(t_array()).body(vec![ret(
        e_array_assoc(vec![(
            int(0),
            e_array_assoc(vec![(e_str("engine"), e_this_prop("engine"))]),
        )]),
    )])
}

/// Builds `__unserialize`.
fn unserialize_method() -> MethodBuilder {
    let message = "Invalid serialization data for Random\\Randomizer object";
    method("__unserialize")
        .param("data", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            if_then(
                bin(
                    bin(count(var("data")), BinOp::StrictNotEq, int(1)),
                    BinOp::Or,
                    bin(
                        not(call("isset", vec![index(var("data"), int(0))])),
                        BinOp::Or,
                        not(call("is_array", vec![index(var("data"), int(0))])),
                    ),
                ),
                vec![throw_(invalid_serialization(message))],
            ),
            assign("_members", index(var("data"), int(0))),
            if_then(
                bin(
                    not(call(
                        "isset",
                        vec![index(var("_members"), e_str("engine"))],
                    )),
                    BinOp::Or,
                    not(e_instance_of(
                        index(var("_members"), e_str("engine")),
                        "\\Random\\Engine",
                    )),
                ),
                vec![throw_(invalid_serialization(message))],
            ),
            if_then(
                call("isset", vec![e_this_prop("engine")]),
                vec![throw_(invalid_serialization(message))],
            ),
            expr_stmt(method_call(
                e_this(),
                "__construct",
                vec![index(var("_members"), e_str("engine"))],
            )),
        ])
}

/// Builds `__clone`.
fn clone_method() -> MethodBuilder {
    method("__clone").body(vec![throw_(unique_new(
        "\\Error",
        vec![e_str(
            "Trying to clone an uncloneable object of class Random\\Randomizer",
        )],
    ))])
}

/// Builds `nextInt`.
fn next_int_method() -> MethodBuilder {
    let mut body = draw_statements();
    body.push(assign("_raw", int(0)));
    body.push(accumulate_raw("_size", int(0)));
    body.push(ret(self_call(
        "logicalShiftRight",
        vec![var("_raw"), int(1)],
    )));
    method("nextInt").returns(TypeExpr::Int).body(body)
}

/// Builds `nextFloat`.
fn next_float_method() -> MethodBuilder {
    method("nextFloat").returns(TypeExpr::Float).body(vec![
        assign("_raw", self_call("nextBits", vec![int(64)])),
        ret(bin(
            cast(
                CastType::Float,
                self_call("logicalShiftRight", vec![var("_raw"), int(11)]),
            ),
            BinOp::Mul,
            float(1.0 / 9007199254740992.0),
        )),
    ])
}

/// Builds `getFloat`.
fn get_float_method() -> MethodBuilder {
    method("getFloat")
        .param("min", TypeExpr::Float)
        .param("max", TypeExpr::Float)
        .param_default(
            "boundary",
            t_class("\\Random\\IntervalBoundary"),
            e_class_const("\\Random\\IntervalBoundary", "ClosedOpen"),
        )
        .returns(TypeExpr::Float)
        .body(vec![
            if_then(
                bin(
                    call("is_nan", vec![var("min")]),
                    BinOp::Or,
                    call("is_infinite", vec![var("min")]),
                ),
                vec![throw_(value_error(
                    "Random\\Randomizer::getFloat(): Argument #1 ($min) must be finite",
                ))],
            ),
            if_then(
                bin(
                    call("is_nan", vec![var("max")]),
                    BinOp::Or,
                    call("is_infinite", vec![var("max")]),
                ),
                vec![throw_(value_error(
                    "Random\\Randomizer::getFloat(): Argument #2 ($max) must be finite",
                ))],
            ),
            if_chain(
                bin(
                    var("boundary"),
                    BinOp::StrictEq,
                    e_class_const("\\Random\\IntervalBoundary", "ClosedClosed"),
                ),
                vec![if_then(
                    bin(var("max"), BinOp::Lt, var("min")),
                    vec![throw_(value_error(
                        "Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)",
                    ))],
                )],
                vec![(
                    bin(var("max"), BinOp::LtEq, var("min")),
                    vec![throw_(value_error(
                        "Random\\Randomizer::getFloat(): Argument #2 ($max) must be greater than argument #1 ($min)",
                    ))],
                )],
                None,
            ),
            assign("_result", float(0.0)),
            if_chain(
                bin(
                    var("boundary"),
                    BinOp::StrictEq,
                    e_class_const("\\Random\\IntervalBoundary", "ClosedOpen"),
                ),
                vec![assign(
                    "_result",
                    method_call(e_this(), "gammaClosedOpen", vec![var("min"), var("max")]),
                )],
                vec![
                    (
                        bin(
                            var("boundary"),
                            BinOp::StrictEq,
                            e_class_const("\\Random\\IntervalBoundary", "ClosedClosed"),
                        ),
                        vec![assign(
                            "_result",
                            method_call(
                                e_this(),
                                "gammaClosedClosed",
                                vec![var("min"), var("max")],
                            ),
                        )],
                    ),
                    (
                        bin(
                            var("boundary"),
                            BinOp::StrictEq,
                            e_class_const("\\Random\\IntervalBoundary", "OpenClosed"),
                        ),
                        vec![assign(
                            "_result",
                            method_call(
                                e_this(),
                                "gammaOpenClosed",
                                vec![var("min"), var("max")],
                            ),
                        )],
                    ),
                ],
                Some(vec![assign(
                    "_result",
                    method_call(e_this(), "gammaOpenOpen", vec![var("min"), var("max")]),
                )]),
            ),
            if_then(
                call("is_nan", vec![var("_result")]),
                vec![throw_(value_error(
                    "The given interval is empty, there are no floats between argument #1 ($min) and argument #2 ($max)",
                ))],
            ),
            ret(var("_result")),
        ])
}

/// Builds `getInt`.
fn get_int_method() -> MethodBuilder {
    let mut bad_scaling = draw_statements();
    bad_scaling.push(assign("_raw", int(0)));
    bad_scaling.push(accumulate_raw("_size", int(0)));
    bad_scaling.push(assign(
        "_draw",
        bin(
            bin(var("_raw"), BinOp::BitAnd, e_int(0xFFFFFFFF)),
            BinOp::ShiftRight,
            int(1),
        ),
    ));
    bad_scaling.push(assign(
        "_offset",
        bin(
            bin(
                bin(
                    bin(
                        cast(CastType::Float, var("max")),
                        BinOp::Sub,
                        cast(CastType::Float, var("min")),
                    ),
                    BinOp::Add,
                    float(1.0),
                ),
                BinOp::Mul,
                bin(
                    cast(CastType::Float, var("_draw")),
                    BinOp::Div,
                    float(2147483648.0),
                ),
            ),
            BinOp::Add,
            cast(CastType::Float, var("min")),
        ),
    ));
    bad_scaling.push(ret(cast(CastType::Int, var("_offset"))));

    method("getInt")
        .param("min", TypeExpr::Int)
        .param("max", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            if_then(
                bin(var("max"), BinOp::Lt, var("min")),
                vec![throw_(value_error(
                    "Random\\Randomizer::getInt(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)",
                ))],
            ),
            assign("_engine", e_this_prop("engine")),
            if_then(
                e_instance_of(var("_engine"), "\\Random\\Engine\\Mt19937"),
                vec![
                    assign("_vars", call("get_object_vars", vec![var("_engine")])),
                    if_then(
                        bin(
                            call(
                                "isset",
                                vec![index(var("_vars"), e_str("__elephc_mode"))],
                            ),
                            BinOp::And,
                            bin(
                                index(var("_vars"), e_str("__elephc_mode")),
                                BinOp::StrictEq,
                                int(1),
                            ),
                        ),
                        bad_scaling,
                    ),
                ],
            ),
            ret(self_call("rangePlain", vec![var("min"), var("max")])),
        ])
}

/// Builds `getBytes`.
fn get_bytes_method() -> MethodBuilder {
    let pending_loop = s_for(
        Some(assign("_i", int(0))),
        Some(bin(
            bin(var("_i"), BinOp::Lt, var("_pending_size")),
            BinOp::And,
            bin(var("_total"), BinOp::Lt, var("length")),
        )),
        Some(add_assign("_i", int(1))),
        vec![
            append("_result", index(var("_pending"), var("_i"))),
            add_assign("_total", int(1)),
        ],
    );

    let fast_body = {
        let mut body = draw_statements();
        body.push(if_then(
            bin(var("_size"), BinOp::StrictEq, int(8)),
            vec![
                append(
                    "_result",
                    call("substr", vec![var("_value"), int(0), int(8)]),
                ),
                add_assign("_total", int(8)),
            ],
        ));
        body.push(if_then(
            bin(var("_size"), BinOp::StrictNotEq, int(8)),
            vec![
                assign("_pending", var("_value")),
                assign("_pending_size", var("_size")),
                assign("_has_pending", e_bool(true)),
                assign("_fast", e_bool(false)),
            ],
        ));
        body
    };

    let slow_loop = s_while(
        bin(var("_total"), BinOp::Lt, var("length")),
        {
            let mut body = draw_statements();
            body.push(s_for(
                Some(assign("_i", int(0))),
                Some(bin(
                    bin(var("_i"), BinOp::Lt, var("_size")),
                    BinOp::And,
                    bin(var("_total"), BinOp::Lt, var("length")),
                )),
                Some(add_assign("_i", int(1))),
                vec![
                    append("_result", index(var("_value"), var("_i"))),
                    add_assign("_total", int(1)),
                ],
            ));
            body
        },
    );

    method("getBytes")
        .param("length", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            if_then(
                bin(var("length"), BinOp::Lt, int(1)),
                vec![throw_(value_error(
                    "Random\\Randomizer::getBytes(): Argument #1 ($length) must be greater than 0",
                ))],
            ),
            assign("_result", e_str("")),
            assign("_total", int(0)),
            assign("_pending", e_str("")),
            assign("_pending_size", int(0)),
            assign("_has_pending", e_bool(false)),
            assign("_fast", e_bool(true)),
            s_while(
                bin(
                    var("_fast"),
                    BinOp::And,
                    bin(bin(var("_total"), BinOp::Add, int(8)), BinOp::LtEq, var("length")),
                ),
                fast_body,
            ),
            if_then(var("_has_pending"), vec![pending_loop]),
            slow_loop,
            ret(var("_result")),
        ])
}

/// Builds `getBytesFromString`.
fn get_bytes_from_string_method() -> MethodBuilder {
    let slow_path = s_while(
        bin(len(var("_result")), BinOp::Lt, var("length")),
        vec![
            assign(
                "_offset",
                self_call("rangePlain", vec![int(0), var("_max_offset")]),
            ),
            append("_result", index(var("string"), var("_offset"))),
        ],
    );

    let fast_loop = s_while(
        bin(var("_total"), BinOp::Lt, var("length")),
        {
            let mut body = draw_statements();
            body.push(assign("_raw", int(0)));
            body.push(accumulate_raw("_size", int(0)));
            body.push(assign(
                "_offsets",
                bin(var("_raw"), BinOp::BitAnd, var("_mask")),
            ));
            body.push(s_for(
                Some(assign("_i", int(0))),
                Some(bin(var("_i"), BinOp::Lt, var("_size"))),
                Some(add_assign("_i", int(1))),
                vec![
                    assign("_offset", low_byte(var("_offsets"))),
                    s_assign(
                        "_offsets",
                        bin(var("_offsets"), BinOp::ShiftRight, int(8)),
                    ),
                    if_then(
                        bin(var("_offset"), BinOp::Gt, var("_max_offset")),
                        vec![
                            add_assign("_failures", int(1)),
                            if_then(
                                bin(var("_failures"), BinOp::Gt, int(50)),
                                vec![throw_broken()],
                            ),
                            s_continue(1),
                        ],
                    ),
                    assign("_failures", int(0)),
                    append("_result", index(var("string"), var("_offset"))),
                    add_assign("_total", int(1)),
                    if_then(
                        bin(var("_total"), BinOp::GtEq, var("length")),
                        vec![s_break(1)],
                    ),
                ],
            ));
            body
        },
    );

    method("getBytesFromString")
        .param("string", TypeExpr::Str)
        .param("length", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            assign("_source_length", len(var("string"))),
            if_then(
                bin(var("_source_length"), BinOp::Lt, int(1)),
                vec![throw_(value_error(
                    "Random\\Randomizer::getBytesFromString(): Argument #1 ($string) must not be empty",
                ))],
            ),
            if_then(
                bin(var("length"), BinOp::Lt, int(1)),
                vec![throw_(value_error(
                    "Random\\Randomizer::getBytesFromString(): Argument #2 ($length) must be greater than 0",
                ))],
            ),
            assign("_max_offset", bin(var("_source_length"), BinOp::Sub, int(1))),
            assign("_result", e_str("")),
            if_chain(
                bin(var("_max_offset"), BinOp::Gt, int(255)),
                vec![slow_path],
                vec![],
                Some(vec![
                    assign("_mask", var("_max_offset")),
                    s_assign(
                        "_mask",
                        bin(var("_mask"), BinOp::BitOr, bin(var("_mask"), BinOp::ShiftRight, int(1))),
                    ),
                    s_assign(
                        "_mask",
                        bin(var("_mask"), BinOp::BitOr, bin(var("_mask"), BinOp::ShiftRight, int(2))),
                    ),
                    s_assign(
                        "_mask",
                        bin(var("_mask"), BinOp::BitOr, bin(var("_mask"), BinOp::ShiftRight, int(4))),
                    ),
                    s_for(
                        Some(assign("_i", int(0))),
                        Some(bin(var("_i"), BinOp::Lt, int(7))),
                        Some(add_assign("_i", int(1))),
                        vec![s_assign(
                            "_mask",
                            bin(
                                var("_mask"),
                                BinOp::BitOr,
                                bin(var("_mask"), BinOp::ShiftLeft, int(8)),
                            ),
                        )],
                    ),
                    assign("_failures", int(0)),
                    assign("_total", int(0)),
                    fast_loop,
                ]),
            ),
            ret(var("_result")),
        ])
}

/// Builds `shuffleArray`.
fn shuffle_array_method() -> MethodBuilder {
    method("shuffleArray")
        .param("array", t_array())
        .returns(t_array())
        .body(vec![
            assign("_values", call("array_values", vec![var("array")])),
            assign("_count", count(var("_values"))),
            s_for(
                Some(assign(
                    "_left",
                    bin(var("_count"), BinOp::Sub, int(1)),
                )),
                Some(bin(var("_left"), BinOp::Gt, int(0))),
                Some(s_assign("_left", bin(var("_left"), BinOp::Sub, int(1)))),
                vec![
                    assign(
                        "_index",
                        self_call("rangePlain", vec![int(0), var("_left")]),
                    ),
                    if_then(
                        bin(var("_index"), BinOp::StrictNotEq, var("_left")),
                        vec![swap(
                            "_values",
                            var("_left"),
                            var("_index"),
                        )],
                    ),
                ],
            ),
            ret(var("_values")),
        ])
}

/// Builds `shuffleBytes`.
fn shuffle_bytes_method() -> MethodBuilder {
    method("shuffleBytes")
        .param("bytes", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            assign("_length", len(var("bytes"))),
            if_then(
                bin(var("_length"), BinOp::Lt, int(2)),
                vec![ret(var("bytes"))],
            ),
            assign("_characters", e_array(vec![])),
            s_for(
                Some(assign("_i", int(0))),
                Some(bin(var("_i"), BinOp::Lt, var("_length"))),
                Some(add_assign("_i", int(1))),
                vec![push("_characters", index(var("bytes"), var("_i")))],
            ),
            s_for(
                Some(assign(
                    "_left",
                    bin(var("_length"), BinOp::Sub, int(1)),
                )),
                Some(bin(var("_left"), BinOp::Gt, int(0))),
                Some(s_assign("_left", bin(var("_left"), BinOp::Sub, int(1)))),
                vec![
                    assign(
                        "_index",
                        self_call("rangePlain", vec![int(0), var("_left")]),
                    ),
                    if_then(
                        bin(var("_index"), BinOp::StrictNotEq, var("_left")),
                        vec![swap(
                            "_characters",
                            var("_left"),
                            var("_index"),
                        )],
                    ),
                ],
            ),
            assign("_result", e_str("")),
            s_foreach(
                var("_characters"),
                None,
                "_character",
                vec![append("_result", var("_character"))],
            ),
            ret(var("_result")),
        ])
}

/// Builds `pickArrayKeys`.
fn pick_array_keys_method() -> MethodBuilder {
    method("pickArrayKeys")
        .param("array", t_array())
        .param("num", TypeExpr::Int)
        .returns(t_array())
        .body(vec![
            assign("_available", count(var("array"))),
            if_then(
                bin(var("_available"), BinOp::StrictEq, int(0)),
                vec![throw_(value_error(
                    "Random\\Randomizer::pickArrayKeys(): Argument #1 ($array) must not be empty",
                ))],
            ),
            assign("_keys", call("array_keys", vec![var("array")])),
            if_then(
                bin(var("num"), BinOp::StrictEq, int(1)),
                vec![ret(e_array(vec![index(
                    var("_keys"),
                    self_call(
                        "rangePlain",
                        vec![int(0), bin(var("_available"), BinOp::Sub, int(1))],
                    ),
                )]))],
            ),
            if_then(
                bin(
                    bin(var("num"), BinOp::Lt, int(1)),
                    BinOp::Or,
                    bin(var("num"), BinOp::Gt, var("_available")),
                ),
                vec![throw_(value_error(
                    "Random\\Randomizer::pickArrayKeys(): Argument #2 ($num) must be between 1 and the number of elements in argument #1 ($array)",
                ))],
            ),
            assign("_negative", e_bool(false)),
            assign("_num", var("num")),
            if_then(
                bin(
                    var("num"),
                    BinOp::Gt,
                    bin(var("_available"), BinOp::ShiftRight, int(1)),
                ),
                vec![
                    assign("_negative", e_bool(true)),
                    assign("_num", bin(var("_available"), BinOp::Sub, var("num"))),
                ],
            ),
            assign("_selected", e_array(vec![])),
            assign("_remaining", var("_num")),
            assign("_failures", int(0)),
            s_while(
                bin(var("_remaining"), BinOp::Gt, int(0)),
                vec![
                    assign(
                        "_index",
                        self_call(
                            "rangePlain",
                            vec![int(0), bin(var("_available"), BinOp::Sub, int(1))],
                        ),
                    ),
                    if_chain(
                        call("isset", vec![index(var("_selected"), var("_index"))]),
                        vec![
                            add_assign("_failures", int(1)),
                            if_then(
                                bin(var("_failures"), BinOp::Gt, int(50)),
                                vec![throw_broken()],
                            ),
                        ],
                        vec![],
                        Some(vec![
                            s_array_assign("_selected", var("_index"), e_bool(true)),
                            add_assign("_remaining", int(-1)),
                            assign("_failures", int(0)),
                        ]),
                    ),
                ],
            ),
            assign("_result", e_array(vec![])),
            s_for(
                Some(assign("_i", int(0))),
                Some(bin(var("_i"), BinOp::Lt, var("_available"))),
                Some(add_assign("_i", int(1))),
                vec![if_then(
                    bin(
                        call("isset", vec![index(var("_selected"), var("_i"))]),
                        BinOp::StrictNotEq,
                        var("_negative"),
                    ),
                    vec![push("_result", index(var("_keys"), var("_i")))],
                )],
            ),
            ret(var("_result")),
        ])
}

/// Builds the private `nextBits()` accumulator.
fn next_bits_method() -> MethodBuilder {
    method("nextBits")
        .private()
        .param("bits", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            assign("_result", int(0)),
            assign("_total", int(0)),
            s_while(
                bin(var("_total"), BinOp::Lt, bin(var("bits"), BinOp::ShiftRight, int(3))),
                {
                    let mut body = draw_statements();
                    body.push(assign("_raw", int(0)));
                    body.push(accumulate_raw("_size", var("_total")));
                    body.push(s_assign(
                        "_result",
                        bin(var("_result"), BinOp::BitOr, var("_raw")),
                    ));
                    body.push(add_assign("_total", var("_size")));
                    body
                },
            ),
            ret(var("_result")),
        ])
}

/// Builds `rangePlain(min, max)` with php-src's 32/64-bit dispatch.
fn range_plain_method() -> MethodBuilder {
    method("rangePlain")
        .private()
        .param("min", TypeExpr::Int)
        .param("max", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            assign("_min_parts", self_call("split", vec![var("min")])),
            assign("_max_parts", self_call("split", vec![var("max")])),
            assign(
                "_umax",
                self_call("subtract", vec![var("_max_parts"), var("_min_parts")]),
            ),
            if_then(
                bin(
                    bin(limb(var("_umax"), 1), BinOp::StrictEq, int(0)),
                    BinOp::And,
                    bin(
                        limb(var("_umax"), 0),
                        BinOp::LtEq,
                        e_int(0xFFFFFFFF),
                    ),
                ),
                vec![ret(bin(
                    var("min"),
                    BinOp::Add,
                    self_call("range32", vec![limb(var("_umax"), 0)]),
                ))],
            ),
            assign("_result", self_call("range64", vec![var("_umax")])),
            ret(self_call(
                "join",
                vec![self_call(
                    "add",
                    vec![var("_result"), var("_min_parts")],
                )],
            )),
        ])
}

/// Builds the private 32-bit rejection sampler.
fn range32_method() -> MethodBuilder {
    method("range32")
        .private()
        .param("umax", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            assign(
                "_raw",
                bin(
                    self_call("nextBits", vec![int(32)]),
                    BinOp::BitAnd,
                    e_int(0xFFFFFFFF),
                ),
            ),
            if_then(
                bin(var("umax"), BinOp::StrictEq, e_int(0xFFFFFFFF)),
                vec![ret(var("_raw"))],
            ),
            assign("_count", bin(var("umax"), BinOp::Add, int(1))),
            if_then(
                bin(
                    bin(var("_count"), BinOp::BitAnd, bin(var("_count"), BinOp::Sub, int(1))),
                    BinOp::StrictEq,
                    int(0),
                ),
                vec![ret(bin(
                    var("_raw"),
                    BinOp::BitAnd,
                    bin(var("_count"), BinOp::Sub, int(1)),
                ))],
            ),
            assign(
                "_limit",
                bin(
                    bin(
                        e_int(0xFFFFFFFF),
                        BinOp::Sub,
                        bin(e_int(0xFFFFFFFF), BinOp::Mod, var("_count")),
                    ),
                    BinOp::Sub,
                    int(1),
                ),
            ),
            assign("_attempts", int(0)),
            s_while(
                bin(var("_raw"), BinOp::Gt, var("_limit")),
                vec![
                    add_assign("_attempts", int(1)),
                    if_then(
                        bin(var("_attempts"), BinOp::Gt, int(50)),
                        vec![throw_broken()],
                    ),
                    assign(
                        "_raw",
                        bin(
                            self_call("nextBits", vec![int(32)]),
                            BinOp::BitAnd,
                            e_int(0xFFFFFFFF),
                        ),
                    ),
                ],
            ),
            ret(bin(var("_raw"), BinOp::Mod, var("_count"))),
        ])
}

/// Builds the private unsigned 64-bit rejection sampler.
fn range64_method() -> MethodBuilder {
    method("range64")
        .private()
        .param("umax", int_array())
        .returns(int_array())
        .body(vec![
            assign(
                "_raw",
                self_call("split", vec![self_call("nextBits", vec![int(64)])]),
            ),
            if_then(
                bin(
                    bin(limb(var("umax"), 0), BinOp::StrictEq, e_int(0xFFFFFFFF)),
                    BinOp::And,
                    bin(limb(var("umax"), 1), BinOp::StrictEq, e_int(0xFFFFFFFF)),
                ),
                vec![ret(var("_raw"))],
            ),
            assign(
                "_count",
                self_call(
                    "add",
                    vec![var("umax"), e_array(vec![int(1), int(0)])],
                ),
            ),
            assign(
                "_low_power",
                bin(
                    bin(
                        bin(limb(var("_count"), 1), BinOp::StrictEq, int(0)),
                        BinOp::And,
                        bin(limb(var("_count"), 0), BinOp::StrictNotEq, int(0)),
                    ),
                    BinOp::And,
                    bin(
                        bin(
                            limb(var("_count"), 0),
                            BinOp::BitAnd,
                            bin(limb(var("_count"), 0), BinOp::Sub, int(1)),
                        ),
                        BinOp::StrictEq,
                        int(0),
                    ),
                ),
            ),
            assign(
                "_high_power",
                bin(
                    bin(
                        bin(limb(var("_count"), 0), BinOp::StrictEq, int(0)),
                        BinOp::And,
                        bin(limb(var("_count"), 1), BinOp::StrictNotEq, int(0)),
                    ),
                    BinOp::And,
                    bin(
                        bin(
                            limb(var("_count"), 1),
                            BinOp::BitAnd,
                            bin(limb(var("_count"), 1), BinOp::Sub, int(1)),
                        ),
                        BinOp::StrictEq,
                        int(0),
                    ),
                ),
            ),
            assign(
                "_power_of_two",
                bin(var("_low_power"), BinOp::Or, var("_high_power")),
            ),
            if_then(
                var("_power_of_two"),
                vec![ret(e_array(vec![
                    bin(limb(var("_raw"), 0), BinOp::BitAnd, limb(var("umax"), 0)),
                    bin(limb(var("_raw"), 1), BinOp::BitAnd, limb(var("umax"), 1)),
                ]))],
            ),
            assign(
                "_remainder",
                self_call(
                    "modulus",
                    vec![
                        e_array(vec![e_int(0xFFFFFFFF), e_int(0xFFFFFFFF)]),
                        var("_count"),
                    ],
                ),
            ),
            assign(
                "_limit",
                self_call(
                    "subtract",
                    vec![
                        self_call(
                            "subtract",
                            vec![
                                e_array(vec![e_int(0xFFFFFFFF), e_int(0xFFFFFFFF)]),
                                var("_remainder"),
                            ],
                        ),
                        e_array(vec![int(1), int(0)]),
                    ],
                ),
            ),
            assign("_attempts", int(0)),
            s_while(
                bin(
                    self_call("compare", vec![var("_raw"), var("_limit")]),
                    BinOp::Gt,
                    int(0),
                ),
                vec![
                    add_assign("_attempts", int(1)),
                    if_then(
                        bin(var("_attempts"), BinOp::Gt, int(50)),
                        vec![throw_broken()],
                    ),
                    assign(
                        "_raw",
                        self_call("split", vec![self_call("nextBits", vec![int(64)])]),
                    ),
                ],
            ),
            ret(self_call(
                "modulus",
                vec![var("_raw"), var("_count")],
            )),
        ])
}

/// Builds the private small unsigned 64-bit range helper used by the gamma sections.
fn range64_small_method() -> MethodBuilder {
    method("range64Small")
        .private()
        .param("umax", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            assign(
                "_parts",
                self_call(
                    "range64",
                    vec![e_array(vec![
                        bin(var("umax"), BinOp::BitAnd, e_int(0xFFFFFFFF)),
                        bin(
                            bin(var("umax"), BinOp::ShiftRight, int(32)),
                            BinOp::BitAnd,
                            e_int(0xFFFFFFFF),
                        ),
                    ])],
                ),
            ),
            ret(self_call("join", vec![var("_parts")])),
        ])
}

/// Builds the private `typeName()` message helper.
fn type_name_method() -> MethodBuilder {
    method("typeName")
        .private()
        .static_()
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            assign("_type", call("gettype", vec![var("value")])),
            if_chain(
                bin(var("_type"), BinOp::StrictEq, e_str("double")),
                vec![ret(e_str("float"))],
                vec![
                    (bin(var("_type"), BinOp::StrictEq, e_str("integer")), vec![ret(e_str("int"))]),
                    (bin(var("_type"), BinOp::StrictEq, e_str("boolean")), vec![ret(e_str("bool"))]),
                    (bin(var("_type"), BinOp::StrictEq, e_str("NULL")), vec![ret(e_str("null"))]),
                ],
                Some(vec![ret(var("_type"))]),
            ),
        ])
}

/// Builds the private logical right shift used by `nextInt`/`nextFloat`.
fn logical_shift_right_method() -> MethodBuilder {
    method("logicalShiftRight")
        .private()
        .static_()
        .param("value", TypeExpr::Int)
        .param("shift", TypeExpr::Int)
        .returns(TypeExpr::Int)
        .body(vec![
            assign("_parts", self_call("split", vec![var("value")])),
            assign(
                "_low",
                bin(
                    bin(
                        bin(
                            limb(var("_parts"), 0),
                            BinOp::ShiftRight,
                            var("shift"),
                        ),
                        BinOp::BitOr,
                        bin(
                            limb(var("_parts"), 1),
                            BinOp::ShiftLeft,
                            bin(int(32), BinOp::Sub, var("shift")),
                        ),
                    ),
                    BinOp::BitAnd,
                    e_int(0xFFFFFFFF),
                ),
            ),
            assign(
                "_high",
                bin(
                    limb(var("_parts"), 1),
                    BinOp::ShiftRight,
                    var("shift"),
                ),
            ),
            ret(self_call(
                "join",
                vec![e_array(vec![var("_low"), var("_high")])],
            )),
        ])
}

/// Builds the private limb splitter.
fn split_method() -> MethodBuilder {
    method("split")
        .private()
        .static_()
        .param("value", TypeExpr::Int)
        .returns(int_array())
        .returning(e_array(vec![
            bin(var("value"), BinOp::BitAnd, e_int(0xFFFFFFFF)),
            bin(
                bin(var("value"), BinOp::ShiftRight, int(32)),
                BinOp::BitAnd,
                e_int(0xFFFFFFFF),
            ),
        ]))
}

/// Builds the private limb joiner.
fn join_method() -> MethodBuilder {
    method("join")
        .private()
        .static_()
        .param("parts", int_array())
        .returns(TypeExpr::Int)
        .returning(bin(
            bin(index(var("parts"), int(1)), BinOp::ShiftLeft, int(32)),
            BinOp::BitOr,
            index(var("parts"), int(0)),
        ))
}

/// Builds the private limb adder.
fn add_method() -> MethodBuilder {
    method("add")
        .private()
        .static_()
        .param("left", int_array())
        .param("right", int_array())
        .returns(int_array())
        .body(vec![
            assign(
                "_low",
                bin(
                    limb(var("left"), 0),
                    BinOp::Add,
                    limb(var("right"), 0),
                ),
            ),
            assign(
                "_carry",
                cast(
                    CastType::Int,
                    e_ternary(
                        bin(var("_low"), BinOp::Gt, e_int(0xFFFFFFFF)),
                        int(1),
                        int(0),
                    ),
                ),
            ),
            s_assign("_low", bin(var("_low"), BinOp::BitAnd, e_int(0xFFFFFFFF))),
            assign(
                "_high",
                bin(
                    bin(
                        bin(
                            limb(var("left"), 1),
                            BinOp::Add,
                            limb(var("right"), 1),
                        ),
                        BinOp::Add,
                        var("_carry"),
                    ),
                    BinOp::BitAnd,
                    e_int(0xFFFFFFFF),
                ),
            ),
            ret(e_array(vec![var("_low"), var("_high")])),
        ])
}

/// Builds the private limb subtractor.
fn subtract_method() -> MethodBuilder {
    method("subtract")
        .private()
        .static_()
        .param("left", int_array())
        .param("right", int_array())
        .returns(int_array())
        .body(vec![
            assign(
                "_low",
                bin(
                    limb(var("left"), 0),
                    BinOp::Sub,
                    limb(var("right"), 0),
                ),
            ),
            assign(
                "_borrow",
                cast(
                    CastType::Int,
                    e_ternary(
                        bin(var("_low"), BinOp::Lt, int(0)),
                        int(1),
                        int(0),
                    ),
                ),
            ),
            s_assign("_low", bin(var("_low"), BinOp::BitAnd, e_int(0xFFFFFFFF))),
            assign(
                "_high",
                bin(
                    bin(
                        bin(
                            limb(var("left"), 1),
                            BinOp::Sub,
                            limb(var("right"), 1),
                        ),
                        BinOp::Sub,
                        var("_borrow"),
                    ),
                    BinOp::BitAnd,
                    e_int(0xFFFFFFFF),
                ),
            ),
            ret(e_array(vec![var("_low"), var("_high")])),
        ])
}

/// Builds the private limb comparator.
fn compare_method() -> MethodBuilder {
    method("compare")
        .private()
        .static_()
        .param("left", int_array())
        .param("right", int_array())
        .returns(TypeExpr::Int)
        .body(vec![
            if_then(
                bin(
                    limb(var("left"), 1),
                    BinOp::StrictNotEq,
                    limb(var("right"), 1),
                ),
                vec![ret(e_ternary(
                    bin(
                        limb(var("left"), 1),
                        BinOp::Lt,
                        limb(var("right"), 1),
                    ),
                    e_neg(int(1)),
                    int(1),
                ))],
            ),
            if_then(
                bin(
                    limb(var("left"), 0),
                    BinOp::StrictNotEq,
                    limb(var("right"), 0),
                ),
                vec![ret(e_ternary(
                    bin(
                        limb(var("left"), 0),
                        BinOp::Lt,
                        limb(var("right"), 0),
                    ),
                    e_neg(int(1)),
                    int(1),
                ))],
            ),
            ret(int(0)),
        ])
}

/// Builds the private unsigned 64-bit modulus by binary long division.
fn modulus_method() -> MethodBuilder {
    method("modulus")
        .private()
        .static_()
        .param("value", int_array())
        .param("divisor", int_array())
        .returns(int_array())
        .body(vec![
            assign("_low", int(0)),
            assign("_high", int(0)),
            s_for(
                Some(assign("_bit", int(63))),
                Some(bin(var("_bit"), BinOp::GtEq, int(0))),
                Some(s_assign("_bit", bin(var("_bit"), BinOp::Sub, int(1)))),
                vec![
                    assign("_carry", bin(var("_low"), BinOp::ShiftRight, int(31))),
                    assign(
                        "_next",
                        cast(
                            CastType::Int,
                            e_ternary(
                                bin(var("_bit"), BinOp::GtEq, int(32)),
                                bin(
                                    bin(
                                        limb(var("value"), 1),
                                        BinOp::ShiftRight,
                                        bin(var("_bit"), BinOp::Sub, int(32)),
                                    ),
                                    BinOp::BitAnd,
                                    int(1),
                                ),
                                bin(
                                    bin(
                                        limb(var("value"), 0),
                                        BinOp::ShiftRight,
                                        var("_bit"),
                                    ),
                                    BinOp::BitAnd,
                                    int(1),
                                ),
                            ),
                        ),
                    ),
                    s_assign(
                        "_low",
                        bin(
                            bin(
                                bin(var("_low"), BinOp::ShiftLeft, int(1)),
                                BinOp::BitAnd,
                                e_int(0xFFFFFFFF),
                            ),
                            BinOp::BitOr,
                            var("_next"),
                        ),
                    ),
                    s_assign(
                        "_high",
                        bin(
                            bin(
                                bin(var("_high"), BinOp::ShiftLeft, int(1)),
                                BinOp::BitAnd,
                                e_int(0xFFFFFFFF),
                            ),
                            BinOp::BitOr,
                            var("_carry"),
                        ),
                    ),
                    if_then(
                        bin(
                            self_call(
                                "compare",
                                vec![
                                    e_array(vec![var("_low"), var("_high")]),
                                    var("divisor"),
                                ],
                            ),
                            BinOp::GtEq,
                            int(0),
                        ),
                        vec![
                            assign(
                                "_reduced",
                                self_call(
                                    "subtract",
                                    vec![
                                        e_array(vec![var("_low"), var("_high")]),
                                        var("divisor"),
                                    ],
                                ),
                            ),
                            s_assign("_low", limb(var("_reduced"), 0)),
                            s_assign("_high", limb(var("_reduced"), 1)),
                        ],
                    ),
                ],
            ),
            ret(e_array(vec![var("_low"), var("_high")])),
        ])
}

/// Builds the private `gammaMax()` helper.
fn gamma_max_method() -> MethodBuilder {
    method("gammaMax")
        .private()
        .param("left", TypeExpr::Float)
        .param("right", TypeExpr::Float)
        .returns(TypeExpr::Float)
        .body(vec![if_chain(
            bin(
                call("abs", vec![var("left")]),
                BinOp::Gt,
                call("abs", vec![var("right")]),
            ),
            vec![ret(bin(
                bridge("elephc_random_next_up", vec![var("left")]),
                BinOp::Sub,
                var("left"),
            ))],
            vec![],
            Some(vec![ret(bin(
                var("right"),
                BinOp::Sub,
                bridge("elephc_random_next_down", vec![var("right")]),
            ))]),
        )])
}

/// Builds the private `ceilint()` helper.
fn ceilint_method() -> MethodBuilder {
    method("ceilint")
        .private()
        .param("min", TypeExpr::Float)
        .param("max", TypeExpr::Float)
        .param("gamma", TypeExpr::Float)
        .returns(TypeExpr::Int)
        .body(vec![
            assign(
                "_steps",
                bin(
                    bin(var("max"), BinOp::Div, var("gamma")),
                    BinOp::Sub,
                    bin(var("min"), BinOp::Div, var("gamma")),
                ),
            ),
            if_chain(
                bin(
                    call("abs", vec![var("min")]),
                    BinOp::LtEq,
                    call("abs", vec![var("max")]),
                ),
                vec![assign(
                    "_correction",
                    bin(
                        bin(e_neg(var("min")), BinOp::Div, var("gamma")),
                        BinOp::Sub,
                        bin(
                            var("_steps"),
                            BinOp::Sub,
                            bin(var("max"), BinOp::Div, var("gamma")),
                        ),
                    ),
                )],
                vec![],
                Some(vec![assign(
                    "_correction",
                    bin(
                        bin(var("max"), BinOp::Div, var("gamma")),
                        BinOp::Sub,
                        bin(
                            var("_steps"),
                            BinOp::Add,
                            bin(var("min"), BinOp::Div, var("gamma")),
                        ),
                    ),
                )]),
            ),
            assign("_ceil", call("ceil", vec![var("_steps")])),
            ret(e_ternary(
                bin(var("_steps"), BinOp::StrictNotEq, var("_ceil")),
                cast(CastType::Int, var("_ceil")),
                bin(
                    cast(CastType::Int, var("_ceil")),
                    BinOp::Add,
                    e_ternary(
                        bin(var("_correction"), BinOp::Gt, float(0.0)),
                        int(1),
                        int(0),
                    ),
                ),
            )),
        ])
}

/// Builds `gammaClosedOpen()`.
fn gamma_closed_open_method() -> MethodBuilder {
    method("gammaClosedOpen")
        .private()
        .param("min", TypeExpr::Float)
        .param("max", TypeExpr::Float)
        .returns(TypeExpr::Float)
        .body(vec![
            assign("_gamma", method_call(e_this(), "gammaMax", vec![var("min"), var("max")])),
            assign(
                "_hi",
                method_call(
                    e_this(),
                    "ceilint",
                    vec![var("min"), var("max"), var("_gamma")],
                ),
            ),
            if_then(
                bin(
                    bin(var("max"), BinOp::LtEq, var("min")),
                    BinOp::Or,
                    bin(var("_hi"), BinOp::Lt, int(1)),
                ),
                vec![ret(e_const("NAN"))],
            ),
            assign(
                "_k",
                bin(
                    method_call(
                        e_this(),
                        "range64Small",
                        vec![bin(var("_hi"), BinOp::Sub, int(1))],
                    ),
                    BinOp::Add,
                    int(1),
                ),
            ),
            if_chain(
                bin(
                    call("abs", vec![var("min")]),
                    BinOp::LtEq,
                    call("abs", vec![var("max")]),
                ),
                vec![
                    if_then(
                        bin(var("_k"), BinOp::StrictEq, var("_hi")),
                        vec![ret(var("min"))],
                    ),
                    ret(gamma_value("max", true, var("_k"))),
                ],
                vec![],
                Some(vec![
                    assign("_k", bin(var("_k"), BinOp::Sub, int(1))),
                    ret(gamma_value("min", false, var("_k"))),
                ]),
            ),
        ])
}

/// Builds `gammaClosedClosed()`.
fn gamma_closed_closed_method() -> MethodBuilder {
    method("gammaClosedClosed")
        .private()
        .param("min", TypeExpr::Float)
        .param("max", TypeExpr::Float)
        .returns(TypeExpr::Float)
        .body(vec![
            assign("_gamma", method_call(e_this(), "gammaMax", vec![var("min"), var("max")])),
            assign(
                "_hi",
                method_call(
                    e_this(),
                    "ceilint",
                    vec![var("min"), var("max"), var("_gamma")],
                ),
            ),
            if_then(
                bin(var("max"), BinOp::Lt, var("min")),
                vec![ret(e_const("NAN"))],
            ),
            assign(
                "_k",
                method_call(e_this(), "range64Small", vec![var("_hi")]),
            ),
            if_chain(
                bin(
                    call("abs", vec![var("min")]),
                    BinOp::LtEq,
                    call("abs", vec![var("max")]),
                ),
                vec![
                    if_then(
                        bin(var("_k"), BinOp::StrictEq, var("_hi")),
                        vec![ret(var("min"))],
                    ),
                    ret(gamma_value("max", true, var("_k"))),
                ],
                vec![],
                Some(vec![
                    if_then(
                        bin(var("_k"), BinOp::StrictEq, var("_hi")),
                        vec![ret(var("max"))],
                    ),
                    ret(gamma_value("min", false, var("_k"))),
                ]),
            ),
        ])
}

/// Builds `gammaOpenClosed()`.
fn gamma_open_closed_method() -> MethodBuilder {
    method("gammaOpenClosed")
        .private()
        .param("min", TypeExpr::Float)
        .param("max", TypeExpr::Float)
        .returns(TypeExpr::Float)
        .body(vec![
            assign("_gamma", method_call(e_this(), "gammaMax", vec![var("min"), var("max")])),
            assign(
                "_hi",
                method_call(
                    e_this(),
                    "ceilint",
                    vec![var("min"), var("max"), var("_gamma")],
                ),
            ),
            if_then(
                bin(
                    bin(var("max"), BinOp::LtEq, var("min")),
                    BinOp::Or,
                    bin(var("_hi"), BinOp::Lt, int(1)),
                ),
                vec![ret(e_const("NAN"))],
            ),
            assign(
                "_k",
                method_call(
                    e_this(),
                    "range64Small",
                    vec![bin(var("_hi"), BinOp::Sub, int(1))],
                ),
            ),
            if_chain(
                bin(
                    call("abs", vec![var("min")]),
                    BinOp::LtEq,
                    call("abs", vec![var("max")]),
                ),
                vec![ret(gamma_value("max", true, var("_k")))],
                vec![],
                Some(vec![
                    if_then(
                        bin(
                            var("_k"),
                            BinOp::StrictEq,
                            bin(var("_hi"), BinOp::Sub, int(1)),
                        ),
                        vec![ret(var("max"))],
                    ),
                    assign("_k", bin(var("_k"), BinOp::Add, int(1))),
                    ret(gamma_value("min", false, var("_k"))),
                ]),
            ),
        ])
}

/// Builds `gammaOpenOpen()`.
fn gamma_open_open_method() -> MethodBuilder {
    method("gammaOpenOpen")
        .private()
        .param("min", TypeExpr::Float)
        .param("max", TypeExpr::Float)
        .returns(TypeExpr::Float)
        .body(vec![
            assign("_gamma", method_call(e_this(), "gammaMax", vec![var("min"), var("max")])),
            assign(
                "_hi",
                method_call(
                    e_this(),
                    "ceilint",
                    vec![var("min"), var("max"), var("_gamma")],
                ),
            ),
            if_then(
                bin(
                    bin(var("max"), BinOp::LtEq, var("min")),
                    BinOp::Or,
                    bin(var("_hi"), BinOp::Lt, int(2)),
                ),
                vec![ret(e_const("NAN"))],
            ),
            assign(
                "_k",
                bin(
                    method_call(
                        e_this(),
                        "range64Small",
                        vec![bin(var("_hi"), BinOp::Sub, int(2))],
                    ),
                    BinOp::Add,
                    int(1),
                ),
            ),
            if_chain(
                bin(
                    call("abs", vec![var("min")]),
                    BinOp::LtEq,
                    call("abs", vec![var("max")]),
                ),
                vec![ret(gamma_value("max", true, var("_k")))],
                vec![],
                Some(vec![ret(gamma_value("min", false, var("_k")))]),
            ),
        ])
}

/// Builds the shared gamma-section reconstruction expression.
fn gamma_value(anchor: &str, positive_anchor: bool, k: Expr) -> Expr {
    let high = cast(CastType::Float, bin(k.clone(), BinOp::ShiftRight, int(2)));
    let low = cast(CastType::Float, bin(k, BinOp::BitAnd, int(3)));
    let anchor_term = bin(var(anchor), BinOp::Mul, float(0.25));
    let (inner, operator) = if positive_anchor {
        (
            bin(
                anchor_term,
                BinOp::Sub,
                bin(high, BinOp::Mul, var("_gamma")),
            ),
            BinOp::Sub,
        )
    } else {
        (
            bin(
                anchor_term,
                BinOp::Add,
                bin(high, BinOp::Mul, var("_gamma")),
            ),
            BinOp::Add,
        )
    };
    bin(
        bin(float(4.0), BinOp::Mul, inner),
        operator,
        bin(low, BinOp::Mul, var("_gamma")),
    )
}

/// Builds the shared per-draw statements used by every consumer.
fn draw_statements() -> Vec<Stmt> {
    vec![
        assign(
            "_value",
            method_call(e_this_prop("engine"), "generate", vec![]),
        ),
        assign("_size", len(var("_value"))),
        if_then(
            bin(var("_size"), BinOp::StrictEq, int(0)),
            vec![throw_empty_engine()],
        ),
        if_then(
            bin(var("_size"), BinOp::Gt, int(8)),
            vec![assign("_size", int(8))],
        ),
    ]
}

/// Accumulates `$value` bytes into `$_raw`, using `bound` as the loop bound.
fn accumulate_raw(bound: &str, base: Expr) -> Stmt {
    s_for(
        Some(assign("_i", int(0))),
        Some(bin(var("_i"), BinOp::Lt, var(bound))),
        Some(add_assign("_i", int(1))),
        vec![s_assign(
            "_raw",
            bin(
                var("_raw"),
                BinOp::BitOr,
                bin(
                    ord(index(var("_value"), var("_i"))),
                    BinOp::ShiftLeft,
                    bin(bin(base.clone(), BinOp::Add, var("_i")), BinOp::Mul, int(8)),
                ),
            ),
        )],
    )
}

/// Builds an in-place swap of two indexes in a local array.
fn swap(array: &str, left: Expr, right: Expr) -> Stmt {
    s_if(
        e_bool(true),
        vec![
            assign("_temporary", index(var(array), left.clone())),
            s_array_assign(array, left, index(var(array), right.clone())),
            s_array_assign(array, right, var("_temporary")),
        ],
        Vec::new(),
        None,
    )
}

/// Builds a `ValueError` construction with php-src's message.
fn value_error(message: &str) -> Expr {
    unique_new("\\ValueError", vec![e_str(message)])
}

/// Builds an `Exception` for invalid serialization data.
fn invalid_serialization(message: &str) -> Expr {
    unique_new("\\Exception", vec![e_str(message)])
}
