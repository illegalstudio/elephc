//! Purpose:
//! Builds `filter_input()`, `filter_input_array()`, `filter_has_var()`, `filter_var_array()`
//! and their shared definition-array handler.
//!
//! Called from:
//! - `crate::filter_prelude::filter_declarations`.
//!
//! Key details:
//! - Input storage is the corresponding request superglobal. In CLI builds those are seeded
//!   empty arrays, and in web builds they are populated by the request parser.
//! - A missing key follows php-src's inverted NULL_ON_FAILURE rule and THROW_ON_FAILURE
//!   message, including the `default` option lookup before the sentinel is returned.
//! - Definition arrays warn for unknown integer filters and then fall back to unsafe_raw.

use crate::parser::ast::{BinOp, CastType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_concat_all, e_const, e_index, e_int, e_not,
    e_null, e_str, e_var, function, s_array_assign, s_assign, s_continue, s_foreach, s_if,
    s_return, s_switch, s_throw, t_array, t_bool, t_int, t_mixed,
};

/// Returns the input/array function declarations.
pub(super) fn declarations() -> Program {
    vec![
        storage_function(),
        input_type_decl(),
        array_handler_decl(),
        filter_var_array_decl(),
        filter_input_array_decl(),
        filter_input_decl(),
        filter_has_var_decl(),
    ]
}

/// Builds the copy helper body for one superglobal.
fn copy_global(global: &str) -> Vec<Stmt> {
    vec![
        s_assign("copy", e_array(vec![])),
        s_foreach(e_var(global), Some("key"), "value", vec![s_array_assign(
            "copy",
            e_var("key"),
            e_var("value"),
        )]),
        s_if(
            e_binop(e_call("count", vec![e_var("copy")]), BinOp::StrictEq, e_int(0)),
            vec![s_return(e_null())],
            vec![],
            None,
        ),
        s_return(e_var("copy")),
    ]
}

/// Builds `__elephc_filter_storage(int $type): ?array`, a copy of one request superglobal.
fn storage_function() -> Stmt {
    function("__elephc_filter_storage")
        .param("type", t_int())
        .returns(crate::synthetic_class::t_nullable(t_array()))
        .body(vec![
            s_if(
                e_call("array_key_exists", vec![
                    e_var("type"),
                    crate::synthetic_class::e_static_prop("__ElephcFilterState", "raw"),
                ]),
                vec![s_return(crate::synthetic_class::e_cast(
                    crate::parser::ast::CastType::Array,
                    e_index(
                        crate::synthetic_class::e_static_prop("__ElephcFilterState", "raw"),
                        e_var("type"),
                    ),
                ))],
                vec![],
                None,
            ),
            s_switch(
            e_var("type"),
            vec![
                (vec![e_const("INPUT_POST")], copy_global("_POST")),
                (vec![e_const("INPUT_GET")], copy_global("_GET")),
                (vec![e_const("INPUT_COOKIE")], copy_global("_COOKIE")),
                (vec![e_const("INPUT_ENV")], copy_global("_ENV")),
                (vec![e_const("INPUT_SERVER")], copy_global("_SERVER")),
            ],
            Some(vec![s_return(e_null())]),
        )])
        .build()
}

/// Builds `__elephc_filter_validate_input_type(int $type, string $caller, string $param): void`.
fn input_type_decl() -> Stmt {
    let valid = e_binop(
        e_binop(
            e_binop(e_var("type"), BinOp::StrictEq, e_const("INPUT_POST")),
            BinOp::Or,
            e_binop(e_var("type"), BinOp::StrictEq, e_const("INPUT_GET")),
        ),
        BinOp::Or,
        e_binop(
            e_binop(
                e_binop(e_var("type"), BinOp::StrictEq, e_const("INPUT_COOKIE")),
                BinOp::Or,
                e_binop(e_var("type"), BinOp::StrictEq, e_const("INPUT_ENV")),
            ),
            BinOp::Or,
            e_binop(e_var("type"), BinOp::StrictEq, e_const("INPUT_SERVER")),
        ),
    );
    function("__elephc_filter_validate_input_type")
        .param("type", t_int())
        .param("caller", TypeExpr::Str)
        .param("param", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![s_if(
            e_not(valid),
            vec![s_throw(crate::synthetic_class::e_new_fq(
                "ValueError",
                vec![e_concat_all(vec![
                    e_var("caller"),
                    e_str("(): Argument #1 ($"),
                    e_var("param"),
                    e_str(") must be an INPUT_* constant"),
                ])],
            ))],
            vec![],
            None,
        )])
        .build()
}

