//! Purpose:
//! Declares `Random\Engine\Mt19937`, `Random\Engine\PcgOneseq128XslRr64`,
//! `Random\Engine\Xoshiro256StarStar` and `Random\Engine\Secure`.
//!
//! Called from:
//! - `crate::random_prelude::random_declarations`.
//!
//! Key details:
//! - Each engine owns one opaque bridge handle. `__serialize`, `__unserialize`,
//!   `__debugInfo`, `__clone` and `__destruct` are the whole lifecycle; there is no second
//!   owner and no second free path.
//! - The three seeded engines share `generate`, `__serialize`, `__debugInfo`, `__clone`,
//!   `__destruct`, `stateArray`, `replaceHandle`, `typeName` and `bytes`; only the
//!   constructor, `__unserialize` and the jump methods differ.
//! - Constructor seed validation produces PHP's runtime `TypeError`/`ValueError` messages
//!   rather than relying on a static signature, because php-src checks `string|int|null` at
//!   run time and a compiled call cannot reproduce that with a narrower declaration.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

use super::{
    add_assign, append, assign, bin, bridge, call, chr, count, expr_stmt, handle, if_chain,
    if_then, index, int, len, low_byte, method_call, not, push, ret, self_call, set_property,
    throw_, unique_new, var,
};

/// Builds all four engine classes in the `Random\Engine` namespace.
pub(super) fn declarations() -> Program {
    vec![s_namespace(
        "Random\\Engine",
        vec![mt19937(), pcg(), xoshiro(), secure()],
    )]
}

/// Builds `Random\Engine\Mt19937`.
fn mt19937() -> Stmt {
    let constructor = method("__construct")
        .param_default("seed", t_mixed(), e_null())
        .param_default("mode", TypeExpr::Int, int(0))
        .body(vec![
            if_then(
                bin(
                    bin(var("seed"), BinOp::StrictNotEq, e_null()),
                    BinOp::And,
                    not(call("is_int", vec![var("seed")])),
                ),
                vec![throw_(type_error_expr(concat_expr(
                    "Random\\Engine\\Mt19937::__construct(): Argument #1 ($seed) must be of type ?int, ",
                    self_call("typeName", vec![var("seed")]),
                    " given",
                )))],
            ),
            if_then(
                bin(
                    bin(var("mode"), BinOp::StrictNotEq, int(0)),
                    BinOp::And,
                    bin(var("mode"), BinOp::StrictNotEq, int(1)),
                ),
                vec![throw_(value_error(
                    "Random\\Engine\\Mt19937::__construct(): Argument #2 ($mode) must be either MT_RAND_MT19937 or MT_RAND_PHP",
                ))],
            ),
            if_then(
                bin(
                    bin(var("mode"), BinOp::StrictEq, int(1)),
                    BinOp::And,
                    not(e_self_static_prop("__elephc_restoring")),
                ),
                vec![expr_stmt(call(
                    "trigger_error",
                    vec![
                        e_str("The MT_RAND_PHP variant of Mt19937 is deprecated"),
                        e_const("E_USER_DEPRECATED"),
                    ],
                ))],
            ),
            expr_stmt(self_call(
                "replaceHandle",
                vec![
                    int(0),
                    e_ternary(
                        bin(var("seed"), BinOp::StrictEq, e_null()),
                        int(0),
                        int(1),
                    ),
                    e_null_coalesce(var("seed"), int(0)),
                    e_str(""),
                    var("mode"),
                ],
            )),
            set_property("__elephc_mode", var("mode")),
        ]);

    let base = seeded_class("Mt19937", 0, e_int(4), 8, true, constructor, mt_unserialize());
    base.readonly_prop("__elephc_mode", TypeExpr::Int)
        .private_static_prop("__elephc_restoring", TypeExpr::Bool, Some(e_bool(false)))
        .build()
}

