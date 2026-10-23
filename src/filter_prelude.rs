//! Purpose:
//! Injects PHP's `filter_var()` with the `FILTER_VALIDATE_IP` filter (written in
//! elephc-PHP), the ext/filter surface the curl callback fixtures need. `filter_var`
//! validates an IPv4 or IPv6 address — with the RFC 6890 range flags — and returns the
//! validated value or `false`, exactly like php-src's `ext/filter`.
//!
//! Called from:
//! - `crate::pipeline::compile()` via `inject_if_used`, before name resolution, so a
//!   user `filter_var(...)` call resolves to the injected function through the normal
//!   pipeline (functions, loops, string builtins) with no dedicated codegen or
//!   runtime helper.
//!
//! Key details:
//! - Implemented as a prelude rather than a runtime walker because the address grammar
//!   and the RFC 6890 range table are pure PHP control flow; that keeps it correct on
//!   every supported target with no per-target assembly.
//! - Pay-for-use: injected only when the program references `filter_var` and never when
//!   it declares its own (a user definition wins). The `FILTER_*` constants live in the
//!   shared constant catalog, so a program may use them with or without the function.
//! - ONLY `FILTER_VALIDATE_IP` is carried. Any other filter identifier — including
//!   `FILTER_DEFAULT` — raises PHP's unknown-filter warning and answers `false`; the
//!   warning is `trigger_error(..., E_USER_WARNING)` because a prelude function has no
//!   internal-warning channel, and the displayed message matches php's.
//! - The grammar and range table follow php-src `ext/filter/logical_filters.c` on the
//!   PHP 8.5 branch (the vendored 8.5.13-dev tree), with one deliberate divergence from
//!   the older 8.5.10 interpreter binaries: `64:ff9b::/16` is GLOBAL there, so
//!   `NO_RES_RANGE` does not reject it. The full behavior ledger, measured case by case
//!   against `php -r`, lives in the oracle file
//!   `scripts/filter_prelude_source/filter_prelude.php`.
//! - Helpers are `__elephc_filter_*`-prefixed so they cannot collide with user code;
//!   the entry point itself is the PHP-visible `filter_var`.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_index, e_int, e_neg, e_not, e_post_inc, e_str,
    e_ternary, e_var, function, internal_declarations, s_assign, s_expr, s_for, s_foreach, s_if,
    s_return, t_mixed,
};

/// Injects the filter prelude only when the program references `filter_var` and does
/// not declare its own (matching every other pay-for-use prelude).
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        inject_if_used_on_compiler_stack(program, inventory)
    })
}

/// The body of [`inject_if_used`], run on the stack `with_compiler_stack` sized.
fn inject_if_used_on_compiler_stack(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let used = crate::opcache_prelude::detect::program_references(&program, "filter_var")
        && !crate::opcache_prelude::detect::program_declares(&program, "filter_var");
    if !used {
        return program;
    }
    let mut combined = filter_declarations();
    inventory.record_program("filter", &combined);
    combined.extend(program);
    combined
}