/// Builds `__elephc_filter_array_handler(array $input, mixed $options, bool $add_empty,
/// string $caller): mixed`.
fn array_handler_decl() -> Stmt {
    let definition_loop = vec![
        s_if(
            e_not(e_call("is_string", vec![e_var("key")])),
            vec![s_throw(crate::synthetic_class::e_new_fq(
                "TypeError",
                vec![e_concat_all(vec![
                    e_var("caller"),
                    e_str("(): Argument #2 ($options) must contain only string keys"),
                ])],
            ))],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("key"), BinOp::StrictEq, e_str("")),
            vec![s_throw(crate::synthetic_class::e_new_fq(
                "ValueError",
                vec![e_concat_all(vec![
                    e_var("caller"),
                    e_str("(): Argument #2 ($options) cannot contain empty keys"),
                ])],
            ))],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("array_key_exists", vec![e_var("key"), e_var("input")])),
            vec![
                s_if(
                    e_var("add_empty"),
                    vec![s_array_assign("out", e_var("key"), e_null())],
                    vec![],
                    None,
                ),
                s_continue(1),
            ],
            vec![],
            None,
        ),
        s_assign("value", e_index(e_var("input"), e_var("key"))),
        s_if(
            e_call("is_array", vec![e_var("definition")]),
            vec![
                s_assign("state", e_array(vec![e_int(-1), e_const("FILTER_REQUIRE_SCALAR"), e_null()])),
                s_expr_call("__elephc_filter_parse_definition", vec![
                    e_cast(CastType::Array, e_var("definition")),
                    e_var("state"),
                ]),
                s_expr_call("__elephc_filter_check_flags", vec![
                    e_cast(CastType::Int, e_index(e_var("state"), e_int(1))),
                    e_var("caller"),
                    e_int(2),
                ]),
                s_assign("filtered", e_call("__elephc_filter_walk", vec![
                    e_var("value"),
                    e_cast(CastType::Int, e_index(e_var("state"), e_int(0))),
                    e_cast(CastType::Int, e_index(e_var("state"), e_int(1))),
                    e_cast(CastType::Array, e_var("definition")),
                    e_index(e_var("state"), e_int(2)),
                    e_var("caller"),
                    e_bool(true),
                ])),
            ],
            vec![],
            Some(vec![
                s_assign("filter", e_cast(CastType::Int, e_var("definition"))),
                s_if(
                    e_not(e_call("__elephc_filter_filter_exists", vec![e_var("filter")])),
                    vec![s_expr_call("trigger_error", vec![
                        e_concat_all(vec![
                            e_var("caller"),
                            e_str("(): Unknown filter with ID "),
                            e_var("filter"),
                        ]),
                        e_const("E_USER_WARNING"),
                    ])],
                    vec![],
                    None,
                ),
                s_assign("filtered", e_call("__elephc_filter_walk", vec![
                    e_var("value"),
                    e_var("filter"),
                    e_const("FILTER_REQUIRE_SCALAR"),
                    e_array(vec![]),
                    e_null(),
                    e_var("caller"),
                    e_bool(true),
                ])),
            ]),
        ),
        s_array_assign("out", e_var("key"), e_var("filtered")),
    ];
    function("__elephc_filter_array_handler")
        .param("input", t_array())
        .param("options", t_mixed())
        .param("add_empty", t_bool())
        .param("caller", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_call("is_array", vec![e_var("options")]),
                vec![
                    s_assign("out", e_array(vec![])),
                    s_foreach(e_var("options"), Some("key"), "definition", definition_loop),
                    s_return(e_var("out")),
                ],
                vec![],
                Some(vec![
                    // PHP duplicates the input array and filters each entry IN PLACE. The copy
                    // preserves PHP reference entries, so writing a filtered value through a
                    // referenced element also updates the aliased variable, as php-src does.
                    s_assign("out", e_var("input")),
                    s_foreach(
                        e_var("input"),
                        Some("input_key"),
                        "input_value",
                        vec![s_array_assign(
                            "out",
                            e_var("input_key"),
                            e_call("__elephc_filter_walk", vec![
                                e_var("input_value"),
                                e_cast(CastType::Int, e_var("options")),
                                e_const("FILTER_REQUIRE_ARRAY"),
                                e_array(vec![]),
                                e_null(),
                                e_var("caller"),
                                e_bool(false),
                            ]),
                        )],
                    ),
                    s_return(e_var("out")),
                ]),
            ),
        ])
        .build()
}

