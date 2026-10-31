//! Purpose:
//! Builds the synthetic filter helpers that validate filter ids, parse option arrays, name
//! failures and map PHP values to php-src's diagnostic spelling.
//!
//! Called from:
//! - `crate::filter_prelude::filter_declarations`.
//!
//! Key details:
//! - `__elephc_filter_parse_options` mirrors `php_filter_call`'s precedence for `filter`,
//!   `options`, and `flags`, including the REQUIRE_SCALAR addition that only happens when a
//!   `flags` key is actually present.
//! - `__elephc_filter_failure` is the single throw-or-sentinel path for NULL_ON_FAILURE and
//!   THROW_ON_FAILURE.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_concat, e_concat_all, e_const, e_float, e_int, e_new_fq, e_null,
    e_str, e_var, function, s_array_assign, s_assign, s_expr, s_if, s_return, s_return_void, s_throw, t_array, t_bool, t_int, t_mixed,
    t_union,
};
use crate::synthetic_class::e_index;

/// Returns the helpers every public filter function shares.
pub(super) fn declarations() -> Program {
    vec![
        filter_exists_decl(),
        filter_name_decl(),
        type_name_decl(),
        failure_decl(),
        check_flags_decl(),
        parse_options_decl(),
        parse_scalar_options_decl(),
        parse_definition_decl(),
        int_arg_decl(),
        string_arg_decl(),
        options_arg_decl(),
    ]
}

/// Builds `__elephc_filter_filter_exists(int $filter): bool` matching PHP_FILTER_ID_EXISTS.
fn filter_exists_decl() -> Stmt {
    let in_range = |low: i64, high: i64| {
        e_binop(
            e_binop(e_var("filter"), BinOp::GtEq, e_int(low)),
            BinOp::And,
            e_binop(e_var("filter"), BinOp::LtEq, e_int(high)),
        )
    };
    let condition = e_binop(
        e_binop(in_range(0x0100, 0x0115), BinOp::Or, in_range(0x0200, 0x020b)),
        BinOp::Or,
        e_binop(e_var("filter"), BinOp::StrictEq, e_int(0x0400)),
    );
    function("__elephc_filter_filter_exists")
        .param("filter", t_int())
        .returns(t_bool())
        .returning(condition)
        .build()
}

/// Builds `__elephc_filter_name(int $filter): string`, php_find_filter's canonical name.
fn filter_name_decl() -> Stmt {
    let arms = vec![
        (257, "int"),
        (258, "boolean"),
        (259, "float"),
        (272, "validate_regexp"),
        (277, "validate_domain"),
        (273, "validate_url"),
        (274, "validate_email"),
        (275, "validate_ip"),
        (276, "validate_mac"),
        (513, "string"),
        (514, "encoded"),
        (515, "special_chars"),
        (522, "full_special_chars"),
        (516, "unsafe_raw"),
        (517, "email"),
        (518, "url"),
        (519, "number_int"),
        (520, "number_float"),
        (523, "add_slashes"),
        (1024, "callback"),
    ];
    let mut body = Vec::new();
    for (value, name) in arms {
        body.push(s_if(
            e_binop(e_var("filter"), BinOp::StrictEq, e_int(value)),
            vec![s_return(e_str(name))],
            vec![],
            None,
        ));
    }
    body.push(s_return(e_str("unsafe_raw")));
    function("__elephc_filter_name")
        .param("filter", t_int())
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}

/// Builds `__elephc_filter_type_name(mixed $value): string` for php-src error messages.
fn type_name_decl() -> Stmt {
    let arms = [
        ("is_object", None),
        ("is_int", Some("int")),
        ("is_float", Some("float")),
        ("is_string", Some("string")),
        ("is_null", Some("null")),
        ("is_array", Some("array")),
    ];
    let mut body = vec![
        s_if(
            e_call("is_bool", vec![e_var("value")]),
            vec![s_return(crate::synthetic_class::e_ternary(
                e_var("value"),
                e_str("true"),
                e_str("false"),
            ))],
            vec![],
            None,
        ),
    ];
    for (predicate, name) in arms {
        let then_body = match name {
            Some(name) => vec![s_return(e_str(name))],
            None => vec![s_return(e_call("get_class", vec![e_var("value")]))],
        };
        body.push(s_if(
            e_call(predicate, vec![e_var("value")]),
            then_body,
            vec![],
            None,
        ));
    }
    body.push(s_return(e_str("resource")));
    function("__elephc_filter_type_name")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(body)
        .build()
}