/// Builds `Random\Engine\PcgOneseq128XslRr64`.
fn pcg() -> Stmt {
    let constructor = method("__construct")
        .param_default("seed", t_mixed(), e_null())
        .body(vec![
            seed_type_check("PcgOneseq128XslRr64", "string|int|null"),
            assign("_seed_kind", int(0)),
            assign("_seed_int", int(0)),
            assign("_seed_hex", e_str("")),
            if_chain(
                call("is_int", vec![var("seed")]),
                vec![
                    assign("_seed_kind", int(1)),
                    assign("_seed_int", var("seed")),
                ],
                vec![(
                    call("is_string", vec![var("seed")]),
                    vec![
                        if_then(
                            bin(len(var("seed")), BinOp::StrictNotEq, int(16)),
                            vec![throw_(value_error(
                                "Random\\Engine\\PcgOneseq128XslRr64::__construct(): Argument #1 ($seed) must be a 16 byte (128 bit) string",
                            ))],
                        ),
                        assign("_seed_kind", int(2)),
                        assign("_seed_hex", call("bin2hex", vec![var("seed")])),
                    ],
                )],
                None,
            ),
            expr_stmt(self_call(
                "replaceHandle",
                vec![
                    int(1),
                    var("_seed_kind"),
                    var("_seed_int"),
                    var("_seed_hex"),
                    int(0),
                ],
            )),
        ]);

    let base = seeded_class("PcgOneseq128XslRr64", 1, e_int(8), 16, false, constructor, pcg_unserialize());
    base.method(
        method("jump")
            .param("advance", TypeExpr::Int)
            .returns(TypeExpr::Void)
            .body(vec![
                if_then(
                    bin(var("advance"), BinOp::Lt, int(0)),
                    vec![throw_(value_error(
                        "Random\\Engine\\PcgOneseq128XslRr64::jump(): Argument #1 ($advance) must be greater than or equal to 0",
                    ))],
                ),
                expr_stmt(bridge(
                    "elephc_random_engine_jump",
                    vec![handle(), var("advance")],
                )),
            ]),
    )
    .build()
}

/// Builds `Random\Engine\Xoshiro256StarStar`.
fn xoshiro() -> Stmt {
    let constructor = method("__construct")
        .param_default("seed", t_mixed(), e_null())
        .body(vec![
            seed_type_check("Xoshiro256StarStar", "string|int|null"),
            assign("_seed_kind", int(0)),
            assign("_seed_int", int(0)),
            assign("_seed_hex", e_str("")),
            if_chain(
                call("is_int", vec![var("seed")]),
                vec![
                    assign("_seed_kind", int(1)),
                    assign("_seed_int", var("seed")),
                ],
                vec![(
                    call("is_string", vec![var("seed")]),
                    vec![
                        if_then(
                            bin(len(var("seed")), BinOp::StrictNotEq, int(32)),
                            vec![throw_(value_error(
                                "Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must be a 32 byte (256 bit) string",
                            ))],
                        ),
                        if_then(
                            bin(
                                call("bin2hex", vec![var("seed")]),
                                BinOp::StrictEq,
                                e_str("0000000000000000000000000000000000000000000000000000000000000000"),
                            ),
                            vec![throw_(value_error(
                                "Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must not consist entirely of NUL bytes",
                            ))],
                        ),
                        assign("_seed_kind", int(2)),
                        assign("_seed_hex", call("bin2hex", vec![var("seed")])),
                    ],
                )],
                None,
            ),
            expr_stmt(self_call(
                "replaceHandle",
                vec![
                    int(2),
                    var("_seed_kind"),
                    var("_seed_int"),
                    var("_seed_hex"),
                    int(0),
                ],
            )),
        ]);

    let base = seeded_class("Xoshiro256StarStar", 2, e_int(8), 16, false, constructor, xoshiro_unserialize());
    base.method(
        method("jump")
            .returns(TypeExpr::Void)
            .body(vec![expr_stmt(bridge(
                "elephc_random_engine_jump_short",
                vec![handle()],
            ))]),
    )
    .method(
        method("jumpLong")
            .returns(TypeExpr::Void)
            .body(vec![expr_stmt(bridge(
                "elephc_random_engine_jump_long",
                vec![handle()],
            ))]),
    )
    .build()
}