/// Builds `filter_var_array(mixed $array, mixed $options = FILTER_DEFAULT,
/// mixed $add_empty = true): mixed`.
fn filter_var_array_decl() -> Stmt {
    function("filter_var_array")
        .param("array", t_mixed())
        .param_default("options", t_mixed(), e_const("FILTER_DEFAULT"))
        .param_default("add_empty", t_mixed(), e_bool(true))
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_not(e_call("is_array", vec![e_var("array")])),
                vec![s_throw(crate::synthetic_class::e_new_fq(
                    "TypeError",
                    vec![e_concat_all(vec![
                        e_str("filter_var_array(): Argument #1 ($array) must be of type array, "),
                        e_call("__elephc_filter_type_name", vec![e_var("array")]),
                        e_str(" given"),
                    ])],
                ))],
                vec![],
                None,
            ),
            s_assign("options", e_call("__elephc_filter_options_arg", vec![
                e_var("options"),
                e_str("filter_var_array"),
                e_int(2),
            ])),
            s_if(
                e_not(e_call("is_array", vec![e_var("options")])),
                vec![s_if(
                    e_not(e_call("__elephc_filter_filter_exists", vec![e_var("options")])),
                    vec![
                        s_expr_call("trigger_error", vec![
                            e_concat_all(vec![
                                e_str("filter_var_array(): Unknown filter with ID "),
                                e_var("options"),
                            ]),
                            e_const("E_USER_WARNING"),
                        ]),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                )],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_filter_array_handler", vec![
                e_cast(CastType::Array, e_var("array")),
                e_var("options"),
                e_cast(CastType::Bool, e_var("add_empty")),
                e_str("filter_var_array"),
            ])),
        ])
        .build()
}

/// Builds `filter_input_array(mixed $type, mixed $options = FILTER_DEFAULT,
/// mixed $add_empty = true): mixed`.
fn filter_input_array_decl() -> Stmt {
    function("filter_input_array")
        .param("type", t_mixed())
        .param("options", t_mixed())
        .param_default("add_empty", t_mixed(), e_bool(true))
        .returns(t_mixed())
        .body(vec![
            s_assign("type", e_call("__elephc_filter_int_arg", vec![
                e_var("type"),
                e_str("filter_input_array"),
                e_int(1),
                e_str("type"),
            ])),
            s_assign("options", e_call("__elephc_filter_options_arg", vec![
                e_var("options"),
                e_str("filter_input_array"),
                e_int(2),
            ])),
            s_if(
                e_not(e_call("is_array", vec![e_var("options")])),
                vec![s_if(
                    e_not(e_call("__elephc_filter_filter_exists", vec![e_var("options")])),
                    vec![
                        s_expr_call("trigger_error", vec![
                            e_concat_all(vec![
                                e_str("filter_input_array(): Unknown filter with ID "),
                                e_var("options"),
                            ]),
                            e_const("E_USER_WARNING"),
                        ]),
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                )],
                vec![],
                None,
            ),
            s_expr_call("__elephc_filter_validate_input_type", vec![
                e_var("type"),
                e_str("filter_input_array"),
                e_str("type"),
            ]),
            s_assign("storage", e_call("__elephc_filter_storage", vec![e_var("type")])),
            s_if(
                e_binop(e_var("storage"), BinOp::StrictEq, e_null()),
                vec![s_return(e_null())],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_filter_array_handler", vec![
                e_cast(CastType::Array, e_var("storage")),
                e_var("options"),
                e_cast(CastType::Bool, e_var("add_empty")),
                e_str("filter_input_array"),
            ])),
        ])
        .build()
}