/// Builds `__elephc_filter_failure(int $flags, string $message): mixed`.
fn failure_decl() -> Stmt {
    function("__elephc_filter_failure")
        .param("flags", t_int())
        .param("message", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(
                    e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_THROW_ON_FAILURE")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
                vec![s_throw(e_new_fq("Filter\\FilterFailedException", vec![e_var("message")]))],
                vec![],
                None,
            ),
            s_assign("result", e_bool(false)),
            s_if(
                e_binop(
                    e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_NULL_ON_FAILURE")),
                    BinOp::StrictNotEq,
                    e_int(0),
                ),
                vec![s_assign("result", e_null())],
                vec![],
                None,
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// Builds `__elephc_filter_check_flags(int $flags, string $caller, int $arg): void`.
fn check_flags_decl() -> Stmt {
    let both = e_binop(
        e_binop(
            e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_NULL_ON_FAILURE")),
            BinOp::StrictNotEq,
            e_int(0),
        ),
        BinOp::And,
        e_binop(
            e_binop(e_var("flags"), BinOp::BitAnd, e_const("FILTER_THROW_ON_FAILURE")),
            BinOp::StrictNotEq,
            e_int(0),
        ),
    );
    let message = e_concat_all(vec![
        e_var("caller"),
        e_str("(): Argument #"),
        e_var("arg"),
        e_str(" ($options) cannot use both FILTER_NULL_ON_FAILURE and FILTER_THROW_ON_FAILURE"),
    ]);
    function("__elephc_filter_check_flags")
        .param("flags", t_int())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .returns(TypeExpr::Void)
        .body(vec![s_if(
            both,
            vec![s_throw(e_new_fq("ValueError", vec![message]))],
            vec![],
            None,
        )])
        .build()
}

/// Builds the optional-type rejection used by `array|int $options` parameters.
fn reject_options() -> Stmt {
    s_throw(e_new_fq(
        "TypeError",
        vec![e_concat_all(vec![
            e_var("caller"),
            e_str("(): Argument #"),
            e_var("arg"),
            e_str(" ($options) must be of type array|int, "),
            e_call("__elephc_filter_type_name", vec![e_var("options")]),
            e_str(" given"),
        ])],
    ))
}

/// Applies the REQUIRE_SCALAR default after an int/flags value replaces the flag word.
fn scalar_default(flags: Expr) -> Expr {
    let no_array_flag = e_binop(
        e_binop(
            flags.clone(),
            BinOp::BitAnd,
            e_binop(e_const("FILTER_REQUIRE_ARRAY"), BinOp::BitOr, e_const("FILTER_FORCE_ARRAY")),
        ),
        BinOp::StrictEq,
        e_int(0),
    );
    e_ternary_expr(
        no_array_flag,
        e_binop(flags.clone(), BinOp::BitOr, e_const("FILTER_REQUIRE_SCALAR")),
        flags,
    )
}

/// Builds a PHP ternary expression through the shared builder.
fn e_ternary_expr(condition: Expr, then_value: Expr, else_value: Expr) -> Expr {
    crate::synthetic_class::e_ternary(condition, then_value, else_value)
}

/// Builds `__elephc_filter_parse_options(mixed $options, array &$state, string $caller,
/// int $arg): void`.
///
/// Outputs travel through a caller-owned array because the current EIR ownership lowering
/// retains a returned array after a mixed-array branch (the filter leak regression) and does
/// not support a by-reference mixed parameter writeback.
fn parse_options_decl() -> Stmt {
    let write_state = vec![
        s_array_assign("state", e_int(0), e_var("filter")),
        s_array_assign("state", e_int(1), e_var("flags")),
        s_array_assign("state", e_int(2), e_var("callback")),
    ];
    let mut array_branch = vec![
        s_assign("filter", crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_index(e_var("state"), e_int(0)))),
        s_assign("flags", crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_index(e_var("state"), e_int(1)))),
        s_assign("callback", e_null()),
        s_if(
            e_call("array_key_exists", vec![e_str("filter"), e_var("options")]),
            vec![s_assign(
                "filter",
                crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_index(e_var("options"), e_str("filter"))),
            )],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_call("array_key_exists", vec![e_str("options"), e_var("options")]),
                BinOp::And,
                e_binop(e_var("filter"), BinOp::StrictEq, e_const("FILTER_CALLBACK")),
            ),
            vec![
                s_assign("callback", e_index(e_var("options"), e_str("options"))),
                s_assign("flags", e_int(0)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("array_key_exists", vec![e_str("flags"), e_var("options")]),
            vec![
                s_assign(
                    "flags",
                    crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_index(e_var("options"), e_str("flags"))),
                ),
                s_assign("flags", scalar_default(e_var("flags"))),
            ],
            vec![],
            None,
        ),
    ];
    array_branch.extend(write_state.clone());
    array_branch.push(s_return_void());

    function("__elephc_filter_parse_options")
        .param("options", t_mixed())
        .param_by_ref("state", Some(t_array()))
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_call("is_array", vec![e_var("options")]),
                array_branch,
                vec![],
                None,
            ),
            s_expr(e_call("__elephc_filter_parse_scalar_options", vec![
                e_var("options"),
                e_var("state"),
                e_var("caller"),
                e_var("arg"),
            ])),
        ])
        .build()
}