/// `__elephc_filter_ip_v4_part_ok` — transcribed from the PHP form.
fn decl_fn_elephc_filter_ip_v4_part_ok() -> Stmt {
    function("__elephc_filter_ip_v4_part_ok")
        .param("part", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("len", e_call("strlen", vec![e_var("part")])),
            s_if(
                e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("len"), BinOp::Gt, e_int(3))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("len"), BinOp::Gt, e_int(1)), BinOp::And, e_binop(e_index(e_var("part"), e_int(0)), BinOp::StrictEq, e_str("0"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("len"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("c", e_call("ord", vec![e_index(e_var("part"), e_var("i"))])),
                s_if(
                    e_binop(e_binop(e_var("c"), BinOp::Lt, e_int(48)), BinOp::Or, e_binop(e_var("c"), BinOp::Gt, e_int(57))),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_binop(e_cast(CastType::Int, e_var("part")), BinOp::LtEq, e_int(255))),
        ])
        .build()
}

/// `__elephc_filter_ip_v4_valid` — transcribed from the PHP form.
fn decl_fn_elephc_filter_ip_v4_valid() -> Stmt {
    function("__elephc_filter_ip_v4_valid")
        .param("ip", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("parts", e_call("explode", vec![e_str("."), e_var("ip")])),
            s_if(
                e_binop(e_call("count", vec![e_var("parts")]), BinOp::StrictNotEq, e_int(4)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_foreach(e_var("parts"), None, "part", vec![
                s_if(
                    e_not(e_call("__elephc_filter_ip_v4_part_ok", vec![e_var("part")])),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `__elephc_filter_ip_v4_status` — transcribed from the PHP form.
fn decl_fn_elephc_filter_ip_v4_status() -> Stmt {
    function("__elephc_filter_ip_v4_status")
        .param("ip", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("parts", e_call("explode", vec![e_str("."), e_var("ip")])),
            s_assign("a", e_cast(CastType::Int, e_index(e_var("parts"), e_int(0)))),
            s_assign("b", e_cast(CastType::Int, e_index(e_var("parts"), e_int(1)))),
            s_assign("c", e_cast(CastType::Int, e_index(e_var("parts"), e_int(2)))),
            s_if(
                e_binop(e_var("a"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("a"), BinOp::StrictEq, e_int(10)),
                vec![
                    s_return(e_int(2)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(100)), BinOp::And, e_binop(e_var("b"), BinOp::GtEq, e_int(64))), BinOp::And, e_binop(e_var("b"), BinOp::LtEq, e_int(127))),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("a"), BinOp::StrictEq, e_int(127)),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(169)), BinOp::And, e_binop(e_var("b"), BinOp::StrictEq, e_int(254))),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(172)), BinOp::And, e_binop(e_var("b"), BinOp::GtEq, e_int(16))), BinOp::And, e_binop(e_var("b"), BinOp::LtEq, e_int(31))),
                vec![
                    s_return(e_int(2)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(192)), BinOp::And, e_binop(e_var("b"), BinOp::StrictEq, e_int(0))), BinOp::And, e_binop(e_var("c"), BinOp::StrictEq, e_int(0))),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(192)), BinOp::And, e_binop(e_var("b"), BinOp::StrictEq, e_int(88))), BinOp::And, e_binop(e_var("c"), BinOp::StrictEq, e_int(99))),
                vec![
                    s_return(e_int(3)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(192)), BinOp::And, e_binop(e_var("b"), BinOp::StrictEq, e_int(168))),
                vec![
                    s_return(e_int(2)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(198)), BinOp::And, e_binop(e_var("b"), BinOp::GtEq, e_int(18))), BinOp::And, e_binop(e_var("b"), BinOp::LtEq, e_int(19))),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(198)), BinOp::And, e_binop(e_var("b"), BinOp::StrictEq, e_int(51))), BinOp::And, e_binop(e_var("c"), BinOp::StrictEq, e_int(100))),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_var("a"), BinOp::StrictEq, e_int(203)), BinOp::And, e_binop(e_var("b"), BinOp::StrictEq, e_int(0))), BinOp::And, e_binop(e_var("c"), BinOp::StrictEq, e_int(113))),
                vec![
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("a"), BinOp::GtEq, e_int(240)),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// `__elephc_filter_ip_hex_value` — transcribed from the PHP form.
fn decl_fn_elephc_filter_ip_hex_value() -> Stmt {
    function("__elephc_filter_ip_hex_value")
        .param("group", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("len", e_call("strlen", vec![e_var("group")])),
            s_if(
                e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("len"), BinOp::Gt, e_int(4))),
                vec![
                    s_return(e_neg(e_int(1))),
                ],
                vec![],
                None,
            ),
            s_assign("value", e_int(0)),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("len"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("c", e_call("ord", vec![e_index(e_var("group"), e_var("i"))])),
                s_if(
                    e_binop(e_binop(e_var("c"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_int(57))),
                    vec![
                        s_assign("d", e_binop(e_var("c"), BinOp::Sub, e_int(48))),
                    ],
                    vec![
                    (e_binop(e_binop(e_var("c"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_int(102))), vec![
                        s_assign("d", e_binop(e_var("c"), BinOp::Sub, e_int(87))),
                    ]),
                    (e_binop(e_binop(e_var("c"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_int(70))), vec![
                        s_assign("d", e_binop(e_var("c"), BinOp::Sub, e_int(55))),
                    ]),
                ],
                    Some(vec![
                    s_return(e_neg(e_int(1))),
                ]),
                ),
                s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(16)), BinOp::Add, e_var("d"))),
            ]),
            s_return(e_var("value")),
        ])
        .build()
}

