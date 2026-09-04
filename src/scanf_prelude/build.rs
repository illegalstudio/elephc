//! Purpose:
//! Builds php's scanf engine and its by-reference arity wrappers as AST, replacing the PHP source
//! this module used to carry as a raw string and reparse on every compile that touched it.
//!
//! Called from:
//! - `crate::scanf_prelude::inject_if_used`, after include resolution and before name resolution.
//!
//! Key details:
//! - The ENGINE is TRANSCRIBED, not rewritten: every declaration was generated from the parse of
//!   the PHP it replaces (`synthetic_class::transcribe`), and the migration oracle
//!   (`ELEPHC_ORACLE_PHP` / `ELEPHC_ORACLE_WHICH=scanf`) compares the built AST against that parse
//!   node by node.
//! - The WRAPPERS were already a Rust loop printing PHP text, one pair per arity. They stay a
//!   loop and build AST instead, which is smaller than sixteen transcribed copies of two shapes
//!   and is checked by the same oracle, against the very text that loop used to print.
//! - The PHP form stays under `#[cfg(test)]` in the parent module as that oracle's reference.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_bool, e_call, e_cast, e_float, e_index, e_int, e_neg, e_new_fq, e_not, e_null, e_post_inc, e_str, e_ternary, e_var, function, internal_declarations, s_array_push, s_assign, s_break, s_continue, s_expr, s_for, s_if, s_return, s_throw, s_while, t_array, t_mixed, t_nullable, t_union, FunctionBuilder,
};

/// `__elephc_scanf_is_space` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_is_space() -> Stmt {
    function("__elephc_scanf_is_space")
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\n"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\r"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\u{b}"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\u{c}")))),
        ])
        .build()
}

/// `__elephc_scanf_digit_value` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_digit_value() -> Stmt {
    function("__elephc_scanf_digit_value")
        .param("c", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_assign("o", e_call("ord", vec![e_var("c")])),
            s_if(
                e_binop(e_binop(e_var("o"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("o"), BinOp::LtEq, e_int(57))),
                vec![
                    s_return(e_binop(e_var("o"), BinOp::Sub, e_int(48))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("o"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("o"), BinOp::LtEq, e_int(122))),
                vec![
                    s_return(e_binop(e_var("o"), BinOp::Sub, e_int(87))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("o"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("o"), BinOp::LtEq, e_int(90))),
                vec![
                    s_return(e_binop(e_var("o"), BinOp::Sub, e_int(55))),
                ],
                vec![],
                None,
            ),
            s_return(e_int(99)),
        ])
        .build()
}

/// `__elephc_scanf_unsigned_negative` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_unsigned_negative() -> Stmt {
    function("__elephc_scanf_unsigned_negative")
        .param("digits", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("minuend", e_str("18446744073709551616")),
            s_assign("out", e_str("")),
            s_assign("borrow", e_int(0)),
            s_assign("i", e_int(0)),
            s_assign("ml", e_call("strlen", vec![e_var("minuend")])),
            s_assign("dl", e_call("strlen", vec![e_var("digits")])),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_var("ml")), vec![
                s_assign("da", e_binop(e_call("ord", vec![e_index(e_var("minuend"), e_binop(e_binop(e_var("ml"), BinOp::Sub, e_int(1)), BinOp::Sub, e_var("i")))]), BinOp::Sub, e_int(48))),
                s_assign("db", e_ternary(e_binop(e_var("i"), BinOp::Lt, e_var("dl")), e_binop(e_call("ord", vec![e_index(e_var("digits"), e_binop(e_binop(e_var("dl"), BinOp::Sub, e_int(1)), BinOp::Sub, e_var("i")))]), BinOp::Sub, e_int(48)), e_int(0))),
                s_assign("d", e_binop(e_binop(e_var("da"), BinOp::Sub, e_var("db")), BinOp::Sub, e_var("borrow"))),
                s_if(
                    e_binop(e_var("d"), BinOp::Lt, e_int(0)),
                    vec![
                        s_assign("d", e_binop(e_var("d"), BinOp::Add, e_int(10))),
                        s_assign("borrow", e_int(1)),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("borrow", e_int(0)),
                ]),
                ),
                s_assign("out", e_binop(e_cast(CastType::String, e_var("d")), BinOp::Concat, e_var("out"))),
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
            ]),
            s_assign("out", e_call("ltrim", vec![e_var("out"), e_str("0")])),
            s_return(e_ternary(e_binop(e_var("out"), BinOp::StrictEq, e_str("")), e_str("0"), e_var("out"))),
        ])
        .build()
}

