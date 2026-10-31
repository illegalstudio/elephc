//! Purpose:
//! Builds the synthetic filter value dispatcher: one scalar filter call, recursive array
//! walking, regexp/email/URL validation and the full-special-chars helper.
//!
//! Called from:
//! - `crate::filter_prelude::filter_declarations`.
//!
//! Key details:
//! - Scalar values are stringified exactly once, before the filter runs, like php_zval_filter.
//! - Array recursion preserves keys and never wraps nested elements in FORCE_ARRAY.
//! - `default` is applied after any failure sentinel, matching php_zval_filter's handle_default.
//! - The PCRE patterns and URL checks are copied from php-src 8.5.10 logical_filters.c.

use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_array_assoc, e_binop, e_bool, e_call, e_cast, e_concat_all, e_const,
    e_index, e_int, e_not, e_null, e_str, e_ternary, e_var, function, s_array_assign,
    s_assign, s_break, s_continue, s_expr, s_foreach, s_if, s_return, s_switch, s_throw, s_while, t_array, t_bool,
    t_int, t_mixed,
};

/// Returns the scalar dispatcher and its validation helpers.
pub(super) fn declarations() -> Program {
    vec![
        bridge_decl(),
        validation_failed_decl(),
        apply_decl(),
        walk_decl(),
        full_special_chars_decl(),
        userinfo_decl(),
        url_decl(),
        ipv6_decl(),
        regexp_decl(),
        email_decl(),
        mac_decl(),
        domain_decl(),
        ip_decl(),
    ]
}

/// Builds one call to the PHP bridge dispatcher.
#[allow(clippy::too_many_arguments)]
fn bridge(
    filter: Expr,
    flags: Expr,
    value: Expr,
    option: Expr,
    option2: Expr,
    range_flags: Expr,
    int_min: Expr,
    int_max: Expr,
    float_min: Expr,
    float_max: Expr,
) -> Expr {
    e_call(
        "__elephc_filter_bridge",
        vec![
            filter, flags, value, option, option2, range_flags, int_min, int_max,
            float_min, float_max,
        ],
    )
}

/// Builds a bridge call with every option unset.
fn bridge_plain(filter: Expr) -> Expr {
    bridge(
        filter,
        e_var("flags"),
        e_var("string"),
        e_str(""),
        e_str(""),
        e_int(0),
        e_int(0),
        e_int(0),
        crate::synthetic_class::e_float(0.0),
        crate::synthetic_class::e_float(0.0),
    )
}