/// `__elephc_filter_ip_v6_groups` — transcribed from the PHP form.
fn decl_fn_elephc_filter_ip_v6_groups() -> Stmt {
    function("__elephc_filter_ip_v6_groups")
        .param("ip", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_if(
                e_binop(e_binop(e_call("strlen", vec![e_var("ip")]), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_call("strlen", vec![e_var("ip")]), BinOp::Gt, e_int(45))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            // `-v4-` is this parser's private placeholder for a verified IPv4 tail; an
            // address that spells it literally is not an IPv6 address.
            s_if(
                e_binop(e_call("strpos", vec![e_var("ip"), e_str("-v4-")]), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("hasV4", e_bool(false)),
            s_assign("rest", e_var("ip")),
            s_assign("lastColon", e_call("strrpos", vec![e_var("rest"), e_str(":")])),
            s_if(
                e_binop(e_var("lastColon"), BinOp::StrictNotEq, e_bool(false)),
                vec![
                    s_assign("tail", e_call("substr", vec![e_var("rest"), e_binop(e_var("lastColon"), BinOp::Add, e_int(1))])),
                    s_if(
                        e_binop(e_call("substr_count", vec![e_var("tail"), e_str(".")]), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_if(
                                e_not(e_call("__elephc_filter_ip_v4_valid", vec![e_var("tail")])),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("hasV4", e_bool(true)),
                            s_assign("rest", e_binop(e_call("substr", vec![e_var("rest"), e_int(0), e_binop(e_var("lastColon"), BinOp::Add, e_int(1))]), BinOp::Concat, e_str("-v4-"))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_assign("segs", e_call("explode", vec![e_str(":"), e_var("rest")])),
            s_assign("count", e_call("count", vec![e_var("segs")])),
            s_assign("runStart", e_neg(e_int(1))),
            s_assign("runLen", e_int(0)),
            s_assign("runClosed", e_bool(false)),
            s_assign("hexGroups", e_int(0)),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("seg", e_index(e_var("segs"), e_var("i"))),
                s_if(
                    e_binop(e_var("seg"), BinOp::StrictEq, e_str("")),
                    vec![
                        s_if(
                            e_var("runClosed"),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("runStart"), BinOp::StrictEq, e_neg(e_int(1))),
                            vec![
                                s_assign("runStart", e_var("i")),
                                s_assign("runLen", e_int(1)),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("runLen", e_binop(e_var("runLen"), BinOp::Add, e_int(1))),
                        ]),
                        ),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("seg"), BinOp::StrictNotEq, e_str("-v4-")),
                        vec![
                            s_if(
                                e_binop(e_call("__elephc_filter_ip_hex_value", vec![e_var("seg")]), BinOp::Lt, e_int(0)),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("hexGroups", e_binop(e_var("hexGroups"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("runStart"), BinOp::StrictNotEq, e_neg(e_int(1))),
                        vec![
                            s_assign("runClosed", e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                ),
            ]),
            s_assign("total", e_binop(e_var("hexGroups"), BinOp::Add, e_ternary(e_var("hasV4"), e_int(2), e_int(0)))),
            s_if(
                e_binop(e_var("runStart"), BinOp::StrictEq, e_neg(e_int(1))),
                vec![
                    s_if(
                        e_binop(e_var("total"), BinOp::StrictNotEq, e_int(8)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![
                (e_binop(e_var("total"), BinOp::Gt, e_int(7)), vec![
                    s_return(e_bool(false)),
                ]),
                (e_binop(e_var("runLen"), BinOp::GtEq, e_int(2)), vec![
                    s_if(
                        e_binop(e_binop(e_var("runStart"), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("runStart"), BinOp::Add, e_var("runLen")), BinOp::StrictEq, e_var("count"))),
                        vec![
                            s_if(
                                e_binop(e_var("runLen"), BinOp::StrictNotEq, e_int(3)),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![
                        (e_binop(e_binop(e_var("runStart"), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("runStart"), BinOp::Add, e_var("runLen")), BinOp::StrictNotEq, e_var("count"))), vec![
                            s_return(e_bool(false)),
                        ]),
                    ],
                        None,
                    ),
                ]),
                (e_binop(e_binop(e_var("runStart"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_binop(e_var("runStart"), BinOp::Add, e_var("runLen")), BinOp::StrictEq, e_var("count"))), vec![
                    s_return(e_bool(false)),
                ]),
            ],
                None,
            ),
            s_if(
                e_var("hasV4"),
                vec![
                    s_assign("tail", e_call("substr", vec![e_var("ip"), e_binop(e_call("strrpos", vec![e_var("ip"), e_str(":")]), BinOp::Add, e_int(1))])),
                    s_assign("parts", e_call("explode", vec![e_str("."), e_var("tail")])),
                    s_assign("bytes", e_array(vec![])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_int(10))), Some(s_expr(e_post_inc("i"))), vec![
                        s_expr(e_call("array_push", vec![e_var("bytes"), e_int(0)])),
                    ]),
                    s_expr(e_call("array_push", vec![e_var("bytes"), e_int(255)])),
                    s_expr(e_call("array_push", vec![e_var("bytes"), e_int(255)])),
                    s_expr(e_call("array_push", vec![e_var("bytes"), e_cast(CastType::Int, e_index(e_var("parts"), e_int(0)))])),
                    s_expr(e_call("array_push", vec![e_var("bytes"), e_cast(CastType::Int, e_index(e_var("parts"), e_int(1)))])),
                    s_expr(e_call("array_push", vec![e_var("bytes"), e_cast(CastType::Int, e_index(e_var("parts"), e_int(2)))])),
                    s_expr(e_call("array_push", vec![e_var("bytes"), e_cast(CastType::Int, e_index(e_var("parts"), e_int(3)))])),
                    s_return(e_var("bytes")),
                ],
                vec![],
                None,
            ),
            s_assign("groups", e_array(vec![])),
            s_assign("inRun", e_bool(false)),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("seg", e_index(e_var("segs"), e_var("i"))),
                s_if(
                    e_binop(e_var("seg"), BinOp::StrictEq, e_str("")),
                    vec![
                        s_if(
                            e_not(e_var("inRun")),
                            vec![
                                s_assign("inRun", e_bool(true)),
                                s_assign("zeros", e_binop(e_int(8), BinOp::Sub, e_var("total"))),
                                s_for(Some(s_assign("z", e_int(0))), Some(e_binop(e_var("z"), BinOp::Lt, e_var("zeros"))), Some(s_expr(e_post_inc("z"))), vec![
                                    s_expr(e_call("array_push", vec![e_var("groups"), e_int(0)])),
                                ]),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    Some(vec![
                    s_expr(e_call("array_push", vec![e_var("groups"), e_call("__elephc_filter_ip_hex_value", vec![e_var("seg")])])),
                ]),
                ),
            ]),
            s_assign("bytes", e_array(vec![])),
            s_foreach(e_var("groups"), None, "g", vec![
                s_expr(e_call("array_push", vec![e_var("bytes"), e_cast(CastType::Int, e_binop(e_cast(CastType::Int, e_var("g")), BinOp::Div, e_int(256)))])),
                s_expr(e_call("array_push", vec![e_var("bytes"), e_binop(e_cast(CastType::Int, e_var("g")), BinOp::Mod, e_int(256))])),
            ]),
            s_return(e_var("bytes")),
        ])
        .build()
}

/// `__elephc_filter_ip_v6_status` — transcribed from the PHP form.
fn decl_fn_elephc_filter_ip_v6_status() -> Stmt {
    function("__elephc_filter_ip_v6_status")
        .param_untyped("bytes")
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("allZero", e_bool(true)),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_int(16))), Some(s_expr(e_post_inc("i"))), vec![
                s_if(
                    e_binop(e_index(e_var("bytes"), e_var("i")), BinOp::StrictNotEq, e_int(0)),
                    vec![
                        s_assign("allZero", e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_if(
                e_var("allZero"),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_assign("loopback", e_bool(true)),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_int(15))), Some(s_expr(e_post_inc("i"))), vec![
                s_if(
                    e_binop(e_index(e_var("bytes"), e_var("i")), BinOp::StrictNotEq, e_int(0)),
                    vec![
                        s_assign("loopback", e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_if(
                e_var("loopback"),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_index(e_var("bytes"), e_int(0)), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_index(e_var("bytes"), e_int(1)), BinOp::StrictEq, e_int(100))), BinOp::And, e_binop(e_index(e_var("bytes"), e_int(2)), BinOp::StrictEq, e_int(255))), BinOp::And, e_binop(e_index(e_var("bytes"), e_int(3)), BinOp::StrictEq, e_int(155))),
                vec![
                    s_return(e_int(3)),
                ],
                vec![],
                None,
            ),
            s_assign("mapped", e_binop(e_binop(e_index(e_var("bytes"), e_int(10)), BinOp::StrictEq, e_int(255)), BinOp::And, e_binop(e_index(e_var("bytes"), e_int(11)), BinOp::StrictEq, e_int(255)))),
            s_if(
                e_var("mapped"),
                vec![
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_int(10))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_index(e_var("bytes"), e_var("i")), BinOp::StrictNotEq, e_int(0)),
                            vec![
                                s_assign("mapped", e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_var("mapped"),
                        vec![
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_index(e_var("bytes"), e_int(0)), BinOp::StrictEq, e_int(254)), BinOp::And, e_binop(e_binop(e_index(e_var("bytes"), e_int(1)), BinOp::BitAnd, e_int(192)), BinOp::StrictEq, e_int(128))),
                vec![
                    s_return(e_int(1)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_index(e_var("bytes"), e_int(0)), BinOp::StrictEq, e_int(252)), BinOp::Or, e_binop(e_index(e_var("bytes"), e_int(0)), BinOp::StrictEq, e_int(253))),
                vec![
                    s_return(e_int(2)),
                ],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
        .build()
}

/// `filter_var` — transcribed from the PHP form.
fn decl_fn_filter_var() -> Stmt {
    function("filter_var")
        .param_untyped("value")
        .param_untyped_default("filter", e_int(516))
        .param_untyped_default("options", e_int(0))
        .body(vec![
            s_if(
                e_binop(e_var("filter"), BinOp::StrictNotEq, e_int(275)),
                vec![
                    s_expr(e_call("trigger_error", vec![e_binop(e_str("filter_var(): Unknown filter with ID "), BinOp::Concat, e_cast(CastType::String, e_var("filter"))), e_int(512)])),
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_call("is_object", vec![e_var("value")])), BinOp::Or, e_call("is_resource", vec![e_var("value")])),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_assign("ip", e_cast(CastType::String, e_var("value"))),
            s_assign("flags", e_var("options")),
            s_assign("isV6", e_binop(e_call("strpos", vec![e_var("ip"), e_str(":")]), BinOp::StrictNotEq, e_bool(false))),
            s_assign("isV4", e_binop(e_not(e_var("isV6")), BinOp::And, e_binop(e_call("strpos", vec![e_var("ip"), e_str(".")]), BinOp::StrictNotEq, e_bool(false)))),
            s_if(
                e_binop(e_not(e_var("isV6")), BinOp::And, e_not(e_var("isV4"))),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1048576)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2097152)), BinOp::StrictEq, e_int(0))), BinOp::And, e_var("isV6")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2097152)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1048576)), BinOp::StrictEq, e_int(0))), BinOp::And, e_var("isV4")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("isV6"),
                vec![
                    s_assign("bytes", e_call("__elephc_filter_ip_v6_groups", vec![e_var("ip")])),
                    s_if(
                        e_not(e_call("is_array", vec![e_var("bytes")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("status", e_call("__elephc_filter_ip_v6_status", vec![e_var("bytes")])),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_not(e_call("__elephc_filter_ip_v4_valid", vec![e_var("ip")])),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_assign("status", e_call("__elephc_filter_ip_v4_status", vec![e_var("ip")])),
            ]),
            ),
            s_if(
                e_binop(e_var("status"), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(536870912)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_var("status"), BinOp::StrictNotEq, e_int(3))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(8388608)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_var("status"), BinOp::StrictEq, e_int(2))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(4194304)), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_var("status"), BinOp::StrictEq, e_int(1))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_return(e_var("ip")),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn filter_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_fn_elephc_filter_ip_v4_part_ok(),
            decl_fn_elephc_filter_ip_v4_valid(),
            decl_fn_elephc_filter_ip_v4_status(),
            decl_fn_elephc_filter_ip_hex_value(),
            decl_fn_elephc_filter_ip_v6_groups(),
            decl_fn_elephc_filter_ip_v6_status(),
            decl_fn_filter_var(),
        ]
    })
}
#[cfg(test)]
mod tests {
    //! Purpose:
    //! The parse-parity oracle for the built filter surface: the built AST must equal
    //! the parse of the oracle PHP, declaration by declaration, plus behavior probes
    //! the codegen tests do not cover.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.

    use super::*;

    /// Parses the oracle PHP exactly as the compiler would have at injection time.
    fn parsed_oracle() -> Program {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/filter_prelude_source/filter_prelude.php"
        );
        let php = std::fs::read_to_string(path).expect("oracle must read");
        let tokens = crate::lexer::tokenize(&php).expect("oracle must tokenize");
        crate::parser::parse_internal(&tokens).expect("oracle must parse")
    }

    /// Removes span payloads so a built node and a parsed node compare on
    /// structure alone.
    fn strip_spans(rendered: &str) -> String {
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered;
        while let Some(at) = rest.find("Span {") {
            cleaned.push_str(&rest[..at]);
            cleaned.push_str("Span");
            let after = &rest[at..];
            let close = after.find('}').map(|end| end + 1).unwrap_or(after.len());
            rest = &after[close..];
        }
        cleaned.push_str(rest);
        cleaned
    }

    /// The transcription driver is gone; this test is its permanent replacement:
    /// every declaration in the oracle must rebuild identically.
    #[test]
    fn built_declarations_match_the_oracle_php() {
        let built = filter_declarations();
        let parsed = parsed_oracle();
        assert_eq!(
            built.len(),
            parsed.len(),
            "declaration COUNT differs — built {} vs parsed {}",
            built.len(),
            parsed.len()
        );
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            let left = strip_spans(&format!("{built_stmt:?}"));
            let right = strip_spans(&format!("{parsed_stmt:?}"));
            assert_eq!(left, right, "built declaration differs from its PHP oracle");
        }
    }
}