/// Builds `Random\Engine\Secure`.
fn secure() -> Stmt {
    class("Secure")
        .final_()
        .implements("\\Random\\CryptoSafeEngine")
        .method(method("generate").returns(TypeExpr::Str).body(vec![
            assign(
                "_bytes",
                call("hex2bin", vec![bridge("elephc_random_bytes_hex", vec![int(8)])]),
            ),
            if_then(
                bin(
                    bridge("elephc_random_csprng_failed", vec![]),
                    BinOp::StrictNotEq,
                    int(0),
                ),
                vec![throw_(e_new_fq(
                    "\\Random\\RandomException",
                    vec![e_str("Failed to generate random bytes")],
                ))],
            ),
            ret(e_ternary(
                bin(var("_bytes"), BinOp::StrictEq, e_bool(false)),
                e_str(""),
                var("_bytes"),
            )),
        ]))
        .method(method("__serialize").returns(t_array()).body(vec![throw_(
            unique_new(
                "\\Exception",
                vec![e_str("Serialization of 'Random\\Engine\\Secure' is not allowed")],
            ),
        )]))
        .method(method("__clone").body(vec![throw_(unique_new(
            "\\Error",
            vec![e_str(
                "Trying to clone an uncloneable object of class Random\\Engine\\Secure",
            )],
        ))]))
        .method(method("__debugInfo").returns(t_array()).returning(e_array(vec![])))
        .build()
}

/// Adds the shared engine methods to `name` and returns the finished declaration.
fn seeded_class(
    name: &str,
    kind: i64,
    size: Expr,
    chunk: i64,
    with_count: bool,
    constructor: MethodBuilder,
    unserialize: MethodBuilder,
) -> ClassBuilder {
    class(name)
        .final_()
        .implements("\\Random\\Engine")
        .private_prop("__elephc_handle", TypeExpr::Int, Some(int(0)))
        .method(constructor)
        .method(
            method("generate")
                .returns(TypeExpr::Str)
                .body(vec![ret(self_call(
                    "bytes",
                    vec![
                        bridge("elephc_random_engine_generate", vec![handle()]),
                        size,
                    ],
                ))]),
        )
        .method(
            method("__serialize").returns(t_array()).body(vec![ret(
                e_array_assoc(vec![
                    (int(0), e_array(vec![])),
                    (int(1), method_call(e_this(), "stateArray", vec![])),
                ]),
            )]),
        )
        .method(unserialize)
        .method(
            method("__debugInfo").returns(t_array()).body(vec![ret(e_array_assoc(vec![(
                e_str("__states"),
                method_call(e_this(), "stateArray", vec![]),
            )]))]),
        )
        .method(method("__clone").body(vec![set_property(
            "__elephc_handle",
            bridge("elephc_random_engine_clone", vec![handle()]),
        )]))
        .method(method("__destruct").body(vec![if_then(
            bin(handle(), BinOp::StrictNotEq, int(0)),
            vec![
                expr_stmt(bridge("elephc_random_engine_free", vec![handle()])),
                set_property("__elephc_handle", int(0)),
            ],
        )]))
        .method(state_array_method(chunk, with_count))
        .method(replace_handle_method(kind))
        .method(type_name_method())
        .method(bytes_method())
}

/// Builds the shared `stateArray()` method.
fn state_array_method(chunk: i64, with_count: bool) -> MethodBuilder {
    let mut body = vec![
        assign(
            "_hex",
            bridge("elephc_random_engine_state_hex", vec![handle()]),
        ),
        assign("_states", e_array(vec![])),
        assign("_length", len(var("_hex"))),
        s_for(
            Some(assign("_i", int(0))),
            Some(bin(var("_i"), BinOp::Lt, var("_length"))),
            Some(add_assign("_i", int(chunk))),
            vec![push(
                "_states",
                call(
                    "substr",
                    vec![var("_hex"), var("_i"), int(chunk)],
                ),
            )],
        ),
    ];
    if with_count {
        body.push(push(
            "_states",
            bridge("elephc_random_engine_count", vec![handle()]),
        ));
        body.push(push(
            "_states",
            bridge("elephc_random_engine_mode", vec![handle()]),
        ));
    }
    body.push(ret(var("_states")));
    method("stateArray").private().returns(t_array()).body(body)
}

