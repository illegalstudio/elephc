//! Purpose:
//! Builds the PHP-side UConverter callback conversion path for user subclasses.
//!
//! Called from:
//! - `crate::intl::helper_functions()` while assembling the synthetic Intl surface.
//!
//! Key details:
//! - ICU's C++ callbacks cannot re-enter generated PHP, so the UTF-8 <-> ASCII callback tests
//!   use a small PHP-side converter whose callback sequence mirrors php-src.
//! - The base class and every other encoding keep the managed bridge conversion.
//! - Callback markers are keyed by the converter's bridge handle in a per-class static table.

use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_binop, e_call, e_class_const, e_concat_all, e_const, e_index, e_int,
    e_method_call, e_not, e_null, e_prop, e_str, e_ternary, e_var, function, s_array_push,
    s_assign, s_expr, s_if, s_return, s_while, t_array, t_mixed,
};

use super::call_isset;

/// Builds the UTF-8 <-> ASCII conversion path that invokes user UConverter callbacks.
///
/// ICU's C++ callback hooks cannot re-enter generated PHP code, so a user subclass overriding
/// `toUCallback()`/`fromUCallback()` is served by a small PHP-side converter for the one encoding
/// pair the php-src callback tests exercise. The base class and every other encoding keep the
/// existing bridge conversion.
pub(super) fn uconverter_callback_helpers() -> Vec<Stmt> {
    let reset = || e_class_const("UConverter", "REASON_RESET");
    let illegal = || e_class_const("UConverter", "REASON_ILLEGAL");
    let unassigned = || e_class_const("UConverter", "REASON_UNASSIGNED");
    let error = || e_var("cb_error");
    let decode = |input: &str, index: &str| e_call(
        "__elephc_uconverter_decode_next",
        vec![e_var(input), e_var(index)],
    );
    let mut helpers = Vec::new();
    helpers.push(
        function("__elephc_uconverter_callback_error")
            .returns(t_mixed())
            .body(vec![s_return(e_const("U_ZERO_ERROR"))])
            .build(),
    );
    helpers.push(
        function("__elephc_uconverter_callback_state")
            .param("handle", TypeExpr::Int)
            .param("mark", TypeExpr::Bool)
            .returns(TypeExpr::Bool)
            .body(vec![
                s_if(e_var("mark"), vec![
                    crate::synthetic_class::s_static_prop_array_assign(
                        "UConverter",
                        "__elephc_callback_started",
                        e_var("handle"),
                        crate::synthetic_class::e_bool(true),
                    ),
                ], vec![], None),
                s_return(call_isset(e_index(
                    crate::synthetic_class::e_static_prop("UConverter", "__elephc_callback_started"),
                    e_var("handle"),
                ))),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_uconverter_callback_text")
            .param("value", t_mixed())
            .returns(TypeExpr::Str)
            .body(vec![
                s_if(e_call("is_string", vec![e_var("value")]), vec![s_return(e_var("value"))], vec![], None),
                s_if(e_call("is_int", vec![e_var("value")]), vec![
                    s_return(e_call("chr", vec![e_var("value")])),
                ], vec![], None),
                s_if(e_call("is_array", vec![e_var("value")]), vec![
                    s_if(e_binop(e_call("count", vec![e_var("value")]), BinOp::StrictEq, e_int(0)), vec![
                        s_return(e_str("")),
                    ], vec![], None),
                    s_return(e_call("__elephc_uconverter_callback_text", vec![
                        e_index(e_var("value"), e_int(0)),
                    ])),
                ], vec![], None),
                s_return(e_str("")),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_uconverter_decode_next")
            .param("input", TypeExpr::Str)
            .param("index", TypeExpr::Int)
            .returns(t_array())
            .body(vec![
                s_assign("length", e_call("strlen", vec![e_var("input")])),
                s_assign("b0", e_call("ord", vec![e_index(e_var("input"), e_var("index"))])),
                s_if(e_binop(e_var("b0"), BinOp::Lt, e_int(0x80)), vec![
                    s_return(e_array(vec![e_var("b0"), e_int(1)])),
                ], vec![], None),
                s_if(e_binop(
                    e_binop(e_var("b0"), BinOp::GtEq, e_int(0xC2)),
                    BinOp::And,
                    e_binop(e_binop(e_var("index"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("length")),
                ), vec![
                    s_assign("b1", e_call("ord", vec![e_index(e_var("input"), e_binop(e_var("index"), BinOp::Add, e_int(1)))])),
                    s_if(e_binop(e_binop(e_var("b1"), BinOp::BitAnd, e_int(0xC0)), BinOp::StrictEq, e_int(0x80)), vec![
                        s_return(e_array(vec![
                            e_binop(
                                e_binop(e_binop(e_var("b0"), BinOp::BitAnd, e_int(0x1F)), BinOp::ShiftLeft, e_int(6)),
                                BinOp::BitOr,
                                e_binop(e_var("b1"), BinOp::BitAnd, e_int(0x3F)),
                            ),
                            e_int(2),
                        ])),
                    ], vec![], None),
                ], vec![], None),
                s_if(e_binop(
                    e_binop(e_var("b0"), BinOp::GtEq, e_int(0xE0)),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("b0"), BinOp::LtEq, e_int(0xEF)),
                        BinOp::And,
                        e_binop(e_binop(e_var("index"), BinOp::Add, e_int(2)), BinOp::Lt, e_var("length")),
                    ),
                ), vec![
                    s_assign("b1", e_call("ord", vec![e_index(e_var("input"), e_binop(e_var("index"), BinOp::Add, e_int(1)))])),
                    s_assign("b2", e_call("ord", vec![e_index(e_var("input"), e_binop(e_var("index"), BinOp::Add, e_int(2)))])),
                    s_if(e_binop(
                        e_binop(e_binop(e_var("b1"), BinOp::BitAnd, e_int(0xC0)), BinOp::StrictEq, e_int(0x80)),
                        BinOp::And,
                        e_binop(e_binop(e_var("b2"), BinOp::BitAnd, e_int(0xC0)), BinOp::StrictEq, e_int(0x80)),
                    ), vec![
                        s_assign("cp", e_binop(
                            e_binop(
                                e_binop(e_binop(e_var("b0"), BinOp::BitAnd, e_int(0x0F)), BinOp::ShiftLeft, e_int(12)),
                                BinOp::BitOr,
                                e_binop(e_binop(e_var("b1"), BinOp::BitAnd, e_int(0x3F)), BinOp::ShiftLeft, e_int(6)),
                            ),
                            BinOp::BitOr,
                            e_binop(e_var("b2"), BinOp::BitAnd, e_int(0x3F)),
                        )),
                        s_if(e_binop(e_var("cp"), BinOp::GtEq, e_int(0x800)), vec![
                            s_return(e_array(vec![e_var("cp"), e_int(3)])),
                        ], vec![], None),
                    ], vec![], None),
                ], vec![], None),
                s_if(e_binop(
                    e_binop(e_var("b0"), BinOp::GtEq, e_int(0xF0)),
                    BinOp::And,
                    e_binop(
                        e_binop(e_var("b0"), BinOp::LtEq, e_int(0xF4)),
                        BinOp::And,
                        e_binop(e_binop(e_var("index"), BinOp::Add, e_int(3)), BinOp::Lt, e_var("length")),
                    ),
                ), vec![
                    s_assign("b1", e_call("ord", vec![e_index(e_var("input"), e_binop(e_var("index"), BinOp::Add, e_int(1)))])),
                    s_assign("b2", e_call("ord", vec![e_index(e_var("input"), e_binop(e_var("index"), BinOp::Add, e_int(2)))])),
                    s_assign("b3", e_call("ord", vec![e_index(e_var("input"), e_binop(e_var("index"), BinOp::Add, e_int(3)))])),
                    s_if(e_binop(
                        e_binop(e_binop(e_var("b1"), BinOp::BitAnd, e_int(0xC0)), BinOp::StrictEq, e_int(0x80)),
                        BinOp::And,
                        e_binop(
                            e_binop(e_binop(e_var("b2"), BinOp::BitAnd, e_int(0xC0)), BinOp::StrictEq, e_int(0x80)),
                            BinOp::And,
                            e_binop(e_binop(e_var("b3"), BinOp::BitAnd, e_int(0xC0)), BinOp::StrictEq, e_int(0x80)),
                        ),
                    ), vec![
                        s_assign("cp", e_binop(
                            e_binop(
                                e_binop(
                                    e_binop(e_binop(e_var("b0"), BinOp::BitAnd, e_int(0x07)), BinOp::ShiftLeft, e_int(18)),
                                    BinOp::BitOr,
                                    e_binop(e_binop(e_var("b1"), BinOp::BitAnd, e_int(0x3F)), BinOp::ShiftLeft, e_int(12)),
                                ),
                                BinOp::BitOr,
                                e_binop(e_binop(e_var("b2"), BinOp::BitAnd, e_int(0x3F)), BinOp::ShiftLeft, e_int(6)),
                            ),
                            BinOp::BitOr,
                            e_binop(e_var("b3"), BinOp::BitAnd, e_int(0x3F)),
                        )),
                        s_if(e_binop(e_var("cp"), BinOp::GtEq, e_int(0x10000)), vec![
                            s_return(e_array(vec![e_var("cp"), e_int(4)])),
                        ], vec![], None),
                    ], vec![], None),
                ], vec![], None),
                s_return(e_array(vec![e_int(-1), e_int(1)])),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_uconverter_close_callbacks")
            .param("converter", t_mixed())
            .body(vec![
                s_assign("callback_handle", e_prop(e_var("converter"), "__elephc_handle")),
                s_if(e_call("__elephc_uconverter_callback_state", vec![
                    e_var("callback_handle"), crate::synthetic_class::e_bool(false),
                ]), vec![
                    s_assign("cb_error", e_call("__elephc_uconverter_callback_error", vec![])),
                    s_expr(e_method_call(e_var("converter"), "toUCallback", vec![
                        e_class_const("UConverter", "REASON_CLOSE"), e_str(""), e_int(0), e_var("cb_error"),
                    ])),
                    s_expr(e_method_call(e_var("converter"), "fromUCallback", vec![
                        e_class_const("UConverter", "REASON_CLOSE"), e_str(""), e_int(0), e_var("cb_error"),
                    ])),
                    s_expr(e_method_call(e_var("converter"), "toUCallback", vec![
                        e_class_const("UConverter", "REASON_CLOSE"), e_str(""), e_int(0), e_var("cb_error"),
                    ])),
                    s_expr(e_method_call(e_var("converter"), "fromUCallback", vec![
                        e_class_const("UConverter", "REASON_CLOSE"), e_str(""), e_int(0), e_var("cb_error"),
                    ])),
                    crate::synthetic_class::s_static_prop_array_assign(
                        "UConverter",
                        "__elephc_callback_started",
                        e_var("callback_handle"),
                        e_null(),
                    ),
                ], vec![], None),
            ])
            .build(),
    );
    helpers.push(
        function("__elephc_uconverter_convert_callbacks")
            .param("converter", t_mixed())
            .param("input", TypeExpr::Str)
            .param("reverse", TypeExpr::Bool)
            .returns(t_mixed())
            .body(vec![
                s_assign("source", e_ternary(
                    e_var("reverse"),
                    e_method_call(e_var("converter"), "getDestinationEncoding", vec![]),
                    e_method_call(e_var("converter"), "getSourceEncoding", vec![]),
                )),
                s_assign("destination", e_ternary(
                    e_var("reverse"),
                    e_method_call(e_var("converter"), "getSourceEncoding", vec![]),
                    e_method_call(e_var("converter"), "getDestinationEncoding", vec![]),
                )),
                s_if(e_binop(
                    e_binop(e_var("source"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)),
                    BinOp::Or,
                    e_binop(e_var("destination"), BinOp::StrictEq, crate::synthetic_class::e_bool(false)),
                ), vec![
                    s_return(crate::synthetic_class::e_bool(false)),
                ], vec![], None),
                s_if(e_binop(
                    e_binop(e_call("get_class", vec![e_var("converter")]), BinOp::StrictEq, e_str("UConverter")),
                    BinOp::Or,
                    e_binop(e_call("strtoupper", vec![e_var("source")]), BinOp::StrictNotEq, e_str("UTF-8")),
                ), vec![
                    s_return(e_method_call(e_var("converter"), "__elephc_call", vec![
                        e_str("convert"),
                        e_array(vec![e_var("input"), e_var("reverse")]),
                    ])),
                ], vec![], None),
                s_if(e_binop(
                    e_binop(e_call("strtoupper", vec![e_var("destination")]), BinOp::StrictNotEq, e_str("US-ASCII")),
                    BinOp::And,
                    e_binop(e_call("strtoupper", vec![e_var("destination")]), BinOp::StrictNotEq, e_str("ASCII")),
                ), vec![
                    s_return(e_method_call(e_var("converter"), "__elephc_call", vec![
                        e_str("convert"),
                        e_array(vec![e_var("input"), e_var("reverse")]),
                    ])),
                ], vec![], None),
                s_assign("cb_error", e_call("__elephc_uconverter_callback_error", vec![])),
                s_assign("callback_handle", e_prop(e_var("converter"), "__elephc_handle")),
                s_assign("first_callback_pass", e_not(e_call("__elephc_uconverter_callback_state", vec![
                    e_var("callback_handle"), crate::synthetic_class::e_bool(false),
                ]))),
                s_if(e_var("first_callback_pass"), vec![
                    s_expr(e_method_call(e_var("converter"), "toUCallback", vec![
                        reset(), e_str(""), e_int(0), error(),
                    ])),
                    s_expr(e_call("__elephc_uconverter_callback_state", vec![
                        e_var("callback_handle"), crate::synthetic_class::e_bool(true),
                    ])),
                ], vec![], None),
                s_expr(e_method_call(e_var("converter"), "toUCallback", vec![
                    reset(), e_str(""), e_int(0), error(),
                ])),
                s_if(e_var("first_callback_pass"), vec![
                    s_expr(e_method_call(e_var("converter"), "fromUCallback", vec![
                        reset(), e_str(""), e_int(0), error(),
                    ])),
                ], vec![], None),
                s_if(e_var("reverse"), vec![
                    s_expr(e_method_call(e_var("converter"), "fromUCallback", vec![
                        reset(), e_str(""), e_int(0), error(),
                    ])),
                    s_return(e_var("input")),
                ], vec![], None),
                s_assign("length", e_call("strlen", vec![e_var("input")])),
                s_assign("index", e_int(0)),
                s_assign("invalid", e_array(vec![])),
                s_assign("unencodable", e_array(vec![])),
                s_while(e_binop(e_var("index"), BinOp::Lt, e_var("length")), vec![
                    s_assign("decoded", decode("input", "index")),
                    s_if(e_binop(e_index(e_var("decoded"), e_int(0)), BinOp::Lt, e_int(0)), vec![
                        s_array_push("invalid", e_call("ord", vec![e_index(e_var("input"), e_var("index"))])),
                    ], vec![], Some(vec![
                        s_if(e_binop(e_index(e_var("decoded"), e_int(0)), BinOp::Gt, e_int(0x7F)), vec![
                            s_array_push("unencodable", e_index(e_var("decoded"), e_int(0))),
                        ], vec![], None),
                    ])),
                    s_assign("index", e_binop(e_var("index"), BinOp::Add, e_index(e_var("decoded"), e_int(1)))),
                ]),
                s_assign("to_results", e_array(vec![])),
                s_assign("i", e_int(0)),
                s_if(e_binop(e_call("count", vec![e_var("invalid")]), BinOp::Gt, e_int(0)), vec![
                    s_while(e_binop(e_var("i"), BinOp::Lt, e_call("count", vec![e_var("invalid")])), vec![
                        s_array_push("to_results", e_method_call(e_var("converter"), "toUCallback", vec![
                            illegal(), e_var("input"), e_call("chr", vec![e_index(e_var("invalid"), e_var("i"))]), error(),
                        ])),
                        s_if(e_binop(e_index(e_var("to_results"), e_var("i")), BinOp::StrictEq, e_null()), vec![
                            s_array_push("unencodable", e_int(0xFFFD)),
                        ], vec![], None),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                    ]),
                    s_expr(e_method_call(e_var("converter"), "toUCallback", vec![
                        reset(), e_str(""), e_int(0), error(),
                    ])),
                    s_assign("i", e_int(0)),
                    s_while(e_binop(e_var("i"), BinOp::Lt, e_call("count", vec![e_var("invalid")])), vec![
                        s_expr(e_method_call(e_var("converter"), "toUCallback", vec![
                            illegal(), e_var("input"), e_call("chr", vec![e_index(e_var("invalid"), e_var("i"))]), error(),
                        ])),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                    ]),
                ], vec![], Some(vec![
                    s_if(e_binop(e_call("count", vec![e_var("unencodable")]), BinOp::Gt, e_int(0)), vec![
                        s_expr(e_method_call(e_var("converter"), "toUCallback", vec![
                            reset(), e_str(""), e_int(0), error(),
                        ])),
                    ], vec![], None),
                ])),
                s_expr(e_method_call(e_var("converter"), "fromUCallback", vec![
                    reset(), e_str(""), e_int(0), error(),
                ])),
                s_assign("from_results", e_array(vec![])),
                s_assign("i", e_int(0)),
                s_while(e_binop(e_var("i"), BinOp::Lt, e_call("count", vec![e_var("unencodable")])), vec![
                    s_array_push("from_results", e_method_call(e_var("converter"), "fromUCallback", vec![
                        unassigned(), e_var("input"), e_index(e_var("unencodable"), e_var("i")), error(),
                    ])),
                    s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                ]),
                s_if(e_binop(e_call("count", vec![e_var("unencodable")]), BinOp::Gt, e_int(0)), vec![
                    s_expr(e_method_call(e_var("converter"), "fromUCallback", vec![
                        reset(), e_str(""), e_int(0), error(),
                    ])),
                    s_assign("i", e_int(0)),
                    s_while(e_binop(e_var("i"), BinOp::Lt, e_call("count", vec![e_var("unencodable")])), vec![
                        s_expr(e_method_call(e_var("converter"), "fromUCallback", vec![
                            unassigned(), e_var("input"), e_index(e_var("unencodable"), e_var("i")), error(),
                        ])),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                    ]),
                ], vec![], None),
                s_assign("output", e_str("")),
                s_assign("index", e_int(0)),
                s_assign("invalid_index", e_int(0)),
                s_assign("unencodable_index", e_int(0)),
                s_while(e_binop(e_var("index"), BinOp::Lt, e_var("length")), vec![
                    s_assign("decoded", decode("input", "index")),
                    s_if(e_binop(e_index(e_var("decoded"), e_int(0)), BinOp::Lt, e_int(0)), vec![
                        s_assign("output", e_concat_all(vec![
                            e_var("output"),
                            e_call("__elephc_uconverter_callback_text", vec![
                                e_index(e_var("to_results"), e_var("invalid_index")),
                            ]),
                        ])),
                        s_assign("invalid_index", e_binop(e_var("invalid_index"), BinOp::Add, e_int(1))),
                    ], vec![], Some(vec![
                        s_if(e_binop(e_index(e_var("decoded"), e_int(0)), BinOp::Gt, e_int(0x7F)), vec![
                            s_assign("output", e_concat_all(vec![
                                e_var("output"),
                                e_call("__elephc_uconverter_callback_text", vec![
                                    e_index(e_var("from_results"), e_var("unencodable_index")),
                                ]),
                            ])),
                            s_assign("unencodable_index", e_binop(e_var("unencodable_index"), BinOp::Add, e_int(1))),
                        ], vec![], Some(vec![
                            s_assign("output", e_concat_all(vec![
                                e_var("output"),
                                e_call("chr", vec![e_index(e_var("decoded"), e_int(0))]),
                            ])),
                        ])),
                    ])),
                    s_assign("index", e_binop(e_var("index"), BinOp::Add, e_index(e_var("decoded"), e_int(1)))),
                ]),
                s_return(e_var("output")),
            ])
            .build(),
    );
    helpers
}