/// Builds `__elephc_filter_parse_scalar_options(mixed, array &$state, string, int): void`.
fn parse_scalar_options_decl() -> Stmt {
    let int_branch = vec![
        s_assign("flags", e_var("options")),
        s_array_assign("state", e_int(1), scalar_default(e_var("flags"))),
        s_return_void(),
    ];
    let scalar_branch = vec![
        s_array_assign(
            "state",
            e_int(1),
            scalar_default(crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_var("options"))),
        ),
        s_return_void(),
    ];
    function("__elephc_filter_parse_scalar_options")
        .param("options", t_mixed())
        .param_by_ref("state", Some(t_array()))
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(e_call("is_int", vec![e_var("options")]), int_branch, vec![], None),
            s_if(
                e_binop(
                    e_call("is_float", vec![e_var("options")]),
                    BinOp::Or,
                    e_binop(
                        e_call("is_bool", vec![e_var("options")]),
                        BinOp::Or,
                        e_binop(
                            e_call("is_string", vec![e_var("options")]),
                            BinOp::And,
                            e_call("is_numeric", vec![e_var("options")]),
                        ),
                    ),
                ),
                scalar_branch,
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("is_string", vec![e_var("options")]),
                    BinOp::And,
                    crate::synthetic_class::e_not(e_call("is_numeric", vec![e_var("options")])),
                ),
                vec![reject_options()],
                vec![],
                None,
            ),
            s_if(e_call("is_null", vec![e_var("options")]), vec![
                s_array_assign("state", e_int(1), scalar_default(e_int(0))),
                s_return_void(),
            ], vec![], None),
            reject_options(),
        ])
        .build()
}

/// Builds `__elephc_filter_parse_definition(array $definition, array &$state): void`.
fn parse_definition_decl() -> Stmt {
    function("__elephc_filter_parse_definition")
        .param("definition", t_array())
        .param_by_ref("state", Some(t_array()))
        .returns(TypeExpr::Void)
        .body(vec![
            s_assign("filter", e_int(-1)),
            s_assign("flags", e_const("FILTER_REQUIRE_SCALAR")),
            s_assign("callback", e_null()),
            s_if(
                e_call("array_key_exists", vec![e_str("filter"), e_var("definition")]),
                vec![s_assign(
                    "filter",
                    crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_index(e_var("definition"), e_str("filter"))),
                )],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("array_key_exists", vec![e_str("options"), e_var("definition")]),
                    BinOp::And,
                    e_binop(e_var("filter"), BinOp::StrictEq, e_const("FILTER_CALLBACK")),
                ),
                vec![
                    s_assign("callback", e_index(e_var("definition"), e_str("options"))),
                    s_assign("flags", e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("array_key_exists", vec![e_str("flags"), e_var("definition")]),
                vec![
                    s_assign(
                        "flags",
                        crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_index(e_var("definition"), e_str("flags"))),
                    ),
                    s_assign("flags", scalar_default(e_var("flags"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("filter"), BinOp::StrictEq, e_int(-1)),
                vec![s_assign("filter", e_const("FILTER_DEFAULT"))],
                vec![],
                None,
            ),
            s_array_assign("state", e_int(0), e_var("filter")),
            s_array_assign("state", e_int(1), e_var("flags")),
            s_array_assign("state", e_int(2), e_var("callback")),
        ])
        .build()
}

/// Builds `__elephc_filter_int_arg(mixed $value, string $caller, int $arg, string $param): int`.
fn int_arg_decl() -> Stmt {
    function("__elephc_filter_int_arg")
        .param("value", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .param("param", TypeExpr::Str)
        .returns(t_int())
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_binop(
                        e_call("is_float", vec![e_var("value")]),
                        BinOp::Or,
                        e_call("is_bool", vec![e_var("value")]),
                    ),
                    BinOp::Or,
                    e_binop(
                        e_call("is_string", vec![e_var("value")]),
                        BinOp::And,
                        e_call("is_numeric", vec![e_var("value")]),
                    ),
                ),
                vec![s_return(crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_var("value")))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_null", vec![e_var("value")]),
                vec![s_return(e_int(0))],
                vec![],
                None,
            ),
            s_throw(e_new_fq(
                "TypeError",
                vec![e_concat_all(vec![
                    e_var("caller"),
                    e_str("(): Argument #"),
                    e_var("arg"),
                    e_str(" ($"),
                    e_var("param"),
                    e_str(") must be of type int, "),
                    e_call("__elephc_filter_type_name", vec![e_var("value")]),
                    e_str(" given"),
                ])],
            )),
        ])
        .build()
}