/// Builds the shared `replaceHandle()` method.
fn replace_handle_method(kind: i64) -> MethodBuilder {
    method("replaceHandle")
        .private()
        .param("kind", TypeExpr::Int)
        .param("seedKind", TypeExpr::Int)
        .param("seedInt", TypeExpr::Int)
        .param("seedHex", TypeExpr::Str)
        .param("mode", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            if_then(
                bin(handle(), BinOp::StrictNotEq, int(0)),
                vec![
                    expr_stmt(bridge("elephc_random_engine_free", vec![handle()])),
                    set_property("__elephc_handle", int(0)),
                ],
            ),
            assign(
                "_handle",
                bridge(
                    "elephc_random_engine_new",
                    vec![
                        int(kind),
                        var("seedKind"),
                        var("seedInt"),
                        var("seedHex"),
                        var("mode"),
                    ],
                ),
            ),
            if_then(
                bin(var("_handle"), BinOp::StrictEq, int(0)),
                vec![throw_(unique_new(
                    "\\Random\\RandomException",
                    vec![e_str("Failed to generate a random seed")],
                ))],
            ),
            set_property("__elephc_handle", var("_handle")),
        ])
}

/// Builds the shared `typeName()` message helper.
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
                    (
                        bin(var("_type"), BinOp::StrictEq, e_str("integer")),
                        vec![ret(e_str("int"))],
                    ),
                    (
                        bin(var("_type"), BinOp::StrictEq, e_str("boolean")),
                        vec![ret(e_str("bool"))],
                    ),
                    (
                        bin(var("_type"), BinOp::StrictEq, e_str("NULL")),
                        vec![ret(e_str("null"))],
                    ),
                ],
                Some(vec![ret(var("_type"))]),
            ),
        ])
}

/// Builds the shared little-endian `bytes()` helper.
fn bytes_method() -> MethodBuilder {
    method("bytes")
        .private()
        .static_()
        .param("value", TypeExpr::Int)
        .param("size", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            assign("_result", e_str("")),
            s_for(
                Some(assign("_i", int(0))),
                Some(bin(var("_i"), BinOp::Lt, var("size"))),
                Some(add_assign("_i", int(1))),
                vec![append(
                    "_result",
                    chr(low_byte(bin(
                        var("value"),
                        BinOp::ShiftRight,
                        bin(var("_i"), BinOp::Mul, int(8)),
                    ))),
                )],
            ),
            ret(var("_result")),
        ])
}