/// Builds `__elephc_filter_bridge(...): mixed`, dispatching to the typed bridge entry point
/// and decoding its outcome.
fn bridge_decl() -> Stmt {
    let mut cases = Vec::new();
    cases.push((vec![e_int(257)], vec![s_assign("kind", e_call(
        "elephc_filter_apply_int",
        vec![e_var("flags"), e_call("bin2hex", vec![e_var("value")]), e_var("int_min"), e_var("int_max"), e_var("range_flags")],
    )), s_break(1)]));
    cases.push((vec![e_int(258)], vec![s_assign("kind", e_call(
        "elephc_filter_apply_bool",
        vec![e_call("bin2hex", vec![e_var("value")])],
    )), s_break(1)]));
    cases.push((vec![e_int(259)], vec![s_assign("kind", e_call(
        "elephc_filter_apply_float",
        vec![
            e_var("flags"), e_call("bin2hex", vec![e_var("value")]),
            e_call("bin2hex", vec![e_var("option")]), e_call("bin2hex", vec![e_var("option2")]),
            e_var("range_flags"), e_var("float_min"), e_var("float_max"),
        ],
    )), s_break(1)]));
    cases.push((vec![e_int(277)], vec![s_assign("kind", e_call(
        "elephc_filter_apply_domain",
        vec![e_var("flags"), e_call("bin2hex", vec![e_var("value")])],
    )), s_break(1)]));
    cases.push((vec![e_int(275)], vec![s_assign("kind", e_call(
        "elephc_filter_apply_ip",
        vec![e_var("flags"), e_call("bin2hex", vec![e_var("value")])],
    )), s_break(1)]));
    cases.push((vec![e_int(276)], vec![s_assign("kind", e_call(
        "elephc_filter_apply_mac",
        vec![e_var("flags"), e_call("bin2hex", vec![e_var("value")]),
             e_call("bin2hex", vec![e_var("option")])],
    )), s_break(1)]));
    function("__elephc_filter_bridge")
        .param("filter", t_int())
        .param("flags", t_int())
        .param("value", TypeExpr::Str)
        .param("option", TypeExpr::Str)
        .param("option2", TypeExpr::Str)
        .param("range_flags", t_int())
        .param("int_min", t_int())
        .param("int_max", t_int())
        .param("float_min", TypeExpr::Float)
        .param("float_max", TypeExpr::Float)
        .returns(t_mixed())
        .body(vec![
            s_switch(
                e_var("filter"),
                cases,
                Some(vec![s_assign("kind", e_call(
                    "elephc_filter_apply_sanitize",
                    vec![e_var("filter"), e_var("flags"), e_call("bin2hex", vec![e_var("value")])],
                ))]),
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                vec![s_return(e_call("hex2bin", vec![e_call("elephc_filter_result_hex", vec![])]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                vec![s_return(e_call("elephc_filter_result_int", vec![]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)),
                vec![s_return(e_call("elephc_filter_result_float", vec![]))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)),
                vec![s_return(e_binop(
                    e_call("elephc_filter_result_bool", vec![]),
                    BinOp::StrictNotEq,
                    e_int(0),
                ))],
                vec![],
                None,
            ),
            s_return(e_null()),
        ])
        .build()
}

/// Builds `__elephc_filter_validation_failed(int $flags, int $filter, string $value): mixed`.
fn validation_failed_decl() -> Stmt {
    function("__elephc_filter_validation_failed")
        .param("flags", t_int())
        .param("filter", t_int())
        .param("value", TypeExpr::Str)
        .returns(t_mixed())
        .returning(e_call(
            "__elephc_filter_failure",
            vec![
                e_var("flags"),
                e_concat_all(vec![
                    e_str("filter validation failed: filter "),
                    e_call("__elephc_filter_name", vec![e_var("filter")]),
                    e_str(" not satisfied by '"),
                    e_var("value"),
                    e_str("'"),
                ]),
            ],
        ))
        .build()
}

/// Builds `__elephc_filter_apply(...)`, one scalar value's filter operation.
fn apply_decl() -> Stmt {
    function("__elephc_filter_apply")
        .param("value", t_mixed())
        .param("filter", t_int())
        .param("flags", t_int())
        .param("options", t_array())
        .param("callback", t_mixed())
        .param("caller", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_call("__elephc_filter_filter_exists", vec![e_var("filter")])),
                vec![s_assign("filter", e_const("FILTER_DEFAULT"))],
                vec![],
                None,
            ),
            crate::synthetic_class::s_typed_assign(t_array(), "inner", e_array(vec![])),
            s_if(
                e_binop(
                    e_call("is_array", vec![e_var("options")]),
                    BinOp::And,
                    e_call("array_key_exists", vec![e_str("options"), e_var("options")]),
                ),
                vec![s_if(
                    e_binop(
                        e_call("is_array", vec![e_index(e_var("options"), e_str("options"))]),
                        BinOp::And,
                        e_binop(e_var("filter"), BinOp::StrictNotEq, e_const("FILTER_CALLBACK")),
                    ),
                    vec![s_assign("inner", e_cast(CastType::Array, e_index(e_var("options"), e_str("options"))))],
                    vec![],
                    None,
                )],
                vec![],
                None,
            ),
            s_assign("result", e_bool(false)),
            s_if(
                e_binop(
                    e_call("is_object", vec![e_var("value")]),
                    BinOp::And,
                    e_not(crate::synthetic_class::e_instance_of(e_var("value"), "\\Stringable")),
                ),
                vec![s_assign(
                    "result",
                    e_call(
                        "__elephc_filter_failure",
                        vec![
                            e_var("flags"),
                            e_concat_all(vec![
                                e_str("filter validation failed: object of type "),
                                e_call("get_class", vec![e_var("value")]),
                                e_str(" has no __toString() method"),
                            ]),
                        ],
                    ),
                )],
                vec![],
                Some(vec![
                    s_assign("string", e_cast(CastType::String, e_var("value"))),
                    s_if(
                        e_binop(e_var("filter"), BinOp::StrictEq, e_const("FILTER_CALLBACK")),
                        vec![
                            s_if(
                                e_not(e_call("is_callable", vec![e_var("callback")])),
                                vec![s_throw(crate::synthetic_class::e_new_fq(
                                    "TypeError",
                                    vec![e_concat_all(vec![
                                        e_var("caller"),
                                        e_str("(): Option must be a valid callback"),
                                    ])],
                                ))],
                                vec![],
                                None,
                            ),
                            s_assign("result", e_call("call_user_func", vec![e_var("callback"), e_cast(CastType::String, e_var("string"))])),
                        ],
                        vec![],
                        Some(vec![
                            filter_switch(),
                            s_if(
                                e_binop(
                                    e_binop(e_var("result"), BinOp::StrictEq, e_bool(false)),
                                    BinOp::And,
                                    e_binop(
                                        e_call("elephc_filter_result_kind", vec![]),
                                        BinOp::StrictEq,
                                        e_int(0),
                                    ),
                                ),
                                vec![s_assign("result", e_call(
                                    "__elephc_filter_validation_failed",
                                    vec![e_var("flags"), e_var("filter"), e_var("string")],
                                ))],
                                vec![],
                                None,
                            ),
                        ]),
                    ),
                ]),
            ),
            s_if(
                e_binop(
                    e_call("is_array", vec![e_var("inner")]),
                    BinOp::And,
                    e_call("array_key_exists", vec![e_str("default"), e_var("inner")]),
                ),
                vec![s_if(
                    e_ternary(
                        e_binop(
                            e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_NULL_ON_FAILURE")),
                            BinOp::StrictNotEq,
                            e_int(0),
                        ),
                        e_binop(e_var("result"), BinOp::StrictEq, e_null()),
                        e_binop(e_var("result"), BinOp::StrictEq, e_bool(false)),
                    ),
                    vec![s_assign("result", e_index(e_var("inner"), e_str("default")))],
                    vec![],
                    None,
                )],
                vec![],
                None,
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// Builds the filter-id switch with option extraction in each arm.
fn filter_switch() -> Stmt {
    let mut cases = Vec::new();
    let mut int_body = int_arm();
    int_body.push(s_break(1));
    cases.push((vec![e_int(257)], int_body));
    cases.push((vec![e_int(258)], vec![s_assign("result", bridge_plain(e_int(258))), s_break(1)]));
    let mut float_body = float_arm();
    float_body.push(s_break(1));
    cases.push((vec![e_int(259)], float_body));
    cases.push((vec![e_int(272)], vec![s_assign("result", e_call(
        "__elephc_filter_validate_regexp",
        vec![e_var("string"), e_var("flags"), e_cast(CastType::Array, e_var("inner")), e_var("caller")],
    )), s_break(1)]));
    cases.push((vec![e_int(273)], vec![s_assign("result", e_ternary(
        e_call("__elephc_filter_validate_url", vec![
            e_var("string"), e_var("flags"), e_cast(CastType::Array, e_var("inner")), e_var("caller"),
        ]),
        e_var("string"),
        e_call("__elephc_filter_validation_failed", vec![e_var("flags"), e_int(273), e_var("string")]),
    )), s_break(1)]));
    cases.push((vec![e_int(274)], vec![s_assign("result", e_call(
        "__elephc_filter_validate_email",
        vec![e_var("string"), e_var("flags"), e_cast(CastType::Array, e_var("inner")), e_var("caller")],
    )), s_break(1)]));
    cases.push((vec![e_int(275)], vec![s_assign("result", e_call(
        "__elephc_filter_validate_ip",
        vec![e_var("string"), e_var("flags"), e_var("caller")],
    )), s_break(1)]));
    cases.push((vec![e_int(276)], vec![s_assign("result", e_call(
        "__elephc_filter_validate_mac",
        vec![e_var("string"), e_var("flags"), e_cast(CastType::Array, e_var("inner")), e_var("caller")],
    )), s_break(1)]));
    cases.push((vec![e_int(277)], vec![s_assign("result", e_call(
        "__elephc_filter_validate_domain",
        vec![e_var("string"), e_var("flags"), e_var("caller")],
    )), s_break(1)]));
    for id in [513, 514, 515, 516, 517, 518, 519, 520] {
        cases.push((vec![e_int(id)], vec![s_assign("result", bridge_plain(e_int(id))), s_break(1)]));
    }
    cases.push((
        vec![e_int(522)],
        vec![
            s_assign(
                "result",
                e_call("__elephc_filter_full_special_chars", vec![e_var("string"), e_var("flags")]),
            ),
            s_break(1),
        ],
    ));
    cases.push((vec![e_int(523)], vec![s_assign("result", e_call("addslashes", vec![e_var("string")])), s_break(1)]));
    crate::synthetic_class::s_switch(
        e_var("filter"),
        cases,
        Some(vec![s_assign("result", bridge_plain(e_const("FILTER_DEFAULT")))]),
    )
}

/// Builds the FILTER_VALIDATE_INT arm, reading min_range and max_range.
fn int_arm() -> Vec<Stmt> {
    vec![
        s_assign("int_min", e_int(0)),
        s_assign("int_min_set", e_int(0)),
        s_assign("int_max", e_int(0)),
        s_assign("int_max_set", e_int(0)),
        s_if(
            e_call("array_key_exists", vec![e_str("min_range"), e_var("inner")]),
            vec![
                s_assign("int_min", e_cast(CastType::Int, e_index(e_var("inner"), e_str("min_range")))),
                s_assign("int_min_set", e_int(1)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("array_key_exists", vec![e_str("max_range"), e_var("inner")]),
            vec![
                s_assign("int_max", e_cast(CastType::Int, e_index(e_var("inner"), e_str("max_range")))),
                s_assign("int_max_set", e_int(1)),
            ],
            vec![],
            None,
        ),
        s_assign("range_flags", e_binop(e_var("int_min_set"), BinOp::BitOr, e_binop(
            e_var("int_max_set"),
            BinOp::ShiftLeft,
            e_int(1),
        ))),
        s_assign("result", bridge(
            e_int(257),
            e_var("flags"),
            e_var("string"),
            e_str(""),
            e_str(""),
            e_var("range_flags"),
            e_var("int_min"),
            e_var("int_max"),
            crate::synthetic_class::e_float(0.0),
            crate::synthetic_class::e_float(0.0),
        )),
    ]
}

/// Builds the FILTER_VALIDATE_FLOAT arm, reading decimal/thousand and range options.
fn float_arm() -> Vec<Stmt> {
    vec![
        s_assign("float_min", crate::synthetic_class::e_float(0.0)),
        s_assign("float_min_set", e_int(0)),
        s_assign("float_max", crate::synthetic_class::e_float(0.0)),
        s_assign("float_max_set", e_int(0)),
        s_assign("decimal", e_str("")),
        s_assign("thousand", e_str("")),
        s_if(
            e_call("array_key_exists", vec![e_str("min_range"), e_var("inner")]),
            vec![
                s_assign("float_min", e_cast(CastType::Float, e_index(e_var("inner"), e_str("min_range")))),
                s_assign("float_min_set", e_int(1)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("array_key_exists", vec![e_str("max_range"), e_var("inner")]),
            vec![
                s_assign("float_max", e_cast(CastType::Float, e_index(e_var("inner"), e_str("max_range")))),
                s_assign("float_max_set", e_int(1)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("array_key_exists", vec![e_str("decimal"), e_var("inner")]),
                BinOp::And,
                e_call("is_string", vec![e_index(e_var("inner"), e_str("decimal"))]),
            ),
            vec![
                s_assign("decimal", e_index(e_var("inner"), e_str("decimal"))),
                s_if(
                    e_binop(e_call("strlen", vec![e_var("decimal")]), BinOp::StrictNotEq, e_int(1)),
                    vec![s_throw(crate::synthetic_class::e_new_fq(
                        "ValueError",
                        vec![e_concat_all(vec![
                            e_var("caller"),
                            e_str("(): \"decimal\" option must be one character long"),
                        ])],
                    ))],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("array_key_exists", vec![e_str("thousand"), e_var("inner")]),
                BinOp::And,
                e_call("is_string", vec![e_index(e_var("inner"), e_str("thousand"))]),
            ),
            vec![
                s_assign("thousand", e_index(e_var("inner"), e_str("thousand"))),
                s_if(
                    e_binop(e_call("strlen", vec![e_var("thousand")]), BinOp::Lt, e_int(1)),
                    vec![s_throw(crate::synthetic_class::e_new_fq(
                        "ValueError",
                        vec![e_concat_all(vec![
                            e_var("caller"),
                            e_str("(): \"thousand\" option must not be empty"),
                        ])],
                    ))],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_assign("range_flags", e_binop(e_var("float_min_set"), BinOp::BitOr, e_binop(
            e_var("float_max_set"),
            BinOp::ShiftLeft,
            e_int(1),
        ))),
        s_assign("result", bridge(
            e_int(259),
            e_var("flags"),
            e_var("string"),
            e_var("decimal"),
            e_var("thousand"),
            e_var("range_flags"),
            e_int(0),
            e_int(0),
            e_var("float_min"),
            e_var("float_max"),
        )),
    ]
}

/// Builds `__elephc_filter_walk(...)`, php_filter_call's array recursion.
fn walk_decl() -> Stmt {
    function("__elephc_filter_walk")
        .param("value", t_mixed())
        .param("filter", t_int())
        .param("flags", t_int())
        .param("options", t_array())
        .param("callback", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("top", t_bool())
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_if(
                        e_var("top"),
                        vec![s_if(
                            e_binop(
                                e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_REQUIRE_SCALAR")),
                                BinOp::StrictNotEq,
                                e_int(0),
                            ),
                            vec![s_return(e_call(
                                "__elephc_filter_failure",
                                vec![
                                    e_var("flags"),
                                    e_str("filter validation failed: not a scalar value (got an array)"),
                                ],
                            ))],
                            vec![],
                            None,
                        )],
                        vec![],
                        None,
                    ),
                    s_assign("out", e_array(vec![])),
                    s_foreach(
                        e_var("value"),
                        Some("key"),
                        "element",
                        vec![
                            s_if(
                                e_call("is_array", vec![e_var("element")]),
                                vec![s_assign(
                                    "nested",
                                    e_call("__elephc_filter_walk", vec![
                                        e_var("element"),
                                        e_var("filter"),
                                        e_var("flags"),
                                        e_cast(CastType::Array, e_var("options")),
                                        e_var("callback"),
                                        e_var("caller"),
                                        e_bool(false),
                                    ]),
                                )],
                                vec![],
                                Some(vec![s_assign(
                                    "nested",
                                    e_call("__elephc_filter_apply", vec![
                                        e_var("element"),
                                        e_var("filter"),
                                        e_var("flags"),
                                        e_var("options"),
                                        e_var("callback"),
                                        e_var("caller"),
                                    ]),
                                )]),
                            ),
                            s_array_assign("out", e_var("key"), e_var("nested")),
                        ],
                    ),
                    s_return(e_var("out")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("top"),
                vec![s_if(
                    e_binop(
                        e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_REQUIRE_ARRAY")),
                        BinOp::StrictNotEq,
                        e_int(0),
                    ),
                    vec![s_return(e_call(
                        "__elephc_filter_failure",
                        vec![
                            e_var("flags"),
                            e_concat_all(vec![
                                e_str("filter validation failed: not an array (got "),
                                e_call("__elephc_filter_type_name", vec![e_var("value")]),
                                e_str(")"),
                            ]),
                        ],
                    ))],
                    vec![],
                    None,
                )],
                vec![],
                None,
            ),
            s_assign("result", e_call("__elephc_filter_apply", vec![
                e_var("value"),
                e_var("filter"),
                e_var("flags"),
                e_var("options"),
                e_var("callback"),
                e_var("caller"),
            ])),
            s_if(
                e_binop(
                    e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_FORCE_ARRAY")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
                vec![s_return(e_array(vec![e_var("result")]))],
                vec![],
                None,
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// Builds `__elephc_filter_validate_regexp(string, int, array, string): mixed`.
fn regexp_decl() -> Stmt {
    function("__elephc_filter_validate_regexp")
        .param("value", TypeExpr::Str)
        .param("flags", t_int())
        .param("options", t_array())
        .param("caller", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_assign("regexp_set", e_int(0)),
            s_assign("regexp", e_str("")),
            s_if(
                e_binop(
                    e_call("array_key_exists", vec![e_str("regexp"), e_var("options")]),
                    BinOp::And,
                    e_call("is_string", vec![e_index(e_var("options"), e_str("regexp"))]),
                ),
                vec![
                    s_assign("regexp", e_cast(CastType::String, e_index(e_var("options"), e_str("regexp")))),
                    s_assign("regexp_set", e_int(1)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("regexp_set"), BinOp::StrictEq, e_int(0)),
                vec![s_throw(crate::synthetic_class::e_new_fq(
                    "ValueError",
                    vec![e_concat_all(vec![
                        e_var("caller"),
                        e_str("(): \"regexp\" option is missing"),
                    ])],
                ))],
                vec![],
                None,
            ),
            s_expr(e_call(
                "elephc_filter_apply_regexp",
                vec![e_call("bin2hex", vec![e_var("value")]), e_call("bin2hex", vec![e_var("regexp")])],
            )),
            s_if(
                e_binop(e_call("elephc_filter_result_bool", vec![]), BinOp::StrictEq, e_int(1)),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            s_return(e_call(
                "__elephc_filter_validation_failed",
                vec![e_var("flags"), e_int(272), e_var("value")],
            )),
        ])
        .build()
}

/// Builds `__elephc_filter_validate_email(string, int, array, string): mixed`.
fn email_decl() -> Stmt {
    function("__elephc_filter_validate_email")
        .param("value", TypeExpr::Str)
        .param("flags", t_int())
        .param("options", t_array())
        .param("caller", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_expr(e_call(
                "elephc_filter_apply_email",
                vec![e_var("flags"), e_call("bin2hex", vec![e_var("value")])],
            )),
            s_if(
                e_binop(e_call("elephc_filter_result_bool", vec![]), BinOp::StrictEq, e_int(1)),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            s_return(e_call(
                "__elephc_filter_validation_failed",
                vec![e_var("flags"), e_int(274), e_var("value")],
            )),
        ])
        .build()
}

/// Builds `__elephc_filter_validate_mac(string, int, array, string): mixed`.
fn mac_decl() -> Stmt {
    function("__elephc_filter_validate_mac")
        .param("value", TypeExpr::Str)
        .param("flags", t_int())
        .param("options", t_array())
        .param("caller", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_assign("separator", e_str("")),
            s_if(
                e_binop(
                    e_call("array_key_exists", vec![e_str("separator"), e_var("options")]),
                    BinOp::And,
                    e_call("is_string", vec![e_index(e_var("options"), e_str("separator"))]),
                ),
                vec![
                    s_assign("separator", e_index(e_var("options"), e_str("separator"))),
                    s_if(
                        e_binop(e_call("strlen", vec![e_var("separator")]), BinOp::StrictNotEq, e_int(1)),
                        vec![s_throw(crate::synthetic_class::e_new_fq(
                            "ValueError",
                            vec![e_concat_all(vec![
                                e_var("caller"),
                                e_str("(): \"separator\" option must be one character long"),
                            ])],
                        ))],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("result", bridge(
                e_int(276),
                e_var("flags"),
                e_var("value"),
                e_var("separator"),
                e_str(""),
                e_int(0),
                e_int(0),
                e_int(0),
                crate::synthetic_class::e_float(0.0),
                crate::synthetic_class::e_float(0.0),
            )),
            s_if(
                e_binop(e_var("result"), BinOp::StrictEq, e_bool(false)),
                vec![s_return(e_call(
                    "__elephc_filter_validation_failed",
                    vec![e_var("flags"), e_int(276), e_var("value")],
                ))],
                vec![],
                None,
            ),
            s_return(e_var("value")),
        ])
        .build()
}

/// Builds the full-special-chars helper, preserving existing entities like php-src.
fn full_special_chars_decl() -> Stmt {
    function("__elephc_filter_full_special_chars")
        .param("value", TypeExpr::Str)
        .param("flags", t_int())
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("quotes", e_const("ENT_QUOTES")),
            s_if(
                e_binop(
                    e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_FLAG_NO_ENCODE_QUOTES")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
                vec![s_assign("quotes", e_const("ENT_NOQUOTES"))],
                vec![],
                None,
            ),
            s_assign("probe", e_call("htmlentities", vec![
                e_var("value"), e_var("quotes"), e_str("UTF-8"),
            ])),
            s_if(
                e_binop(
                    e_binop(e_var("value"), BinOp::StrictNotEq, e_str("")),
                    BinOp::And,
                    e_binop(e_var("probe"), BinOp::StrictEq, e_str("")),
                ),
                vec![s_return(e_str(""))],
                vec![],
                None,
            ),
            s_assign("out", e_str("")),
            s_assign("i", e_int(0)),
            s_assign("len", e_call("strlen", vec![e_var("value")])),
            s_while(
                e_binop(e_var("i"), BinOp::Lt, e_var("len")),
                vec![s_if(
                    e_binop(e_index(e_var("value"), e_var("i")), BinOp::StrictEq, e_str("&")),
                    vec![
                        s_assign("matched", e_bool(false)),
                        s_assign("semi", e_call("strpos", vec![e_var("value"), e_str(";"), e_binop(e_var("i"), BinOp::Add, e_int(1))])),
                        s_if(
                            e_binop(
                                e_binop(
                                    e_var("semi"),
                                    BinOp::StrictNotEq,
                                    e_bool(false),
                                ),
                                BinOp::And,
                                e_binop(
                                    e_binop(e_var("semi"), BinOp::Sub, e_var("i")),
                                    BinOp::LtEq,
                                    e_int(32),
                                ),
                            ),
                            vec![
                                s_assign("entity", e_call("substr", vec![
                                    e_var("value"),
                                    e_var("i"),
                                    e_binop(e_binop(e_var("semi"), BinOp::Sub, e_var("i")), BinOp::Add, e_int(1)),
                                ])),
                                s_assign("decoded", e_call("html_entity_decode", vec![e_var("entity")])),
                                s_if(
                                    e_binop(e_var("decoded"), BinOp::StrictNotEq, e_var("entity")),
                                    vec![
                                        s_assign("out", e_concat_all(vec![e_var("out"), e_var("entity")])),
                                        s_assign("i", e_binop(e_var("semi"), BinOp::Add, e_int(1))),
                                        s_assign("matched", e_bool(true)),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_not(e_var("matched")),
                            vec![
                                s_assign("out", e_concat_all(vec![e_var("out"), e_str("&amp;")])),
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    Some(vec![
                        s_assign("next", e_call("strpos", vec![e_var("value"), e_str("&"), e_var("i")])),
                        s_if(
                            e_binop(e_var("next"), BinOp::StrictEq, e_bool(false)),
                            vec![s_assign("next", e_var("len"))],
                            vec![],
                            None,
                        ),
                        s_assign("segment", e_call("substr", vec![
                            e_var("value"),
                            e_var("i"),
                            e_binop(e_var("next"), BinOp::Sub, e_var("i")),
                        ])),
                        s_assign("out", e_concat_all(vec![
                            e_var("out"),
                            e_call("htmlentities", vec![e_var("segment"), e_var("quotes"), e_str("UTF-8")]),
                        ])),
                        s_assign("i", e_var("next")),
                    ]),
                )],
            ),
            s_return(e_var("out")),
        ])
        .build()
}

/// Builds `__elephc_filter_is_userinfo_valid(string $value): bool`.
fn userinfo_decl() -> Stmt {
    function("__elephc_filter_is_userinfo_valid")
        .param("value", TypeExpr::Str)
        .returns(t_bool())
        .body(vec![
            s_assign("len", e_call("strlen", vec![e_var("value")])),
            s_assign("i", e_int(0)),
            s_while(
                e_binop(e_var("i"), BinOp::Lt, e_var("len")),
                vec![
                    s_assign("char", e_index(e_var("value"), e_var("i"))),
                    s_if(
                        e_binop(
                            e_binop(
                                e_call("ctype_alnum", vec![e_var("char")]),
                                BinOp::Or,
                                e_binop(
                                    e_call("strpos", vec![e_str("-._~!$&'()*+,;=:"), e_var("char")]),
                                    BinOp::StrictNotEq,
                                    e_bool(false),
                                ),
                            ),
                            BinOp::Or,
                            e_binop(
                                e_binop(e_var("char"), BinOp::StrictEq, e_str("%")),
                                BinOp::And,
                                e_binop(
                                    e_binop(
                                        e_var("i"),
                                        BinOp::LtEq,
                                        e_binop(e_var("len"), BinOp::Sub, e_int(3)),
                                    ),
                                    BinOp::And,
                                    e_binop(
                                        e_call("ctype_digit", vec![e_index(e_var("value"), e_binop(e_var("i"), BinOp::Add, e_int(1)))]),
                                        BinOp::And,
                                        e_binop(
                                            e_call("strpos", vec![e_str("0123456789abcdefABCDEF"), e_index(e_var("value"), e_binop(e_var("i"), BinOp::Add, e_int(2)))]),
                                            BinOp::StrictNotEq,
                                            e_bool(false),
                                        ),
                                    ),
                                ),
                            ),
                        ),
                        vec![s_if(
                            e_binop(e_var("char"), BinOp::StrictEq, e_str("%")),
                            vec![s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(3)))],
                            vec![],
                            Some(vec![s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1)))]),
                        )],
                        vec![],
                        Some(vec![s_return(e_bool(false))]),
                    ),
                ],
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Builds `__elephc_filter_is_ipv6(string $value): bool`.
fn ipv6_decl() -> Stmt {
    function("__elephc_filter_is_ipv6")
        .param("value", TypeExpr::Str)
        .returns(t_bool())
        .returning(e_binop(
            e_call("__elephc_filter_apply", vec![
                e_var("value"),
                e_int(275),
                e_const("FILTER_FLAG_IPV6"),
                e_array(vec![]),
                e_null(),
                e_str("filter_var"),
            ]),
            BinOp::StrictNotEq,
            e_bool(false),
        ))
        .build()
}

/// Builds `__elephc_filter_validate_url(string $value, int $flags, array $options,
/// string $caller): bool`.
fn url_decl() -> Stmt {
    let failure = e_bool(false);
    function("__elephc_filter_validate_url")
        .param("value", TypeExpr::Str)
        .param("flags", t_int())
        .param("options", t_array())
        .param("caller", TypeExpr::Str)
        .returns(t_bool())
        .body(vec![
            s_assign("sanitized", e_call("__elephc_filter_apply", vec![
                e_var("value"),
                e_const("FILTER_SANITIZE_URL"),
                e_int(0),
                e_array(vec![]),
                e_null(),
                e_var("caller"),
            ])),
            s_if(
                e_binop(
                    e_not(e_call("is_string", vec![e_var("sanitized")])),
                    BinOp::Or,
                    e_binop(
                        e_call("strlen", vec![e_var("sanitized")]),
                        BinOp::StrictNotEq,
                        e_call("strlen", vec![e_var("value")]),
                    ),
                ),
                vec![s_return(failure.clone())],
                vec![],
                None,
            ),
            s_assign("parser_set", e_bool(false)),
            s_assign("parser", e_str("")),
            s_if(
                e_binop(
                    e_call("array_key_exists", vec![e_str("uri_parser_class"), e_var("options")]),
                    BinOp::And,
                    e_call("is_string", vec![e_index(e_var("options"), e_str("uri_parser_class"))]),
                ),
                vec![
                    s_assign("parser", e_index(e_var("options"), e_str("uri_parser_class"))),
                    s_assign("parser_set", e_bool(true)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_var("parser_set"),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("parser"), BinOp::StrictNotEq, e_str("Uri\\Rfc3986Uri")),
                        BinOp::And,
                        e_binop(e_var("parser"), BinOp::StrictNotEq, e_str("Uri\\WhatWgUri")),
                    ),
                ),
                vec![s_throw(crate::synthetic_class::e_new_fq(
                    "ValueError",
                    vec![e_concat_all(vec![
                        e_var("caller"),
                        e_str("(): \"uri_parser_class\" option has invalid value"),
                    ])],
                ))],
                vec![],
                None,
            ),
            s_assign("parts", e_call("parse_url", vec![e_var("value")])),
            s_if(
                e_not(e_call("is_array", vec![e_var("parts")])),
                vec![s_return(failure.clone())],
                vec![],
                None,
            ),
            s_assign("scheme", optional_index("scheme")),
            s_assign("host", optional_index("host")),
            s_assign("path", optional_index("path")),
            s_assign("query", optional_index("query")),
            s_assign("user", optional_index("user")),
            s_assign("pass", optional_index("pass")),
            s_assign("scheme_text", e_call("strtolower", vec![e_cast(CastType::String, e_var("scheme"))])),
            s_if(
                e_binop(
                    e_call("is_string", vec![e_var("scheme")]),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("scheme_text"), BinOp::StrictEq, e_str("http")),
                        BinOp::Or,
                        e_binop(e_var("scheme_text"), BinOp::StrictEq, e_str("https")),
                    ),
                ),
                vec![
                    s_if(
                        e_not(e_call("is_string", vec![e_var("host")])),
                        vec![s_return(failure.clone())],
                        vec![],
                        None,
                    ),
                    s_assign("host_text", e_cast(CastType::String, e_var("host"))),
                    s_assign("host_ok", e_bool(false)),
                    s_if(
                        e_binop(
                            e_binop(e_index(e_var("host_text"), e_int(0)), BinOp::StrictEq, e_str("[")),
                            BinOp::And,
                            e_binop(e_call("substr", vec![e_var("host_text"), e_int(-1)]), BinOp::StrictEq, e_str("]")),
                        ),
                        vec![s_assign("host_ok", e_call("__elephc_filter_is_ipv6", vec![
                            e_call("substr", vec![e_var("host_text"), e_int(1), e_binop(e_call("strlen", vec![e_var("host_text")]), BinOp::Sub, e_int(2))]),
                        ]))],
                        vec![],
                        Some(vec![s_assign("host_ok", e_binop(
                            e_call("__elephc_filter_apply", vec![
                                e_var("host_text"),
                                e_const("FILTER_VALIDATE_DOMAIN"),
                                e_const("FILTER_FLAG_HOSTNAME"),
                                e_array(vec![]),
                                e_null(),
                                e_var("caller"),
                            ]),
                            BinOp::StrictNotEq,
                            e_bool(false),
                        ))]),
                    ),
                    s_if(
                        e_not(e_var("host_ok")),
                        vec![s_return(failure.clone())],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_not(e_call("is_string", vec![e_var("scheme")])),
                    BinOp::Or,
                    e_binop(
                        e_not(e_call("is_string", vec![e_var("host")])),
                        BinOp::And,
                        e_binop(
                            e_binop(e_var("scheme_text"), BinOp::StrictNotEq, e_str("mailto")),
                            BinOp::And,
                            e_binop(
                                e_binop(e_var("scheme_text"), BinOp::StrictNotEq, e_str("news")),
                                BinOp::And,
                                e_binop(e_var("scheme_text"), BinOp::StrictNotEq, e_str("file")),
                            ),
                        ),
                    ),
                ),
                vec![s_return(failure.clone())],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_FLAG_PATH_REQUIRED")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
                vec![s_if(
                    e_not(e_call("is_string", vec![e_var("path")])),
                    vec![s_return(failure.clone())],
                    vec![],
                    None,
                )],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_FLAG_QUERY_REQUIRED")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
                vec![s_if(
                    e_not(e_call("is_string", vec![e_var("query")])),
                    vec![s_return(failure.clone())],
                    vec![],
                    None,
                )],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("is_string", vec![e_var("user")]),
                    BinOp::And,
                    e_not(e_call("__elephc_filter_is_userinfo_valid", vec![e_cast(CastType::String, e_var("user"))])),
                ),
                vec![s_return(failure.clone())],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("is_string", vec![e_var("pass")]),
                    BinOp::And,
                    e_not(e_call("__elephc_filter_is_userinfo_valid", vec![e_cast(CastType::String, e_var("pass"))])),
                ),
                vec![s_return(failure.clone())],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Builds `$parts['name'] ?? null`.
fn optional_index(name: &str) -> Expr {
    e_ternary(
        e_call("array_key_exists", vec![e_str(name), e_var("parts")]),
        e_cast(CastType::String, e_index(e_var("parts"), e_str(name))),
        e_null(),
    )
}


/// Builds a domain/IP helper: a successful bridge bool means the original value passes.
fn bool_filter_helper(name: &str, filter: i64) -> Stmt {
    function(name)
        .param("value", TypeExpr::Str)
        .param("flags", t_int())
        .param("caller", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_expr(bridge(
                e_int(filter),
                e_var("flags"),
                e_var("value"),
                e_str(""),
                e_str(""),
                e_int(0),
                e_int(0),
                e_int(0),
                crate::synthetic_class::e_float(0.0),
                crate::synthetic_class::e_float(0.0),
            )),
            s_if(
                e_binop(e_call("elephc_filter_result_bool", vec![]), BinOp::StrictEq, e_int(1)),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            s_return(e_call(
                "__elephc_filter_validation_failed",
                vec![e_var("flags"), e_int(filter), e_var("value")],
            )),
        ])
        .build()
}

/// Builds `__elephc_filter_validate_domain(...)`.
fn domain_decl() -> Stmt { bool_filter_helper("__elephc_filter_validate_domain", 277) }

/// Builds `__elephc_filter_validate_ip(...)`.
fn ip_decl() -> Stmt { bool_filter_helper("__elephc_filter_validate_ip", 275) }

/// Keeps imports used by helper builders that are currently shared through the module.
#[allow(dead_code)]
fn _markers() -> (Expr, Stmt) {
    (e_array_assoc(vec![]), s_continue(1))
}