/// `__elephc_scanf_unsigned` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_unsigned() -> Stmt {
    function("__elephc_scanf_unsigned")
        .param("digits", TypeExpr::Str)
        .param("sign", TypeExpr::Int)
        .returns(t_union(vec![TypeExpr::Int, TypeExpr::Str]))
        .body(vec![
            s_assign("ulongMax", e_str("18446744073709551615")),
            s_assign("normalized", e_call("ltrim", vec![e_var("digits"), e_str("0")])),
            s_if(
                e_binop(e_var("normalized"), BinOp::StrictEq, e_str("")),
                vec![
                    s_assign("normalized", e_str("0")),
                ],
                vec![],
                None,
            ),
            s_assign("saturated", e_bool(false)),
            s_if(
                e_binop(e_call("strlen", vec![e_var("normalized")]), BinOp::Gt, e_int(20)),
                vec![
                    s_assign("saturated", e_bool(true)),
                ],
                vec![
                (e_binop(e_binop(e_call("strlen", vec![e_var("normalized")]), BinOp::StrictEq, e_int(20)), BinOp::And, e_binop(e_call("strcmp", vec![e_var("normalized"), e_var("ulongMax")]), BinOp::Gt, e_int(0))), vec![
                    s_assign("saturated", e_bool(true)),
                ]),
            ],
                None,
            ),
            s_if(
                e_var("saturated"),
                vec![
                    s_return(e_var("ulongMax")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("sign"), BinOp::Lt, e_int(0)),
                vec![
                    s_if(
                        e_binop(e_var("normalized"), BinOp::StrictEq, e_str("0")),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("__elephc_scanf_unsigned_negative", vec![e_var("normalized")])),
                ],
                vec![],
                None,
            ),
            s_assign("intMax", e_cast(CastType::String, e_int(9223372036854775807))),
            s_if(
                e_binop(e_binop(e_call("strlen", vec![e_var("normalized")]), BinOp::Lt, e_call("strlen", vec![e_var("intMax")])), BinOp::Or, e_binop(e_binop(e_call("strlen", vec![e_var("normalized")]), BinOp::StrictEq, e_call("strlen", vec![e_var("intMax")])), BinOp::And, e_binop(e_call("strcmp", vec![e_var("normalized"), e_var("intMax")]), BinOp::LtEq, e_int(0)))),
                vec![
                    s_return(e_cast(CastType::Int, e_var("normalized"))),
                ],
                vec![],
                None,
            ),
            s_return(e_var("normalized")),
        ])
        .build()
}