/// Builds `Random\Engine\Mt19937::__unserialize()`.
fn mt_unserialize() -> MethodBuilder {
    let message = "Invalid serialization data for Random\\Engine\\Mt19937 object";
    let mut loop_body = vec![if_then(
        bin(
            bin(
                not(call("isset", vec![index(var("_states"), var("_i"))])),
                BinOp::Or,
                not(call(
                    "is_string",
                    vec![index(var("_states"), var("_i"))],
                )),
            ),
            BinOp::Or,
            bin(
                len(index(var("_states"), var("_i"))),
                BinOp::StrictNotEq,
                int(8),
            ),
        ),
        vec![throw_(invalid_serialization(message))],
    )];
    loop_body.push(append(
        "_state_hex",
        index(var("_states"), var("_i")),
    ));

    method("__unserialize")
        .param("data", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            if_then(
                bin(
                    bin(count(var("data")), BinOp::StrictNotEq, int(2)),
                    BinOp::Or,
                    bin(
                        bin(
                            not(call("isset", vec![index(var("data"), int(0))])),
                            BinOp::Or,
                            not(call("is_array", vec![index(var("data"), int(0))])),
                        ),
                        BinOp::Or,
                        bin(
                            bin(
                                not(call("isset", vec![index(var("data"), int(1))])),
                                BinOp::Or,
                                not(call("is_array", vec![index(var("data"), int(1))])),
                            ),
                            BinOp::Or,
                            e_bool(false),
                        ),
                    ),
                ),
                vec![throw_(invalid_serialization(message))],
            ),
            assign("_states", index(var("data"), int(1))),
            if_then(
                bin(count(var("_states")), BinOp::StrictNotEq, int(626)),
                vec![throw_(invalid_serialization(message))],
            ),
            assign("_state_hex", e_str("")),
            s_for(
                Some(assign("_i", int(0))),
                Some(bin(var("_i"), BinOp::Lt, int(624))),
                Some(add_assign("_i", int(1))),
                loop_body,
            ),
            assign("_count", index(var("_states"), int(624))),
            assign("_mode", index(var("_states"), int(625))),
            if_then(
                bin(
                    bin(not(call("is_int", vec![var("_count")])), BinOp::Or,
                        bin(var("_count"), BinOp::Lt, int(0))),
                    BinOp::Or,
                    bin(var("_count"), BinOp::Gt, int(624)),
                ),
                vec![throw_(invalid_serialization(message))],
            ),
            if_then(
                bin(
                    not(call("is_int", vec![var("_mode")])),
                    BinOp::Or,
                    bin(
                        bin(var("_mode"), BinOp::StrictNotEq, int(0)),
                        BinOp::And,
                        bin(var("_mode"), BinOp::StrictNotEq, int(1)),
                    ),
                ),
                vec![throw_(invalid_serialization(message))],
            ),
            s_try(
                vec![
                    s_self_static_prop_assign("__elephc_restoring", e_bool(true)),
                    expr_stmt(method_call(
                        e_this(),
                        "__construct",
                        vec![e_null(), var("_mode")],
                    )),
                ],
                vec![],
                Some(vec![s_self_static_prop_assign(
                    "__elephc_restoring",
                    e_bool(false),
                )]),
            ),
            if_then(
                bin(
                    bridge(
                        "elephc_random_engine_restore_hex",
                        vec![handle(), var("_state_hex"), var("_count"), var("_mode")],
                    ),
                    BinOp::StrictNotEq,
                    int(1),
                ),
                vec![throw_(invalid_serialization(message))],
            ),
        ])
}

/// Builds `Random\Engine\PcgOneseq128XslRr64::__unserialize()`.
fn pcg_unserialize() -> MethodBuilder {
    let message = "Invalid serialization data for Random\\Engine\\PcgOneseq128XslRr64 object";
    let mut loop_body = vec![if_then(
        bin(
            bin(
                not(call("isset", vec![index(var("_states"), var("_i"))])),
                BinOp::Or,
                not(call(
                    "is_string",
                    vec![index(var("_states"), var("_i"))],
                )),
            ),
            BinOp::Or,
            bin(
                len(index(var("_states"), var("_i"))),
                BinOp::StrictNotEq,
                int(16),
            ),
        ),
        vec![throw_(invalid_serialization(message))],
    )];
    loop_body.push(append(
        "_state_hex",
        index(var("_states"), var("_i")),
    ));

    method("__unserialize")
        .param("data", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            state_envelope_check(message),
            assign("_states", index(var("data"), int(1))),
            if_then(
                bin(count(var("_states")), BinOp::StrictNotEq, int(2)),
                vec![throw_(invalid_serialization(message))],
            ),
            assign("_state_hex", e_str("")),
            s_for(
                Some(assign("_i", int(0))),
                Some(bin(var("_i"), BinOp::Lt, int(2))),
                Some(add_assign("_i", int(1))),
                loop_body,
            ),
            expr_stmt(self_call(
                "replaceHandle",
                vec![int(1), int(1), int(0), e_str(""), int(0)],
            )),
            if_then(
                bin(
                    bridge(
                        "elephc_random_engine_restore_hex",
                        vec![handle(), var("_state_hex"), int(0), int(0)],
                    ),
                    BinOp::StrictNotEq,
                    int(1),
                ),
                vec![throw_(invalid_serialization(message))],
            ),
        ])
}