/// Builds `filter_input(mixed $type, mixed $var_name, mixed $filter = FILTER_DEFAULT,
/// mixed $options = 0): mixed`.
fn filter_input_decl() -> Stmt {
    let missing = vec![
        s_if(
            e_binop(
                e_binop(
                    e_binop(e_cast(CastType::Int, e_index(e_var("state"), e_int(1))), BinOp::BitAnd, e_const("FILTER_NULL_ON_FAILURE")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
                BinOp::And,
                e_binop(
                    e_binop(e_cast(CastType::Int, e_index(e_var("state"), e_int(1))), BinOp::BitAnd, e_const("FILTER_THROW_ON_FAILURE")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
            ),
            vec![s_throw(crate::synthetic_class::e_new_fq(
                "ValueError",
                vec![e_str("filter_input(): Argument #4 ($options) cannot use both FILTER_NULL_ON_FAILURE and FILTER_THROW_ON_FAILURE")],
            ))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_binop(e_cast(CastType::Int, e_index(e_var("state"), e_int(1))), BinOp::BitAnd, e_const("FILTER_THROW_ON_FAILURE")),
                BinOp::StrictNotEq,
                e_int(0),
            ),
            vec![s_throw(crate::synthetic_class::e_new_fq(
                "Filter\\FilterFailedException",
                vec![e_concat_all(vec![
                    e_str("input value '"),
                    e_var("var_name"),
                    e_str("' not found"),
                ])],
            ))],
            vec![],
            None,
        ),
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
                    e_call("array_key_exists", vec![
                        e_str("default"),
                        e_index(e_var("options"), e_str("options")),
                    ]),
                ),
                vec![s_return(e_index(
                    e_index(e_var("options"), e_str("options")),
                    e_str("default"),
                ))],
                vec![],
                None,
            )],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_binop(e_cast(CastType::Int, e_index(e_var("state"), e_int(1))), BinOp::BitAnd, e_const("FILTER_NULL_ON_FAILURE")),
                BinOp::StrictNotEq,
                e_int(0),
            ),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_return(e_null()),
    ];
    function("filter_input")
        .param("type", t_mixed())
        .param("var_name", t_mixed())
        .param_default("filter", t_mixed(), e_const("FILTER_DEFAULT"))
        .param_default("options", t_mixed(), e_int(0))
        .returns(t_mixed())
        .body(vec![
            s_assign("type", e_call("__elephc_filter_int_arg", vec![
                e_var("type"), e_str("filter_input"), e_int(1), e_str("type"),
            ])),
            s_assign("var_name", e_call("__elephc_filter_string_arg", vec![
                e_var("var_name"), e_str("filter_input"), e_int(2), e_str("var_name"),
            ])),
            s_assign("filter", e_call("__elephc_filter_int_arg", vec![
                e_var("filter"), e_str("filter_input"), e_int(3), e_str("filter"),
            ])),
            s_if(
                e_not(e_call("__elephc_filter_filter_exists", vec![e_var("filter")])),
                vec![
                    s_expr_call("trigger_error", vec![
                        e_concat_all(vec![
                            e_str("filter_input(): Unknown filter with ID "),
                            e_var("filter"),
                        ]),
                        e_const("E_USER_WARNING"),
                    ]),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("state", e_array(vec![
                e_cast(CastType::Int, e_var("filter")),
                e_const("FILTER_REQUIRE_SCALAR"),
                e_null(),
            ])),
            s_expr_call("__elephc_filter_parse_options", vec![
                e_var("options"),
                e_var("state"),
                e_str("filter_input"),
                e_int(4),
            ]),
            s_expr_call("__elephc_filter_check_flags", vec![
                e_cast(CastType::Int, e_index(e_var("state"), e_int(1))),
                e_str("filter_input"),
                e_int(4),
            ]),
            s_expr_call("__elephc_filter_validate_input_type", vec![
                e_var("type"), e_str("filter_input"), e_str("type"),
            ]),
            s_assign("storage", e_call("__elephc_filter_storage", vec![e_var("type")])),
            s_if(
                e_call("array_key_exists", vec![e_var("var_name"), e_cast(CastType::Array, e_var("storage"))]),
                vec![s_return(e_call("__elephc_filter_walk", vec![
                    e_index(e_cast(CastType::Array, e_var("storage")), e_var("var_name")),
                    e_cast(CastType::Int, e_index(e_var("state"), e_int(0))),
                    e_cast(CastType::Int, e_index(e_var("state"), e_int(1))),
                    crate::synthetic_class::e_cast(
                        CastType::Array,
                        crate::synthetic_class::e_ternary(
                            e_call("is_array", vec![e_var("options")]),
                            e_var("options"),
                            e_array(vec![]),
                        ),
                    ),
                    e_index(e_var("state"), e_int(2)),
                    e_str("filter_input"),
                    e_bool(true),
                ]))],
                vec![],
                Some(missing),
            ),
        ])
        .build()
}

/// Builds `filter_has_var(mixed $input_type, mixed $var_name): bool`.
fn filter_has_var_decl() -> Stmt {
    function("filter_has_var")
        .param("input_type", t_mixed())
        .param("var_name", t_mixed())
        .returns(t_bool())
        .body(vec![
            s_assign("input_type", e_call("__elephc_filter_int_arg", vec![
                e_var("input_type"), e_str("filter_has_var"), e_int(1), e_str("input_type"),
            ])),
            s_assign("var_name", e_call("__elephc_filter_string_arg", vec![
                e_var("var_name"), e_str("filter_has_var"), e_int(2), e_str("var_name"),
            ])),
            s_expr_call("__elephc_filter_validate_input_type", vec![
                e_var("input_type"), e_str("filter_has_var"), e_str("input_type"),
            ]),
            s_assign("storage", e_call("__elephc_filter_storage", vec![e_var("input_type")])),
            s_if(
                e_binop(
                    e_call("is_array", vec![e_var("storage")]),
                    BinOp::And,
                    e_call("array_key_exists", vec![e_var("var_name"), e_cast(CastType::Array, e_var("storage"))]),
                ),
                vec![s_return(e_bool(true))],
                vec![],
                None,
            ),
            s_return(e_bool(false)),
        ])
        .build()
}

/// Wraps an expression as a statement call.
fn s_expr_call(name: &str, args: Vec<Expr>) -> Stmt {
    crate::synthetic_class::s_expr(e_call(name, args))
}

/// Keeps the `storage_function` helper wired into the declaration set.
#[allow(dead_code)]
fn _storage_marker() -> Stmt { storage_function() }