/// `__elephc_scanf_int` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_int() -> Stmt {
    function("__elephc_scanf_int")
        .param("s", TypeExpr::Str)
        .param("si", TypeExpr::Int)
        .param("sl", TypeExpr::Int)
        .param("width", TypeExpr::Int)
        .param("conv", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_assign("start", e_var("si")),
            s_assign("sign", e_int(1)),
            s_assign("signed", e_bool(false)),
            s_if(
                e_binop(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), BinOp::And, e_binop(e_binop(e_index(e_var("s"), e_var("si")), BinOp::StrictEq, e_str("-")), BinOp::Or, e_binop(e_index(e_var("s"), e_var("si")), BinOp::StrictEq, e_str("+")))),
                vec![
                    s_assign("signed", e_bool(true)),
                    s_if(
                        e_binop(e_index(e_var("s"), e_var("si")), BinOp::StrictEq, e_str("-")),
                        vec![
                            s_assign("sign", e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                ],
                vec![],
                None,
            ),
            s_assign("base", e_int(10)),
            s_assign("hexPrefix", e_binop(e_binop(e_binop(e_binop(e_not(e_var("signed")), BinOp::And, e_binop(e_binop(e_var("si"), BinOp::Add, e_int(2)), BinOp::Lt, e_var("sl"))), BinOp::And, e_binop(e_index(e_var("s"), e_var("si")), BinOp::StrictEq, e_str("0"))), BinOp::And, e_binop(e_binop(e_index(e_var("s"), e_binop(e_var("si"), BinOp::Add, e_int(1))), BinOp::StrictEq, e_str("x")), BinOp::Or, e_binop(e_index(e_var("s"), e_binop(e_var("si"), BinOp::Add, e_int(1))), BinOp::StrictEq, e_str("X")))), BinOp::And, e_binop(e_call("__elephc_scanf_digit_value", vec![e_index(e_var("s"), e_binop(e_var("si"), BinOp::Add, e_int(2)))]), BinOp::Lt, e_int(16)))),
            s_if(
                e_binop(e_binop(e_var("conv"), BinOp::StrictEq, e_str("x")), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("X"))),
                vec![
                    s_assign("base", e_int(16)),
                    s_if(
                        e_var("hexPrefix"),
                        vec![
                            s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(2))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![
                (e_binop(e_var("conv"), BinOp::StrictEq, e_str("o")), vec![
                    s_assign("base", e_int(8)),
                ]),
                (e_binop(e_var("conv"), BinOp::StrictEq, e_str("i")), vec![
                    s_if(
                        e_var("hexPrefix"),
                        vec![
                            s_assign("base", e_int(16)),
                            s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(2))),
                        ],
                        vec![
                        (e_binop(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), BinOp::And, e_binop(e_index(e_var("s"), e_var("si")), BinOp::StrictEq, e_str("0"))), vec![
                            s_assign("base", e_int(8)),
                        ]),
                    ],
                        None,
                    ),
                ]),
            ],
                None,
            ),
            s_assign("digits", e_str("")),
            s_while(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), vec![
                s_if(
                    e_binop(e_binop(e_var("width"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_binop(e_var("si"), BinOp::Sub, e_var("start")), BinOp::GtEq, e_var("width"))),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("d", e_call("__elephc_scanf_digit_value", vec![e_index(e_var("s"), e_var("si"))])),
                s_if(
                    e_binop(e_var("d"), BinOp::GtEq, e_var("base")),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("digits", e_binop(e_var("digits"), BinOp::Concat, e_index(e_var("s"), e_var("si")))),
                s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
            ]),
            s_if(
                e_binop(e_var("digits"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_array(vec![e_var("si"), e_bool(false), e_int(0)])),
                ],
                vec![],
                None,
            ),
            s_assign("magnitude", e_int(0)),
            s_assign("overflow", e_bool(false)),
            s_assign("i", e_int(0)),
            s_assign("len", e_call("strlen", vec![e_var("digits")])),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_var("len")), vec![
                s_assign("d", e_call("__elephc_scanf_digit_value", vec![e_index(e_var("digits"), e_var("i"))])),
                s_if(
                    e_binop(e_var("magnitude"), BinOp::Gt, e_call("intdiv", vec![e_binop(e_int(9223372036854775807), BinOp::Sub, e_var("d")), e_var("base")])),
                    vec![
                        s_assign("overflow", e_bool(true)),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("magnitude", e_binop(e_binop(e_var("magnitude"), BinOp::Mul, e_var("base")), BinOp::Add, e_var("d"))),
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
            ]),
            s_if(
                e_binop(e_var("conv"), BinOp::StrictEq, e_str("u")),
                vec![
                    s_return(e_array(vec![e_var("si"), e_bool(true), e_call("__elephc_scanf_unsigned", vec![e_var("digits"), e_var("sign")])])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("overflow"),
                vec![
                    s_return(e_array(vec![e_var("si"), e_bool(true), e_ternary(e_binop(e_var("sign"), BinOp::Lt, e_int(0)), e_int(-9223372036854775808), e_int(9223372036854775807))])),
                ],
                vec![],
                None,
            ),
            s_return(e_array(vec![e_var("si"), e_bool(true), e_binop(e_var("sign"), BinOp::Mul, e_var("magnitude"))])),
        ])
        .build()
}

/// `__elephc_scanf_float` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_float() -> Stmt {
    function("__elephc_scanf_float")
        .param("s", TypeExpr::Str)
        .param("si", TypeExpr::Int)
        .param("sl", TypeExpr::Int)
        .param("width", TypeExpr::Int)
        .returns(t_array())
        .body(vec![
            s_assign("start", e_var("si")),
            s_assign("text", e_str("")),
            s_assign("best", e_str("")),
            s_assign("bestEnd", e_var("si")),
            s_assign("seenDigit", e_bool(false)),
            s_assign("seenDot", e_bool(false)),
            s_assign("seenExp", e_bool(false)),
            s_while(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), vec![
                s_if(
                    e_binop(e_binop(e_var("width"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_binop(e_var("si"), BinOp::Sub, e_var("start")), BinOp::GtEq, e_var("width"))),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("c", e_index(e_var("s"), e_var("si"))),
                s_assign("o", e_call("ord", vec![e_var("c")])),
                s_if(
                    e_binop(e_binop(e_var("o"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("o"), BinOp::LtEq, e_int(57))),
                    vec![
                        s_assign("seenDigit", e_bool(true)),
                        s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_var("c"))),
                        s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        s_assign("best", e_var("text")),
                        s_assign("bestEnd", e_var("si")),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(".")), BinOp::And, e_not(e_var("seenDot"))), BinOp::And, e_not(e_var("seenExp"))),
                    vec![
                        s_assign("seenDot", e_bool(true)),
                        s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_var("c"))),
                        s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        s_if(
                            e_var("seenDigit"),
                            vec![
                                s_assign("best", e_var("text")),
                                s_assign("bestEnd", e_var("si")),
                            ],
                            vec![],
                            None,
                        ),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("e")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("E"))), BinOp::And, e_var("seenDigit")), BinOp::And, e_not(e_var("seenExp"))),
                    vec![
                        s_assign("seenExp", e_bool(true)),
                        s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_var("c"))),
                        s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("tail", e_ternary(e_binop(e_var("text"), BinOp::StrictEq, e_str("")), e_str(""), e_call("substr", vec![e_var("text"), e_neg(e_int(1))]))),
                s_if(
                    e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("-")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("+"))), BinOp::And, e_binop(e_binop(e_binop(e_var("text"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("tail"), BinOp::StrictEq, e_str("e"))), BinOp::Or, e_binop(e_var("tail"), BinOp::StrictEq, e_str("E")))),
                    vec![
                        s_assign("text", e_binop(e_var("text"), BinOp::Concat, e_var("c"))),
                        s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_break(1),
            ]),
            s_if(
                e_binop(e_var("best"), BinOp::StrictEq, e_str("")),
                vec![
                    s_return(e_array(vec![e_var("si"), e_bool(false), e_float(0.0)])),
                ],
                vec![],
                None,
            ),
            s_return(e_array(vec![e_var("bestEnd"), e_bool(true), e_cast(CastType::Float, e_var("best"))])),
        ])
        .build()
}