/// Builds `Random\Engine\Xoshiro256StarStar::__unserialize()`.
fn xoshiro_unserialize() -> MethodBuilder {
    let message =
        "Invalid serialization data for Random\\Engine\\Xoshiro256StarStar object";
    let mut loop_body = vec![if_then(
        bin(
            bin(
                not(call("isset", vec![index(var("_states"), var("_i"))])),
                BinOp::Or,
                not(call(
                    "is_string",
                    vec![index(var("_states"), var("_i"))],
                )),
            ),
            BinOp::Or,
            bin(
                len(index(var("_states"), var("_i"))),
                BinOp::StrictNotEq,
                int(16),
            ),
        ),
        vec![throw_(invalid_serialization(message))],
    )];
    loop_body.push(append(
        "_state_hex",
        index(var("_states"), var("_i")),
    ));

    method("__unserialize")
        .param("data", t_array())
        .returns(TypeExpr::Void)
        .body(vec![
            state_envelope_check(message),
            assign("_states", index(var("data"), int(1))),
            if_then(
                bin(count(var("_states")), BinOp::StrictNotEq, int(4)),
                vec![throw_(invalid_serialization(message))],
            ),
            assign("_state_hex", e_str("")),
            s_for(
                Some(assign("_i", int(0))),
                Some(bin(var("_i"), BinOp::Lt, int(4))),
                Some(add_assign("_i", int(1))),
                loop_body,
            ),
            expr_stmt(self_call(
                "replaceHandle",
                vec![int(2), int(1), int(0), e_str(""), int(0)],
            )),
            if_then(
                bin(
                    bridge(
                        "elephc_random_engine_restore_hex",
                        vec![handle(), var("_state_hex"), int(0), int(0)],
                    ),
                    BinOp::StrictNotEq,
                    int(1),
                ),
                vec![throw_(invalid_serialization(message))],
            ),
        ])
}

/// Builds the shared two-element `[members, states]` envelope validation.
fn state_envelope_check(message: &str) -> Stmt {
    if_then(
        bin(
            bin(count(var("data")), BinOp::StrictNotEq, int(2)),
            BinOp::Or,
            bin(
                bin(
                    not(call("isset", vec![index(var("data"), int(0))])),
                    BinOp::Or,
                    not(call("is_array", vec![index(var("data"), int(0))])),
                ),
                BinOp::Or,
                bin(
                    bin(
                        not(call("isset", vec![index(var("data"), int(1))])),
                        BinOp::Or,
                        not(call("is_array", vec![index(var("data"), int(1))])),
                    ),
                    BinOp::Or,
                    e_bool(false),
                ),
            ),
        ),
        vec![throw_(invalid_serialization(message))],
    )
}

/// Builds the shared `string|int|null` type check.
fn seed_type_check(class: &str, types: &str) -> Stmt {
    if_then(
        bin(
            bin(var("seed"), BinOp::StrictNotEq, e_null()),
            BinOp::And,
            bin(
                not(call("is_int", vec![var("seed")])),
                BinOp::And,
                not(call("is_string", vec![var("seed")])),
            ),
        ),
        vec![throw_(type_error_expr(concat_expr(
            &format!(
                "Random\\Engine\\{class}::__construct(): Argument #1 ($seed) must be of type {types}, "
            ),
            self_call("typeName", vec![var("seed")]),
            " given",
        )))],
    )
}

/// Builds a `TypeError` construction from a computed message.
fn type_error_expr(message: Expr) -> Expr {
    unique_new("\\TypeError", vec![message])
}

/// Builds a `ValueError` construction with php-src's message.
fn value_error(message: &str) -> Expr {
    unique_new("\\ValueError", vec![e_str(message)])
}

/// Builds an `Exception` for invalid serialization data.
fn invalid_serialization(message: &str) -> Expr {
    unique_new("\\Exception", vec![e_str(message)])
}

/// Builds `prefix . middle . suffix`.
fn concat_expr(prefix: &str, middle: Expr, suffix: &str) -> Expr {
    e_concat_all(vec![e_str(prefix), middle, e_str(suffix)])
}