/// Builds `__elephc_filter_string_arg(mixed $value, string $caller, int $arg, string $param): string`.
fn string_arg_decl() -> Stmt {
    function("__elephc_filter_string_arg")
        .param("value", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .param("param", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![s_return(e_var("value"))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_call("is_int", vec![e_var("value")]),
                    BinOp::Or,
                    e_binop(
                        e_call("is_float", vec![e_var("value")]),
                        BinOp::Or,
                        e_call("is_bool", vec![e_var("value")]),
                    ),
                ),
                vec![s_return(crate::synthetic_class::e_cast(crate::parser::ast::CastType::String, e_var("value")))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_null", vec![e_var("value")]),
                vec![s_return(e_str(""))],
                vec![],
                None,
            ),
            s_if(
                crate::synthetic_class::e_instance_of(e_var("value"), "\\Stringable"),
                vec![s_return(crate::synthetic_class::e_cast(crate::parser::ast::CastType::String, e_var("value")))],
                vec![],
                None,
            ),
            s_throw(e_new_fq(
                "TypeError",
                vec![e_concat_all(vec![
                    e_var("caller"),
                    e_str("(): Argument #"),
                    e_var("arg"),
                    e_str(" ($"),
                    e_var("param"),
                    e_str(") must be of type string, "),
                    e_call("__elephc_filter_type_name", vec![e_var("value")]),
                    e_str(" given"),
                ])],
            )),
        ])
        .build()
}


/// Builds `__elephc_filter_options_arg(mixed $options, string $caller, int $arg): mixed`.
fn options_arg_decl() -> Stmt {
    function("__elephc_filter_options_arg")
        .param("options", t_mixed())
        .param("caller", TypeExpr::Str)
        .param("arg", t_int())
        .returns(t_mixed())
        .body(vec![
            s_if(e_call("is_array", vec![e_var("options")]), vec![s_return(e_var("options"))], vec![], None),
            s_if(e_call("is_int", vec![e_var("options")]), vec![s_return(e_var("options"))], vec![], None),
            s_if(
                e_binop(
                    e_call("is_float", vec![e_var("options")]),
                    BinOp::Or,
                    e_binop(
                        e_call("is_bool", vec![e_var("options")]),
                        BinOp::Or,
                        e_binop(
                            e_call("is_string", vec![e_var("options")]),
                            BinOp::And,
                            e_call("is_numeric", vec![e_var("options")]),
                        ),
                    ),
                ),
                vec![s_return(crate::synthetic_class::e_cast(crate::parser::ast::CastType::Int, e_var("options")))],
                vec![],
                None,
            ),
            s_if(e_call("is_null", vec![e_var("options")]), vec![s_return(e_int(0))], vec![], None),
            s_throw(e_new_fq(
                "TypeError",
                vec![e_concat_all(vec![
                    e_var("caller"),
                    e_str("(): Argument #"),
                    e_var("arg"),
                    e_str(" ($options) must be of type array|int, "),
                    e_call("__elephc_filter_type_name", vec![e_var("options")]),
                    e_str(" given"),
                ])],
            )),
        ])
        .build()
}

/// Keeps the float import used by the shared type helper surface.
#[allow(dead_code)]
fn _float_marker() -> Expr { e_float(0.0) }

/// Keeps the union type helper referenced while the option signatures stay explicit.
#[allow(dead_code)]
fn _union_marker() -> TypeExpr { t_union(vec![t_array(), t_int()]) }

/// Keeps `e_concat` referenced for future option diagnostics.
#[allow(dead_code)]
fn _concat_marker(left: Expr, right: Expr) -> Expr { e_concat(left, right) }