/// `__elephc_scanf_class_members` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_class_members() -> Stmt {
    function("__elephc_scanf_class_members")
        .param("body", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("members", e_str("")),
            s_assign("i", e_int(0)),
            s_assign("len", e_call("strlen", vec![e_var("body")])),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_var("len")), vec![
                s_assign("c", e_index(e_var("body"), e_var("i"))),
                s_if(
                    e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("-")), BinOp::And, e_binop(e_var("i"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("len"))),
                    vec![
                        s_assign("from", e_call("ord", vec![e_index(e_var("body"), e_binop(e_var("i"), BinOp::Sub, e_int(1)))])),
                        s_assign("to", e_call("ord", vec![e_index(e_var("body"), e_binop(e_var("i"), BinOp::Add, e_int(1)))])),
                        s_if(
                            e_binop(e_var("to"), BinOp::GtEq, e_var("from")),
                            vec![
                                s_assign("k", e_var("from")),
                                s_while(e_binop(e_var("k"), BinOp::LtEq, e_var("to")), vec![
                                    s_assign("members", e_binop(e_var("members"), BinOp::Concat, e_call("chr", vec![e_var("k")]))),
                                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                                ]),
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(2))),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_assign("members", e_binop(e_var("members"), BinOp::Concat, e_var("c"))),
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
            ]),
            s_return(e_var("members")),
        ])
        .build()
}

/// `__elephc_scanf_is_conversion` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_is_conversion() -> Stmt {
    function("__elephc_scanf_is_conversion")
        .param("conv", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("conv"), BinOp::StrictEq, e_str("c")), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("d"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("D"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("e"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("E"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("f"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("g"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("i"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("n"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("o"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("s"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("u"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("x"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("X")))),
        ])
        .build()
}

/// `__elephc_scanf_ref` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_ref() -> Stmt {
    function("__elephc_scanf_ref")
        .param("s", TypeExpr::Str)
        .param("fmt", TypeExpr::Str)
        .returns(t_array())
        .body(vec![
            s_assign("sl", e_call("strlen", vec![e_var("s")])),
            s_assign("fl", e_call("strlen", vec![e_var("fmt")])),
            s_assign("si", e_int(0)),
            s_assign("fi", e_int(0)),
            s_assign("values", e_array(vec![])),
            s_assign("assigned", e_int(0)),
            s_assign("eof", e_bool(false)),
            s_assign("stop", e_bool(false)),
            s_while(e_binop(e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_not(e_var("stop"))), BinOp::And, e_not(e_var("eof"))), vec![
                s_assign("fc", e_index(e_var("fmt"), e_var("fi"))),
                s_if(
                    e_call("__elephc_scanf_is_space", vec![e_var("fc")]),
                    vec![
                        s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                        s_while(e_binop(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), BinOp::And, e_call("__elephc_scanf_is_space", vec![e_index(e_var("s"), e_var("si"))])), vec![
                            s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        ]),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("fc"), BinOp::StrictNotEq, e_str("%")),
                    vec![
                        s_if(
                            e_binop(e_var("si"), BinOp::GtEq, e_var("sl")),
                            vec![
                                s_assign("eof", e_bool(true)),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_index(e_var("s"), e_var("si")), BinOp::StrictNotEq, e_var("fc")),
                            vec![
                                s_assign("stop", e_bool(true)),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                s_assign("suppress", e_bool(false)),
                s_if(
                    e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("*"))),
                    vec![
                        s_assign("suppress", e_bool(true)),
                        s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                    ],
                    vec![],
                    None,
                ),
                s_assign("width", e_int(0)),
                s_while(e_binop(e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("fmt"), e_var("fi"))]), BinOp::GtEq, e_int(48))), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("fmt"), e_var("fi"))]), BinOp::LtEq, e_int(57))), vec![
                    s_assign("width", e_binop(e_binop(e_var("width"), BinOp::Mul, e_int(10)), BinOp::Add, e_binop(e_call("ord", vec![e_index(e_var("fmt"), e_var("fi"))]), BinOp::Sub, e_int(48)))),
                    s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                ]),
                s_if(
                    e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_binop(e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("l")), BinOp::Or, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("h"))), BinOp::Or, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("L")))),
                    vec![
                        s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                    ],
                    vec![],
                    None,
                ),
                s_assign("conv", e_ternary(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), e_index(e_var("fmt"), e_var("fi")), e_str("\0"))),
                s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                s_if(
                    e_binop(e_var("conv"), BinOp::StrictEq, e_str("%")),
                    vec![
                        s_if(
                            e_binop(e_var("si"), BinOp::GtEq, e_var("sl")),
                            vec![
                                s_assign("eof", e_bool(true)),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_index(e_var("s"), e_var("si")), BinOp::StrictNotEq, e_str("%")),
                            vec![
                                s_assign("stop", e_bool(true)),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("class", e_str("")),
                s_assign("negated", e_bool(false)),
                s_if(
                    e_binop(e_var("conv"), BinOp::StrictEq, e_str("[")),
                    vec![
                        s_assign("body", e_str("")),
                        s_if(
                            e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("^"))),
                            vec![
                                s_assign("negated", e_bool(true)),
                                s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("]"))),
                            vec![
                                s_assign("body", e_str("]")),
                                s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("closed", e_bool(false)),
                        s_while(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), vec![
                            s_if(
                                e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("]")),
                                vec![
                                    s_assign("closed", e_bool(true)),
                                    s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("body", e_binop(e_var("body"), BinOp::Concat, e_index(e_var("fmt"), e_var("fi")))),
                            s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                        ]),
                        s_if(
                            e_not(e_var("closed")),
                            vec![
                                s_throw(e_new_fq("ValueError", vec![e_str("Unmatched [ in format string")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("class", e_call("__elephc_scanf_class_members", vec![e_var("body")])),
                    ],
                    vec![
                    (e_not(e_call("__elephc_scanf_is_conversion", vec![e_var("conv")])), vec![
                        s_throw(e_new_fq("ValueError", vec![e_binop(e_binop(e_str("Bad scan conversion character \""), BinOp::Concat, e_var("conv")), BinOp::Concat, e_str("\""))])),
                    ]),
                ],
                    None,
                ),
                s_if(
                    e_binop(e_var("conv"), BinOp::StrictEq, e_str("n")),
                    vec![
                        s_assign("assigned", e_binop(e_var("assigned"), BinOp::Add, e_int(1))),
                        s_if(
                            e_not(e_var("suppress")),
                            vec![
                                s_array_push("values", e_var("si")),
                            ],
                            vec![],
                            None,
                        ),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_var("conv"), BinOp::StrictNotEq, e_str("c")), BinOp::And, e_binop(e_var("conv"), BinOp::StrictNotEq, e_str("["))),
                    vec![
                        s_while(e_binop(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), BinOp::And, e_call("__elephc_scanf_is_space", vec![e_index(e_var("s"), e_var("si"))])), vec![
                            s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        ]),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("si"), BinOp::GtEq, e_var("sl")),
                    vec![
                        s_assign("eof", e_bool(true)),
                        s_if(
                            e_not(e_var("suppress")),
                            vec![
                                s_array_push("values", e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("ok", e_bool(false)),
                s_assign("value", e_null()),
                s_if(
                    e_binop(e_var("conv"), BinOp::StrictEq, e_str("s")),
                    vec![
                        s_assign("start", e_var("si")),
                        s_while(e_binop(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), BinOp::And, e_not(e_call("__elephc_scanf_is_space", vec![e_index(e_var("s"), e_var("si"))]))), vec![
                            s_if(
                                e_binop(e_binop(e_var("width"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_binop(e_var("si"), BinOp::Sub, e_var("start")), BinOp::GtEq, e_var("width"))),
                                vec![
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        ]),
                        s_assign("ok", e_binop(e_var("si"), BinOp::Gt, e_var("start"))),
                        s_assign("value", e_call("substr", vec![e_var("s"), e_var("start"), e_binop(e_var("si"), BinOp::Sub, e_var("start"))])),
                    ],
                    vec![
                    (e_binop(e_var("conv"), BinOp::StrictEq, e_str("c")), vec![
                        s_assign("take", e_ternary(e_binop(e_var("width"), BinOp::Gt, e_int(0)), e_var("width"), e_int(1))),
                        s_assign("start", e_var("si")),
                        s_while(e_binop(e_binop(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), BinOp::And, e_not(e_call("__elephc_scanf_is_space", vec![e_index(e_var("s"), e_var("si"))]))), BinOp::And, e_binop(e_binop(e_var("si"), BinOp::Sub, e_var("start")), BinOp::Lt, e_var("take"))), vec![
                            s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        ]),
                        s_assign("ok", e_bool(true)),
                        s_assign("value", e_call("substr", vec![e_var("s"), e_var("start"), e_binop(e_var("si"), BinOp::Sub, e_var("start"))])),
                    ]),
                    (e_binop(e_var("conv"), BinOp::StrictEq, e_str("[")), vec![
                        s_assign("start", e_var("si")),
                        s_while(e_binop(e_var("si"), BinOp::Lt, e_var("sl")), vec![
                            s_if(
                                e_binop(e_binop(e_var("width"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_binop(e_var("si"), BinOp::Sub, e_var("start")), BinOp::GtEq, e_var("width"))),
                                vec![
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("inside", e_binop(e_call("strpos", vec![e_var("class"), e_index(e_var("s"), e_var("si"))]), BinOp::StrictNotEq, e_bool(false))),
                            s_if(
                                e_var("negated"),
                                vec![
                                    s_assign("inside", e_not(e_var("inside"))),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_not(e_var("inside")),
                                vec![
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("si", e_binop(e_var("si"), BinOp::Add, e_int(1))),
                        ]),
                        s_assign("ok", e_binop(e_var("si"), BinOp::Gt, e_var("start"))),
                        s_assign("value", e_call("substr", vec![e_var("s"), e_var("start"), e_binop(e_var("si"), BinOp::Sub, e_var("start"))])),
                    ]),
                    (e_binop(e_binop(e_binop(e_binop(e_var("conv"), BinOp::StrictEq, e_str("e")), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("E"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("f"))), BinOp::Or, e_binop(e_var("conv"), BinOp::StrictEq, e_str("g"))), vec![
                        s_assign("r", e_call("__elephc_scanf_float", vec![e_var("s"), e_var("si"), e_var("sl"), e_var("width")])),
                        s_assign("si", e_index(e_var("r"), e_int(0))),
                        s_assign("ok", e_index(e_var("r"), e_int(1))),
                        s_assign("value", e_index(e_var("r"), e_int(2))),
                    ]),
                ],
                    Some(vec![
                    s_assign("r", e_call("__elephc_scanf_int", vec![e_var("s"), e_var("si"), e_var("sl"), e_var("width"), e_var("conv")])),
                    s_assign("si", e_index(e_var("r"), e_int(0))),
                    s_assign("ok", e_index(e_var("r"), e_int(1))),
                    s_assign("value", e_index(e_var("r"), e_int(2))),
                ]),
                ),
                s_if(
                    e_not(e_var("ok")),
                    vec![
                        s_if(
                            e_binop(e_var("si"), BinOp::GtEq, e_var("sl")),
                            vec![
                                s_assign("eof", e_bool(true)),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("stop", e_bool(true)),
                        ]),
                        ),
                        s_if(
                            e_not(e_var("suppress")),
                            vec![
                                s_array_push("values", e_null()),
                            ],
                            vec![],
                            None,
                        ),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("assigned", e_binop(e_var("assigned"), BinOp::Add, e_int(1))),
                s_if(
                    e_not(e_var("suppress")),
                    vec![
                        s_array_push("values", e_var("value")),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_while(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), vec![
                s_assign("fc", e_index(e_var("fmt"), e_var("fi"))),
                s_if(
                    e_binop(e_var("fc"), BinOp::StrictNotEq, e_str("%")),
                    vec![
                        s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                s_assign("suppress", e_bool(false)),
                s_if(
                    e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("*"))),
                    vec![
                        s_assign("suppress", e_bool(true)),
                        s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                    ],
                    vec![],
                    None,
                ),
                s_while(e_binop(e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("fmt"), e_var("fi"))]), BinOp::GtEq, e_int(48))), BinOp::And, e_binop(e_call("ord", vec![e_index(e_var("fmt"), e_var("fi"))]), BinOp::LtEq, e_int(57))), vec![
                    s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                ]),
                s_if(
                    e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_binop(e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("l")), BinOp::Or, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("h"))), BinOp::Or, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("L")))),
                    vec![
                        s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                    ],
                    vec![],
                    None,
                ),
                s_assign("conv", e_ternary(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), e_index(e_var("fmt"), e_var("fi")), e_str("\0"))),
                s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                s_if(
                    e_binop(e_var("conv"), BinOp::StrictEq, e_str("%")),
                    vec![
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("conv"), BinOp::StrictEq, e_str("[")),
                    vec![
                        s_if(
                            e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("^"))),
                            vec![
                                s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), BinOp::And, e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("]"))),
                            vec![
                                s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("closed", e_bool(false)),
                        s_while(e_binop(e_var("fi"), BinOp::Lt, e_var("fl")), vec![
                            s_if(
                                e_binop(e_index(e_var("fmt"), e_var("fi")), BinOp::StrictEq, e_str("]")),
                                vec![
                                    s_assign("closed", e_bool(true)),
                                    s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("fi", e_binop(e_var("fi"), BinOp::Add, e_int(1))),
                        ]),
                        s_if(
                            e_not(e_var("closed")),
                            vec![
                                s_throw(e_new_fq("ValueError", vec![e_str("Unmatched [ in format string")])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![
                    (e_not(e_call("__elephc_scanf_is_conversion", vec![e_var("conv")])), vec![
                        s_throw(e_new_fq("ValueError", vec![e_binop(e_binop(e_str("Bad scan conversion character \""), BinOp::Concat, e_var("conv")), BinOp::Concat, e_str("\""))])),
                    ]),
                ],
                    None,
                ),
                s_if(
                    e_not(e_var("suppress")),
                    vec![
                        s_array_push("values", e_null()),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_assign("exhausted", e_binop(e_var("eof"), BinOp::And, e_binop(e_var("assigned"), BinOp::StrictEq, e_int(0)))),
            s_return(e_array(vec![e_ternary(e_var("exhausted"), e_neg(e_int(1)), e_var("assigned")), e_var("values"), e_ternary(e_var("exhausted"), e_int(1), e_int(0)), e_call("count", vec![e_var("values")])])),
        ])
        .build()
}

/// `__elephc_scanf` — transcribed from the PHP form.
fn decl_fn_elephc_scanf() -> Stmt {
    function("__elephc_scanf")
        .param("s", TypeExpr::Str)
        .param("fmt", TypeExpr::Str)
        .returns(t_nullable(t_array()))
        .body(vec![
            s_assign("r", e_call("__elephc_scanf_ref", vec![e_var("s"), e_var("fmt")])),
            s_if(
                e_binop(e_index(e_var("r"), e_int(2)), BinOp::StrictEq, e_int(1)),
                vec![
                    s_return(e_null()),
                ],
                vec![],
                None,
            ),
            s_assign("values", e_index(e_var("r"), e_int(1))),
            s_assign("n", e_cast(CastType::Int, e_index(e_var("r"), e_int(3)))),
            s_assign("out", e_array(vec![])),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("n"))), Some(s_expr(e_post_inc("i"))), vec![
                s_array_push("out", e_index(e_var("values"), e_var("i"))),
            ]),
            s_return(e_var("out")),
        ])
        .build()
}

/// `__elephc_fscanf` — transcribed from the PHP form.
fn decl_fn_elephc_fscanf() -> Stmt {
    function("__elephc_fscanf")
        .param("stream", t_mixed())
        .param("format", TypeExpr::Str)
        .returns(t_union(vec![t_array(), TypeExpr::False, TypeExpr::Void]))
        .body(vec![
            s_assign("line", e_call("fgets", vec![e_var("stream")])),
            s_if(
                e_binop(e_var("line"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_scanf", vec![e_var("line"), e_var("format")])),
        ])
        .build()
}

/// `__elephc_scanf_arity` — transcribed from the PHP form.
fn decl_fn_elephc_scanf_arity() -> Stmt {
    function("__elephc_scanf_arity")
        .param("found", TypeExpr::Int)
        .param("wanted", TypeExpr::Int)
        .returns(TypeExpr::Void)
        .body(vec![
            s_if(
                e_binop(e_var("found"), BinOp::Gt, e_var("wanted")),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("Different numbers of variable names and field specifiers")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("found"), BinOp::Lt, e_var("wanted")),
                vec![
                    s_throw(e_new_fq("ValueError", vec![e_str("Variable is not assigned by any conversion specifiers")])),
                ],
                vec![],
                None,
            ),
        ])
        .build()
}


/// The `$vals[N]` → `$tN` → `$vN` pair one by-reference output needs.
///
/// The element goes through an OWNED local before it crosses the reference. Assigning `$vals[$i]`
/// straight to `&$vN` stored a BORROWED pointer into the caller's slot, and the callee's own
/// cleanup then freed the array it pointed into — the caller read freed memory and the program
/// segfaulted on the first use of the variable.
fn wrapper_assignments(count: usize) -> Vec<Stmt> {
    let mut out = Vec::new();
    for index in 0..count {
        out.push(s_assign(
            &format!("t{index}"),
            e_index(e_var("vals"), e_int(index as i64)),
        ));
        out.push(s_assign(&format!("v{index}"), e_var(&format!("t{index}"))));
    }
    out
}

/// Declares the by-reference outputs, one per arity.
///
/// DECLARED `mixed`, not left untyped. An untyped by-reference parameter takes its type from the
/// call site, and a prelude function is resolved before any call site is seen — so it fell back to
/// the `int` placeholder, the caller handed over a Mixed cell pointer, and the callee wrote an int
/// through it. The declaration is what pins the two together.
fn with_ref_params(mut builder: FunctionBuilder, count: usize) -> FunctionBuilder {
    for index in 0..count {
        builder = builder.param_by_ref(&format!("v{index}"), Some(t_mixed()));
    }
    builder
}

/// `__elephc_scanf_vars_<count>` — the string form, one per arity.
fn decl_fn_scanf_vars(count: usize) -> Stmt {
    let mut body = vec![
        s_assign("r", e_call("__elephc_scanf_ref", vec![e_var("s"), e_var("fmt")])),
        s_assign("vals", e_index(e_var("r"), e_int(1))),
        s_expr(e_call(
            "__elephc_scanf_arity",
            vec![
                e_cast(CastType::Int, e_index(e_var("r"), e_int(3))),
                e_int(count as i64),
            ],
        )),
    ];
    body.extend(wrapper_assignments(count));
    body.push(s_return(e_cast(CastType::Int, e_index(e_var("r"), e_int(0)))));
    with_ref_params(
        function(&format!("__elephc_scanf_vars_{count}"))
            .param("s", TypeExpr::Str)
            .param("fmt", TypeExpr::Str),
        count,
    )
    .returns(TypeExpr::Int)
    .body(body)
    .build()
}

/// `__elephc_fscanf_vars_<count>` — the stream form, one per arity.
///
/// It answers `int|false` rather than `int`: php's `fscanf()` reports `false` for a stream already
/// at end of file, which is the arm that terminates a read loop.
fn decl_fn_fscanf_vars(count: usize) -> Stmt {
    let mut body = vec![
        s_assign("line", e_call("fgets", vec![e_var("stream")])),
        s_if(
            e_binop(e_var("line"), BinOp::StrictEq, e_bool(false)),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_assign("r", e_call("__elephc_scanf_ref", vec![e_var("line"), e_var("fmt")])),
        s_assign("vals", e_index(e_var("r"), e_int(1))),
        s_expr(e_call(
            "__elephc_scanf_arity",
            vec![
                e_cast(CastType::Int, e_index(e_var("r"), e_int(3))),
                e_int(count as i64),
            ],
        )),
    ];
    body.extend(wrapper_assignments(count));
    body.push(s_return(e_cast(CastType::Int, e_index(e_var("r"), e_int(0)))));
    with_ref_params(
        function(&format!("__elephc_fscanf_vars_{count}"))
            .param("stream", t_mixed())
            .param("fmt", TypeExpr::Str),
        count,
    )
    .returns(t_union(vec![TypeExpr::Int, TypeExpr::False]))
    .body(body)
    .build()
}

/// Builds the engine and every arity wrapper, in the order the PHP declared them.
pub(crate) fn scanf_declarations() -> Program {
    internal_declarations(|| {
        let mut out = vec![
            decl_fn_elephc_scanf_is_space(),
            decl_fn_elephc_scanf_digit_value(),
            decl_fn_elephc_scanf_unsigned_negative(),
            decl_fn_elephc_scanf_unsigned(),
            decl_fn_elephc_scanf_int(),
            decl_fn_elephc_scanf_float(),
            decl_fn_elephc_scanf_class_members(),
            decl_fn_elephc_scanf_is_conversion(),
            decl_fn_elephc_scanf_ref(),
            decl_fn_elephc_scanf(),
            decl_fn_elephc_fscanf(),
            decl_fn_elephc_scanf_arity(),
        ];
        for count in 1..=crate::scanf_prelude::SCANF_MAX_VARS {
            out.push(decl_fn_scanf_vars(count));
            out.push(decl_fn_fscanf_vars(count));
        }
        out
    })
}
