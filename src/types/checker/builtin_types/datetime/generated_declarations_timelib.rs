//! Purpose:
//! Direct AST declarations for the php-src-compatible DateTime family.
//!
//! Called from:
//! - The DateTime builtin declaration injector.
//!
//! Key details:
//! - Generated in tests from the audited declaration model; production performs no PHP parsing.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `DateTimeInterface::format` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_0_format() -> MethodBuilder {
method("format")
    .param("format", TypeExpr::Str)
    .returns(TypeExpr::Str)
}

/// `DateTimeInterface::getTimestamp` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_1_gettimestamp() -> MethodBuilder {
method("getTimestamp")
    .returns(TypeExpr::Int)
}

/// `DateTimeInterface::getMicrosecond` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_2_getmicrosecond() -> MethodBuilder {
method("getMicrosecond")
    .returns(TypeExpr::Int)
}

/// `DateTimeInterface::getTimezone` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_3_gettimezone() -> MethodBuilder {
method("getTimezone")
    .returns(t_union(vec![t_class("DateTimeZone"), TypeExpr::False]))
}

/// `DateTimeInterface::getOffset` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_4_getoffset() -> MethodBuilder {
method("getOffset")
    .returns(TypeExpr::Int)
}

/// `DateTimeInterface::diff` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_5_diff() -> MethodBuilder {
method("diff")
    .param("targetObject", t_class("DateTimeInterface"))
    .param_default("absolute", TypeExpr::Bool, e_bool(false))
    .returns(t_class("DateInterval"))
}

/// `DateTimeInterface::__wakeup` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_6_wakeup() -> MethodBuilder {
method("__wakeup")
    .attr("\\Deprecated", vec![e_named_arg("since", e_str("8.5")), e_named_arg("message", e_str("this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()"))])
    .returns(TypeExpr::Void)
}

/// `DateTimeInterface::__serialize` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_7_serialize() -> MethodBuilder {
method("__serialize")
    .returns(t_array())
}

/// `DateTimeInterface::__unserialize` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_8_unserialize() -> MethodBuilder {
method("__unserialize")
    .param("data", t_array())
    .returns(TypeExpr::Void)
}

/// `DateTimeInterface::__elephc_debug_dump` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_9_elephc_debug_dump() -> MethodBuilder {
method("__elephc_debug_dump")
    .returns(TypeExpr::Void)
}

/// `DateTimeInterface::__elephc_print_r_dump` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_10_elephc_print_r_dump() -> MethodBuilder {
method("__elephc_print_r_dump")
    .returns(TypeExpr::Void)
}

/// `DateTimeInterface::__elephc_assert_comparable` — transcribed method builder.
fn decl_stmt_bootstrap_1_method_11_elephc_assert_comparable() -> MethodBuilder {
method("__elephc_assert_comparable")
    .returns(TypeExpr::Void)
}

/// `bootstrap 1` — transcribed from the PHP form.
fn decl_stmt_bootstrap_1() -> Stmt {
    interface("DateTimeInterface")
        .constant_full("ATOM", e_str("Y-m-d\\TH:i:sP"), Some(TypeExpr::Str), vec![])
        .constant_full("COOKIE", e_str("l, d-M-Y H:i:s T"), Some(TypeExpr::Str), vec![])
        .constant_full("ISO8601", e_str("Y-m-d\\TH:i:sO"), Some(TypeExpr::Str), vec![])
        .constant_full("ISO8601_EXPANDED", e_str("X-m-d\\TH:i:sP"), Some(TypeExpr::Str), vec![])
        .constant_full("RFC822", e_str("D, d M y H:i:s O"), Some(TypeExpr::Str), vec![])
        .constant_full("RFC850", e_str("l, d-M-y H:i:s T"), Some(TypeExpr::Str), vec![])
        .constant_full("RFC1036", e_str("D, d M y H:i:s O"), Some(TypeExpr::Str), vec![])
        .constant_full("RFC1123", e_str("D, d M Y H:i:s O"), Some(TypeExpr::Str), vec![])
        .constant_full("RFC7231", e_str("D, d M Y H:i:s \\G\\M\\T"), Some(TypeExpr::Str), vec![attr("\\Deprecated", vec![e_named_arg("since", e_str("8.5")), e_named_arg("message", e_str("as this format ignores the associated timezone and always uses GMT"))])])
        .constant_full("RFC2822", e_str("D, d M Y H:i:s O"), Some(TypeExpr::Str), vec![])
        .constant_full("RFC3339", e_str("Y-m-d\\TH:i:sP"), Some(TypeExpr::Str), vec![])
        .constant_full("RFC3339_EXTENDED", e_str("Y-m-d\\TH:i:s.vP"), Some(TypeExpr::Str), vec![])
        .constant_full("RSS", e_str("D, d M Y H:i:s O"), Some(TypeExpr::Str), vec![])
        .constant_full("W3C", e_str("Y-m-d\\TH:i:sP"), Some(TypeExpr::Str), vec![])
        .method(decl_stmt_bootstrap_1_method_0_format())
        .method(decl_stmt_bootstrap_1_method_1_gettimestamp())
        .method(decl_stmt_bootstrap_1_method_2_getmicrosecond())
        .method(decl_stmt_bootstrap_1_method_3_gettimezone())
        .method(decl_stmt_bootstrap_1_method_4_getoffset())
        .method(decl_stmt_bootstrap_1_method_5_diff())
        .method(decl_stmt_bootstrap_1_method_6_wakeup())
        .method(decl_stmt_bootstrap_1_method_7_serialize())
        .method(decl_stmt_bootstrap_1_method_8_unserialize())
        .method(decl_stmt_bootstrap_1_method_9_elephc_debug_dump())
        .method(decl_stmt_bootstrap_1_method_10_elephc_print_r_dump())
        .method(decl_stmt_bootstrap_1_method_11_elephc_assert_comparable())
        .build()
}

/// `DateInterval::__construct` — transcribed method builder.
fn decl_class_dateinterval_method_0_construct() -> MethodBuilder {
method("__construct")
    .param("duration", TypeExpr::Str)
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
        s_assign("parsed", e_call("__elephc_timelib_interval_parse", vec![e_var("duration"), e_bool(false)])),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("status")), BinOp::StrictEq, e_str("E")),
            vec![
                s_throw(e_new("DateMalformedIntervalStringException", vec![e_binop(e_binop(e_str("Unknown or bad format ("), BinOp::Concat, e_var("duration")), BinOp::Concat, e_str(")"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("status")), BinOp::StrictNotEq, e_str("O")),
            vec![
                s_throw(e_new("DateMalformedIntervalStringException", vec![e_binop(e_binop(e_str("Failed to parse interval ("), BinOp::Concat, e_var("duration")), BinOp::Concat, e_str(")"))])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "y", e_index(e_var("parsed"), e_str("y"))),
        s_prop_assign(e_this(), "m", e_index(e_var("parsed"), e_str("m"))),
        s_prop_assign(e_this(), "d", e_index(e_var("parsed"), e_str("d"))),
        s_prop_assign(e_this(), "h", e_index(e_var("parsed"), e_str("h"))),
        s_prop_assign(e_this(), "i", e_index(e_var("parsed"), e_str("i"))),
        s_prop_assign(e_this(), "s", e_index(e_var("parsed"), e_str("s"))),
        s_prop_assign(e_this(), "f", e_binop(e_index(e_var("parsed"), e_str("us")), BinOp::Div, e_float(1000000.0))),
        s_prop_assign(e_this(), "invert", e_index(e_var("parsed"), e_str("invert"))),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("days")), BinOp::StrictNotEq, e_neg(e_int(9999999))),
            vec![
                s_prop_assign(e_this(), "days", e_index(e_var("parsed"), e_str("days"))),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "_from_string", e_bool(false)),
        s_prop_assign(e_this(), "_date_string", e_str("")),
        s_prop_assign(e_this(), "_period_from_string", e_bool(false)),
        s_prop_assign(e_this(), "_period_date_string", e_str("")),
        s_prop_assign(e_this(), "_wall", e_bool(true)),
    ])
}

/// `DateInterval::format` — transcribed method builder.
fn decl_class_dateinterval_method_1_format() -> MethodBuilder {
method("format")
    .param("format", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("len", e_call("strlen", vec![e_var("format")])),
        s_assign("p", e_int(0)),
        s_assign("r", e_str("")),
        s_while(e_binop(e_var("p"), BinOp::Lt, e_var("len")), vec![
            s_assign("c", e_index(e_var("format"), e_var("p"))),
            s_if(
                e_binop(e_var("c"), BinOp::StrictEq, e_str("%")),
                vec![
                    s_assign("p", e_binop(e_var("p"), BinOp::Add, e_int(1))),
                    s_if(
                        e_binop(e_var("p"), BinOp::Lt, e_var("len")),
                        vec![
                            s_assign("spec", e_index(e_var("format"), e_var("p"))),
                            s_if(
                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("%")),
                                vec![
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("%"))),
                                ],
                                vec![
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("y")), vec![
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("y"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("Y")), vec![
                                    s_if(
                                        e_binop(e_this_prop("y"), BinOp::Lt, e_int(10)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("y"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("m")), vec![
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("m"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("M")), vec![
                                    s_if(
                                        e_binop(e_this_prop("m"), BinOp::Lt, e_int(10)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("m"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("d")), vec![
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("d"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("D")), vec![
                                    s_if(
                                        e_binop(e_this_prop("d"), BinOp::Lt, e_int(10)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("d"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("h")), vec![
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("h"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("H")), vec![
                                    s_if(
                                        e_binop(e_this_prop("h"), BinOp::Lt, e_int(10)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("h"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("i")), vec![
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("i"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("I")), vec![
                                    s_if(
                                        e_binop(e_this_prop("i"), BinOp::Lt, e_int(10)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("i"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("s")), vec![
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("s"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("S")), vec![
                                    s_if(
                                        e_binop(e_this_prop("s"), BinOp::Lt, e_int(10)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("s"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("f")), vec![
                                    s_assign("us", e_call("intval", vec![e_binop(e_this_prop("f"), BinOp::Mul, e_int(1000000))])),
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_var("us"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("F")), vec![
                                    s_assign("us", e_call("intval", vec![e_binop(e_this_prop("f"), BinOp::Mul, e_int(1000000))])),
                                    s_if(
                                        e_binop(e_var("us"), BinOp::Lt, e_int(100000)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_var("us"), BinOp::Lt, e_int(10000)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_var("us"), BinOp::Lt, e_int(1000)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_var("us"), BinOp::Lt, e_int(100)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_var("us"), BinOp::Lt, e_int(10)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("0"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_var("us"))),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("a")), vec![
                                    s_if(
                                        e_binop(e_this_prop("days"), BinOp::StrictEq, e_bool(false)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("(unknown)"))),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_this_prop("days"))),
                                    ]),
                                    ),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("R")), vec![
                                    s_if(
                                        e_binop(e_this_prop("invert"), BinOp::StrictEq, e_int(1)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("-"))),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("+"))),
                                    ]),
                                    ),
                                ]),
                                (e_binop(e_var("spec"), BinOp::StrictEq, e_str("r")), vec![
                                    s_if(
                                        e_binop(e_this_prop("invert"), BinOp::StrictEq, e_int(1)),
                                        vec![
                                            s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("-"))),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                ]),
                            ],
                                Some(vec![
                                s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_str("%"))),
                                s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_var("spec"))),
                            ]),
                            ),
                            s_assign("p", e_binop(e_var("p"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                Some(vec![
                s_assign("r", e_binop(e_var("r"), BinOp::Concat, e_var("c"))),
                s_assign("p", e_binop(e_var("p"), BinOp::Add, e_int(1))),
            ]),
            ),
        ]),
        s_return(e_var("r")),
    ])
}

/// `DateInterval::createFromDateString` — transcribed method builder.
fn decl_class_dateinterval_method_2_createfromdatestring() -> MethodBuilder {
method("createFromDateString")
    .static_()
    .param("datetime", TypeExpr::Str)
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_assign("parsed", e_call("__elephc_timelib_interval_parse", vec![e_var("datetime"), e_bool(true)])),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("status")), BinOp::StrictEq, e_str("E")),
            vec![
                s_throw(e_new("DateMalformedIntervalStringException", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Unknown or bad format ("), BinOp::Concat, e_var("datetime")), BinOp::Concat, e_str(") at position ")), BinOp::Concat, e_index(e_var("parsed"), e_str("position"))), BinOp::Concat, e_str(" (")), BinOp::Concat, e_index(e_var("parsed"), e_str("character"))), BinOp::Concat, e_str("): ")), BinOp::Concat, e_index(e_var("parsed"), e_str("message")))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("status")), BinOp::StrictEq, e_str("N")),
            vec![
                s_throw(e_new("DateMalformedIntervalStringException", vec![e_binop(e_binop(e_str("String '"), BinOp::Concat, e_var("datetime")), BinOp::Concat, e_str("' contains non-relative elements"))])),
            ],
            vec![],
            None,
        ),
        s_assign("iv", e_new("DateInterval", vec![e_str("PT0S")])),
        s_prop_assign(e_var("iv"), "y", e_index(e_var("parsed"), e_str("y"))),
        s_prop_assign(e_var("iv"), "m", e_index(e_var("parsed"), e_str("m"))),
        s_prop_assign(e_var("iv"), "d", e_index(e_var("parsed"), e_str("d"))),
        s_prop_assign(e_var("iv"), "h", e_index(e_var("parsed"), e_str("h"))),
        s_prop_assign(e_var("iv"), "i", e_index(e_var("parsed"), e_str("i"))),
        s_prop_assign(e_var("iv"), "s", e_index(e_var("parsed"), e_str("s"))),
        s_prop_assign(e_var("iv"), "f", e_binop(e_index(e_var("parsed"), e_str("us")), BinOp::Div, e_float(1000000.0))),
        s_prop_assign(e_var("iv"), "invert", e_index(e_var("parsed"), e_str("invert"))),
        s_prop_assign(e_var("iv"), "days", e_bool(false)),
        s_prop_assign(e_var("iv"), "_from_string", e_bool(true)),
        s_prop_assign(e_var("iv"), "_date_string", e_var("datetime")),
        s_prop_assign(e_var("iv"), "_wall", e_bool(false)),
        s_return(e_var("iv")),
    ])
}

/// `DateInterval::__elephc_create_from_date_string` — transcribed method builder.
fn decl_class_dateinterval_method_3_elephc_create_from_date_string() -> MethodBuilder {
method("__elephc_create_from_date_string")
    .static_()
    .param("datetime", TypeExpr::Str)
    .param("sourceLine", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_try(vec![
            s_return(e_static_call("DateInterval", "createFromDateString", vec![e_var("datetime")])),
        ], vec![
            (vec!["DateMalformedIntervalStringException"], Some("exception"), vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_binop(e_str("\nWarning: date_interval_create_from_date_string(): "), BinOp::Concat, e_method_call(e_var("exception"), "getMessage", vec![])), e_var("sourceLine")])),
                s_return(e_bool(false)),
            ]),
        ], None),
    ])
}

/// `DateInterval::__elephc_payload` — transcribed method builder.
fn decl_class_dateinterval_method_4_elephc_payload() -> MethodBuilder {
method("__elephc_payload")
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("microseconds", e_cast(CastType::Int, e_call("round", vec![e_binop(e_this_prop("f"), BinOp::Mul, e_float(1000000.0))]))),
        s_assign("days", e_ternary(e_binop(e_this_prop("days"), BinOp::StrictEq, e_bool(false)), e_neg(e_int(9999999)), e_this_prop("days"))),
        s_assign("fields", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_this_prop("y"), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("m")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("d")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("h")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("i")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("s")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("microseconds")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("invert")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("days"))),
        s_if(
            e_binop(e_this_prop("_from_string"), BinOp::Or, e_this_prop("_period_from_string")),
            vec![
                s_assign("dateString", e_ternary(e_this_prop("_from_string"), e_this_prop("_date_string"), e_this_prop("_period_date_string"))),
                s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("R"), BinOp::Concat, e_call("strlen", vec![e_var("dateString")])), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("dateString")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("fields"))),
            ],
            vec![],
            None,
        ),
        s_return(e_binop(e_ternary(e_this_prop("_wall"), e_str("W\t"), e_str("C\t")), BinOp::Concat, e_var("fields"))),
    ])
}

/// `DateInterval::__elephc_mark_civil` — transcribed method builder.
fn decl_class_dateinterval_method_5_elephc_mark_civil() -> MethodBuilder {
method("__elephc_mark_civil")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_prop_assign(e_this(), "_wall", e_bool(false)),
    ])
}

/// `DateInterval::__elephc_clone` — transcribed method builder.
fn decl_class_dateinterval_method_6_elephc_clone() -> MethodBuilder {
method("__elephc_clone")
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_clone(e_this())),
    ])
}

/// `DateInterval::__elephc_clone_storage` — transcribed method builder.
fn decl_class_dateinterval_method_7_elephc_clone_storage() -> MethodBuilder {
method("__elephc_clone_storage")
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_call("__elephc_object_clone_internal", vec![e_this()])),
    ])
}

/// `DateInterval::__elephc_clone_interval_for_period` — transcribed method builder.
fn decl_class_dateinterval_method_8_elephc_clone_interval_for_period() -> MethodBuilder {
method("__elephc_clone_interval_for_period")
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("interval", e_clone(e_this())),
        s_if(
            e_prop(e_var("interval"), "_from_string"),
            vec![
                s_prop_assign(e_var("interval"), "_period_from_string", e_bool(true)),
                s_prop_assign(e_var("interval"), "_period_date_string", e_prop(e_var("interval"), "_date_string")),
                s_prop_assign(e_var("interval"), "_from_string", e_bool(false)),
                s_prop_assign(e_var("interval"), "_date_string", e_str("")),
            ],
            vec![],
            None,
        ),
        s_return(e_var("interval")),
    ])
}

/// `DateInterval::__elephc_clone_interval_for_period_storage` — transcribed method builder.
fn decl_class_dateinterval_method_9_elephc_clone_interval_for_period_storage() -> MethodBuilder {
method("__elephc_clone_interval_for_period_storage")
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("interval", e_call("__elephc_object_clone_internal", vec![e_this()])),
        s_if(
            e_prop(e_var("interval"), "_from_string"),
            vec![
                s_prop_assign(e_var("interval"), "_period_from_string", e_bool(true)),
                s_prop_assign(e_var("interval"), "_period_date_string", e_prop(e_var("interval"), "_date_string")),
                s_prop_assign(e_var("interval"), "_from_string", e_bool(false)),
                s_prop_assign(e_var("interval"), "_date_string", e_str("")),
            ],
            vec![],
            None,
        ),
        s_return(e_var("interval")),
    ])
}

/// `DateInterval::__get` — transcribed method builder.
fn decl_class_dateinterval_method_10_get() -> MethodBuilder {
method("__get")
    .param("name", TypeExpr::Str)
    .returns(t_mixed())
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_expr(e_call("__elephc_diag_warning", vec![e_binop(e_str("\nWarning: Undefined property: DateInterval::$"), BinOp::Concat, e_var("name")), e_int(1)])),
        s_return(e_null()),
    ])
}

/// `DateInterval::__wakeup` — transcribed method builder.
fn decl_class_dateinterval_method_11_wakeup() -> MethodBuilder {
method("__wakeup")
    .attr("\\Deprecated", vec![e_named_arg("since", e_str("8.5")), e_named_arg("message", e_str("this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()"))])
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_str("Deprecated: Method DateInterval::__wakeup() is deprecated since 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()\n"), e_int(0), e_const("E_DEPRECATED")])),
        s_if(
            e_binop(e_str("DateInterval"), BinOp::StrictNotEq, e_str("DateInterval")),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateInterval object")])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateInterval::__serialize` — transcribed method builder.
fn decl_class_dateinterval_method_12_serialize() -> MethodBuilder {
method("__serialize")
    .returns(t_array())
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_this_prop("_from_string"),
            vec![
                s_return(e_array_assoc(vec![(e_str("from_string"), e_bool(true)), (e_str("date_string"), e_this_prop("_date_string"))])),
            ],
            vec![],
            None,
        ),
        s_return(e_array_assoc(vec![(e_str("y"), e_this_prop("y")), (e_str("m"), e_this_prop("m")), (e_str("d"), e_this_prop("d")), (e_str("h"), e_this_prop("h")), (e_str("i"), e_this_prop("i")), (e_str("s"), e_this_prop("s")), (e_str("f"), e_this_prop("f")), (e_str("invert"), e_this_prop("invert")), (e_str("days"), e_this_prop("days")), (e_str("from_string"), e_bool(false))])),
    ])
}

/// `DateInterval::__unserialize` — transcribed method builder.
fn decl_class_dateinterval_method_13_unserialize() -> MethodBuilder {
method("__unserialize")
    .param("data", t_array())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_call("array_key_exists", vec![e_str("date_string"), e_var("data")]), BinOp::And, e_call("is_string", vec![e_index(e_var("data"), e_str("date_string"))])),
            vec![
                s_assign("parsed", e_call("__elephc_timelib_interval_restore_parse", vec![e_index(e_var("data"), e_str("date_string"))])),
                s_if(
                    e_binop(e_index(e_var("parsed"), e_str("status")), BinOp::StrictEq, e_str("E")),
                    vec![
                        s_throw(e_new("Error", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Unknown or bad format ("), BinOp::Concat, e_index(e_var("data"), e_str("date_string"))), BinOp::Concat, e_str(") at position ")), BinOp::Concat, e_index(e_var("parsed"), e_str("position"))), BinOp::Concat, e_str(" (")), BinOp::Concat, e_index(e_var("parsed"), e_str("character"))), BinOp::Concat, e_str(") while unserializing: ")), BinOp::Concat, e_index(e_var("parsed"), e_str("message")))])),
                    ],
                    vec![],
                    None,
                ),
                s_prop_assign(e_this(), "y", e_index(e_var("parsed"), e_str("y"))),
                s_prop_assign(e_this(), "m", e_index(e_var("parsed"), e_str("m"))),
                s_prop_assign(e_this(), "d", e_index(e_var("parsed"), e_str("d"))),
                s_prop_assign(e_this(), "h", e_index(e_var("parsed"), e_str("h"))),
                s_prop_assign(e_this(), "i", e_index(e_var("parsed"), e_str("i"))),
                s_prop_assign(e_this(), "s", e_index(e_var("parsed"), e_str("s"))),
                s_prop_assign(e_this(), "f", e_binop(e_index(e_var("parsed"), e_str("us")), BinOp::Div, e_float(1000000.0))),
                s_prop_assign(e_this(), "invert", e_index(e_var("parsed"), e_str("invert"))),
                s_prop_assign(e_this(), "days", e_ternary(e_binop(e_index(e_var("parsed"), e_str("days")), BinOp::StrictEq, e_neg(e_int(9999999))), e_bool(false), e_index(e_var("parsed"), e_str("days")))),
                s_prop_assign(e_this(), "_from_string", e_bool(true)),
                s_prop_assign(e_this(), "_date_string", e_index(e_var("data"), e_str("date_string"))),
                s_prop_assign(e_this(), "_period_from_string", e_bool(false)),
                s_prop_assign(e_this(), "_period_date_string", e_str("")),
                s_prop_assign(e_this(), "_wall", e_bool(false)),
                s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "y", e_call("intval", vec![e_ternary(e_call("array_key_exists", vec![e_str("y"), e_var("data")]), e_index(e_var("data"), e_str("y")), e_neg(e_int(1)))])),
        s_prop_assign(e_this(), "m", e_call("intval", vec![e_ternary(e_call("array_key_exists", vec![e_str("m"), e_var("data")]), e_index(e_var("data"), e_str("m")), e_neg(e_int(1)))])),
        s_prop_assign(e_this(), "d", e_call("intval", vec![e_ternary(e_call("array_key_exists", vec![e_str("d"), e_var("data")]), e_index(e_var("data"), e_str("d")), e_neg(e_int(1)))])),
        s_prop_assign(e_this(), "h", e_call("intval", vec![e_ternary(e_call("array_key_exists", vec![e_str("h"), e_var("data")]), e_index(e_var("data"), e_str("h")), e_neg(e_int(1)))])),
        s_prop_assign(e_this(), "i", e_call("intval", vec![e_ternary(e_call("array_key_exists", vec![e_str("i"), e_var("data")]), e_index(e_var("data"), e_str("i")), e_neg(e_int(1)))])),
        s_prop_assign(e_this(), "s", e_call("intval", vec![e_ternary(e_call("array_key_exists", vec![e_str("s"), e_var("data")]), e_index(e_var("data"), e_str("s")), e_neg(e_int(1)))])),
        s_assign("fValue", e_call("floatval", vec![e_ternary(e_call("array_key_exists", vec![e_str("f"), e_var("data")]), e_index(e_var("data"), e_str("f")), e_float(0.0))])),
        s_if(
            e_binop(e_binop(e_var("fValue"), BinOp::Gt, e_float(9223372036854.775)), BinOp::Or, e_binop(e_var("fValue"), BinOp::Lt, e_neg(e_float(9223372036854.775)))),
            vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_binop(e_binop(e_str("Warning: The float "), BinOp::Concat, e_binop(e_var("fValue"), BinOp::Mul, e_float(1000000.0))), BinOp::Concat, e_str(" is not representable as an int, cast occurred")), e_int(1)])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "f", e_var("fValue")),
        s_prop_assign(e_this(), "invert", e_call("intval", vec![e_ternary(e_call("array_key_exists", vec![e_str("invert"), e_var("data")]), e_index(e_var("data"), e_str("invert")), e_int(0))])),
        s_assign("daysValue", e_ternary(e_call("array_key_exists", vec![e_str("days"), e_var("data")]), e_index(e_var("data"), e_str("days")), e_neg(e_int(1)))),
        s_if(
            e_binop(e_var("daysValue"), BinOp::StrictEq, e_bool(false)),
            vec![
                s_prop_assign(e_this(), "days", e_bool(false)),
            ],
            vec![
            (e_binop(e_call("is_array", vec![e_var("daysValue")]), BinOp::Or, e_call("is_object", vec![e_var("daysValue")])), vec![
                s_prop_assign(e_this(), "days", e_neg(e_int(1))),
            ]),
        ],
            Some(vec![
            s_prop_assign(e_this(), "days", e_call("intval", vec![e_var("daysValue")])),
        ]),
        ),
        s_prop_assign(e_this(), "_from_string", e_bool(false)),
        s_prop_assign(e_this(), "_date_string", e_str("")),
        s_prop_assign(e_this(), "_period_from_string", e_bool(false)),
        s_prop_assign(e_this(), "_period_date_string", e_str("")),
        s_prop_assign(e_this(), "_wall", e_bool(true)),
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
    ])
}

/// `DateInterval::__set_state` — transcribed method builder.
fn decl_class_dateinterval_method_14_set_state() -> MethodBuilder {
method("__set_state")
    .static_()
    .param("array", t_array())
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_assign("iv", e_new("DateInterval", vec![e_str("PT0S")])),
        s_expr(e_method_call(e_var("iv"), "__unserialize", vec![e_var("array")])),
        s_return(e_var("iv")),
    ])
}

/// `DateInterval::__elephc_debug_dump` — transcribed method builder.
fn decl_class_dateinterval_method_15_elephc_debug_dump() -> MethodBuilder {
method("__elephc_debug_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_call("__elephc_var_dump_indent", vec![e_int(0)])])),
        s_assign("field_pad", e_binop(e_var("pad"), BinOp::Concat, e_str("  "))),
        s_assign("property_count", e_call("__elephc_var_dump_object_property_count", vec![e_this()])),
        s_if(
            e_this_prop("_from_string"),
            vec![
                s_echo(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("pad"), BinOp::Concat, e_str("object(")), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(")#")), BinOp::Concat, e_call("spl_object_id", vec![e_this()])), BinOp::Concat, e_str(" (")), BinOp::Concat, e_binop(e_var("property_count"), BinOp::Add, e_int(2))), BinOp::Concat, e_str(") {\n"))),
                s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
                s_expr(e_call("__elephc_var_dump_object_properties", vec![e_this()])),
                s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
                s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"from_string\"]=>\n"))),
                s_echo(e_var("field_pad")),
                s_expr(e_call("var_dump", vec![e_bool(true)])),
                s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"date_string\"]=>\n"))),
                s_echo(e_var("field_pad")),
                s_expr(e_call("var_dump", vec![e_this_prop("_date_string")])),
                s_echo(e_binop(e_var("pad"), BinOp::Concat, e_str("}\n"))),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_echo(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("pad"), BinOp::Concat, e_str("object(")), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(")#")), BinOp::Concat, e_call("spl_object_id", vec![e_this()])), BinOp::Concat, e_str(" (")), BinOp::Concat, e_binop(e_var("property_count"), BinOp::Add, e_int(10))), BinOp::Concat, e_str(") {\n"))),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
        s_expr(e_call("__elephc_var_dump_object_properties", vec![e_this()])),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"y\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("y")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"m\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("m")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"d\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("d")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"h\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("h")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"i\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("i")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"s\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("s")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"f\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("f")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"invert\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("invert")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"days\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("days")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"from_string\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_bool(false)])),
        s_echo(e_binop(e_var("pad"), BinOp::Concat, e_str("}\n"))),
    ])
}

/// `DateInterval::__elephc_print_r_dump` — transcribed method builder.
fn decl_class_dateinterval_method_16_elephc_print_r_dump() -> MethodBuilder {
method("__elephc_print_r_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_echo(e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str(" Object\n(\n"))),
        s_expr(e_call("__elephc_print_r_object_properties", vec![e_this()])),
        s_if(
            e_this_prop("_from_string"),
            vec![
                s_echo(e_str("    [from_string] => 1\n")),
                s_echo(e_binop(e_binop(e_str("    [date_string] => "), BinOp::Concat, e_this_prop("_date_string")), BinOp::Concat, e_str("\n"))),
                s_echo(e_str(")\n")),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_echo(e_binop(e_binop(e_str("    [y] => "), BinOp::Concat, e_this_prop("y")), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [m] => "), BinOp::Concat, e_this_prop("m")), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [d] => "), BinOp::Concat, e_this_prop("d")), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [h] => "), BinOp::Concat, e_this_prop("h")), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [i] => "), BinOp::Concat, e_this_prop("i")), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [s] => "), BinOp::Concat, e_this_prop("s")), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [f] => "), BinOp::Concat, e_this_prop("f")), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [invert] => "), BinOp::Concat, e_this_prop("invert")), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [days] => "), BinOp::Concat, e_this_prop("days")), BinOp::Concat, e_str("\n"))),
        s_echo(e_str("    [from_string] => \n")),
        s_echo(e_str(")\n")),
    ])
}

/// `DateInterval::__elephc_begin_argument_array` — transcribed method builder.
fn decl_class_dateinterval_method_17_elephc_begin_argument_array() -> MethodBuilder {
method("__elephc_begin_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_arguments", e_array(vec![])),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DateInterval::__elephc_append_one_argument` — transcribed method builder.
fn decl_class_dateinterval_method_18_elephc_append_one_argument() -> MethodBuilder {
method("__elephc_append_one_argument")
    .private()
    .param("key", t_mixed())
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_if(
            e_call("is_int", vec![e_var("key")]),
            vec![
                s_if(
                    e_this_prop("__elephc_seen_named_argument"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Cannot use positional argument after named argument during unpacking")])),
                    ],
                    vec![],
                    None,
                ),
                s_array_push("arguments", e_var("value")),
                s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_string", vec![e_var("key")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Keys must be of type int|string during argument unpacking")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(true)),
        s_if(
            e_not(e_binop(e_var("key"), BinOp::StrictEq, e_str("duration"))),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_str("Unknown named parameter $"), BinOp::Concat, e_var("key"))])),
            ],
            vec![],
            None,
        ),
        s_assign("parameterIndex", e_neg(e_int(1))),
        s_if(
            e_binop(e_var("key"), BinOp::StrictEq, e_str("duration")),
            vec![
                s_assign("parameterIndex", e_int(0)),
            ],
            vec![],
            None,
        ),
        s_assign("positionalCount", e_int(0)),
        s_foreach(e_var("arguments"), Some("existingKey"), "existingValue", vec![
            s_if(
                e_call("is_int", vec![e_var("existingKey")]),
                vec![
                    s_expr(e_post_inc("positionalCount")),
                ],
                vec![],
                None,
            ),
        ]),
        s_if(
            e_binop(e_var("parameterIndex"), BinOp::Lt, e_var("positionalCount")),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("array_key_exists", vec![e_var("key"), e_var("arguments")]),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_array_assign("arguments", e_var("key"), e_var("value")),
        s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
    ])
}

/// `DateInterval::__elephc_append_argument_chunk` — transcribed method builder.
fn decl_class_dateinterval_method_19_elephc_append_argument_chunk() -> MethodBuilder {
method("__elephc_append_argument_chunk")
    .private()
    .param("kind", TypeExpr::Int)
    .param("name", TypeExpr::Str)
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
            vec![
                s_if(
                    e_not(e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_instance_of(e_var("value"), "Traversable"))),
                    vec![
                        s_expr(e_static_call("DateTime", "__elephc_argument_type_error", vec![e_var("value"), e_str("Only arrays and Traversables can be unpacked, ")])),
                    ],
                    vec![],
                    None,
                ),
                s_foreach(e_var("value"), Some("key"), "unpackedValue", vec![
                    s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("key"), e_var("unpackedValue")])),
                ]),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
            vec![
                s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("name"), e_var("value")])),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_int(0), e_var("value")])),
    ])
}

/// `DateInterval::__elephc_finish_argument_array` — transcribed method builder.
fn decl_class_dateinterval_method_20_elephc_finish_argument_array() -> MethodBuilder {
method("__elephc_finish_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_assign("hasDuration", e_bool(false)),
        s_assign("nextPosition", e_int(0)),
        s_foreach(e_var("arguments"), Some("key"), "value", vec![
            s_if(
                e_call("is_int", vec![e_var("key")]),
                vec![
                    s_if(
                        e_binop(e_var("nextPosition"), BinOp::Gt, e_int(0)),
                        vec![
                            s_throw(e_new("ArgumentCountError", vec![e_binop(e_binop(e_str("DateInterval::__construct() expects exactly 1 argument, "), BinOp::Concat, e_call("count", vec![e_var("arguments")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("duration", e_var("value")),
                    s_assign("hasDuration", e_bool(true)),
                    s_expr(e_post_inc("nextPosition")),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_var("hasDuration"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Named parameter $duration overwrites previous argument")])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("duration", e_var("value")),
                s_assign("hasDuration", e_bool(true)),
            ]),
            ),
        ]),
        s_if(
            e_not(e_var("hasDuration")),
            vec![
                s_throw(e_new("ArgumentCountError", vec![e_str("DateInterval::__construct() expects exactly 1 argument, 0 given")])),
            ],
            vec![],
            None,
        ),
        s_assign("duration", e_static_call("DateTime", "__elephc_weak_string_argument", vec![e_var("duration"), e_str("DateInterval::__construct(): Argument #1 ($duration) must be of type string, "), e_str("")])),
        s_expr(e_method_call(e_this(), "__construct", vec![e_var("duration")])),
        s_prop_assign(e_this(), "__elephc_arguments", e_null()),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DateInterval::__elephc_is_initialized` — transcribed method builder.
fn decl_class_dateinterval_method_21_elephc_is_initialized() -> MethodBuilder {
method("__elephc_is_initialized")
    .final_()
    .returns(TypeExpr::Bool)
    .body_exact(vec![
        s_return(e_this_prop("__elephc_initialized")),
    ])
}

/// `DateInterval::__elephc_assert_initialized` — transcribed method builder.
fn decl_class_dateinterval_method_22_elephc_assert_initialized() -> MethodBuilder {
method("__elephc_assert_initialized")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_assign("objectClass", e_call("get_class", vec![e_this()])),
                s_assign("inheritance", e_ternary(e_binop(e_var("objectClass"), BinOp::StrictEq, e_str("DateInterval")), e_str(""), e_str(" (inheriting DateInterval)"))),
                s_throw(e_new("DateObjectError", vec![e_binop(e_binop(e_binop(e_str("Object of type "), BinOp::Concat, e_var("objectClass")), BinOp::Concat, e_var("inheritance")), BinOp::Concat, e_str(" has not been correctly initialized by calling parent::__construct() in its constructor"))])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateInterval` — transcribed from the PHP form.
fn decl_class_dateinterval() -> Stmt {
    class("DateInterval")
        .prop("y", TypeExpr::Int, Some(e_int(0)))
        .prop("m", TypeExpr::Int, Some(e_int(0)))
        .prop("d", TypeExpr::Int, Some(e_int(0)))
        .prop("h", TypeExpr::Int, Some(e_int(0)))
        .prop("i", TypeExpr::Int, Some(e_int(0)))
        .prop("s", TypeExpr::Int, Some(e_int(0)))
        .prop("f", TypeExpr::Float, Some(e_float(0.0)))
        .prop("invert", TypeExpr::Int, Some(e_int(0)))
        .prop("days", t_union(vec![TypeExpr::Int, TypeExpr::Bool]), Some(e_bool(false)))
        .private_prop("_from_string", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("_date_string", TypeExpr::Str, Some(e_str("")))
        .private_prop("_period_from_string", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("_period_date_string", TypeExpr::Str, Some(e_str("")))
        .private_prop("_wall", TypeExpr::Bool, Some(e_bool(true)))
        .private_prop("__elephc_initialized", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("__elephc_arguments", t_mixed(), Some(e_null()))
        .private_prop("__elephc_seen_named_argument", TypeExpr::Bool, Some(e_bool(false)))
        .method(decl_class_dateinterval_method_0_construct())
        .method(decl_class_dateinterval_method_1_format())
        .method(decl_class_dateinterval_method_2_createfromdatestring())
        .method(decl_class_dateinterval_method_3_elephc_create_from_date_string())
        .method(decl_class_dateinterval_method_4_elephc_payload())
        .method(decl_class_dateinterval_method_5_elephc_mark_civil())
        .method(decl_class_dateinterval_method_6_elephc_clone())
        .method(decl_class_dateinterval_method_7_elephc_clone_storage())
        .method(decl_class_dateinterval_method_8_elephc_clone_interval_for_period())
        .method(decl_class_dateinterval_method_9_elephc_clone_interval_for_period_storage())
        .method(decl_class_dateinterval_method_10_get())
        .method(decl_class_dateinterval_method_11_wakeup())
        .method(decl_class_dateinterval_method_12_serialize())
        .method(decl_class_dateinterval_method_13_unserialize())
        .method(decl_class_dateinterval_method_14_set_state())
        .method(decl_class_dateinterval_method_15_elephc_debug_dump())
        .method(decl_class_dateinterval_method_16_elephc_print_r_dump())
        .method(decl_class_dateinterval_method_17_elephc_begin_argument_array())
        .method(decl_class_dateinterval_method_18_elephc_append_one_argument())
        .method(decl_class_dateinterval_method_19_elephc_append_argument_chunk())
        .method(decl_class_dateinterval_method_20_elephc_finish_argument_array())
        .method(decl_class_dateinterval_method_21_elephc_is_initialized())
        .method(decl_class_dateinterval_method_22_elephc_assert_initialized())
        .build()
}

/// `DatePeriod::__construct` — transcribed method builder.
fn decl_class_dateperiod_method_0_construct() -> MethodBuilder {
method("__construct")
    .param("start", t_class("DateTimeInterface"))
    .param("interval", t_class("DateInterval"))
    .param("end", t_mixed())
    .param_default("options", TypeExpr::Int, e_int(0))
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
        s_assign("__elephc_uses_recurrence_end", e_bool(false)),
        s_assign("__elephc_recurrence_end", e_int(0)),
        s_if(
            e_call("is_int", vec![e_var("end")]),
            vec![
                s_assign("__elephc_uses_recurrence_end", e_bool(true)),
                s_assign("__elephc_recurrence_end", e_cast(CastType::Int, e_var("end"))),
            ],
            vec![
            (e_instance_of(e_var("end"), "DateTimeInterface"), vec![
                s_assign("__elephc_uses_recurrence_end", e_bool(false)),
            ]),
            (e_binop(e_binop(e_binop(e_call("is_float", vec![e_var("end")]), BinOp::Or, e_binop(e_call("is_string", vec![e_var("end")]), BinOp::And, e_call("is_numeric", vec![e_var("end")]))), BinOp::Or, e_call("is_bool", vec![e_var("end")])), BinOp::Or, e_call("is_null", vec![e_var("end")])), vec![
                s_assign("__elephc_uses_recurrence_end", e_bool(true)),
                s_assign("__elephc_recurrence_end", e_cast(CastType::Int, e_var("end"))),
            ]),
        ],
            Some(vec![
            s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
        ]),
        ),
        s_if(
            e_var("__elephc_uses_recurrence_end"),
            vec![
                s_if(
                    e_binop(e_binop(e_var("__elephc_recurrence_end"), BinOp::Lt, e_int(1)), BinOp::Or, e_binop(e_var("__elephc_recurrence_end"), BinOp::Gt, e_int(2147483639))),
                    vec![
                        s_throw(e_new("DateMalformedPeriodStringException", vec![e_str("DatePeriod::__construct(): Recurrence count must be greater or equal to 1 and lower than 2147483640")])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("totalRecurrences", e_binop(e_binop(e_var("__elephc_recurrence_end"), BinOp::Add, e_ternary(e_binop(e_var("options"), BinOp::BitAnd, e_class_const("DatePeriod", "EXCLUDE_START_DATE")), e_int(0), e_int(1))), BinOp::Add, e_ternary(e_binop(e_var("options"), BinOp::BitAnd, e_class_const("DatePeriod", "INCLUDE_END_DATE")), e_int(1), e_int(0)))),
                s_if(
                    e_binop(e_var("totalRecurrences"), BinOp::Gt, e_int(2147483639)),
                    vec![
                        s_throw(e_new("DateMalformedStringException", vec![e_str("DatePeriod::__construct(): Recurrence count must be greater or equal to 1 and lower than 2147483640 (including options)")])),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "startTs", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("start")])),
        s_prop_assign(e_this(), "startIsImmutable", e_instance_of(e_var("start"), "DateTimeImmutable")),
        s_prop_assign(e_this(), "iv_y", e_prop(e_var("interval"), "y")),
        s_prop_assign(e_this(), "iv_m", e_prop(e_var("interval"), "m")),
        s_prop_assign(e_this(), "iv_d", e_prop(e_var("interval"), "d")),
        s_prop_assign(e_this(), "iv_h", e_prop(e_var("interval"), "h")),
        s_prop_assign(e_this(), "iv_i", e_prop(e_var("interval"), "i")),
        s_prop_assign(e_this(), "iv_s", e_prop(e_var("interval"), "s")),
        s_prop_assign(e_this(), "iv_invert", e_prop(e_var("interval"), "invert")),
        s_if(
            e_var("__elephc_uses_recurrence_end"),
            vec![
                s_prop_assign(e_this(), "useCount", e_int(1)),
                s_prop_assign(e_this(), "_recurrence_count", e_cast(CastType::Int, e_var("__elephc_recurrence_end"))),
                s_prop_assign(e_this(), "endTs", e_int(0)),
            ],
            vec![],
            Some(vec![
            s_prop_assign(e_this(), "useCount", e_int(0)),
            s_prop_assign(e_this(), "_recurrence_count", e_int(0)),
            s_prop_assign(e_this(), "endTs", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("end")])),
        ]),
        ),
        s_prop_assign(e_this(), "excludeStart", e_binop(e_var("options"), BinOp::BitAnd, e_int(1))),
        s_prop_assign(e_this(), "includeEnd", e_binop(e_var("options"), BinOp::BitAnd, e_int(2))),
        s_prop_assign(e_this(), "curTs", e_this_prop("startTs")),
        s_prop_assign(e_this(), "idx", e_int(0)),
        s_prop_assign(e_this(), "_start", e_method_call(e_this(), "__elephc_clone_datetime_interface_storage", vec![e_var("start")])),
        s_prop_assign(e_this(), "_include_start_date", e_binop(e_this_prop("excludeStart"), BinOp::Eq, e_int(0))),
        s_prop_assign(e_this(), "_include_end_date", e_binop(e_this_prop("includeEnd"), BinOp::NotEq, e_int(0))),
        s_if(
            e_var("__elephc_uses_recurrence_end"),
            vec![
                s_prop_assign(e_this(), "_end", e_null()),
            ],
            vec![],
            Some(vec![
            s_prop_assign(e_this(), "_end", e_method_call(e_this(), "__elephc_clone_datetime_interface_storage", vec![e_var("end")])),
        ]),
        ),
        s_prop_assign(e_this(), "_interval", e_method_call(e_var("interval"), "__elephc_clone_interval_for_period_storage", vec![])),
        s_prop_assign(e_this(), "_recurrences", e_binop(e_binop(e_this_prop("_recurrence_count"), BinOp::Add, e_cast(CastType::Int, e_this_prop("_include_start_date"))), BinOp::Add, e_cast(CastType::Int, e_this_prop("_include_end_date")))),
    ])
}

/// `DatePeriod::__elephc_initialize_end_components` — transcribed method builder.
fn decl_class_dateperiod_method_1_elephc_initialize_end_components() -> MethodBuilder {
method("__elephc_initialize_end_components")
    .final_()
    .param("start", t_class("DateTimeInterface"))
    .param("interval", t_class("DateInterval"))
    .param("endTimestamp", TypeExpr::Int)
    .param_default("options", TypeExpr::Int, e_int(0))
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
        s_assign("__elephc_uses_recurrence_end", e_bool(false)),
        s_assign("__elephc_recurrence_end", e_int(0)),
        s_assign("end", e_static_call("DateTimeImmutable", "createFromTimestamp", vec![e_var("endTimestamp")])),
        s_prop_assign(e_this(), "startTs", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("start")])),
        s_prop_assign(e_this(), "startIsImmutable", e_instance_of(e_var("start"), "DateTimeImmutable")),
        s_prop_assign(e_this(), "iv_y", e_prop(e_var("interval"), "y")),
        s_prop_assign(e_this(), "iv_m", e_prop(e_var("interval"), "m")),
        s_prop_assign(e_this(), "iv_d", e_prop(e_var("interval"), "d")),
        s_prop_assign(e_this(), "iv_h", e_prop(e_var("interval"), "h")),
        s_prop_assign(e_this(), "iv_i", e_prop(e_var("interval"), "i")),
        s_prop_assign(e_this(), "iv_s", e_prop(e_var("interval"), "s")),
        s_prop_assign(e_this(), "iv_invert", e_prop(e_var("interval"), "invert")),
        s_if(
            e_var("__elephc_uses_recurrence_end"),
            vec![
                s_prop_assign(e_this(), "useCount", e_int(1)),
                s_prop_assign(e_this(), "_recurrence_count", e_cast(CastType::Int, e_var("__elephc_recurrence_end"))),
                s_prop_assign(e_this(), "endTs", e_int(0)),
            ],
            vec![],
            Some(vec![
            s_prop_assign(e_this(), "useCount", e_int(0)),
            s_prop_assign(e_this(), "_recurrence_count", e_int(0)),
            s_prop_assign(e_this(), "endTs", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("end")])),
        ]),
        ),
        s_prop_assign(e_this(), "excludeStart", e_binop(e_var("options"), BinOp::BitAnd, e_int(1))),
        s_prop_assign(e_this(), "includeEnd", e_binop(e_var("options"), BinOp::BitAnd, e_int(2))),
        s_prop_assign(e_this(), "curTs", e_this_prop("startTs")),
        s_prop_assign(e_this(), "idx", e_int(0)),
        s_prop_assign(e_this(), "_start", e_method_call(e_this(), "__elephc_clone_datetime_interface_storage", vec![e_var("start")])),
        s_prop_assign(e_this(), "_include_start_date", e_binop(e_this_prop("excludeStart"), BinOp::Eq, e_int(0))),
        s_prop_assign(e_this(), "_include_end_date", e_binop(e_this_prop("includeEnd"), BinOp::NotEq, e_int(0))),
        s_if(
            e_var("__elephc_uses_recurrence_end"),
            vec![
                s_prop_assign(e_this(), "_end", e_null()),
            ],
            vec![],
            Some(vec![
            s_prop_assign(e_this(), "_end", e_method_call(e_this(), "__elephc_clone_datetime_interface_storage", vec![e_var("end")])),
        ]),
        ),
        s_prop_assign(e_this(), "_interval", e_method_call(e_var("interval"), "__elephc_clone_interval_for_period_storage", vec![])),
        s_prop_assign(e_this(), "_recurrences", e_binop(e_binop(e_this_prop("_recurrence_count"), BinOp::Add, e_cast(CastType::Int, e_this_prop("_include_start_date"))), BinOp::Add, e_cast(CastType::Int, e_this_prop("_include_end_date")))),
    ])
}

/// `DatePeriod::__elephc_initialize_recurrence_components` — transcribed method builder.
fn decl_class_dateperiod_method_2_elephc_initialize_recurrence_components() -> MethodBuilder {
method("__elephc_initialize_recurrence_components")
    .final_()
    .param("start", t_class("DateTimeInterface"))
    .param("interval", t_class("DateInterval"))
    .param("end", TypeExpr::Int)
    .param_default("options", TypeExpr::Int, e_int(0))
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
        s_assign("__elephc_uses_recurrence_end", e_bool(true)),
        s_assign("__elephc_recurrence_end", e_var("end")),
        s_prop_assign(e_this(), "startTs", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("start")])),
        s_prop_assign(e_this(), "startIsImmutable", e_instance_of(e_var("start"), "DateTimeImmutable")),
        s_prop_assign(e_this(), "iv_y", e_prop(e_var("interval"), "y")),
        s_prop_assign(e_this(), "iv_m", e_prop(e_var("interval"), "m")),
        s_prop_assign(e_this(), "iv_d", e_prop(e_var("interval"), "d")),
        s_prop_assign(e_this(), "iv_h", e_prop(e_var("interval"), "h")),
        s_prop_assign(e_this(), "iv_i", e_prop(e_var("interval"), "i")),
        s_prop_assign(e_this(), "iv_s", e_prop(e_var("interval"), "s")),
        s_prop_assign(e_this(), "iv_invert", e_prop(e_var("interval"), "invert")),
        s_if(
            e_var("__elephc_uses_recurrence_end"),
            vec![
                s_prop_assign(e_this(), "useCount", e_int(1)),
                s_prop_assign(e_this(), "_recurrence_count", e_cast(CastType::Int, e_var("__elephc_recurrence_end"))),
                s_prop_assign(e_this(), "endTs", e_int(0)),
            ],
            vec![],
            Some(vec![
            s_prop_assign(e_this(), "useCount", e_int(0)),
            s_prop_assign(e_this(), "_recurrence_count", e_int(0)),
            s_prop_assign(e_this(), "endTs", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("end")])),
        ]),
        ),
        s_prop_assign(e_this(), "excludeStart", e_binop(e_var("options"), BinOp::BitAnd, e_int(1))),
        s_prop_assign(e_this(), "includeEnd", e_binop(e_var("options"), BinOp::BitAnd, e_int(2))),
        s_prop_assign(e_this(), "curTs", e_this_prop("startTs")),
        s_prop_assign(e_this(), "idx", e_int(0)),
        s_prop_assign(e_this(), "_start", e_method_call(e_this(), "__elephc_clone_datetime_interface_storage", vec![e_var("start")])),
        s_prop_assign(e_this(), "_include_start_date", e_binop(e_this_prop("excludeStart"), BinOp::Eq, e_int(0))),
        s_prop_assign(e_this(), "_include_end_date", e_binop(e_this_prop("includeEnd"), BinOp::NotEq, e_int(0))),
        s_if(
            e_var("__elephc_uses_recurrence_end"),
            vec![
                s_prop_assign(e_this(), "_end", e_null()),
            ],
            vec![],
            Some(vec![
            s_prop_assign(e_this(), "_end", e_method_call(e_this(), "__elephc_clone_datetime_interface_storage", vec![e_var("end")])),
        ]),
        ),
        s_prop_assign(e_this(), "_interval", e_method_call(e_var("interval"), "__elephc_clone_interval_for_period_storage", vec![])),
        s_prop_assign(e_this(), "_recurrences", e_binop(e_binop(e_this_prop("_recurrence_count"), BinOp::Add, e_cast(CastType::Int, e_this_prop("_include_start_date"))), BinOp::Add, e_cast(CastType::Int, e_this_prop("_include_end_date")))),
    ])
}

/// `DatePeriod::__elephc_weak_string_argument` — transcribed method builder.
fn decl_class_dateperiod_method_3_elephc_weak_string_argument() -> MethodBuilder {
method("__elephc_weak_string_argument")
    .private()
    .static_()
    .param("value", t_mixed())
    .param("line", TypeExpr::Int)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_if(
            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
            vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: DatePeriod::__construct(): Passing null to parameter #1 ($start) of type string is deprecated"), e_var("line"), e_const("E_DEPRECATED")])),
                s_return(e_str("")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_binop(e_call("is_object", vec![e_var("value")]), BinOp::And, e_not(e_instance_of(e_var("value"), "Stringable")))),
            vec![
                s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
            ],
            vec![],
            None,
        ),
        s_return(e_cast(CastType::String, e_var("value"))),
    ])
}

/// `DatePeriod::__elephc_clone_datetime_interface` — transcribed method builder.
fn decl_class_dateperiod_method_4_elephc_clone_datetime_interface() -> MethodBuilder {
method("__elephc_clone_datetime_interface")
    .private()
    .param("value", t_mixed())
    .returns(t_union(vec![t_class("DateTime"), t_class("DateTimeImmutable")]))
    .body_exact(vec![
        s_if(
            e_instance_of(e_var("value"), "DateTimeImmutable"),
            vec![
                s_return(e_method_call(e_var("value"), "__elephc_clone_for_period", vec![])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_instance_of(e_var("value"), "DateTime"),
            vec![
                s_return(e_method_call(e_var("value"), "__elephc_clone_for_period", vec![])),
            ],
            vec![],
            None,
        ),
        s_throw(e_new("DateMalformedPeriodStringException", vec![e_str("Invalid DatePeriod boundary")])),
    ])
}

/// `DatePeriod::__elephc_clone_datetime_interface_storage` — transcribed method builder.
fn decl_class_dateperiod_method_5_elephc_clone_datetime_interface_storage() -> MethodBuilder {
method("__elephc_clone_datetime_interface_storage")
    .private()
    .param("value", t_mixed())
    .returns(t_union(vec![t_class("DateTime"), t_class("DateTimeImmutable")]))
    .body_exact(vec![
        s_if(
            e_instance_of(e_var("value"), "DateTimeImmutable"),
            vec![
                s_return(e_method_call(e_var("value"), "__elephc_clone_for_period_storage", vec![])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_instance_of(e_var("value"), "DateTime"),
            vec![
                s_return(e_method_call(e_var("value"), "__elephc_clone_for_period_storage", vec![])),
            ],
            vec![],
            None,
        ),
        s_throw(e_new("DateMalformedPeriodStringException", vec![e_str("Invalid DatePeriod boundary")])),
    ])
}

/// `DatePeriod::__elephc_clone_iterator_value` — transcribed method builder.
fn decl_class_dateperiod_method_6_elephc_clone_iterator_value() -> MethodBuilder {
method("__elephc_clone_iterator_value")
    .private()
    .param("value", t_mixed())
    .returns(t_class("DateTimeInterface"))
    .body_exact(vec![
        s_if(
            e_instance_of(e_var("value"), "DateTimeImmutable"),
            vec![
                s_return(e_static_call("DateTimeImmutable", "createFromInterface", vec![e_var("value")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_instance_of(e_var("value"), "DateTime"),
            vec![
                s_return(e_static_call("DateTime", "createFromInterface", vec![e_var("value")])),
            ],
            vec![],
            None,
        ),
        s_throw(e_new("DateObjectError", vec![e_str("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor")])),
    ])
}

/// `DatePeriod::__elephc_datetime_interface_timestamp` — transcribed method builder.
fn decl_class_dateperiod_method_7_elephc_datetime_interface_timestamp() -> MethodBuilder {
method("__elephc_datetime_interface_timestamp")
    .private()
    .param("value", t_mixed())
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_instance_of(e_var("value"), "DateTimeImmutable"),
            vec![
                s_if(
                    e_not(e_method_call(e_var("value"), "__elephc_is_initialized", vec![])),
                    vec![
                        s_throw(e_new("DateObjectError", vec![e_str("Object of type DateTimeInterface has not been correctly initialized by calling parent::__construct() in its constructor")])),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_method_call(e_var("value"), "getTimestamp", vec![])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_instance_of(e_var("value"), "DateTime"),
            vec![
                s_if(
                    e_not(e_method_call(e_var("value"), "__elephc_is_initialized", vec![])),
                    vec![
                        s_throw(e_new("DateObjectError", vec![e_str("Object of type DateTimeInterface has not been correctly initialized by calling parent::__construct() in its constructor")])),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_method_call(e_var("value"), "getTimestamp", vec![])),
            ],
            vec![],
            None,
        ),
        s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
    ])
}

/// `DatePeriod::__elephc_add_interval` — transcribed method builder.
fn decl_class_dateperiod_method_8_elephc_add_interval() -> MethodBuilder {
method("__elephc_add_interval")
    .private()
    .param("value", t_mixed())
    .param("interval", t_class("DateInterval"))
    .returns(t_class("DateTimeInterface"))
    .body_exact(vec![
        s_if(
            e_instance_of(e_var("value"), "DateTimeImmutable"),
            vec![
                s_return(e_method_call(e_var("value"), "add", vec![e_var("interval")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_instance_of(e_var("value"), "DateTime"),
            vec![
                s_expr(e_method_call(e_var("value"), "add", vec![e_var("interval")])),
                s_return(e_var("value")),
            ],
            vec![],
            None,
        ),
        s_throw(e_new("DateObjectError", vec![e_str("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor")])),
    ])
}

/// `DatePeriod::_advance` — transcribed method builder.
fn decl_class_dateperiod_method_9_advance() -> MethodBuilder {
method("_advance")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("cursor", e_this_prop("_cursor")),
        s_assign("interval", e_method_call(e_this(), "getDateInterval", vec![])),
        s_prop_assign(e_this(), "_cursor", e_method_call(e_this(), "__elephc_add_interval", vec![e_var("cursor"), e_var("interval")])),
    ])
}

/// `DatePeriod::rewind` — transcribed method builder.
fn decl_class_dateperiod_method_10_rewind() -> MethodBuilder {
method("rewind")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_prop_assign(e_this(), "_cursor", e_method_call(e_this(), "__elephc_clone_datetime_interface_storage", vec![e_this_prop("_start")])),
        s_prop_assign(e_this(), "idx", e_int(0)),
        s_if(
            e_this_prop("excludeStart"),
            vec![
                s_expr(e_method_call(e_this(), "_advance", vec![])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DatePeriod::valid` — transcribed method builder.
fn decl_class_dateperiod_method_11_valid() -> MethodBuilder {
method("valid")
    .private()
    .returns(TypeExpr::Bool)
    .body_exact(vec![
        s_if(
            e_this_prop("useCount"),
            vec![
                s_assign("includedEnd", e_ternary(e_binop(e_this_prop("includeEnd"), BinOp::StrictNotEq, e_int(0)), e_int(1), e_int(0))),
                s_return(e_binop(e_this_prop("idx"), BinOp::LtEq, e_binop(e_binop(e_this_prop("_recurrence_count"), BinOp::Sub, e_this_prop("excludeStart")), BinOp::Add, e_var("includedEnd")))),
            ],
            vec![],
            None,
        ),
        s_assign("cursor", e_this_prop("_cursor")),
        s_assign("end", e_this_prop("_end")),
        s_if(
            e_binop(e_not(e_instance_of(e_var("cursor"), "DateTimeInterface")), BinOp::Or, e_not(e_instance_of(e_var("end"), "DateTimeInterface"))),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor")])),
            ],
            vec![],
            None,
        ),
        s_assign("cursorTimestamp", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("cursor")])),
        s_assign("endTimestamp", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("end")])),
        s_if(
            e_binop(e_var("cursorTimestamp"), BinOp::Lt, e_var("endTimestamp")),
            vec![
                s_return(e_bool(true)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("cursorTimestamp"), BinOp::Gt, e_var("endTimestamp")),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("cursorMicrosecond", e_method_call(e_var("cursor"), "getMicrosecond", vec![])),
        s_assign("endMicrosecond", e_method_call(e_var("end"), "getMicrosecond", vec![])),
        s_if(
            e_binop(e_var("cursorMicrosecond"), BinOp::Lt, e_var("endMicrosecond")),
            vec![
                s_return(e_bool(true)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("cursorMicrosecond"), BinOp::Gt, e_var("endMicrosecond")),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_return(e_binop(e_this_prop("includeEnd"), BinOp::StrictNotEq, e_int(0))),
    ])
}

/// `DatePeriod::current` — transcribed method builder.
fn decl_class_dateperiod_method_12_current() -> MethodBuilder {
method("current")
    .private()
    .returns(t_class("DateTimeInterface"))
    .body_exact(vec![
        s_assign("cursor", e_this_prop("_cursor")),
        s_if(
            e_not(e_instance_of(e_var("cursor"), "DateTimeInterface")),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor")])),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_this(), "__elephc_clone_iterator_value", vec![e_var("cursor")])),
    ])
}

/// `DatePeriod::key` — transcribed method builder.
fn decl_class_dateperiod_method_13_key() -> MethodBuilder {
method("key")
    .private()
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_this_prop("idx")),
    ])
}

/// `DatePeriod::next` — transcribed method builder.
fn decl_class_dateperiod_method_14_next() -> MethodBuilder {
method("next")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "_advance", vec![])),
        s_prop_assign(e_this(), "idx", e_binop(e_this_prop("idx"), BinOp::Add, e_int(1))),
    ])
}

/// `DatePeriod::getStartDate` — transcribed method builder.
fn decl_class_dateperiod_method_15_getstartdate() -> MethodBuilder {
method("getStartDate")
    .returns(t_class("DateTimeInterface"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("start", e_this_prop("_start")),
        s_if(
            e_not(e_instance_of(e_var("start"), "DateTimeInterface")),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor")])),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_this(), "__elephc_clone_datetime_interface", vec![e_var("start")])),
    ])
}

/// `DatePeriod::getEndDate` — transcribed method builder.
fn decl_class_dateperiod_method_16_getenddate() -> MethodBuilder {
method("getEndDate")
    .returns(t_nullable(t_class("DateTimeInterface")))
    .body_exact(vec![
        s_assign("end", e_this_prop("_end")),
        s_if(
            e_binop(e_var("end"), BinOp::StrictEq, e_null()),
            vec![
                s_return(e_null()),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_this(), "__elephc_clone_datetime_interface", vec![e_var("end")])),
    ])
}

/// `DatePeriod::getDateInterval` — transcribed method builder.
fn decl_class_dateperiod_method_17_getdateinterval() -> MethodBuilder {
method("getDateInterval")
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("interval", e_this_prop("_interval")),
        s_if(
            e_not(e_instance_of(e_var("interval"), "DateInterval")),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor")])),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_var("interval"), "__elephc_clone", vec![])),
    ])
}

/// `DatePeriod::getRecurrences` — transcribed method builder.
fn decl_class_dateperiod_method_18_getrecurrences() -> MethodBuilder {
method("getRecurrences")
    .returns(t_nullable(TypeExpr::Int))
    .body_exact(vec![
        s_if(
            e_binop(e_this_prop("_recurrence_count"), BinOp::Eq, e_int(0)),
            vec![
                s_return(e_null()),
            ],
            vec![],
            Some(vec![
            s_return(e_this_prop("_recurrence_count")),
        ]),
        ),
    ])
}

/// `DatePeriod::getIterator` — transcribed method builder.
fn decl_class_dateperiod_method_19_getiterator() -> MethodBuilder {
method("getIterator")
    .returns(t_class("Iterator"))
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor")])),
            ],
            vec![],
            None,
        ),
        s_assign("items", e_array(vec![])),
        s_expr(e_method_call(e_this(), "rewind", vec![])),
        s_while(e_method_call(e_this(), "valid", vec![]), vec![
            s_array_push("items", e_method_call(e_this(), "current", vec![])),
            s_expr(e_method_call(e_this(), "next", vec![])),
        ]),
        s_assign("onCurrent", closure()
            .param_untyped("value")
            .returns(t_mixed())
            .body(vec![
                s_if(
                    e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                    vec![
                        s_assign("current", e_method_call(e_this(), "current", vec![])),
                        s_assign("result", e_var("current")),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("result", e_method_call(e_this(), "__elephc_clone_datetime_interface", vec![e_var("value")])),
                    s_assign("current", e_method_call(e_this(), "__elephc_clone_datetime_interface_storage", vec![e_var("value")])),
                ]),
                ),
                s_prop_assign(e_this(), "_current", e_var("current")),
                s_return(e_var("result")),
            ])
            .build()),
        s_return(e_new("InternalIterator", vec![e_var("items"), e_var("onCurrent")])),
    ])
}

/// `DatePeriod::createFromISO8601String` — transcribed method builder.
fn decl_class_dateperiod_method_20_createfromiso8601string() -> MethodBuilder {
method("createFromISO8601String")
    .static_()
    .param("specification", TypeExpr::Str)
    .param_default("options", TypeExpr::Int, e_int(0))
    .returns(t_class("static"))
    .body_exact(vec![
        s_assign("result", e_null()),
        s_if(
            e_binop(e_static_class(), BinOp::StrictEq, e_named_class("DatePeriod")),
            vec![
                s_assign("result", e_call("__elephc_new_instance_without_constructor", vec![e_str("DatePeriod")])),
            ],
            vec![],
            Some(vec![
            s_assign("result", e_call("__elephc_new_instance_without_constructor", vec![e_static_class()])),
        ]),
        ),
        s_assign("typedResult", e_method_call(e_var("result"), "__elephc_factory_result", vec![])),
        s_expr(e_call("unset", vec![e_var("result")])),
        s_assign("parsed", e_call("__elephc_timelib_period_parse", vec![e_var("specification")])),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("status")), BinOp::StrictNotEq, e_str("P")),
            vec![
                s_throw(e_new("DateMalformedPeriodStringException", vec![e_binop(e_binop(e_str("Unknown or bad format ("), BinOp::Concat, e_var("specification")), BinOp::Concat, e_str(")"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_index(e_var("parsed"), e_str("has_start"))),
            vec![
                s_throw(e_new("DateMalformedPeriodStringException", vec![e_binop(e_binop(e_str("DatePeriod::createFromISO8601String(): ISO interval must contain a start date, \""), BinOp::Concat, e_var("specification")), BinOp::Concat, e_str("\" given"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_index(e_var("parsed"), e_str("has_interval"))),
            vec![
                s_throw(e_new("DateMalformedPeriodStringException", vec![e_binop(e_binop(e_str("DatePeriod::createFromISO8601String(): ISO interval must contain an interval, \""), BinOp::Concat, e_var("specification")), BinOp::Concat, e_str("\" given"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_not(e_index(e_var("parsed"), e_str("has_end"))), BinOp::And, e_binop(e_index(e_var("parsed"), e_str("recurrences")), BinOp::StrictEq, e_int(0))),
            vec![
                s_throw(e_new("DateMalformedPeriodStringException", vec![e_binop(e_binop(e_str("DatePeriod::createFromISO8601String(): ISO interval must contain an end date or a recurrence count, \""), BinOp::Concat, e_var("specification")), BinOp::Concat, e_str("\" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("start", e_static_call("DateTimeImmutable", "createFromTimestamp", vec![e_index(e_var("parsed"), e_str("start"))])),
        s_assign("endTimestamp", e_int(0)),
        s_if(
            e_index(e_var("parsed"), e_str("has_end")),
            vec![
                s_assign("endTimestamp", e_index(e_var("parsed"), e_str("end"))),
            ],
            vec![],
            None,
        ),
        s_assign("interval", e_new("DateInterval", vec![e_str("PT0S")])),
        s_prop_assign(e_var("interval"), "y", e_index(e_var("parsed"), e_str("y"))),
        s_prop_assign(e_var("interval"), "m", e_index(e_var("parsed"), e_str("m"))),
        s_prop_assign(e_var("interval"), "d", e_index(e_var("parsed"), e_str("d"))),
        s_prop_assign(e_var("interval"), "h", e_index(e_var("parsed"), e_str("h"))),
        s_prop_assign(e_var("interval"), "i", e_index(e_var("parsed"), e_str("i"))),
        s_prop_assign(e_var("interval"), "s", e_index(e_var("parsed"), e_str("s"))),
        s_prop_assign(e_var("interval"), "f", e_binop(e_index(e_var("parsed"), e_str("us")), BinOp::Div, e_float(1000000.0))),
        s_if(
            e_index(e_var("parsed"), e_str("has_end")),
            vec![
                s_expr(e_method_call(e_var("typedResult"), "__elephc_initialize_end_components", vec![e_var("start"), e_var("interval"), e_var("endTimestamp"), e_var("options")])),
            ],
            vec![],
            Some(vec![
            s_expr(e_method_call(e_var("typedResult"), "__elephc_initialize_recurrence_components", vec![e_var("start"), e_var("interval"), e_index(e_var("parsed"), e_str("recurrences")), e_var("options")])),
        ]),
        ),
        s_expr(e_call("unset", vec![e_var("interval")])),
        s_expr(e_call("unset", vec![e_var("start")])),
        s_expr(e_call("unset", vec![e_var("parsed")])),
        s_return(e_var("typedResult")),
    ])
}

/// `DatePeriod::__elephc_deprecated_string_constructor` — transcribed method builder.
fn decl_class_dateperiod_method_21_elephc_deprecated_string_constructor() -> MethodBuilder {
method("__elephc_deprecated_string_constructor")
    .static_()
    .param("specification", TypeExpr::Str)
    .param_default("options", TypeExpr::Int, e_int(0))
    .param_default("line", TypeExpr::Int, e_int(0))
    .returns(t_class("DatePeriod"))
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: Calling DatePeriod::__construct(string $isostr, int $options = 0) is deprecated, use DatePeriod::createFromISO8601String() instead"), e_var("line"), e_const("E_DEPRECATED")])),
        s_return(e_static_call("DatePeriod", "createFromISO8601String", vec![e_var("specification"), e_var("options")])),
    ])
}

/// `DatePeriod::__elephc_initialize_from_iso8601_string` — transcribed method builder.
fn decl_class_dateperiod_method_22_elephc_initialize_from_iso8601_string() -> MethodBuilder {
method("__elephc_initialize_from_iso8601_string")
    .private()
    .param("start", TypeExpr::Str)
    .param_default("interval", TypeExpr::Int, e_int(0))
    .param_default("line", TypeExpr::Int, e_int(0))
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("__elephc_options", e_var("interval")),
        s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: Calling DatePeriod::__construct(string $isostr, int $options = 0) is deprecated, use DatePeriod::createFromISO8601String() instead"), e_var("line"), e_const("E_DEPRECATED")])),
        s_assign("parsed", e_call("__elephc_timelib_period_parse", vec![e_var("start")])),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("status")), BinOp::StrictNotEq, e_str("P")),
            vec![
                s_throw(e_new("DateMalformedPeriodStringException", vec![e_binop(e_binop(e_str("Unknown or bad format ("), BinOp::Concat, e_var("start")), BinOp::Concat, e_str(")"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_index(e_var("parsed"), e_str("has_start"))),
            vec![
                s_throw(e_new("DateMalformedPeriodStringException", vec![e_binop(e_binop(e_str("DatePeriod::__construct(): ISO interval must contain a start date, \""), BinOp::Concat, e_var("start")), BinOp::Concat, e_str("\" given"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_index(e_var("parsed"), e_str("has_interval"))),
            vec![
                s_throw(e_new("DateMalformedPeriodStringException", vec![e_binop(e_binop(e_str("DatePeriod::__construct(): ISO interval must contain an interval, \""), BinOp::Concat, e_var("start")), BinOp::Concat, e_str("\" given"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_not(e_index(e_var("parsed"), e_str("has_end"))), BinOp::And, e_binop(e_index(e_var("parsed"), e_str("recurrences")), BinOp::StrictEq, e_int(0))),
            vec![
                s_throw(e_new("DateMalformedPeriodStringException", vec![e_binop(e_binop(e_str("DatePeriod::__construct(): ISO interval must contain an end date or a recurrence count, \""), BinOp::Concat, e_var("start")), BinOp::Concat, e_str("\" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("periodStart", e_static_call("DateTimeImmutable", "createFromTimestamp", vec![e_index(e_var("parsed"), e_str("start"))])),
        s_assign("periodInterval", e_new("DateInterval", vec![e_str("PT0S")])),
        s_prop_assign(e_var("periodInterval"), "y", e_index(e_var("parsed"), e_str("y"))),
        s_prop_assign(e_var("periodInterval"), "m", e_index(e_var("parsed"), e_str("m"))),
        s_prop_assign(e_var("periodInterval"), "d", e_index(e_var("parsed"), e_str("d"))),
        s_prop_assign(e_var("periodInterval"), "h", e_index(e_var("parsed"), e_str("h"))),
        s_prop_assign(e_var("periodInterval"), "i", e_index(e_var("parsed"), e_str("i"))),
        s_prop_assign(e_var("periodInterval"), "s", e_index(e_var("parsed"), e_str("s"))),
        s_prop_assign(e_var("periodInterval"), "f", e_binop(e_index(e_var("parsed"), e_str("us")), BinOp::Div, e_float(1000000.0))),
        s_if(
            e_index(e_var("parsed"), e_str("has_end")),
            vec![
                s_assign("periodEnd", e_static_call("DateTimeImmutable", "createFromTimestamp", vec![e_index(e_var("parsed"), e_str("end"))])),
                s_expr(e_method_call(e_this(), "__construct", vec![e_var("periodStart"), e_var("periodInterval"), e_var("periodEnd"), e_var("__elephc_options")])),
            ],
            vec![],
            Some(vec![
            s_expr(e_method_call(e_this(), "__construct", vec![e_var("periodStart"), e_var("periodInterval"), e_index(e_var("parsed"), e_str("recurrences")), e_var("__elephc_options")])),
        ]),
        ),
    ])
}

/// `DatePeriod::__elephc_initialize_from_argument_array` — transcribed method builder.
fn decl_class_dateperiod_method_23_elephc_initialize_from_argument_array() -> MethodBuilder {
method("__elephc_initialize_from_argument_array")
    .private()
    .param("arguments", t_mixed())
    .param("line", TypeExpr::Int)
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("count", e_call("count", vec![e_var("arguments")])),
        s_if(
            e_binop(e_var("count"), BinOp::StrictEq, e_int(0)),
            vec![
                s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
            ],
            vec![],
            None,
        ),
        s_assign("hasStart", e_bool(false)),
        s_assign("hasInterval", e_bool(false)),
        s_assign("hasEnd", e_bool(false)),
        s_assign("hasOptions", e_bool(false)),
        s_assign("nextPosition", e_int(0)),
        s_assign("seenNamed", e_bool(false)),
        s_foreach(e_var("arguments"), Some("key"), "value", vec![
            s_if(
                e_call("is_int", vec![e_var("key")]),
                vec![
                    s_if(
                        e_var("seenNamed"),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Cannot use positional argument after named argument during unpacking")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("nextPosition"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_assign("start", e_var("value")),
                            s_assign("hasStart", e_bool(true)),
                        ],
                        vec![
                        (e_binop(e_var("nextPosition"), BinOp::StrictEq, e_int(1)), vec![
                            s_assign("interval", e_var("value")),
                            s_assign("hasInterval", e_bool(true)),
                        ]),
                        (e_binop(e_var("nextPosition"), BinOp::StrictEq, e_int(2)), vec![
                            s_assign("end", e_var("value")),
                            s_assign("hasEnd", e_bool(true)),
                        ]),
                        (e_binop(e_var("nextPosition"), BinOp::StrictEq, e_int(3)), vec![
                            s_assign("options", e_var("value")),
                            s_assign("hasOptions", e_bool(true)),
                        ]),
                    ],
                        Some(vec![
                        s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
                    ]),
                    ),
                    s_expr(e_post_inc("nextPosition")),
                ],
                vec![],
                Some(vec![
                s_assign("seenNamed", e_bool(true)),
                s_if(
                    e_binop(e_var("key"), BinOp::StrictEq, e_str("start")),
                    vec![
                        s_if(
                            e_var("hasStart"),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Named parameter $start overwrites previous argument")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("start", e_var("value")),
                        s_assign("hasStart", e_bool(true)),
                    ],
                    vec![
                    (e_binop(e_var("key"), BinOp::StrictEq, e_str("interval")), vec![
                        s_if(
                            e_var("hasInterval"),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Named parameter $interval overwrites previous argument")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("interval", e_var("value")),
                        s_assign("hasInterval", e_bool(true)),
                    ]),
                    (e_binop(e_var("key"), BinOp::StrictEq, e_str("end")), vec![
                        s_if(
                            e_var("hasEnd"),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Named parameter $end overwrites previous argument")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("end", e_var("value")),
                        s_assign("hasEnd", e_bool(true)),
                    ]),
                    (e_binop(e_var("key"), BinOp::StrictEq, e_str("options")), vec![
                        s_if(
                            e_var("hasOptions"),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Named parameter $options overwrites previous argument")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("options", e_var("value")),
                        s_assign("hasOptions", e_bool(true)),
                    ]),
                ],
                    Some(vec![
                    s_throw(e_new("Error", vec![e_binop(e_str("Unknown named parameter $"), BinOp::Concat, e_var("key"))])),
                ]),
                ),
            ]),
            ),
        ]),
        s_if(
            e_not(e_var("hasStart")),
            vec![
                s_throw(e_new("ArgumentCountError", vec![e_str("DatePeriod::__construct(): Argument #1 ($start) not passed")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_not(e_var("hasInterval")), BinOp::And, e_binop(e_var("hasEnd"), BinOp::Or, e_var("hasOptions"))),
            vec![
                s_throw(e_new("ArgumentCountError", vec![e_str("DatePeriod::__construct(): Argument #2 ($interval) must be passed explicitly, because the default value is not known")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_not(e_var("hasEnd")), BinOp::And, e_var("hasOptions")),
            vec![
                s_throw(e_new("ArgumentCountError", vec![e_str("DatePeriod::__construct(): Argument #3 ($end) must be passed explicitly, because the default value is not known")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_var("hasEnd")),
            vec![
                s_assign("specification", e_var("start")),
                s_if(
                    e_binop(e_call("is_array", vec![e_var("specification")]), BinOp::Or, e_binop(e_call("is_object", vec![e_var("specification")]), BinOp::And, e_not(e_instance_of(e_var("specification"), "Stringable")))),
                    vec![
                        s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_call("is_null", vec![e_var("specification")]),
                    vec![
                        s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: DatePeriod::__construct(): Passing null to parameter #1 ($start) of type string is deprecated"), e_var("line"), e_const("E_DEPRECATED")])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("specification", e_cast(CastType::String, e_var("specification"))),
                s_assign("stringOptions", e_int(0)),
                s_if(
                    e_var("hasInterval"),
                    vec![
                        s_assign("stringOptions", e_var("interval")),
                        s_if(
                            e_not(e_binop(e_binop(e_binop(e_binop(e_call("is_int", vec![e_var("stringOptions")]), BinOp::Or, e_call("is_float", vec![e_var("stringOptions")])), BinOp::Or, e_call("is_bool", vec![e_var("stringOptions")])), BinOp::Or, e_call("is_null", vec![e_var("stringOptions")])), BinOp::Or, e_binop(e_call("is_string", vec![e_var("stringOptions")]), BinOp::And, e_call("is_numeric", vec![e_var("stringOptions")])))),
                            vec![
                                s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("stringOptions", e_cast(CastType::Int, e_var("stringOptions"))),
                    ],
                    vec![],
                    None,
                ),
                s_expr(e_method_call(e_this(), "__elephc_initialize_from_iso8601_string", vec![e_var("specification"), e_var("stringOptions"), e_var("line")])),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_not(e_instance_of(e_var("start"), "DateTimeInterface")), BinOp::Or, e_not(e_instance_of(e_var("interval"), "DateInterval"))),
            vec![
                s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
            ],
            vec![],
            None,
        ),
        s_assign("objectOptions", e_int(0)),
        s_if(
            e_var("hasOptions"),
            vec![
                s_assign("objectOptions", e_var("options")),
                s_if(
                    e_not(e_binop(e_binop(e_binop(e_binop(e_call("is_int", vec![e_var("objectOptions")]), BinOp::Or, e_call("is_float", vec![e_var("objectOptions")])), BinOp::Or, e_call("is_bool", vec![e_var("objectOptions")])), BinOp::Or, e_call("is_null", vec![e_var("objectOptions")])), BinOp::Or, e_binop(e_call("is_string", vec![e_var("objectOptions")]), BinOp::And, e_call("is_numeric", vec![e_var("objectOptions")])))),
                    vec![
                        s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("objectOptions", e_cast(CastType::Int, e_var("objectOptions"))),
            ],
            vec![],
            None,
        ),
        s_expr(e_method_call(e_this(), "__construct", vec![e_var("start"), e_var("interval"), e_var("end"), e_var("objectOptions")])),
    ])
}

/// `DatePeriod::__elephc_begin_argument_array` — transcribed method builder.
fn decl_class_dateperiod_method_24_elephc_begin_argument_array() -> MethodBuilder {
method("__elephc_begin_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_arguments", e_array(vec![])),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DatePeriod::__elephc_append_one_argument` — transcribed method builder.
fn decl_class_dateperiod_method_25_elephc_append_one_argument() -> MethodBuilder {
method("__elephc_append_one_argument")
    .private()
    .param("key", t_mixed())
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_if(
            e_call("is_int", vec![e_var("key")]),
            vec![
                s_if(
                    e_this_prop("__elephc_seen_named_argument"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Cannot use positional argument after named argument during unpacking")])),
                    ],
                    vec![],
                    None,
                ),
                s_array_push("arguments", e_var("value")),
                s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_string", vec![e_var("key")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Keys must be of type int|string during argument unpacking")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(true)),
        s_if(
            e_not(e_binop(e_binop(e_binop(e_binop(e_var("key"), BinOp::StrictEq, e_str("start")), BinOp::Or, e_binop(e_var("key"), BinOp::StrictEq, e_str("interval"))), BinOp::Or, e_binop(e_var("key"), BinOp::StrictEq, e_str("end"))), BinOp::Or, e_binop(e_var("key"), BinOp::StrictEq, e_str("options")))),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_str("Unknown named parameter $"), BinOp::Concat, e_var("key"))])),
            ],
            vec![],
            None,
        ),
        s_assign("parameterIndex", e_neg(e_int(1))),
        s_if(
            e_binop(e_var("key"), BinOp::StrictEq, e_str("start")),
            vec![
                s_assign("parameterIndex", e_int(0)),
            ],
            vec![
            (e_binop(e_var("key"), BinOp::StrictEq, e_str("interval")), vec![
                s_assign("parameterIndex", e_int(1)),
            ]),
            (e_binop(e_var("key"), BinOp::StrictEq, e_str("end")), vec![
                s_assign("parameterIndex", e_int(2)),
            ]),
        ],
            Some(vec![
            s_assign("parameterIndex", e_int(3)),
        ]),
        ),
        s_assign("positionalCount", e_int(0)),
        s_foreach(e_var("arguments"), Some("existingKey"), "existingValue", vec![
            s_if(
                e_call("is_int", vec![e_var("existingKey")]),
                vec![
                    s_expr(e_post_inc("positionalCount")),
                ],
                vec![],
                None,
            ),
        ]),
        s_if(
            e_binop(e_var("parameterIndex"), BinOp::Lt, e_var("positionalCount")),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("array_key_exists", vec![e_var("key"), e_var("arguments")]),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_array_assign("arguments", e_var("key"), e_var("value")),
        s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
    ])
}

/// `DatePeriod::__elephc_append_argument_chunk` — transcribed method builder.
fn decl_class_dateperiod_method_26_elephc_append_argument_chunk() -> MethodBuilder {
method("__elephc_append_argument_chunk")
    .private()
    .param("kind", TypeExpr::Int)
    .param("name", TypeExpr::Str)
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
            vec![
                s_if(
                    e_not(e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_instance_of(e_var("value"), "Traversable"))),
                    vec![
                        s_expr(e_static_call("DateTime", "__elephc_argument_type_error", vec![e_var("value"), e_str("Only arrays and Traversables can be unpacked, ")])),
                    ],
                    vec![],
                    None,
                ),
                s_foreach(e_var("value"), Some("key"), "unpackedValue", vec![
                    s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("key"), e_var("unpackedValue")])),
                ]),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
            vec![
                s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("name"), e_var("value")])),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_int(0), e_var("value")])),
    ])
}

/// `DatePeriod::__elephc_finish_argument_array` — transcribed method builder.
fn decl_class_dateperiod_method_27_elephc_finish_argument_array() -> MethodBuilder {
method("__elephc_finish_argument_array")
    .private()
    .param("line", TypeExpr::Int)
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_initialize_from_argument_array", vec![e_this_prop("__elephc_arguments"), e_var("line")])),
        s_prop_assign(e_this(), "__elephc_arguments", e_null()),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DatePeriod::__elephc_factory_result` — transcribed method builder.
fn decl_class_dateperiod_method_28_elephc_factory_result() -> MethodBuilder {
method("__elephc_factory_result")
    .final_()
    .returns(t_class("DatePeriod"))
    .body_exact(vec![
        s_return(e_this()),
    ])
}

/// `DatePeriod::__elephc_weak_options` — transcribed method builder.
fn decl_class_dateperiod_method_29_elephc_weak_options() -> MethodBuilder {
method("__elephc_weak_options")
    .private()
    .static_()
    .param("value", t_mixed())
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_call("is_int", vec![e_var("value")]), BinOp::Or, e_call("is_float", vec![e_var("value")])), BinOp::Or, e_call("is_bool", vec![e_var("value")])), BinOp::Or, e_call("is_null", vec![e_var("value")])), BinOp::Or, e_binop(e_call("is_string", vec![e_var("value")]), BinOp::And, e_call("is_numeric", vec![e_var("value")]))),
            vec![
                s_return(e_cast(CastType::Int, e_var("value"))),
            ],
            vec![],
            None,
        ),
        s_throw(e_new("TypeError", vec![e_str("DatePeriod::__construct() accepts (DateTimeInterface, DateInterval, int [, int]), or (DateTimeInterface, DateInterval, DateTime [, int]), or (string [, int]) as arguments")])),
    ])
}

/// `DatePeriod::__elephc_debug_dump` — transcribed method builder.
fn decl_class_dateperiod_method_30_elephc_debug_dump() -> MethodBuilder {
method("__elephc_debug_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_call("__elephc_var_dump_indent", vec![e_int(0)])])),
        s_assign("field_pad", e_binop(e_var("pad"), BinOp::Concat, e_str("  "))),
        s_assign("start", e_this_prop("start")),
        s_assign("current", e_this_prop("current")),
        s_assign("end", e_this_prop("end")),
        s_assign("interval", e_this_prop("interval")),
        s_assign("property_count", e_call("__elephc_var_dump_object_property_count", vec![e_this()])),
        s_echo(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("pad"), BinOp::Concat, e_str("object(")), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(")#")), BinOp::Concat, e_call("spl_object_id", vec![e_this()])), BinOp::Concat, e_str(" (")), BinOp::Concat, e_binop(e_var("property_count"), BinOp::Add, e_int(7))), BinOp::Concat, e_str(") {\n"))),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
        s_expr(e_call("__elephc_var_dump_object_properties", vec![e_this()])),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"start\"]=>\n"))),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
        s_expr(e_method_call(e_var("start"), "__elephc_debug_dump", vec![])),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"current\"]=>\n"))),
        s_if(
            e_binop(e_var("current"), BinOp::StrictEq, e_null()),
            vec![
                s_echo(e_var("field_pad")),
                s_expr(e_call("var_dump", vec![e_null()])),
            ],
            vec![],
            Some(vec![
            s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
            s_expr(e_method_call(e_var("current"), "__elephc_debug_dump", vec![])),
            s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        ]),
        ),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"end\"]=>\n"))),
        s_if(
            e_binop(e_var("end"), BinOp::StrictEq, e_null()),
            vec![
                s_echo(e_var("field_pad")),
                s_expr(e_call("var_dump", vec![e_null()])),
            ],
            vec![],
            Some(vec![
            s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
            s_expr(e_method_call(e_var("end"), "__elephc_debug_dump", vec![])),
            s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        ]),
        ),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"interval\"]=>\n"))),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
        s_expr(e_method_call(e_var("interval"), "__elephc_debug_dump", vec![])),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"recurrences\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("recurrences")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"include_start_date\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("include_start_date")])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"include_end_date\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("include_end_date")])),
        s_echo(e_binop(e_var("pad"), BinOp::Concat, e_str("}\n"))),
    ])
}

/// `DatePeriod::__elephc_assert_initialized` — transcribed method builder.
fn decl_class_dateperiod_method_31_elephc_assert_initialized() -> MethodBuilder {
method("__elephc_assert_initialized")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_assign("objectClass", e_call("get_class", vec![e_this()])),
                s_assign("inheritance", e_ternary(e_binop(e_var("objectClass"), BinOp::StrictEq, e_str("DatePeriod")), e_str(""), e_str(" (inheriting DatePeriod)"))),
                s_throw(e_new("DateObjectError", vec![e_binop(e_binop(e_binop(e_str("Object of type "), BinOp::Concat, e_var("objectClass")), BinOp::Concat, e_var("inheritance")), BinOp::Concat, e_str(" has not been correctly initialized by calling parent::__construct() in its constructor"))])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DatePeriod::__elephc_assert_iterable_initialized` — transcribed method builder.
fn decl_class_dateperiod_method_32_elephc_assert_iterable_initialized() -> MethodBuilder {
method("__elephc_assert_iterable_initialized")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor")])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DatePeriod::__elephc_assert_foreach_by_reference` — transcribed method builder.
fn decl_class_dateperiod_method_33_elephc_assert_foreach_by_reference() -> MethodBuilder {
method("__elephc_assert_foreach_by_reference")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_throw(e_new("Error", vec![e_str("An iterator cannot be used with foreach by reference")])),
    ])
}

/// `DatePeriod::__propget_start` — transcribed method builder.
fn decl_class_dateperiod_method_34_propget_start() -> MethodBuilder {
method("__propget_start")
    .returns(t_nullable(t_union(vec![t_class("DateTime"), t_class("DateTimeImmutable")])))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("value", e_this_prop("_start")),
        s_if(
            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
            vec![
                s_return(e_null()),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_this(), "__elephc_clone_datetime_interface", vec![e_var("value")])),
    ])
}

/// `DatePeriod::__propget_current` — transcribed method builder.
fn decl_class_dateperiod_method_35_propget_current() -> MethodBuilder {
method("__propget_current")
    .returns(t_nullable(t_union(vec![t_class("DateTime"), t_class("DateTimeImmutable")])))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("value", e_this_prop("_current")),
        s_if(
            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
            vec![
                s_return(e_null()),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_this(), "__elephc_clone_datetime_interface", vec![e_var("value")])),
    ])
}

/// `DatePeriod::__propget_end` — transcribed method builder.
fn decl_class_dateperiod_method_36_propget_end() -> MethodBuilder {
method("__propget_end")
    .returns(t_nullable(t_union(vec![t_class("DateTime"), t_class("DateTimeImmutable")])))
    .body_exact(vec![
        s_assign("value", e_this_prop("_end")),
        s_if(
            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
            vec![
                s_return(e_null()),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_this(), "__elephc_clone_datetime_interface", vec![e_var("value")])),
    ])
}

/// `DatePeriod::__propget_interval` — transcribed method builder.
fn decl_class_dateperiod_method_37_propget_interval() -> MethodBuilder {
method("__propget_interval")
    .returns(t_nullable(t_class("DateInterval")))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("value", e_this_prop("_interval")),
        s_if(
            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
            vec![
                s_return(e_null()),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_var("value"), "__elephc_clone", vec![])),
    ])
}

/// `DatePeriod::__propget_recurrences` — transcribed method builder.
fn decl_class_dateperiod_method_38_propget_recurrences() -> MethodBuilder {
method("__propget_recurrences")
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_this_prop("_recurrences")),
    ])
}

/// `DatePeriod::__propget_include_start_date` — transcribed method builder.
fn decl_class_dateperiod_method_39_propget_include_start_date() -> MethodBuilder {
method("__propget_include_start_date")
    .returns(TypeExpr::Bool)
    .body_exact(vec![
        s_return(e_this_prop("_include_start_date")),
    ])
}

/// `DatePeriod::__propget_include_end_date` — transcribed method builder.
fn decl_class_dateperiod_method_40_propget_include_end_date() -> MethodBuilder {
method("__propget_include_end_date")
    .returns(TypeExpr::Bool)
    .body_exact(vec![
        s_return(e_this_prop("_include_end_date")),
    ])
}

/// `DatePeriod::__wakeup` — transcribed method builder.
fn decl_class_dateperiod_method_41_wakeup() -> MethodBuilder {
method("__wakeup")
    .attr("\\Deprecated", vec![e_named_arg("since", e_str("8.5")), e_named_arg("message", e_str("this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()"))])
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_str("Deprecated: Method DatePeriod::__wakeup() is deprecated since 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()\n"), e_int(0), e_const("E_DEPRECATED")])),
        s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
    ])
}

/// `DatePeriod::__serialize` — transcribed method builder.
fn decl_class_dateperiod_method_42_serialize() -> MethodBuilder {
method("__serialize")
    .returns(t_array())
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_array_assoc(vec![(e_str("start"), e_this_prop("start")), (e_str("current"), e_this_prop("current")), (e_str("end"), e_this_prop("end")), (e_str("interval"), e_this_prop("interval")), (e_str("recurrences"), e_this_prop("recurrences")), (e_str("include_start_date"), e_this_prop("include_start_date")), (e_str("include_end_date"), e_this_prop("include_end_date"))])),
    ])
}

/// `DatePeriod::__unserialize` — transcribed method builder.
fn decl_class_dateperiod_method_43_unserialize() -> MethodBuilder {
method("__unserialize")
    .param("data", t_array())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_call("array_key_exists", vec![e_str("start"), e_var("data")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
            ],
            vec![],
            None,
        ),
        s_assign("serializedStart", e_index(e_var("data"), e_str("start"))),
        s_if(
            e_binop(e_var("serializedStart"), BinOp::StrictNotEq, e_null()),
            vec![
                s_if(
                    e_instance_of(e_var("serializedStart"), "DateTimeImmutable"),
                    vec![
                        s_if(
                            e_not(e_method_call(e_var("serializedStart"), "__elephc_is_initialized", vec![])),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("startSnapshot", e_method_call(e_var("serializedStart"), "__elephc_clone_for_period_storage", vec![])),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_instance_of(e_var("serializedStart"), "DateTime"),
                        vec![
                            s_if(
                                e_not(e_method_call(e_var("serializedStart"), "__elephc_is_initialized", vec![])),
                                vec![
                                    s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("startSnapshot", e_method_call(e_var("serializedStart"), "__elephc_clone_for_period_storage", vec![])),
                        ],
                        vec![],
                        Some(vec![
                        s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                    ]),
                    ),
                ]),
                ),
                s_prop_assign(e_this(), "_start", e_var("startSnapshot")),
                s_prop_assign(e_this(), "startTs", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("startSnapshot")])),
                s_prop_assign(e_this(), "startIsImmutable", e_instance_of(e_var("startSnapshot"), "DateTimeImmutable")),
                s_prop_assign(e_this(), "curTs", e_this_prop("startTs")),
                s_prop_assign(e_this(), "idx", e_int(0)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("array_key_exists", vec![e_str("end"), e_var("data")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
            ],
            vec![],
            None,
        ),
        s_assign("serializedEnd", e_index(e_var("data"), e_str("end"))),
        s_if(
            e_binop(e_var("serializedEnd"), BinOp::StrictNotEq, e_null()),
            vec![
                s_if(
                    e_binop(e_this_prop("_start"), BinOp::StrictEq, e_null()),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_instance_of(e_var("serializedEnd"), "DateTimeImmutable"),
                    vec![
                        s_if(
                            e_not(e_method_call(e_var("serializedEnd"), "__elephc_is_initialized", vec![])),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("endSnapshot", e_method_call(e_var("serializedEnd"), "__elephc_clone_for_period_storage", vec![])),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_instance_of(e_var("serializedEnd"), "DateTime"),
                        vec![
                            s_if(
                                e_not(e_method_call(e_var("serializedEnd"), "__elephc_is_initialized", vec![])),
                                vec![
                                    s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("endSnapshot", e_method_call(e_var("serializedEnd"), "__elephc_clone_for_period_storage", vec![])),
                        ],
                        vec![],
                        Some(vec![
                        s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                    ]),
                    ),
                ]),
                ),
                s_prop_assign(e_this(), "_end", e_var("endSnapshot")),
                s_prop_assign(e_this(), "endTs", e_method_call(e_this(), "__elephc_datetime_interface_timestamp", vec![e_var("endSnapshot")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("array_key_exists", vec![e_str("current"), e_var("data")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
            ],
            vec![],
            None,
        ),
        s_assign("serializedCurrent", e_index(e_var("data"), e_str("current"))),
        s_if(
            e_binop(e_var("serializedCurrent"), BinOp::StrictNotEq, e_null()),
            vec![
                s_if(
                    e_binop(e_this_prop("_start"), BinOp::StrictEq, e_null()),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_instance_of(e_var("serializedCurrent"), "DateTimeImmutable"),
                    vec![
                        s_if(
                            e_not(e_method_call(e_var("serializedCurrent"), "__elephc_is_initialized", vec![])),
                            vec![
                                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("currentSnapshot", e_method_call(e_var("serializedCurrent"), "__elephc_clone_for_period_storage", vec![])),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_instance_of(e_var("serializedCurrent"), "DateTime"),
                        vec![
                            s_if(
                                e_not(e_method_call(e_var("serializedCurrent"), "__elephc_is_initialized", vec![])),
                                vec![
                                    s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("currentSnapshot", e_method_call(e_var("serializedCurrent"), "__elephc_clone_for_period_storage", vec![])),
                        ],
                        vec![],
                        Some(vec![
                        s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
                    ]),
                    ),
                ]),
                ),
                s_prop_assign(e_this(), "_current", e_var("currentSnapshot")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("array_key_exists", vec![e_str("interval"), e_var("data")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
            ],
            vec![],
            None,
        ),
        s_assign("serializedInterval", e_index(e_var("data"), e_str("interval"))),
        s_if(
            e_binop(e_binop(e_not(e_instance_of(e_var("serializedInterval"), "DateInterval")), BinOp::Or, e_binop(e_call("get_class", vec![e_var("serializedInterval")]), BinOp::StrictNotEq, e_str("DateInterval"))), BinOp::Or, e_not(e_method_call(e_var("serializedInterval"), "__elephc_is_initialized", vec![]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
            ],
            vec![],
            None,
        ),
        s_assign("intervalSnapshot", e_method_call(e_var("serializedInterval"), "__elephc_clone_storage", vec![])),
        s_prop_assign(e_this(), "_interval", e_var("intervalSnapshot")),
        s_prop_assign(e_this(), "iv_y", e_prop(e_var("intervalSnapshot"), "y")),
        s_prop_assign(e_this(), "iv_m", e_prop(e_var("intervalSnapshot"), "m")),
        s_prop_assign(e_this(), "iv_d", e_prop(e_var("intervalSnapshot"), "d")),
        s_prop_assign(e_this(), "iv_h", e_prop(e_var("intervalSnapshot"), "h")),
        s_prop_assign(e_this(), "iv_i", e_prop(e_var("intervalSnapshot"), "i")),
        s_prop_assign(e_this(), "iv_s", e_prop(e_var("intervalSnapshot"), "s")),
        s_prop_assign(e_this(), "iv_invert", e_prop(e_var("intervalSnapshot"), "invert")),
        s_if(
            e_binop(e_binop(e_binop(e_not(e_call("array_key_exists", vec![e_str("recurrences"), e_var("data")])), BinOp::Or, e_not(e_call("is_int", vec![e_index(e_var("data"), e_str("recurrences"))]))), BinOp::Or, e_binop(e_index(e_var("data"), e_str("recurrences")), BinOp::Lt, e_int(0))), BinOp::Or, e_binop(e_index(e_var("data"), e_str("recurrences")), BinOp::Gt, e_int(2147483647))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "_recurrences", e_index(e_var("data"), e_str("recurrences"))),
        s_prop_assign(e_this(), "_recurrence_count", e_binop(e_binop(e_this_prop("_recurrences"), BinOp::Sub, e_ternary(e_this_prop("_include_start_date"), e_int(1), e_int(0))), BinOp::Sub, e_ternary(e_this_prop("_include_end_date"), e_int(1), e_int(0)))),
        s_if(
            e_binop(e_not(e_call("array_key_exists", vec![e_str("include_start_date"), e_var("data")])), BinOp::Or, e_not(e_call("is_bool", vec![e_index(e_var("data"), e_str("include_start_date"))]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "_include_start_date", e_index(e_var("data"), e_str("include_start_date"))),
        s_prop_assign(e_this(), "excludeStart", e_ternary(e_this_prop("_include_start_date"), e_int(0), e_int(1))),
        s_prop_assign(e_this(), "_recurrence_count", e_binop(e_binop(e_this_prop("_recurrences"), BinOp::Sub, e_ternary(e_this_prop("_include_start_date"), e_int(1), e_int(0))), BinOp::Sub, e_ternary(e_this_prop("_include_end_date"), e_int(1), e_int(0)))),
        s_if(
            e_binop(e_not(e_call("array_key_exists", vec![e_str("include_end_date"), e_var("data")])), BinOp::Or, e_not(e_call("is_bool", vec![e_index(e_var("data"), e_str("include_end_date"))]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DatePeriod object")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "_include_end_date", e_index(e_var("data"), e_str("include_end_date"))),
        s_prop_assign(e_this(), "includeEnd", e_ternary(e_this_prop("_include_end_date"), e_int(2), e_int(0))),
        s_prop_assign(e_this(), "_recurrence_count", e_binop(e_binop(e_this_prop("_recurrences"), BinOp::Sub, e_ternary(e_this_prop("_include_start_date"), e_int(1), e_int(0))), BinOp::Sub, e_ternary(e_this_prop("_include_end_date"), e_int(1), e_int(0)))),
        s_prop_assign(e_this(), "useCount", e_ternary(e_binop(e_this_prop("_end"), BinOp::StrictEq, e_null()), e_int(1), e_int(0))),
        s_prop_assign(e_this(), "_cursor", e_null()),
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
    ])
}

/// `DatePeriod::__set_state` — transcribed method builder.
fn decl_class_dateperiod_method_44_set_state() -> MethodBuilder {
method("__set_state")
    .static_()
    .param("array", t_array())
    .returns(t_class("DatePeriod"))
    .body_exact(vec![
        s_assign("result", e_new("DatePeriod", vec![e_new("DateTime", vec![e_str("@0")]), e_new("DateInterval", vec![e_str("P1D")]), e_int(1)])),
        s_prop_assign(e_var("result"), "_start", e_null()),
        s_prop_assign(e_var("result"), "_current", e_null()),
        s_prop_assign(e_var("result"), "_cursor", e_null()),
        s_prop_assign(e_var("result"), "_end", e_null()),
        s_prop_assign(e_var("result"), "_interval", e_null()),
        s_prop_assign(e_var("result"), "_recurrences", e_int(0)),
        s_prop_assign(e_var("result"), "_include_start_date", e_bool(false)),
        s_prop_assign(e_var("result"), "_include_end_date", e_bool(false)),
        s_prop_assign(e_var("result"), "startTs", e_int(0)),
        s_prop_assign(e_var("result"), "endTs", e_int(0)),
        s_prop_assign(e_var("result"), "startIsImmutable", e_bool(false)),
        s_prop_assign(e_var("result"), "excludeStart", e_int(0)),
        s_prop_assign(e_var("result"), "includeEnd", e_int(0)),
        s_prop_assign(e_var("result"), "curTs", e_int(0)),
        s_prop_assign(e_var("result"), "idx", e_int(0)),
        s_prop_assign(e_var("result"), "useCount", e_int(0)),
        s_prop_assign(e_var("result"), "_recurrence_count", e_int(0)),
        s_prop_assign(e_var("result"), "__elephc_initialized", e_bool(false)),
        s_expr(e_method_call(e_var("result"), "__unserialize", vec![e_var("array")])),
        s_return(e_var("result")),
    ])
}

/// `DatePeriod` — transcribed from the PHP form.
fn decl_class_dateperiod() -> Stmt {
    class("DatePeriod")
        .implements("IteratorAggregate")
        .implements("Traversable")
        .constant_full("EXCLUDE_START_DATE", e_int(1), Some(TypeExpr::Int), vec![])
        .constant_full("INCLUDE_END_DATE", e_int(2), Some(TypeExpr::Int), vec![])
        .private_prop("startTs", TypeExpr::Int, Some(e_int(0)))
        .private_prop("endTs", TypeExpr::Int, Some(e_int(0)))
        .private_prop("startIsImmutable", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("__elephc_initialized", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("__elephc_arguments", t_mixed(), Some(e_null()))
        .private_prop("__elephc_seen_named_argument", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("iv_y", TypeExpr::Int, Some(e_int(0)))
        .private_prop("iv_m", TypeExpr::Int, Some(e_int(0)))
        .private_prop("iv_d", TypeExpr::Int, Some(e_int(0)))
        .private_prop("iv_h", TypeExpr::Int, Some(e_int(0)))
        .private_prop("iv_i", TypeExpr::Int, Some(e_int(0)))
        .private_prop("iv_s", TypeExpr::Int, Some(e_int(0)))
        .private_prop("iv_invert", TypeExpr::Int, Some(e_int(0)))
        .private_prop("excludeStart", TypeExpr::Int, Some(e_int(0)))
        .private_prop("includeEnd", TypeExpr::Int, Some(e_int(0)))
        .private_prop("curTs", TypeExpr::Int, Some(e_int(0)))
        .private_prop("idx", TypeExpr::Int, Some(e_int(0)))
        .private_prop("useCount", TypeExpr::Int, Some(e_int(0)))
        .private_prop("_recurrence_count", TypeExpr::Int, Some(e_int(0)))
        .private_prop("_start", t_nullable(t_class("DateTimeInterface")), Some(e_null()))
        .private_prop("_current", t_nullable(t_class("DateTimeInterface")), Some(e_null()))
        .private_prop("_cursor", t_nullable(t_class("DateTimeInterface")), Some(e_null()))
        .private_prop("_end", t_nullable(t_class("DateTimeInterface")), Some(e_null()))
        .private_prop("_interval", t_nullable(t_class("DateInterval")), Some(e_null()))
        .private_prop("_recurrences", TypeExpr::Int, Some(e_int(0)))
        .private_prop("_include_start_date", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("_include_end_date", TypeExpr::Bool, Some(e_bool(false)))
        .virtual_get_prop("start", t_nullable(t_class("DateTimeInterface")))
        .virtual_get_prop("current", t_nullable(t_class("DateTimeInterface")))
        .virtual_get_prop("end", t_nullable(t_class("DateTimeInterface")))
        .virtual_get_prop("interval", t_nullable(t_class("DateInterval")))
        .virtual_get_prop("recurrences", TypeExpr::Int)
        .virtual_get_prop("include_start_date", TypeExpr::Bool)
        .virtual_get_prop("include_end_date", TypeExpr::Bool)
        .method(decl_class_dateperiod_method_0_construct())
        .method(decl_class_dateperiod_method_1_elephc_initialize_end_components())
        .method(decl_class_dateperiod_method_2_elephc_initialize_recurrence_components())
        .method(decl_class_dateperiod_method_3_elephc_weak_string_argument())
        .method(decl_class_dateperiod_method_4_elephc_clone_datetime_interface())
        .method(decl_class_dateperiod_method_5_elephc_clone_datetime_interface_storage())
        .method(decl_class_dateperiod_method_6_elephc_clone_iterator_value())
        .method(decl_class_dateperiod_method_7_elephc_datetime_interface_timestamp())
        .method(decl_class_dateperiod_method_8_elephc_add_interval())
        .method(decl_class_dateperiod_method_9_advance())
        .method(decl_class_dateperiod_method_10_rewind())
        .method(decl_class_dateperiod_method_11_valid())
        .method(decl_class_dateperiod_method_12_current())
        .method(decl_class_dateperiod_method_13_key())
        .method(decl_class_dateperiod_method_14_next())
        .method(decl_class_dateperiod_method_15_getstartdate())
        .method(decl_class_dateperiod_method_16_getenddate())
        .method(decl_class_dateperiod_method_17_getdateinterval())
        .method(decl_class_dateperiod_method_18_getrecurrences())
        .method(decl_class_dateperiod_method_19_getiterator())
        .method(decl_class_dateperiod_method_20_createfromiso8601string())
        .method(decl_class_dateperiod_method_21_elephc_deprecated_string_constructor())
        .method(decl_class_dateperiod_method_22_elephc_initialize_from_iso8601_string())
        .method(decl_class_dateperiod_method_23_elephc_initialize_from_argument_array())
        .method(decl_class_dateperiod_method_24_elephc_begin_argument_array())
        .method(decl_class_dateperiod_method_25_elephc_append_one_argument())
        .method(decl_class_dateperiod_method_26_elephc_append_argument_chunk())
        .method(decl_class_dateperiod_method_27_elephc_finish_argument_array())
        .method(decl_class_dateperiod_method_28_elephc_factory_result())
        .method(decl_class_dateperiod_method_29_elephc_weak_options())
        .method(decl_class_dateperiod_method_30_elephc_debug_dump())
        .method(decl_class_dateperiod_method_31_elephc_assert_initialized())
        .method(decl_class_dateperiod_method_32_elephc_assert_iterable_initialized())
        .method(decl_class_dateperiod_method_33_elephc_assert_foreach_by_reference())
        .method(decl_class_dateperiod_method_34_propget_start())
        .method(decl_class_dateperiod_method_35_propget_current())
        .method(decl_class_dateperiod_method_36_propget_end())
        .method(decl_class_dateperiod_method_37_propget_interval())
        .method(decl_class_dateperiod_method_38_propget_recurrences())
        .method(decl_class_dateperiod_method_39_propget_include_start_date())
        .method(decl_class_dateperiod_method_40_propget_include_end_date())
        .method(decl_class_dateperiod_method_41_wakeup())
        .method(decl_class_dateperiod_method_42_serialize())
        .method(decl_class_dateperiod_method_43_unserialize())
        .method(decl_class_dateperiod_method_44_set_state())
        .build()
}

/// `DateTime::__construct` — transcribed method builder.
fn decl_class_datetime_method_0_construct() -> MethodBuilder {
method("__construct")
    .param_default("datetime", TypeExpr::Str, e_str("now"))
    .param_default("timezone", t_nullable(t_class("DateTimeZone")), e_null())
    .body_exact(vec![
        s_assign("__originalDateTime", e_binop(e_var("datetime"), BinOp::Concat, e_str(""))),
        s_if(
            e_binop(e_binop(e_var("__originalDateTime"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("__originalDateTime"), BinOp::StrictEq, e_str("now"))),
            vec![
                s_static_prop_assign("DateTime", "lastParseResult", e_str("")),
            ],
            vec![],
            Some(vec![
            s_assign("__parseResult", e_static_call("DateTime", "__elephc_date_parse", vec![e_var("__originalDateTime")])),
            s_if(
                e_binop(e_binop(e_index(e_var("__parseResult"), e_str("error_count")), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_index(e_var("__parseResult"), e_str("warning_count")), BinOp::StrictEq, e_int(0))),
                vec![
                    s_static_prop_assign("DateTime", "lastParseResult", e_str("")),
                ],
                vec![],
                Some(vec![
                s_static_prop_assign("DateTime", "lastParseResult", e_var("__parseResult")),
            ]),
            ),
        ]),
        ),
        s_if(
            e_binop(e_var("datetime"), BinOp::StrictEq, e_str("")),
            vec![
                s_assign("datetime", e_str("now")),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "microsecond", e_static_call("DateTime", "__elephc_extract_micros", vec![e_var("datetime")])),
        s_assign("datetime", e_static_call("DateTime", "__elephc_strip_micros", vec![e_var("datetime")])),
        s_if(
            e_binop(e_call("substr", vec![e_var("__originalDateTime"), e_int(0), e_int(1)]), BinOp::StrictEq, e_str("@")),
            vec![
                s_assign("__ts", e_call("strtotime", vec![e_var("datetime")])),
                s_if(
                    e_binop(e_var("__ts"), BinOp::StrictEq, e_bool(false)),
                    vec![
                        s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str(""), e_var("__originalDateTime")])])),
                    ],
                    vec![],
                    None,
                ),
                s_prop_assign(e_this(), "timestamp", e_var("__ts")),
                s_prop_assign(e_this(), "timezone_name", e_str("+00:00")),
                s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_assign("__zoneData", e_call("explode", vec![e_str("\t"), e_static_call("DateTime", "__elephc_extract_constructor_zone", vec![e_var("datetime")])])),
        s_assign("__detectedZone", e_index(e_var("__zoneData"), e_int(0))),
        s_assign("datetime", e_index(e_var("__zoneData"), e_int(1))),
        s_if(
            e_binop(e_var("__detectedZone"), BinOp::StrictNotEq, e_str("")),
            vec![
                s_if(
                    e_binop(e_var("datetime"), BinOp::StrictEq, e_str("now")),
                    vec![
                        s_assign("__ts", e_call("microtime", vec![e_bool(true)])),
                        s_prop_assign(e_this(), "timestamp", e_call("intval", vec![e_var("__ts")])),
                        s_prop_assign(e_this(), "microsecond", e_call("intval", vec![e_binop(e_binop(e_var("__ts"), BinOp::Sub, e_this_prop("timestamp")), BinOp::Mul, e_int(1000000))])),
                        s_if(
                            e_binop(e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("__detectedZone")]), BinOp::StrictNotEq, e_int(3)),
                            vec![
                                s_assign("__saved", e_call("date_default_timezone_get", vec![])),
                                s_assign("__wall", e_call("date", vec![e_str("Y-m-d H:i:s"), e_this_prop("timestamp")])),
                                s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("__detectedZone")])])),
                                s_prop_assign(e_this(), "timestamp", e_call("strtotime", vec![e_var("__wall")])),
                                s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("__saved", e_call("date_default_timezone_get", vec![])),
                    s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("__detectedZone")])])),
                    s_assign("__ts", e_call("strtotime", vec![e_var("datetime")])),
                    s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
                    s_if(
                        e_binop(e_var("__ts"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str(""), e_var("__originalDateTime")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "timestamp", e_var("__ts")),
                ]),
                ),
                s_prop_assign(e_this(), "timezone_name", e_var("__detectedZone")),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_var("timezone"), BinOp::StrictEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_var("datetime"), BinOp::StrictEq, e_str("now")),
                        vec![
                            s_assign("__ts", e_call("microtime", vec![e_bool(true)])),
                            s_prop_assign(e_this(), "timestamp", e_call("intval", vec![e_var("__ts")])),
                            s_prop_assign(e_this(), "microsecond", e_call("intval", vec![e_binop(e_binop(e_var("__ts"), BinOp::Sub, e_this_prop("timestamp")), BinOp::Mul, e_int(1000000))])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("__ts", e_call("strtotime", vec![e_var("datetime")])),
                        s_if(
                            e_binop(e_var("__ts"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str(""), e_var("__originalDateTime")])])),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "timestamp", e_var("__ts")),
                    ]),
                    ),
                    s_prop_assign(e_this(), "timezone_name", e_call("date_default_timezone_get", vec![])),
                ],
                vec![],
                Some(vec![
                s_assign("tzname", e_method_call(e_var("timezone"), "getName", vec![])),
                s_if(
                    e_binop(e_var("datetime"), BinOp::StrictEq, e_str("now")),
                    vec![
                        s_assign("__ts", e_call("microtime", vec![e_bool(true)])),
                        s_prop_assign(e_this(), "timestamp", e_call("intval", vec![e_var("__ts")])),
                        s_prop_assign(e_this(), "microsecond", e_call("intval", vec![e_binop(e_binop(e_var("__ts"), BinOp::Sub, e_this_prop("timestamp")), BinOp::Mul, e_int(1000000))])),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("saved", e_call("date_default_timezone_get", vec![])),
                    s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("tzname")])])),
                    s_assign("__ts", e_call("strtotime", vec![e_var("datetime")])),
                    s_if(
                        e_binop(e_var("__ts"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_expr(e_call("date_default_timezone_set", vec![e_var("saved")])),
                            s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str(""), e_var("__originalDateTime")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "timestamp", e_var("__ts")),
                    s_expr(e_call("date_default_timezone_set", vec![e_var("saved")])),
                ]),
                ),
                s_prop_assign(e_this(), "timezone_name", e_var("tzname")),
            ]),
            ),
        ]),
        ),
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
    ])
}

/// `DateTime::getTimestamp` — transcribed method builder.
fn decl_class_datetime_method_1_gettimestamp() -> MethodBuilder {
method("getTimestamp")
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_this_prop("timestamp")),
    ])
}

/// `DateTime::getMicrosecond` — transcribed method builder.
fn decl_class_datetime_method_2_getmicrosecond() -> MethodBuilder {
method("getMicrosecond")
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_this_prop("microsecond")),
    ])
}

/// `DateTime::__elephc_set_microsecond_raw` — transcribed method builder.
fn decl_class_datetime_method_3_elephc_set_microsecond_raw() -> MethodBuilder {
method("__elephc_set_microsecond_raw")
    .param("microsecond", TypeExpr::Int)
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_prop_assign(e_this(), "microsecond", e_var("microsecond")),
    ])
}

/// `DateTime::getTimezone` — transcribed method builder.
fn decl_class_datetime_method_4_gettimezone() -> MethodBuilder {
method("getTimezone")
    .returns(t_class("DateTimeZone"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_new("DateTimeZone", vec![e_this_prop("timezone_name")])),
    ])
}

/// `DateTime::format` — transcribed method builder.
fn decl_class_datetime_method_5_format() -> MethodBuilder {
method("format")
    .param("format", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("saved", e_call("date_default_timezone_get", vec![])),
        s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_this_prop("timezone_name")])])),
        s_if(
            e_this_prop("__elephc_civil_override"),
            vec![
                s_assign("civil", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_this_prop("timezone_name"), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("__elephc_civil_year")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("__elephc_civil_month")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("__elephc_civil_day"))),
                s_assign("r", e_call("elephc_tz_format_civil", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_var("format"), e_call("strlen", vec![e_var("format")]), e_var("civil"), e_call("strlen", vec![e_var("civil")])])),
                s_expr(e_call("date_default_timezone_set", vec![e_var("saved")])),
                s_return(e_var("r")),
            ],
            vec![],
            None,
        ),
        s_assign("us", e_this_prop("microsecond")),
        s_assign("fmt", e_str("")),
        s_assign("flen", e_call("strlen", vec![e_var("format")])),
        s_assign("k", e_int(0)),
        s_while(e_binop(e_var("k"), BinOp::Lt, e_var("flen")), vec![
            s_assign("ch", e_index(e_var("format"), e_var("k"))),
            s_if(
                e_binop(e_var("ch"), BinOp::StrictEq, e_str("\\")),
                vec![
                    s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("ch"))),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_if(
                        e_binop(e_var("k"), BinOp::Lt, e_var("flen")),
                        vec![
                            s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_index(e_var("format"), e_var("k")))),
                            s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
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
                e_binop(e_var("ch"), BinOp::StrictEq, e_str("u")),
                vec![
                    s_assign("s", e_binop(e_str(""), BinOp::Concat, e_var("us"))),
                    s_while(e_binop(e_call("strlen", vec![e_var("s")]), BinOp::Lt, e_int(6)), vec![
                        s_assign("s", e_binop(e_str("0"), BinOp::Concat, e_var("s"))),
                    ]),
                    s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("s"))),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("ch"), BinOp::StrictEq, e_str("v")),
                vec![
                    s_assign("ms", e_call("intdiv", vec![e_var("us"), e_int(1000)])),
                    s_assign("s", e_binop(e_str(""), BinOp::Concat, e_var("ms"))),
                    s_while(e_binop(e_call("strlen", vec![e_var("s")]), BinOp::Lt, e_int(3)), vec![
                        s_assign("s", e_binop(e_str("0"), BinOp::Concat, e_var("s"))),
                    ]),
                    s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("s"))),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("T")), BinOp::And, e_binop(e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("timezone_name")]), BinOp::StrictEq, e_int(1))),
                vec![
                    s_assign("zoneLiteral", e_binop(e_binop(e_str("GMT"), BinOp::Concat, e_call("substr", vec![e_this_prop("timezone_name"), e_int(0), e_int(3)])), BinOp::Concat, e_call("substr", vec![e_this_prop("timezone_name"), e_int(4), e_int(2)]))),
                    s_assign("zoneLength", e_call("strlen", vec![e_var("zoneLiteral")])),
                    s_assign("zoneIndex", e_int(0)),
                    s_while(e_binop(e_var("zoneIndex"), BinOp::Lt, e_var("zoneLength")), vec![
                        s_assign("fmt", e_binop(e_binop(e_var("fmt"), BinOp::Concat, e_str("\\")), BinOp::Concat, e_index(e_var("zoneLiteral"), e_var("zoneIndex")))),
                        s_assign("zoneIndex", e_binop(e_var("zoneIndex"), BinOp::Add, e_int(1))),
                    ]),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("e")), BinOp::Or, e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("T")), BinOp::And, e_binop(e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("timezone_name")]), BinOp::StrictEq, e_int(2)))),
                vec![
                    s_assign("zoneLiteral", e_this_prop("timezone_name")),
                    s_assign("zoneLength", e_call("strlen", vec![e_var("zoneLiteral")])),
                    s_assign("zoneIndex", e_int(0)),
                    s_while(e_binop(e_var("zoneIndex"), BinOp::Lt, e_var("zoneLength")), vec![
                        s_assign("fmt", e_binop(e_binop(e_var("fmt"), BinOp::Concat, e_str("\\")), BinOp::Concat, e_index(e_var("zoneLiteral"), e_var("zoneIndex")))),
                        s_assign("zoneIndex", e_binop(e_var("zoneIndex"), BinOp::Add, e_int(1))),
                    ]),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("X")), BinOp::Or, e_binop(e_var("ch"), BinOp::StrictEq, e_str("x"))),
                vec![
                    s_assign("year", e_call("intval", vec![e_call("date", vec![e_str("Y"), e_this_prop("timestamp")])])),
                    s_if(
                        e_binop(e_var("year"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("year", e_neg(e_var("year"))),
                            s_assign("sign", e_str("-")),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("sign", e_str("+")),
                    ]),
                    ),
                    s_assign("s", e_binop(e_str(""), BinOp::Concat, e_var("year"))),
                    s_while(e_binop(e_call("strlen", vec![e_var("s")]), BinOp::Lt, e_int(4)), vec![
                        s_assign("s", e_binop(e_str("0"), BinOp::Concat, e_var("s"))),
                    ]),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("x")), BinOp::And, e_binop(e_var("sign"), BinOp::StrictEq, e_str("+"))), BinOp::And, e_binop(e_call("strlen", vec![e_var("s")]), BinOp::LtEq, e_int(4))),
                        vec![
                            s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("s"))),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("fmt", e_binop(e_binop(e_var("fmt"), BinOp::Concat, e_var("sign")), BinOp::Concat, e_var("s"))),
                    ]),
                    ),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("ch"))),
            s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
        ]),
        s_assign("r", e_call("date", vec![e_var("fmt"), e_this_prop("timestamp")])),
        s_expr(e_call("date_default_timezone_set", vec![e_var("saved")])),
        s_return(e_var("r")),
    ])
}

/// `DateTime::getOffset` — transcribed method builder.
fn decl_class_datetime_method_6_getoffset() -> MethodBuilder {
method("getOffset")
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__saved", e_call("date_default_timezone_get", vec![])),
        s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_this_prop("timezone_name")])])),
        s_assign("__off", e_call("intval", vec![e_call("date", vec![e_str("Z"), e_this_prop("timestamp")])])),
        s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
        s_return(e_var("__off")),
    ])
}

/// `DateTime::diff` — transcribed method builder.
fn decl_class_datetime_method_7_diff() -> MethodBuilder {
method("diff")
    .param("targetObject", t_class("DateTimeInterface"))
    .param_default("absolute", TypeExpr::Bool, e_bool(false))
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("leftTimestamp", e_this_prop("timestamp")),
        s_assign("leftMicrosecond", e_this_prop("microsecond")),
        s_assign("leftTimezone", e_this_prop("timezone_name")),
        s_assign("rightTimestamp", e_method_call(e_var("targetObject"), "getTimestamp", vec![])),
        s_assign("rightMicrosecond", e_method_call(e_var("targetObject"), "getMicrosecond", vec![])),
        s_assign("rightTimezone", e_method_call(e_var("targetObject"), "format", vec![e_str("e")])),
        s_assign("parsed", e_call("__elephc_timelib_diff", vec![e_var("leftTimestamp"), e_var("leftMicrosecond"), e_var("leftTimezone"), e_var("rightTimestamp"), e_var("rightMicrosecond"), e_var("rightTimezone")])),
        s_assign("interval", e_new("DateInterval", vec![e_str("PT0S")])),
        s_prop_assign(e_var("interval"), "y", e_index(e_var("parsed"), e_str("y"))),
        s_prop_assign(e_var("interval"), "m", e_index(e_var("parsed"), e_str("m"))),
        s_prop_assign(e_var("interval"), "d", e_index(e_var("parsed"), e_str("d"))),
        s_prop_assign(e_var("interval"), "h", e_index(e_var("parsed"), e_str("h"))),
        s_prop_assign(e_var("interval"), "i", e_index(e_var("parsed"), e_str("i"))),
        s_prop_assign(e_var("interval"), "s", e_index(e_var("parsed"), e_str("s"))),
        s_prop_assign(e_var("interval"), "f", e_binop(e_index(e_var("parsed"), e_str("us")), BinOp::Div, e_float(1000000.0))),
        s_prop_assign(e_var("interval"), "invert", e_index(e_var("parsed"), e_str("invert"))),
        s_if(
            e_var("absolute"),
            vec![
                s_prop_assign(e_var("interval"), "invert", e_int(0)),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_var("interval"), "days", e_index(e_var("parsed"), e_str("days"))),
        s_expr(e_method_call(e_var("interval"), "__elephc_mark_civil", vec![])),
        s_return(e_var("interval")),
    ])
}

/// `DateTime::setTimestamp` — transcribed method builder.
fn decl_class_datetime_method_8_settimestamp() -> MethodBuilder {
method("setTimestamp")
    .param("timestamp", TypeExpr::Int)
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_prop_assign(e_this(), "microsecond", e_int(0)),
        s_prop_assign(e_this(), "timestamp", e_var("timestamp")),
        s_prop_assign(e_this(), "__elephc_civil_override", e_bool(false)),
        s_return(e_this()),
    ])
}

/// `DateTime::setMicrosecond` — transcribed method builder.
fn decl_class_datetime_method_9_setmicrosecond() -> MethodBuilder {
method("setMicrosecond")
    .param("microsecond", TypeExpr::Int)
    .returns(t_class("static"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_binop(e_binop(e_var("microsecond"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("microsecond"), BinOp::Gt, e_int(999999))),
            vec![
                s_throw(e_new("DateRangeError", vec![e_binop(e_binop(e_str("DateTime::setMicrosecond(): Argument #1 ($microsecond) must be between 0 and 999999, "), BinOp::Concat, e_var("microsecond")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "microsecond", e_var("microsecond")),
        s_return(e_this()),
    ])
}

/// `DateTime::setTime` — transcribed method builder.
fn decl_class_datetime_method_10_settime() -> MethodBuilder {
method("setTime")
    .param("hour", TypeExpr::Int)
    .param("minute", TypeExpr::Int)
    .param_default("second", TypeExpr::Int, e_int(0))
    .param_default("microsecond", TypeExpr::Int, e_int(0))
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__payload", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("T\t"), BinOp::Concat, e_var("hour")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("minute")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("second")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("microsecond"))),
        s_assign("__parsed", e_call("__elephc_timelib_set_civil", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_var("__payload")])),
        s_prop_assign(e_this(), "microsecond", e_index(e_var("__parsed"), e_str("microsecond"))),
        s_prop_assign(e_this(), "timestamp", e_index(e_var("__parsed"), e_str("timestamp"))),
        s_prop_assign(e_this(), "__elephc_civil_override", e_bool(false)),
        s_return(e_this()),
    ])
}

/// `DateTime::setDate` — transcribed method builder.
fn decl_class_datetime_method_11_setdate() -> MethodBuilder {
method("setDate")
    .param("year", TypeExpr::Int)
    .param("month", TypeExpr::Int)
    .param("day", TypeExpr::Int)
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__payload", e_binop(e_binop(e_binop(e_binop(e_binop(e_str("D\t"), BinOp::Concat, e_var("year")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("month")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("day"))),
        s_assign("__parsed", e_call("__elephc_timelib_set_civil", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_var("__payload")])),
        s_prop_assign(e_this(), "microsecond", e_index(e_var("__parsed"), e_str("microsecond"))),
        s_prop_assign(e_this(), "timestamp", e_index(e_var("__parsed"), e_str("timestamp"))),
        s_prop_assign(e_this(), "__elephc_civil_override", e_bool(false)),
        s_return(e_this()),
    ])
}

/// `DateTime::setTimezone` — transcribed method builder.
fn decl_class_datetime_method_12_settimezone() -> MethodBuilder {
method("setTimezone")
    .param("timezone", t_class("DateTimeZone"))
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_prop_assign(e_this(), "timezone_name", e_method_call(e_var("timezone"), "getName", vec![])),
        s_return(e_this()),
    ])
}

/// `DateTime::add` — transcribed method builder.
fn decl_class_datetime_method_13_add() -> MethodBuilder {
method("add")
    .param("interval", t_mixed())
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_not(e_instance_of(e_var("interval"), "DateInterval")),
            vec![
                s_assign("__actual", e_call("gettype", vec![e_var("interval")])),
                s_if(
                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("boolean")),
                    vec![
                        s_assign("__actual", e_ternary(e_var("interval"), e_str("true"), e_str("false"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("__actual"), BinOp::StrictEq, e_str("integer")),
                        vec![
                            s_assign("__actual", e_str("int")),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("__actual"), BinOp::StrictEq, e_str("double")),
                            vec![
                                s_assign("__actual", e_str("float")),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("__actual"), BinOp::StrictEq, e_str("NULL")),
                                vec![
                                    s_assign("__actual", e_str("null")),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("object")),
                                    vec![
                                        s_assign("__actual", e_call("get_class", vec![e_var("interval")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("DateTime::add(): Argument #1 ($interval) must be of type DateInterval, "), BinOp::Concat, e_var("__actual")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("__interval_result", e_call("__elephc_timelib_apply_interval", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_method_call(e_var("interval"), "__elephc_payload", vec![]), e_bool(false)])),
        s_if(
            e_index(e_var("__interval_result"), e_str("warning")),
            vec![
                s_throw(e_new("DateInvalidOperationException", vec![e_str("DateTime::sub(): Only non-special relative time specifications are supported for subtraction")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "microsecond", e_index(e_var("__interval_result"), e_str("microsecond"))),
        s_prop_assign(e_this(), "timestamp", e_index(e_var("__interval_result"), e_str("timestamp"))),
        s_prop_assign(e_this(), "__elephc_civil_override", e_bool(false)),
        s_return(e_this()),
    ])
}

/// `DateTime::sub` — transcribed method builder.
fn decl_class_datetime_method_14_sub() -> MethodBuilder {
method("sub")
    .param("interval", t_mixed())
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_not(e_instance_of(e_var("interval"), "DateInterval")),
            vec![
                s_assign("__actual", e_call("gettype", vec![e_var("interval")])),
                s_if(
                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("boolean")),
                    vec![
                        s_assign("__actual", e_ternary(e_var("interval"), e_str("true"), e_str("false"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("__actual"), BinOp::StrictEq, e_str("integer")),
                        vec![
                            s_assign("__actual", e_str("int")),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("__actual"), BinOp::StrictEq, e_str("double")),
                            vec![
                                s_assign("__actual", e_str("float")),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("__actual"), BinOp::StrictEq, e_str("NULL")),
                                vec![
                                    s_assign("__actual", e_str("null")),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("object")),
                                    vec![
                                        s_assign("__actual", e_call("get_class", vec![e_var("interval")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("DateTime::sub(): Argument #1 ($interval) must be of type DateInterval, "), BinOp::Concat, e_var("__actual")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("__interval_result", e_call("__elephc_timelib_apply_interval", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_method_call(e_var("interval"), "__elephc_payload", vec![]), e_bool(true)])),
        s_if(
            e_index(e_var("__interval_result"), e_str("warning")),
            vec![
                s_throw(e_new("DateInvalidOperationException", vec![e_str("DateTime::sub(): Only non-special relative time specifications are supported for subtraction")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "microsecond", e_index(e_var("__interval_result"), e_str("microsecond"))),
        s_prop_assign(e_this(), "timestamp", e_index(e_var("__interval_result"), e_str("timestamp"))),
        s_prop_assign(e_this(), "__elephc_civil_override", e_bool(false)),
        s_return(e_this()),
    ])
}

/// `DateTime::modify` — transcribed method builder.
fn decl_class_datetime_method_15_modify() -> MethodBuilder {
method("modify")
    .param("modifier", TypeExpr::Str)
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_binop(e_var("modifier"), BinOp::StrictEq, e_str("")),
            vec![
                s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str("DateTime::modify(): "), e_var("modifier")])])),
            ],
            vec![],
            None,
        ),
        s_assign("__modified", e_call("__elephc_timelib_modify", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_this_prop("timezone_name")]), e_var("modifier")])),
        s_static_prop_assign("DateTime", "lastParseResult", e_index(e_var("__modified"), e_str("parse"))),
        s_if(
            e_binop(e_index(e_var("__modified"), e_str("status")), BinOp::StrictNotEq, e_str("O")),
            vec![
                s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str("DateTime::modify(): "), e_var("modifier")])])),
            ],
            vec![],
            None,
        ),
        s_assign("__ts", e_index(e_var("__modified"), e_str("timestamp"))),
        s_assign("__micro", e_index(e_var("__modified"), e_str("microsecond"))),
        s_assign("__timezone", e_ternary(e_index(e_var("__modified"), e_str("reset_to_utc")), e_str("+00:00"), e_this_prop("timezone_name"))),
        s_prop_assign(e_this(), "microsecond", e_var("__micro")),
        s_prop_assign(e_this(), "timestamp", e_var("__ts")),
        s_prop_assign(e_this(), "__elephc_civil_override", e_bool(false)),
        s_prop_assign(e_this(), "timezone_name", e_var("__timezone")),
        s_return(e_this()),
    ])
}

/// `DateTime::createFromFormat` — transcribed method builder.
fn decl_class_datetime_method_16_createfromformat() -> MethodBuilder {
method("createFromFormat")
    .static_()
    .param("format", TypeExpr::Str)
    .param("datetime", TypeExpr::Str)
    .param_default("timezone", t_nullable(t_class("DateTimeZone")), e_null())
    .returns(t_union(vec![t_class("DateTime"), TypeExpr::False]))
    .body_exact(vec![
        s_if(
            e_call("str_contains", vec![e_var("format"), e_call("chr", vec![e_int(0)])]),
            vec![
                s_throw(e_new("ValueError", vec![e_str("DateTime::createFromFormat(): Argument #1 ($format) must not contain any null bytes")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("str_contains", vec![e_var("datetime"), e_call("chr", vec![e_int(0)])]),
            vec![
                s_throw(e_new("ValueError", vec![e_str("DateTime::createFromFormat(): Argument #2 ($datetime) must not contain any null bytes")])),
            ],
            vec![],
            None,
        ),
        s_assign("timezoneName", e_call("date_default_timezone_get", vec![])),
        s_if(
            e_binop(e_var("timezone"), BinOp::StrictNotEq, e_null()),
            vec![
                s_assign("timezoneName", e_method_call(e_var("timezone"), "getName", vec![])),
            ],
            vec![],
            None,
        ),
        s_assign("parsed", e_call("__elephc_timelib_create_from_format", vec![e_var("format"), e_var("datetime"), e_var("timezoneName")])),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("error_count")), BinOp::Gt, e_int(0)),
            vec![
                s_static_prop_assign("DateTime", "lastParseResult", e_index(e_var("parsed"), e_str("__elephc_serialized"))),
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("object", e_new("DateTime", vec![])),
        s_assign("object", e_method_call(e_var("object"), "setTimestamp", vec![e_index(e_var("parsed"), e_str("__elephc_timestamp"))])),
        s_assign("microsecond", e_int(0)),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("fraction")), BinOp::StrictNotEq, e_bool(false)),
            vec![
                s_assign("microsecond", e_call("intval", vec![e_call("round", vec![e_binop(e_index(e_var("parsed"), e_str("fraction")), BinOp::Mul, e_float(1000000.0))])])),
            ],
            vec![],
            None,
        ),
        s_assign("object", e_method_call(e_var("object"), "setMicrosecond", vec![e_var("microsecond")])),
        s_if(
            e_index(e_var("parsed"), e_str("is_localtime")),
            vec![
                s_assign("zoneType", e_index(e_var("parsed"), e_str("zone_type"))),
                s_if(
                    e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(1)),
                    vec![
                        s_prop_assign(e_var("object"), "timezone_name", e_call("__elephc_timelib_offset_name", vec![e_index(e_var("parsed"), e_str("zone"))])),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_prop_assign(e_var("object"), "timezone_name", e_index(e_var("parsed"), e_str("tz_abbr"))),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(3)),
                            vec![
                                s_prop_assign(e_var("object"), "timezone_name", e_index(e_var("parsed"), e_str("tz_id"))),
                            ],
                            vec![],
                            Some(vec![
                            s_prop_assign(e_var("object"), "timezone_name", e_var("timezoneName")),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
            ],
            vec![],
            Some(vec![
            s_prop_assign(e_var("object"), "timezone_name", e_var("timezoneName")),
        ]),
        ),
        s_static_prop_assign("DateTime", "lastParseResult", e_index(e_var("parsed"), e_str("__elephc_serialized"))),
        s_if(
            e_binop(e_static_class(), BinOp::StrictEq, e_named_class("DateTime")),
            vec![
                s_return(e_var("object")),
            ],
            vec![],
            None,
        ),
        s_assign("result", e_call("__elephc_new_instance_without_constructor", vec![e_static_class()])),
        s_expr(e_method_call(e_var("result"), "__unserialize", vec![e_method_call(e_var("object"), "__serialize", vec![])])),
        s_return(e_var("result")),
    ])
}

/// `DateTime::getLastErrors` — transcribed method builder.
fn decl_class_datetime_method_17_getlasterrors() -> MethodBuilder {
method("getLastErrors")
    .static_()
    .returns(t_union(vec![t_array(), TypeExpr::False]))
    .body_exact(vec![
        s_assign("lastResult", e_static_prop("DateTime", "lastParseResult")),
        s_if(
            e_binop(e_var("lastResult"), BinOp::StrictEq, e_str("")),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("parsed", e_ternary(e_call("is_array", vec![e_var("lastResult")]), e_var("lastResult"), e_call("__elephc_timelib_decode_parse_result", vec![e_var("lastResult")]))),
        s_if(
            e_binop(e_binop(e_index(e_var("parsed"), e_str("error_count")), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_index(e_var("parsed"), e_str("warning_count")), BinOp::StrictEq, e_int(0))),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_return(e_array_assoc(vec![(e_str("warning_count"), e_index(e_var("parsed"), e_str("warning_count"))), (e_str("warnings"), e_index(e_var("parsed"), e_str("warnings"))), (e_str("error_count"), e_index(e_var("parsed"), e_str("error_count"))), (e_str("errors"), e_index(e_var("parsed"), e_str("errors")))])),
    ])
}

/// `DateTime::createFromTimestamp` — transcribed method builder.
fn decl_class_datetime_method_18_createfromtimestamp() -> MethodBuilder {
method("createFromTimestamp")
    .static_()
    .param("timestamp", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
    .returns(t_class("static"))
    .body_exact(vec![
        s_if(
            e_binop(e_call("is_float", vec![e_var("timestamp")]), BinOp::And, e_binop(e_binop(e_not(e_call("is_finite", vec![e_var("timestamp")])), BinOp::Or, e_binop(e_var("timestamp"), BinOp::Lt, e_neg(e_float(9.223372036854776e18)))), BinOp::Or, e_binop(e_var("timestamp"), BinOp::GtEq, e_float(9.223372036854776e18)))),
            vec![
                s_if(
                    e_call("is_nan", vec![e_var("timestamp")]),
                    vec![
                        s_assign("given", e_str("NAN")),
                    ],
                    vec![
                    (e_binop(e_var("timestamp"), BinOp::StrictEq, e_float(f64::INFINITY)), vec![
                        s_assign("given", e_str("INF")),
                    ]),
                    (e_binop(e_var("timestamp"), BinOp::StrictEq, e_neg(e_float(f64::INFINITY))), vec![
                        s_assign("given", e_str("-INF")),
                    ]),
                ],
                    Some(vec![
                    s_assign("given", e_call("sprintf", vec![e_str("%.6g"), e_var("timestamp")])),
                ]),
                ),
                s_throw(e_new("DateRangeError", vec![e_binop(e_binop(e_binop(e_binop(e_static_class(), BinOp::Concat, e_str("::createFromTimestamp(): Argument #1 ($timestamp) must be a finite number between ")), BinOp::Concat, e_str("-9223372036854775808 and 9223372036854775807.999999, ")), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("secs", e_call("intval", vec![e_call("floor", vec![e_var("timestamp")])])),
        s_assign("microseconds", e_call("intval", vec![e_call("round", vec![e_binop(e_binop(e_var("timestamp"), BinOp::Sub, e_var("secs")), BinOp::Mul, e_int(1000000))])])),
        s_if(
            e_binop(e_var("microseconds"), BinOp::GtEq, e_int(1000000)),
            vec![
                s_assign("secs", e_binop(e_var("secs"), BinOp::Add, e_int(1))),
                s_assign("microseconds", e_binop(e_var("microseconds"), BinOp::Sub, e_int(1000000))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_static_class(), BinOp::StrictEq, e_named_class("DateTime")),
            vec![
                s_assign("baseResult", e_new("DateTime", vec![e_binop(e_str("@"), BinOp::Concat, e_var("secs"))])),
                s_prop_assign(e_var("baseResult"), "microsecond", e_var("microseconds")),
                s_return(e_var("baseResult")),
            ],
            vec![],
            None,
        ),
        s_assign("subclassResult", e_call("__elephc_new_instance_without_constructor", vec![e_static_class()])),
        s_expr(e_method_call(e_var("subclassResult"), "__unserialize", vec![e_array_assoc(vec![(e_str("date"), e_binop(e_binop(e_call("gmdate", vec![e_str("x-m-d H:i:s"), e_var("secs")]), BinOp::Concat, e_str(".")), BinOp::Concat, e_call("sprintf", vec![e_str("%06d"), e_var("microseconds")]))), (e_str("timezone_type"), e_int(1)), (e_str("timezone"), e_str("+00:00"))])])),
        s_return(e_var("subclassResult")),
    ])
}

/// `DateTime::createFromInterface` — transcribed method builder.
fn decl_class_datetime_method_19_createfrominterface() -> MethodBuilder {
method("createFromInterface")
    .static_()
    .param("object", t_class("DateTimeInterface"))
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_assign("className", e_static_class()),
        s_assign("timezone", e_method_call(e_var("object"), "format", vec![e_str("e")])),
        s_assign("data", e_array_assoc(vec![(e_str("date"), e_method_call(e_var("object"), "format", vec![e_str("x-m-d H:i:s.u")])), (e_str("timezone_type"), e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("timezone")])), (e_str("timezone"), e_var("timezone"))])),
        s_if(
            e_binop(e_var("className"), BinOp::StrictEq, e_named_class("DateTime")),
            vec![
                s_assign("baseResult", e_new("DateTime", vec![])),
                s_expr(e_method_call(e_var("baseResult"), "__unserialize", vec![e_var("data")])),
                s_return(e_var("baseResult")),
            ],
            vec![],
            None,
        ),
        s_assign("subclassResult", e_call("__elephc_new_instance_without_constructor", vec![e_var("className")])),
        s_expr(e_method_call(e_var("subclassResult"), "__unserialize", vec![e_var("data")])),
        s_return(e_var("subclassResult")),
    ])
}

/// `DateTime::createFromImmutable` — transcribed method builder.
fn decl_class_datetime_method_20_createfromimmutable() -> MethodBuilder {
method("createFromImmutable")
    .static_()
    .param("object", t_class("DateTimeImmutable"))
    .returns(t_class("static"))
    .body_exact(vec![
        s_assign("actualClass", e_object_class_name(e_var("object"))),
        s_if(
            e_not(e_instance_of(e_var("object"), "DateTimeImmutable")),
            vec![
                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("DateTime::createFromImmutable(): Argument #1 ($object) must be of type DateTimeImmutable, "), BinOp::Concat, e_var("actualClass")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("className", e_static_class()),
        s_assign("timezone", e_method_call(e_var("object"), "format", vec![e_str("e")])),
        s_assign("data", e_array_assoc(vec![(e_str("date"), e_method_call(e_var("object"), "format", vec![e_str("x-m-d H:i:s.u")])), (e_str("timezone_type"), e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("timezone")])), (e_str("timezone"), e_var("timezone"))])),
        s_if(
            e_binop(e_var("className"), BinOp::StrictEq, e_named_class("DateTime")),
            vec![
                s_assign("baseResult", e_new("DateTime", vec![])),
                s_expr(e_method_call(e_var("baseResult"), "__unserialize", vec![e_var("data")])),
                s_return(e_var("baseResult")),
            ],
            vec![],
            None,
        ),
        s_assign("subclassResult", e_call("__elephc_new_instance_without_constructor", vec![e_var("className")])),
        s_expr(e_method_call(e_var("subclassResult"), "__unserialize", vec![e_var("data")])),
        s_return(e_var("subclassResult")),
    ])
}

/// `DateTime::setISODate` — transcribed method builder.
fn decl_class_datetime_method_21_setisodate() -> MethodBuilder {
method("setISODate")
    .param("year", TypeExpr::Int)
    .param("week", TypeExpr::Int)
    .param_default("dayOfWeek", TypeExpr::Int, e_int(1))
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("parsed", e_call("__elephc_timelib_set_iso_date", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_var("year"), e_var("week"), e_var("dayOfWeek")])),
        s_assign("timestamp", e_index(e_var("parsed"), e_str("timestamp"))),
        s_assign("microsecond", e_index(e_var("parsed"), e_str("microsecond"))),
        s_assign("civilYear", e_index(e_var("parsed"), e_str("year"))),
        s_assign("civilMonth", e_index(e_var("parsed"), e_str("month"))),
        s_assign("civilDay", e_index(e_var("parsed"), e_str("day"))),
        s_prop_assign(e_this(), "timestamp", e_var("timestamp")),
        s_prop_assign(e_this(), "microsecond", e_var("microsecond")),
        s_prop_assign(e_this(), "__elephc_civil_override", e_bool(true)),
        s_prop_assign(e_this(), "__elephc_civil_year", e_var("civilYear")),
        s_prop_assign(e_this(), "__elephc_civil_month", e_var("civilMonth")),
        s_prop_assign(e_this(), "__elephc_civil_day", e_var("civilDay")),
        s_return(e_this()),
    ])
}

/// `DateTime::__elephc_date_parse_from_format` — transcribed method builder.
fn decl_class_datetime_method_22_elephc_date_parse_from_format() -> MethodBuilder {
method("__elephc_date_parse_from_format")
    .static_()
    .param("format", TypeExpr::Str)
    .param("datetime", TypeExpr::Str)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_call("str_contains", vec![e_var("format"), e_call("chr", vec![e_int(0)])]),
            vec![
                s_throw(e_new("ValueError", vec![e_str("date_parse_from_format(): Argument #1 ($format) must not contain any null bytes")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("str_contains", vec![e_var("datetime"), e_call("chr", vec![e_int(0)])]),
            vec![
                s_throw(e_new("ValueError", vec![e_str("date_parse_from_format(): Argument #2 ($datetime) must not contain any null bytes")])),
            ],
            vec![],
            None,
        ),
        s_return(e_call("__elephc_timelib_date_parse_from_format", vec![e_var("format"), e_var("datetime")])),
    ])
}

/// `DateTime::__elephc_date_parse` — transcribed method builder.
fn decl_class_datetime_method_23_elephc_date_parse() -> MethodBuilder {
method("__elephc_date_parse")
    .static_()
    .param("datetime", TypeExpr::Str)
    .returns(t_mixed())
    .body_exact(vec![
        s_return(e_call("__elephc_timelib_date_parse", vec![e_var("datetime")])),
    ])
}

/// `DateTime::__elephc_gettimeofday` — transcribed method builder.
fn decl_class_datetime_method_24_elephc_gettimeofday() -> MethodBuilder {
method("__elephc_gettimeofday")
    .static_()
    .param_default("as_float", TypeExpr::Bool, e_bool(false))
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("mt", e_call("microtime", vec![e_bool(true)])),
        s_if(
            e_var("as_float"),
            vec![
                s_return(e_var("mt")),
            ],
            vec![],
            None,
        ),
        s_assign("sec", e_cast(CastType::Int, e_var("mt"))),
        s_assign("usec", e_cast(CastType::Int, e_binop(e_binop(e_var("mt"), BinOp::Sub, e_var("sec")), BinOp::Mul, e_float(1000000.0)))),
        s_assign("z", e_call("intval", vec![e_call("date", vec![e_str("Z")])])),
        s_assign("mw", e_call("intdiv", vec![e_neg(e_var("z")), e_int(60)])),
        s_assign("dst", e_call("intval", vec![e_call("date", vec![e_str("I")])])),
        s_return(e_array_assoc(vec![(e_str("sec"), e_var("sec")), (e_str("usec"), e_var("usec")), (e_str("minuteswest"), e_var("mw")), (e_str("dsttime"), e_var("dst"))])),
    ])
}

/// `DateTime::__elephc_idate` — transcribed method builder.
fn decl_class_datetime_method_25_elephc_idate() -> MethodBuilder {
method("__elephc_idate")
    .static_()
    .param("format", TypeExpr::Str)
    .param_default("timestamp", t_nullable(TypeExpr::Int), e_null())
    .param("sourceLine", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_binop(e_call("strlen", vec![e_var("format")]), BinOp::StrictNotEq, e_int(1)),
            vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_str("\nWarning: idate(): idate format is one char"), e_var("sourceLine"), e_const("E_WARNING")])),
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("valid", e_array(vec![e_str("B"), e_str("d"), e_str("G"), e_str("g"), e_str("H"), e_str("h"), e_str("I"), e_str("i"), e_str("L"), e_str("m"), e_str("N"), e_str("n"), e_str("s"), e_str("t"), e_str("U"), e_str("W"), e_str("w"), e_str("Y"), e_str("y"), e_str("z"), e_str("Z")])),
        s_if(
            e_not(e_call("in_array", vec![e_var("format"), e_var("valid"), e_bool(true)])),
            vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_str("\nWarning: idate(): Unrecognized date format token"), e_var("sourceLine"), e_const("E_WARNING")])),
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("timestamp"), BinOp::StrictEq, e_null()),
            vec![
                s_return(e_call("intval", vec![e_call("date", vec![e_var("format")])])),
            ],
            vec![],
            None,
        ),
        s_return(e_call("intval", vec![e_call("date", vec![e_var("format"), e_call("intval", vec![e_var("timestamp")])])])),
    ])
}

/// `DateTime::__elephc_timezone_type` — transcribed method builder.
fn decl_class_datetime_method_26_elephc_timezone_type() -> MethodBuilder {
method("__elephc_timezone_type")
    .static_()
    .param("timezone", TypeExpr::Str)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_binop(e_var("timezone"), BinOp::StrictEq, e_str("")),
            vec![
                s_return(e_int(3)),
            ],
            vec![],
            None,
        ),
        s_assign("__first", e_index(e_var("timezone"), e_int(0))),
        s_if(
            e_binop(e_binop(e_var("__first"), BinOp::StrictEq, e_str("+")), BinOp::Or, e_binop(e_var("__first"), BinOp::StrictEq, e_str("-"))),
            vec![
                s_return(e_int(1)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_binop(e_var("timezone"), BinOp::StrictEq, e_str("UTC")), BinOp::Or, e_binop(e_call("strpos", vec![e_var("timezone"), e_str("/")]), BinOp::StrictNotEq, e_bool(false))), BinOp::Or, e_call("in_array", vec![e_call("strtolower", vec![e_var("timezone")]), e_array(vec![e_str("cst6cdt"), e_str("cuba"), e_str("egypt"), e_str("eire"), e_str("est5edt"), e_str("factory"), e_str("gb"), e_str("gb-eire"), e_str("gmt0"), e_str("greenwich"), e_str("hongkong"), e_str("iceland"), e_str("iran"), e_str("israel"), e_str("jamaica"), e_str("japan"), e_str("kwajalein"), e_str("libya"), e_str("mst7mdt"), e_str("navajo"), e_str("nz"), e_str("nz-chat"), e_str("poland"), e_str("portugal"), e_str("prc"), e_str("pst8pdt"), e_str("roc"), e_str("rok"), e_str("singapore"), e_str("turkey"), e_str("universal"), e_str("utc"), e_str("w-su"), e_str("zulu")]), e_bool(true)])),
            vec![
                s_return(e_int(3)),
            ],
            vec![],
            None,
        ),
        s_return(e_int(2)),
    ])
}

/// `DateTime::__elephc_runtime_timezone_name` — transcribed method builder.
fn decl_class_datetime_method_27_elephc_runtime_timezone_name() -> MethodBuilder {
method("__elephc_runtime_timezone_name")
    .static_()
    .param("zone", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("upper", e_call("strtoupper", vec![e_var("zone")])),
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_binop(e_var("upper"), BinOp::StrictEq, e_str("UTC")), BinOp::Or, e_binop(e_var("upper"), BinOp::StrictEq, e_str("GMT"))), BinOp::Or, e_binop(e_var("upper"), BinOp::StrictEq, e_str("Z"))), BinOp::Or, e_binop(e_var("zone"), BinOp::StrictEq, e_str("+00:00"))), BinOp::Or, e_binop(e_var("zone"), BinOp::StrictEq, e_str("-00:00"))),
            vec![
                s_return(e_str("UTC")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_call("strlen", vec![e_var("zone")]), BinOp::StrictEq, e_int(6)), BinOp::Or, e_binop(e_call("strlen", vec![e_var("zone")]), BinOp::StrictEq, e_int(9))), BinOp::And, e_binop(e_binop(e_index(e_var("zone"), e_int(0)), BinOp::StrictEq, e_str("+")), BinOp::Or, e_binop(e_index(e_var("zone"), e_int(0)), BinOp::StrictEq, e_str("-")))), BinOp::And, e_binop(e_index(e_var("zone"), e_int(3)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("zone"), e_int(1))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("zone"), e_int(2))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("zone"), e_int(4))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("zone"), e_int(5))])),
            vec![
                s_assign("hours", e_call("intval", vec![e_call("substr", vec![e_var("zone"), e_int(1), e_int(2)])])),
                s_assign("minutes", e_call("substr", vec![e_var("zone"), e_int(4), e_int(2)])),
                s_assign("sign", e_ternary(e_binop(e_index(e_var("zone"), e_int(0)), BinOp::StrictEq, e_str("+")), e_str("-"), e_str("+"))),
                s_assign("runtime", e_binop(e_binop(e_str("UTC"), BinOp::Concat, e_var("sign")), BinOp::Concat, e_var("hours"))),
                s_if(
                    e_binop(e_var("minutes"), BinOp::StrictNotEq, e_str("00")),
                    vec![
                        s_assign("runtime", e_binop(e_binop(e_var("runtime"), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("minutes"))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_binop(e_call("strlen", vec![e_var("zone")]), BinOp::StrictEq, e_int(9)), BinOp::And, e_binop(e_index(e_var("zone"), e_int(6)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("zone"), e_int(7))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("zone"), e_int(8))])),
                    vec![
                        s_if(
                            e_binop(e_var("minutes"), BinOp::StrictEq, e_str("00")),
                            vec![
                                s_assign("runtime", e_binop(e_var("runtime"), BinOp::Concat, e_str(":00"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("runtime", e_binop(e_binop(e_var("runtime"), BinOp::Concat, e_str(":")), BinOp::Concat, e_call("substr", vec![e_var("zone"), e_int(7), e_int(2)]))),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_var("runtime")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_call("strlen", vec![e_var("upper")]), BinOp::StrictEq, e_int(1)),
            vec![
                s_assign("code", e_call("ord", vec![e_var("upper")])),
                s_assign("offset", e_int(0)),
                s_if(
                    e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(73))),
                    vec![
                        s_assign("offset", e_binop(e_var("code"), BinOp::Sub, e_int(64))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(75)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(77))),
                        vec![
                            s_assign("offset", e_binop(e_var("code"), BinOp::Sub, e_int(65))),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(78)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(89))),
                            vec![
                                s_assign("offset", e_binop(e_int(77), BinOp::Sub, e_var("code"))),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("upper"), BinOp::StrictEq, e_str("Z")),
                                vec![
                                    s_return(e_str("UTC")),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
                s_if(
                    e_binop(e_var("offset"), BinOp::StrictNotEq, e_int(0)),
                    vec![
                        s_assign("sign", e_ternary(e_binop(e_var("offset"), BinOp::Gt, e_int(0)), e_str("-"), e_str("+"))),
                        s_return(e_binop(e_binop(e_str("UTC"), BinOp::Concat, e_var("sign")), BinOp::Concat, e_cast(CastType::String, e_call("abs", vec![e_var("offset")])))),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_assign("length", e_call("strlen", vec![e_var("zone")])),
        s_if(
            e_binop(e_binop(e_var("length"), BinOp::GtEq, e_int(2)), BinOp::And, e_binop(e_var("length"), BinOp::LtEq, e_int(6))),
            vec![
                s_assign("alpha", e_bool(true)),
                s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                    s_assign("code", e_call("ord", vec![e_index(e_var("zone"), e_var("i"))])),
                    s_if(
                        e_not(e_binop(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(122))))),
                        vec![
                            s_assign("alpha", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                s_if(
                    e_var("alpha"),
                    vec![
                        s_assign("abbrZones", e_array_assoc(vec![(e_str("acdt"), e_str("UTC-10:30")), (e_str("acst"), e_str("UTC-9:30")), (e_str("addt"), e_str("UTC+2")), (e_str("adt"), e_str("UTC+3")), (e_str("aedt"), e_str("UTC-11")), (e_str("aest"), e_str("UTC-10")), (e_str("ahdt"), e_str("UTC+9")), (e_str("ahst"), e_str("UTC+10")), (e_str("akdt"), e_str("UTC+8")), (e_str("akst"), e_str("UTC+9")), (e_str("amt"), e_str("UTC+3:50:40")), (e_str("apt"), e_str("UTC+3")), (e_str("ast"), e_str("UTC+4")), (e_str("awdt"), e_str("UTC-9")), (e_str("awst"), e_str("UTC-8")), (e_str("awt"), e_str("UTC+3")), (e_str("bdst"), e_str("UTC-2")), (e_str("bdt"), e_str("UTC+10")), (e_str("bmt"), e_str("UTC+3:58:29")), (e_str("bst"), e_str("UTC-1")), (e_str("cast"), e_str("UTC-9:30")), (e_str("cat"), e_str("UTC-2")), (e_str("cddt"), e_str("UTC+4")), (e_str("cdt"), e_str("UTC+5")), (e_str("cemt"), e_str("UTC-3")), (e_str("cest"), e_str("UTC-2")), (e_str("cet"), e_str("UTC-1")), (e_str("cmt"), e_str("UTC+4:16:48")), (e_str("cpt"), e_str("UTC+5")), (e_str("cst"), e_str("UTC+6")), (e_str("cwt"), e_str("UTC+5")), (e_str("chst"), e_str("UTC-10")), (e_str("dmt"), e_str("UTC+0:25:21")), (e_str("eat"), e_str("UTC-3")), (e_str("eddt"), e_str("UTC+3")), (e_str("edt"), e_str("UTC+4")), (e_str("eest"), e_str("UTC-3")), (e_str("eet"), e_str("UTC-2")), (e_str("emt"), e_str("UTC+7:17:28")), (e_str("ept"), e_str("UTC+4")), (e_str("est"), e_str("UTC+5")), (e_str("ewt"), e_str("UTC+4")), (e_str("ffmt"), e_str("UTC+4:04:20")), (e_str("fmt"), e_str("UTC+1:07:36")), (e_str("gdt"), e_str("UTC-11")), (e_str("gmt"), e_str("UTC")), (e_str("gst"), e_str("UTC-10")), (e_str("hdt"), e_str("UTC+9:30")), (e_str("hkst"), e_str("UTC-9")), (e_str("hkt"), e_str("UTC-8")), (e_str("hmt"), e_str("UTC+5:29:36")), (e_str("hpt"), e_str("UTC+9:30")), (e_str("hst"), e_str("UTC+10")), (e_str("hwt"), e_str("UTC+9:30")), (e_str("iddt"), e_str("UTC-4")), (e_str("idt"), e_str("UTC-3")), (e_str("imt"), e_str("UTC-6:57:05")), (e_str("ist"), e_str("UTC-2")), (e_str("jdt"), e_str("UTC-10")), (e_str("jmt"), e_str("UTC-2:20:40")), (e_str("jst"), e_str("UTC-9")), (e_str("kdt"), e_str("UTC-10")), (e_str("kmt"), e_str("UTC-1:35:36")), (e_str("kst"), e_str("UTC-8:30")), (e_str("lst"), e_str("UTC-2:36:34")), (e_str("mddt"), e_str("UTC+5")), (e_str("mdst"), e_str("UTC-4:31:19")), (e_str("mdt"), e_str("UTC+6")), (e_str("mest"), e_str("UTC-2")), (e_str("met"), e_str("UTC-1")), (e_str("mmt"), e_str("UTC-2:30:17")), (e_str("mpt"), e_str("UTC+6")), (e_str("msd"), e_str("UTC-4")), (e_str("msk"), e_str("UTC-3")), (e_str("mst"), e_str("UTC+7")), (e_str("mwt"), e_str("UTC+6")), (e_str("nddt"), e_str("UTC+1:30")), (e_str("ndt"), e_str("UTC+2:30:52")), (e_str("npt"), e_str("UTC+2:30")), (e_str("nst"), e_str("UTC+3:30")), (e_str("nwt"), e_str("UTC+2:30")), (e_str("nzdt"), e_str("UTC-13")), (e_str("nzmt"), e_str("UTC-11:30")), (e_str("nzst"), e_str("UTC-12")), (e_str("pddt"), e_str("UTC+6")), (e_str("pdt"), e_str("UTC+7")), (e_str("pkst"), e_str("UTC-6")), (e_str("pkt"), e_str("UTC-5")), (e_str("plmt"), e_str("UTC-7:06:30")), (e_str("pmt"), e_str("UTC+3:40:36")), (e_str("ppmt"), e_str("UTC+4:49")), (e_str("ppt"), e_str("UTC+7")), (e_str("pst"), e_str("UTC+8")), (e_str("pwt"), e_str("UTC+7")), (e_str("qmt"), e_str("UTC+5:14")), (e_str("rmt"), e_str("UTC-1:36:34")), (e_str("sast"), e_str("UTC-2")), (e_str("sdmt"), e_str("UTC+4:40")), (e_str("sjmt"), e_str("UTC+5:36:13")), (e_str("smt"), e_str("UTC+3:51:24")), (e_str("sst"), e_str("UTC+11")), (e_str("tbmt"), e_str("UTC-2:59:11")), (e_str("tmt"), e_str("UTC-3:25:44")), (e_str("uct"), e_str("UTC")), (e_str("utc"), e_str("UTC")), (e_str("wast"), e_str("UTC-2")), (e_str("wat"), e_str("UTC-1")), (e_str("wemt"), e_str("UTC-2")), (e_str("west"), e_str("UTC-1")), (e_str("wet"), e_str("UTC")), (e_str("wib"), e_str("UTC-7")), (e_str("wita"), e_str("UTC-8")), (e_str("wit"), e_str("UTC-9")), (e_str("wmt"), e_str("UTC-1:24")), (e_str("yddt"), e_str("UTC+7")), (e_str("ydt"), e_str("UTC+8")), (e_str("ypt"), e_str("UTC+8")), (e_str("yst"), e_str("UTC+9")), (e_str("ywt"), e_str("UTC+8")), (e_str("a"), e_str("UTC-1")), (e_str("b"), e_str("UTC-2")), (e_str("c"), e_str("UTC-3")), (e_str("d"), e_str("UTC-4")), (e_str("e"), e_str("UTC-5")), (e_str("f"), e_str("UTC-6")), (e_str("g"), e_str("UTC-7")), (e_str("h"), e_str("UTC-8")), (e_str("i"), e_str("UTC-9")), (e_str("k"), e_str("UTC-10")), (e_str("l"), e_str("UTC-11")), (e_str("m"), e_str("UTC-12")), (e_str("n"), e_str("UTC+1")), (e_str("o"), e_str("UTC+2")), (e_str("p"), e_str("UTC+3")), (e_str("q"), e_str("UTC+4")), (e_str("r"), e_str("UTC+5")), (e_str("s"), e_str("UTC+6")), (e_str("t"), e_str("UTC+7")), (e_str("u"), e_str("UTC+8")), (e_str("v"), e_str("UTC+9")), (e_str("w"), e_str("UTC+10")), (e_str("x"), e_str("UTC+11")), (e_str("y"), e_str("UTC+12")), (e_str("z"), e_str("UTC"))])),
                        s_assign("key", e_call("strtolower", vec![e_var("zone")])),
                        s_if(
                            e_call("isset", vec![e_index(e_var("abbrZones"), e_var("key"))]),
                            vec![
                                s_return(e_binop(e_str(""), BinOp::Concat, e_index(e_var("abbrZones"), e_var("key")))),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_return(e_binop(e_str(""), BinOp::Concat, e_var("zone"))),
    ])
}

/// `DateTime::__elephc_date_create` — transcribed method builder.
fn decl_class_datetime_method_28_elephc_date_create() -> MethodBuilder {
method("__elephc_date_create")
    .static_()
    .param_default("datetime", TypeExpr::Str, e_str("now"))
    .param_default("timezone", t_nullable(t_class("DateTimeZone")), e_null())
    .returns(t_mixed())
    .body_exact(vec![
        s_try(vec![
            s_if(
                e_binop(e_var("timezone"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_new("DateTime", vec![e_var("datetime")])),
                ],
                vec![],
                None,
            ),
            s_try(vec![
                s_expr(e_method_call(e_var("timezone"), "__elephc_assert_initialized", vec![])),
            ], vec![
                (vec!["\\DateObjectError"], Some("e"), vec![
                    s_throw(e_new_fq("Error", vec![e_str("The DateTimeZone object has not been correctly initialized by its constructor")])),
                ]),
            ], None),
            s_return(e_new("DateTime", vec![e_var("datetime"), e_var("timezone")])),
        ], vec![
            (vec!["\\DateMalformedStringException"], Some("e"), vec![
                s_return(e_bool(false)),
            ]),
        ], None),
    ])
}

/// `DateTime::__wakeup` — transcribed method builder.
fn decl_class_datetime_method_29_wakeup() -> MethodBuilder {
method("__wakeup")
    .attr("\\Deprecated", vec![e_named_arg("since", e_str("8.5")), e_named_arg("message", e_str("this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()"))])
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_str("Deprecated: Method DateTime::__wakeup() is deprecated since 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()\n"), e_int(0), e_const("E_DEPRECATED")])),
        s_if(
            e_binop(e_str("DateTime"), BinOp::StrictNotEq, e_str("DateInterval")),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTime object")])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateTime::__serialize` — transcribed method builder.
fn decl_class_datetime_method_30_serialize() -> MethodBuilder {
method("__serialize")
    .returns(t_array())
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__tz", e_cast(CastType::String, e_this_prop("timezone_name"))),
        s_assign("__saved", e_call("date_default_timezone_get", vec![])),
        s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("__tz")])])),
        s_assign("__date", e_call("date", vec![e_str("x-m-d H:i:s"), e_this_prop("timestamp")])),
        s_assign("__us", e_call("str_pad", vec![e_cast(CastType::String, e_this_prop("microsecond")), e_int(6), e_str("0"), e_int(1)])),
        s_assign("__date", e_binop(e_binop(e_var("__date"), BinOp::Concat, e_str(".")), BinOp::Concat, e_var("__us"))),
        s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
        s_return(e_array_assoc(vec![(e_str("date"), e_var("__date")), (e_str("timezone_type"), e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("__tz")])), (e_str("timezone"), e_var("__tz"))])),
    ])
}

/// `DateTime::__unserialize` — transcribed method builder.
fn decl_class_datetime_method_31_unserialize() -> MethodBuilder {
method("__unserialize")
    .param("data", t_array())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_binop(e_not(e_call("array_key_exists", vec![e_str("date"), e_var("data")])), BinOp::Or, e_not(e_call("array_key_exists", vec![e_str("timezone_type"), e_var("data")]))), BinOp::Or, e_not(e_call("array_key_exists", vec![e_str("timezone"), e_var("data")]))), BinOp::Or, e_not(e_call("is_string", vec![e_index(e_var("data"), e_str("date"))]))), BinOp::Or, e_not(e_call("is_int", vec![e_index(e_var("data"), e_str("timezone_type"))]))), BinOp::Or, e_not(e_call("is_string", vec![e_index(e_var("data"), e_str("timezone"))]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTime object")])),
            ],
            vec![],
            None,
        ),
        s_assign("__date", e_index(e_var("data"), e_str("date"))),
        s_assign("__tz", e_index(e_var("data"), e_str("timezone"))),
        s_assign("__tzType", e_index(e_var("data"), e_str("timezone_type"))),
        s_assign("__normalizedTz", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_var("__tz")])),
        s_if(
            e_binop(e_binop(e_var("__normalizedTz"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("__tzType"), BinOp::StrictNotEq, e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("__normalizedTz")]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTime object")])),
            ],
            vec![],
            None,
        ),
        s_assign("__tz", e_var("__normalizedTz")),
        s_prop_assign(e_this(), "microsecond", e_static_call("DateTime", "__elephc_extract_micros", vec![e_var("__date")])),
        s_assign("__dateWithoutMicros", e_static_call("DateTime", "__elephc_strip_micros", vec![e_var("__date")])),
        s_assign("__saved", e_call("date_default_timezone_get", vec![])),
        s_if(
            e_binop(e_var("__tzType"), BinOp::StrictEq, e_int(1)),
            vec![
                s_expr(e_call("date_default_timezone_set", vec![e_str("UTC")])),
                s_assign("__timestamp", e_call("strtotime", vec![e_var("__dateWithoutMicros")])),
                s_assign("__offsetSeconds", e_binop(e_binop(e_call("intval", vec![e_call("substr", vec![e_var("__tz"), e_int(1), e_int(2)])]), BinOp::Mul, e_int(3600)), BinOp::Add, e_binop(e_call("intval", vec![e_call("substr", vec![e_var("__tz"), e_int(4), e_int(2)])]), BinOp::Mul, e_int(60)))),
                s_if(
                    e_binop(e_call("strlen", vec![e_var("__tz")]), BinOp::StrictEq, e_int(9)),
                    vec![
                        s_assign("__offsetSeconds", e_binop(e_var("__offsetSeconds"), BinOp::Add, e_call("intval", vec![e_call("substr", vec![e_var("__tz"), e_int(7), e_int(2)])]))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_index(e_var("__tz"), e_int(0)), BinOp::StrictEq, e_str("-")),
                    vec![
                        s_assign("__offsetSeconds", e_neg(e_var("__offsetSeconds"))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__timestamp"), BinOp::StrictNotEq, e_bool(false)),
                    vec![
                        s_assign("__timestamp", e_binop(e_var("__timestamp"), BinOp::Sub, e_var("__offsetSeconds"))),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            Some(vec![
            s_if(
                e_not(e_error_suppress(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("__tz")])]))),
                vec![
                    s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
                    s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTime object")])),
                ],
                vec![],
                None,
            ),
            s_assign("__timestamp", e_call("strtotime", vec![e_var("__dateWithoutMicros")])),
        ]),
        ),
        s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
        s_if(
            e_binop(e_var("__timestamp"), BinOp::StrictEq, e_bool(false)),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTime object")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "timestamp", e_var("__timestamp")),
        s_prop_assign(e_this(), "timezone_name", e_var("__tz")),
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
    ])
}

/// `DateTime::__set_state` — transcribed method builder.
fn decl_class_datetime_method_32_set_state() -> MethodBuilder {
method("__set_state")
    .static_()
    .param("array", t_array())
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_assign("__d", e_new("DateTime", vec![])),
        s_expr(e_method_call(e_var("__d"), "__unserialize", vec![e_var("array")])),
        s_return(e_var("__d")),
    ])
}

/// `DateTime::__elephc_debug_dump` — transcribed method builder.
fn decl_class_datetime_method_33_elephc_debug_dump() -> MethodBuilder {
method("__elephc_debug_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_call("__elephc_var_dump_indent", vec![e_int(0)])])),
        s_assign("field_pad", e_binop(e_var("pad"), BinOp::Concat, e_str("  "))),
        s_assign("property_count", e_call("__elephc_var_dump_object_property_count", vec![e_this()])),
        s_echo(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("pad"), BinOp::Concat, e_str("object(")), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(")#")), BinOp::Concat, e_call("spl_object_id", vec![e_this()])), BinOp::Concat, e_str(" (")), BinOp::Concat, e_binop(e_var("property_count"), BinOp::Add, e_int(3))), BinOp::Concat, e_str(") {\n"))),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
        s_expr(e_call("__elephc_var_dump_object_properties", vec![e_this()])),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"date\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_method_call(e_this(), "format", vec![e_str("x-m-d H:i:s.u")])])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"timezone_type\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("timezone_name")])])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"timezone\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("timezone_name")])),
        s_echo(e_binop(e_var("pad"), BinOp::Concat, e_str("}\n"))),
    ])
}

/// `DateTime::__elephc_print_r_dump` — transcribed method builder.
fn decl_class_datetime_method_34_elephc_print_r_dump() -> MethodBuilder {
method("__elephc_print_r_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_echo(e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str(" Object\n(\n"))),
        s_expr(e_call("__elephc_print_r_object_properties", vec![e_this()])),
        s_echo(e_binop(e_binop(e_str("    [date] => "), BinOp::Concat, e_method_call(e_this(), "format", vec![e_str("x-m-d H:i:s.u")])), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [timezone_type] => "), BinOp::Concat, e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("timezone_name")])), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [timezone] => "), BinOp::Concat, e_this_prop("timezone_name")), BinOp::Concat, e_str("\n"))),
        s_echo(e_str(")\n")),
    ])
}

/// `DateTime::__elephc_clone_for_period` — transcribed method builder.
fn decl_class_datetime_method_35_elephc_clone_for_period() -> MethodBuilder {
method("__elephc_clone_for_period")
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_clone(e_this())),
    ])
}

/// `DateTime::__elephc_clone_for_period_storage` — transcribed method builder.
fn decl_class_datetime_method_36_elephc_clone_for_period_storage() -> MethodBuilder {
method("__elephc_clone_for_period_storage")
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_call("__elephc_object_clone_internal", vec![e_this()])),
    ])
}

/// `DateTime::__elephc_begin_argument_array` — transcribed method builder.
fn decl_class_datetime_method_37_elephc_begin_argument_array() -> MethodBuilder {
method("__elephc_begin_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_arguments", e_array(vec![])),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DateTime::__elephc_append_one_argument` — transcribed method builder.
fn decl_class_datetime_method_38_elephc_append_one_argument() -> MethodBuilder {
method("__elephc_append_one_argument")
    .private()
    .param("key", t_mixed())
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_if(
            e_call("is_int", vec![e_var("key")]),
            vec![
                s_if(
                    e_this_prop("__elephc_seen_named_argument"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Cannot use positional argument after named argument during unpacking")])),
                    ],
                    vec![],
                    None,
                ),
                s_array_push("arguments", e_var("value")),
                s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_string", vec![e_var("key")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Keys must be of type int|string during argument unpacking")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(true)),
        s_if(
            e_not(e_binop(e_binop(e_var("key"), BinOp::StrictEq, e_str("datetime")), BinOp::Or, e_binop(e_var("key"), BinOp::StrictEq, e_str("timezone")))),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_str("Unknown named parameter $"), BinOp::Concat, e_var("key"))])),
            ],
            vec![],
            None,
        ),
        s_assign("parameterIndex", e_neg(e_int(1))),
        s_if(
            e_binop(e_var("key"), BinOp::StrictEq, e_str("datetime")),
            vec![
                s_assign("parameterIndex", e_int(0)),
            ],
            vec![
            (e_binop(e_var("key"), BinOp::StrictEq, e_str("timezone")), vec![
                s_assign("parameterIndex", e_int(1)),
            ]),
        ],
            None,
        ),
        s_assign("positionalCount", e_int(0)),
        s_foreach(e_var("arguments"), Some("existingKey"), "existingValue", vec![
            s_if(
                e_call("is_int", vec![e_var("existingKey")]),
                vec![
                    s_expr(e_post_inc("positionalCount")),
                ],
                vec![],
                None,
            ),
        ]),
        s_if(
            e_binop(e_var("parameterIndex"), BinOp::Lt, e_var("positionalCount")),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("array_key_exists", vec![e_var("key"), e_var("arguments")]),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_array_assign("arguments", e_var("key"), e_var("value")),
        s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
    ])
}

/// `DateTime::__elephc_append_argument_chunk` — transcribed method builder.
fn decl_class_datetime_method_39_elephc_append_argument_chunk() -> MethodBuilder {
method("__elephc_append_argument_chunk")
    .private()
    .param("kind", TypeExpr::Int)
    .param("name", TypeExpr::Str)
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
            vec![
                s_if(
                    e_not(e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_instance_of(e_var("value"), "Traversable"))),
                    vec![
                        s_expr(e_static_call("DateTime", "__elephc_argument_type_error", vec![e_var("value"), e_str("Only arrays and Traversables can be unpacked, ")])),
                    ],
                    vec![],
                    None,
                ),
                s_foreach(e_var("value"), Some("key"), "unpackedValue", vec![
                    s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("key"), e_var("unpackedValue")])),
                ]),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
            vec![
                s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("name"), e_var("value")])),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_int(0), e_var("value")])),
    ])
}

/// `DateTime::__elephc_finish_argument_array` — transcribed method builder.
fn decl_class_datetime_method_40_elephc_finish_argument_array() -> MethodBuilder {
method("__elephc_finish_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_assign("datetime", e_str("now")),
        s_assign("timezone", e_null()),
        s_assign("hasDatetime", e_bool(false)),
        s_assign("hasTimezone", e_bool(false)),
        s_assign("nextPosition", e_int(0)),
        s_foreach(e_var("arguments"), Some("key"), "value", vec![
            s_if(
                e_call("is_int", vec![e_var("key")]),
                vec![
                    s_if(
                        e_binop(e_var("nextPosition"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_assign("datetime", e_var("value")),
                            s_assign("hasDatetime", e_bool(true)),
                        ],
                        vec![
                        (e_binop(e_var("nextPosition"), BinOp::StrictEq, e_int(1)), vec![
                            s_assign("timezone", e_var("value")),
                            s_assign("hasTimezone", e_bool(true)),
                        ]),
                    ],
                        Some(vec![
                        s_throw(e_new("ArgumentCountError", vec![e_binop(e_binop(e_str("DateTime::__construct() expects at most 2 arguments, "), BinOp::Concat, e_call("count", vec![e_var("arguments")])), BinOp::Concat, e_str(" given"))])),
                    ]),
                    ),
                    s_expr(e_post_inc("nextPosition")),
                ],
                vec![
                (e_binop(e_var("key"), BinOp::StrictEq, e_str("datetime")), vec![
                    s_if(
                        e_var("hasDatetime"),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Named parameter $datetime overwrites previous argument")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("datetime", e_var("value")),
                    s_assign("hasDatetime", e_bool(true)),
                ]),
            ],
                Some(vec![
                s_if(
                    e_var("hasTimezone"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Named parameter $timezone overwrites previous argument")])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("timezone", e_var("value")),
                s_assign("hasTimezone", e_bool(true)),
            ]),
            ),
        ]),
        s_assign("datetime", e_static_call("DateTime", "__elephc_weak_string_argument", vec![e_var("datetime"), e_str("DateTime::__construct(): Argument #1 ($datetime) must be of type string, "), e_str("")])),
        s_if(
            e_binop(e_not(e_call("is_null", vec![e_var("timezone")])), BinOp::And, e_not(e_instance_of(e_var("timezone"), "DateTimeZone"))),
            vec![
                s_expr(e_static_call("DateTime", "__elephc_argument_type_error", vec![e_var("timezone"), e_str("DateTime::__construct(): Argument #2 ($timezone) must be of type ?DateTimeZone, ")])),
            ],
            vec![],
            None,
        ),
        s_expr(e_method_call(e_this(), "__construct", vec![e_var("datetime"), e_var("timezone")])),
        s_prop_assign(e_this(), "__elephc_arguments", e_null()),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DateTime::__elephc_date_modify` — transcribed method builder.
fn decl_class_datetime_method_41_elephc_date_modify() -> MethodBuilder {
method("__elephc_date_modify")
    .static_()
    .param("object", t_mixed())
    .param("modifier", TypeExpr::Str)
    .param("sourceLine", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_try(vec![
            s_return(e_method_call(e_var("object"), "modify", vec![e_var("modifier")])),
        ], vec![
            (vec!["\\DateMalformedStringException"], Some("e"), vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_binop(e_str("\nWarning: date_modify(): "), BinOp::Concat, e_call("substr", vec![e_method_call(e_var("e"), "getMessage", vec![]), e_int(20)])), e_var("sourceLine")])),
                s_return(e_bool(false)),
            ]),
        ], None),
    ])
}

/// `DateTime::__elephc_date_timestamp_set` — transcribed method builder.
fn decl_class_datetime_method_42_elephc_date_timestamp_set() -> MethodBuilder {
method("__elephc_date_timestamp_set")
    .static_()
    .param("object", t_mixed())
    .param("timestamp", t_mixed())
    .param("sourceLine", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_binop(e_var("timestamp"), BinOp::StrictEq, e_null()),
            vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: date_timestamp_set(): Passing null to parameter #2 ($timestamp) of type int is deprecated"), e_var("sourceLine"), e_const("E_DEPRECATED")])),
                s_return(e_method_call(e_var("object"), "setTimestamp", vec![e_int(0)])),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_var("object"), "setTimestamp", vec![e_var("timestamp")])),
    ])
}

/// `DateTime::__elephc_date_add` — transcribed method builder.
fn decl_class_datetime_method_43_elephc_date_add() -> MethodBuilder {
method("__elephc_date_add")
    .static_()
    .param("object", t_class("DateTime"))
    .param("interval", t_mixed())
    .param_default("sourceLine", TypeExpr::Int, e_int(0))
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_if(
            e_not(e_instance_of(e_var("interval"), "DateInterval")),
            vec![
                s_assign("__actual", e_call("gettype", vec![e_var("interval")])),
                s_if(
                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("boolean")),
                    vec![
                        s_assign("__actual", e_ternary(e_var("interval"), e_str("true"), e_str("false"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("__actual"), BinOp::StrictEq, e_str("integer")),
                        vec![
                            s_assign("__actual", e_str("int")),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("__actual"), BinOp::StrictEq, e_str("double")),
                            vec![
                                s_assign("__actual", e_str("float")),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("__actual"), BinOp::StrictEq, e_str("NULL")),
                                vec![
                                    s_assign("__actual", e_str("null")),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("object")),
                                    vec![
                                        s_assign("__actual", e_call("get_class", vec![e_var("interval")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("date_add(): Argument #2 ($interval) must be of type DateInterval, "), BinOp::Concat, e_var("__actual")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_return(e_method_call(e_var("object"), "add", vec![e_var("interval")])),
    ])
}

/// `DateTime::__elephc_date_sub` — transcribed method builder.
fn decl_class_datetime_method_44_elephc_date_sub() -> MethodBuilder {
method("__elephc_date_sub")
    .static_()
    .param("object", t_class("DateTime"))
    .param("interval", t_mixed())
    .param_default("sourceLine", TypeExpr::Int, e_int(0))
    .returns(t_class("DateTime"))
    .body_exact(vec![
        s_if(
            e_not(e_instance_of(e_var("interval"), "DateInterval")),
            vec![
                s_assign("__actual", e_call("gettype", vec![e_var("interval")])),
                s_if(
                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("boolean")),
                    vec![
                        s_assign("__actual", e_ternary(e_var("interval"), e_str("true"), e_str("false"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("__actual"), BinOp::StrictEq, e_str("integer")),
                        vec![
                            s_assign("__actual", e_str("int")),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("__actual"), BinOp::StrictEq, e_str("double")),
                            vec![
                                s_assign("__actual", e_str("float")),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("__actual"), BinOp::StrictEq, e_str("NULL")),
                                vec![
                                    s_assign("__actual", e_str("null")),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("object")),
                                    vec![
                                        s_assign("__actual", e_call("get_class", vec![e_var("interval")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("date_sub(): Argument #2 ($interval) must be of type DateInterval, "), BinOp::Concat, e_var("__actual")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_try(vec![
            s_return(e_method_call(e_var("object"), "sub", vec![e_var("interval")])),
        ], vec![
            (vec!["DateInvalidOperationException"], Some("exception"), vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_str("\nWarning: date_sub(): Only non-special relative time specifications are supported for subtraction"), e_var("sourceLine"), e_const("E_WARNING")])),
                s_return(e_var("object")),
            ]),
        ], None),
    ])
}

/// `DateTime::__elephc_strftime` — transcribed method builder.
fn decl_class_datetime_method_45_elephc_strftime() -> MethodBuilder {
method("__elephc_strftime")
    .static_()
    .param("format", TypeExpr::Str)
    .param("timestamp", TypeExpr::Int)
    .param("utc", TypeExpr::Bool)
    .param("sourceLine", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_var("utc"),
            vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: Function gmstrftime() is deprecated since 8.1, use IntlDateFormatter::format() instead"), e_var("sourceLine"), e_const("E_DEPRECATED")])),
            ],
            vec![],
            Some(vec![
            s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: Function strftime() is deprecated since 8.1, use IntlDateFormatter::format() instead"), e_var("sourceLine"), e_const("E_DEPRECATED")])),
        ]),
        ),
        s_if(
            e_binop(e_var("format"), BinOp::StrictEq, e_str("")),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("out", e_str("")),
        s_assign("flen", e_call("strlen", vec![e_var("format")])),
        s_assign("k", e_int(0)),
        s_while(e_binop(e_var("k"), BinOp::Lt, e_var("flen")), vec![
            s_assign("ch", e_index(e_var("format"), e_var("k"))),
            s_if(
                e_binop(e_var("ch"), BinOp::StrictNotEq, e_str("%")),
                vec![
                    s_assign("cc", e_call("ord", vec![e_var("ch")])),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("cc"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("cc"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("cc"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("cc"), BinOp::LtEq, e_int(122)))),
                        vec![
                            s_assign("out", e_binop(e_binop(e_var("out"), BinOp::Concat, e_str("\\")), BinOp::Concat, e_var("ch"))),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("ch"))),
                    ]),
                    ),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
            s_if(
                e_binop(e_var("k"), BinOp::GtEq, e_var("flen")),
                vec![
                    s_break(1),
                ],
                vec![],
                None,
            ),
            s_assign("spec", e_index(e_var("format"), e_var("k"))),
            s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
            s_if(
                e_binop(e_var("spec"), BinOp::StrictEq, e_str("a")),
                vec![
                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("D"))),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("A")),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("l"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("d")),
                        vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("d"))),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("e")),
                            vec![
                                s_if(
                                    e_var("utc"),
                                    vec![
                                        s_assign("dd", e_call("intval", vec![e_call("gmdate", vec![e_str("j"), e_var("timestamp")])])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("dd", e_call("intval", vec![e_call("date", vec![e_str("j"), e_var("timestamp")])])),
                                ]),
                                ),
                                s_assign("ds", e_binop(e_str(""), BinOp::Concat, e_var("dd"))),
                                s_if(
                                    e_binop(e_call("strlen", vec![e_var("ds")]), BinOp::Lt, e_int(2)),
                                    vec![
                                        s_assign("ds", e_binop(e_str(" "), BinOp::Concat, e_var("ds"))),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("ds"))),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("j")),
                                vec![
                                    s_if(
                                        e_var("utc"),
                                        vec![
                                            s_assign("z", e_call("intval", vec![e_call("gmdate", vec![e_str("z"), e_var("timestamp")])])),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_assign("z", e_call("intval", vec![e_call("date", vec![e_str("z"), e_var("timestamp")])])),
                                    ]),
                                    ),
                                    s_assign("z", e_binop(e_var("z"), BinOp::Add, e_int(1))),
                                    s_assign("zs", e_binop(e_str(""), BinOp::Concat, e_var("z"))),
                                    s_while(e_binop(e_call("strlen", vec![e_var("zs")]), BinOp::Lt, e_int(3)), vec![
                                        s_assign("zs", e_binop(e_str("0"), BinOp::Concat, e_var("zs"))),
                                    ]),
                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("zs"))),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("u")),
                                    vec![
                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("N"))),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_if(
                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("w")),
                                        vec![
                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("w"))),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_if(
                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("V")),
                                            vec![
                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("W"))),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_if(
                                                e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("U")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("W"))),
                                                vec![
                                                    s_if(
                                                        e_var("utc"),
                                                        vec![
                                                            s_assign("wd", e_call("intval", vec![e_call("gmdate", vec![e_str("w"), e_var("timestamp")])])),
                                                            s_assign("yd", e_call("intval", vec![e_call("gmdate", vec![e_str("z"), e_var("timestamp")])])),
                                                        ],
                                                        vec![],
                                                        Some(vec![
                                                        s_assign("wd", e_call("intval", vec![e_call("date", vec![e_str("w"), e_var("timestamp")])])),
                                                        s_assign("yd", e_call("intval", vec![e_call("date", vec![e_str("z"), e_var("timestamp")])])),
                                                    ]),
                                                    ),
                                                    s_if(
                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("W")),
                                                        vec![
                                                            s_if(
                                                                e_binop(e_var("wd"), BinOp::StrictEq, e_int(0)),
                                                                vec![
                                                                    s_assign("wd", e_int(6)),
                                                                ],
                                                                vec![],
                                                                Some(vec![
                                                                s_assign("wd", e_binop(e_var("wd"), BinOp::Sub, e_int(1))),
                                                            ]),
                                                            ),
                                                        ],
                                                        vec![],
                                                        None,
                                                    ),
                                                    s_assign("wk", e_call("intdiv", vec![e_binop(e_binop(e_var("yd"), BinOp::Add, e_int(7)), BinOp::Sub, e_var("wd")), e_int(7)])),
                                                    s_assign("ws", e_binop(e_str(""), BinOp::Concat, e_var("wk"))),
                                                    s_while(e_binop(e_call("strlen", vec![e_var("ws")]), BinOp::Lt, e_int(2)), vec![
                                                        s_assign("ws", e_binop(e_str("0"), BinOp::Concat, e_var("ws"))),
                                                    ]),
                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("ws"))),
                                                ],
                                                vec![],
                                                Some(vec![
                                                s_if(
                                                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("G")),
                                                    vec![
                                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("o"))),
                                                    ],
                                                    vec![],
                                                    Some(vec![
                                                    s_if(
                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("g")),
                                                        vec![
                                                            s_if(
                                                                e_var("utc"),
                                                                vec![
                                                                    s_assign("iy", e_call("intval", vec![e_call("gmdate", vec![e_str("o"), e_var("timestamp")])])),
                                                                ],
                                                                vec![],
                                                                Some(vec![
                                                                s_assign("iy", e_call("intval", vec![e_call("date", vec![e_str("o"), e_var("timestamp")])])),
                                                            ]),
                                                            ),
                                                            s_assign("g2", e_binop(e_var("iy"), BinOp::Mod, e_int(100))),
                                                            s_assign("gs", e_binop(e_str(""), BinOp::Concat, e_var("g2"))),
                                                            s_while(e_binop(e_call("strlen", vec![e_var("gs")]), BinOp::Lt, e_int(2)), vec![
                                                                s_assign("gs", e_binop(e_str("0"), BinOp::Concat, e_var("gs"))),
                                                            ]),
                                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("gs"))),
                                                        ],
                                                        vec![],
                                                        Some(vec![
                                                        s_if(
                                                            e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("b")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("h"))),
                                                            vec![
                                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("M"))),
                                                            ],
                                                            vec![],
                                                            Some(vec![
                                                            s_if(
                                                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("B")),
                                                                vec![
                                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("F"))),
                                                                ],
                                                                vec![],
                                                                Some(vec![
                                                                s_if(
                                                                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("m")),
                                                                    vec![
                                                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("m"))),
                                                                    ],
                                                                    vec![],
                                                                    Some(vec![
                                                                    s_if(
                                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("y")),
                                                                        vec![
                                                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("y"))),
                                                                        ],
                                                                        vec![],
                                                                        Some(vec![
                                                                        s_if(
                                                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("Y")),
                                                                            vec![
                                                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("Y"))),
                                                                            ],
                                                                            vec![],
                                                                            Some(vec![
                                                                            s_if(
                                                                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("C")),
                                                                                vec![
                                                                                    s_if(
                                                                                        e_var("utc"),
                                                                                        vec![
                                                                                            s_assign("yy", e_call("intval", vec![e_call("gmdate", vec![e_str("Y"), e_var("timestamp")])])),
                                                                                        ],
                                                                                        vec![],
                                                                                        Some(vec![
                                                                                        s_assign("yy", e_call("intval", vec![e_call("date", vec![e_str("Y"), e_var("timestamp")])])),
                                                                                    ]),
                                                                                    ),
                                                                                    s_assign("cen", e_call("intdiv", vec![e_var("yy"), e_int(100)])),
                                                                                    s_assign("cs", e_binop(e_str(""), BinOp::Concat, e_var("cen"))),
                                                                                    s_while(e_binop(e_call("strlen", vec![e_var("cs")]), BinOp::Lt, e_int(2)), vec![
                                                                                        s_assign("cs", e_binop(e_str("0"), BinOp::Concat, e_var("cs"))),
                                                                                    ]),
                                                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("cs"))),
                                                                                ],
                                                                                vec![],
                                                                                Some(vec![
                                                                                s_if(
                                                                                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("H")),
                                                                                    vec![
                                                                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("H"))),
                                                                                    ],
                                                                                    vec![],
                                                                                    Some(vec![
                                                                                    s_if(
                                                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("k")),
                                                                                        vec![
                                                                                            s_if(
                                                                                                e_var("utc"),
                                                                                                vec![
                                                                                                    s_assign("kh", e_call("intval", vec![e_call("gmdate", vec![e_str("G"), e_var("timestamp")])])),
                                                                                                ],
                                                                                                vec![],
                                                                                                Some(vec![
                                                                                                s_assign("kh", e_call("intval", vec![e_call("date", vec![e_str("G"), e_var("timestamp")])])),
                                                                                            ]),
                                                                                            ),
                                                                                            s_assign("ks", e_binop(e_str(""), BinOp::Concat, e_var("kh"))),
                                                                                            s_if(
                                                                                                e_binop(e_call("strlen", vec![e_var("ks")]), BinOp::Lt, e_int(2)),
                                                                                                vec![
                                                                                                    s_assign("ks", e_binop(e_str(" "), BinOp::Concat, e_var("ks"))),
                                                                                                ],
                                                                                                vec![],
                                                                                                None,
                                                                                            ),
                                                                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("ks"))),
                                                                                        ],
                                                                                        vec![],
                                                                                        Some(vec![
                                                                                        s_if(
                                                                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("I")),
                                                                                            vec![
                                                                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("h"))),
                                                                                            ],
                                                                                            vec![],
                                                                                            Some(vec![
                                                                                            s_if(
                                                                                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("l")),
                                                                                                vec![
                                                                                                    s_if(
                                                                                                        e_var("utc"),
                                                                                                        vec![
                                                                                                            s_assign("hh", e_call("intval", vec![e_call("gmdate", vec![e_str("g"), e_var("timestamp")])])),
                                                                                                        ],
                                                                                                        vec![],
                                                                                                        Some(vec![
                                                                                                        s_assign("hh", e_call("intval", vec![e_call("date", vec![e_str("g"), e_var("timestamp")])])),
                                                                                                    ]),
                                                                                                    ),
                                                                                                    s_assign("hs", e_binop(e_str(""), BinOp::Concat, e_var("hh"))),
                                                                                                    s_if(
                                                                                                        e_binop(e_call("strlen", vec![e_var("hs")]), BinOp::Lt, e_int(2)),
                                                                                                        vec![
                                                                                                            s_assign("hs", e_binop(e_str(" "), BinOp::Concat, e_var("hs"))),
                                                                                                        ],
                                                                                                        vec![],
                                                                                                        None,
                                                                                                    ),
                                                                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("hs"))),
                                                                                                ],
                                                                                                vec![],
                                                                                                Some(vec![
                                                                                                s_if(
                                                                                                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("M")),
                                                                                                    vec![
                                                                                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("i"))),
                                                                                                    ],
                                                                                                    vec![],
                                                                                                    Some(vec![
                                                                                                    s_if(
                                                                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("p")),
                                                                                                        vec![
                                                                                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("A"))),
                                                                                                        ],
                                                                                                        vec![],
                                                                                                        Some(vec![
                                                                                                        s_if(
                                                                                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("P")),
                                                                                                            vec![
                                                                                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("a"))),
                                                                                                            ],
                                                                                                            vec![],
                                                                                                            Some(vec![
                                                                                                            s_if(
                                                                                                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("r")),
                                                                                                                vec![
                                                                                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("h:i:s A"))),
                                                                                                                ],
                                                                                                                vec![],
                                                                                                                Some(vec![
                                                                                                                s_if(
                                                                                                                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("R")),
                                                                                                                    vec![
                                                                                                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("H:i"))),
                                                                                                                    ],
                                                                                                                    vec![],
                                                                                                                    Some(vec![
                                                                                                                    s_if(
                                                                                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("S")),
                                                                                                                        vec![
                                                                                                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("s"))),
                                                                                                                        ],
                                                                                                                        vec![],
                                                                                                                        Some(vec![
                                                                                                                        s_if(
                                                                                                                            e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("T")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("X"))),
                                                                                                                            vec![
                                                                                                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("H:i:s"))),
                                                                                                                            ],
                                                                                                                            vec![],
                                                                                                                            Some(vec![
                                                                                                                            s_if(
                                                                                                                                e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("D")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("x"))),
                                                                                                                                vec![
                                                                                                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("m/d/y"))),
                                                                                                                                ],
                                                                                                                                vec![],
                                                                                                                                Some(vec![
                                                                                                                                s_if(
                                                                                                                                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("F")),
                                                                                                                                    vec![
                                                                                                                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("Y-m-d"))),
                                                                                                                                    ],
                                                                                                                                    vec![],
                                                                                                                                    Some(vec![
                                                                                                                                    s_if(
                                                                                                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("s")),
                                                                                                                                        vec![
                                                                                                                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("U"))),
                                                                                                                                        ],
                                                                                                                                        vec![],
                                                                                                                                        Some(vec![
                                                                                                                                        s_if(
                                                                                                                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("z")),
                                                                                                                                            vec![
                                                                                                                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("O"))),
                                                                                                                                            ],
                                                                                                                                            vec![],
                                                                                                                                            Some(vec![
                                                                                                                                            s_if(
                                                                                                                                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("Z")),
                                                                                                                                                vec![
                                                                                                                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("T"))),
                                                                                                                                                ],
                                                                                                                                                vec![],
                                                                                                                                                Some(vec![
                                                                                                                                                s_if(
                                                                                                                                                    e_binop(e_var("spec"), BinOp::StrictEq, e_str("c")),
                                                                                                                                                    vec![
                                                                                                                                                        s_if(
                                                                                                                                                            e_var("utc"),
                                                                                                                                                            vec![
                                                                                                                                                                s_assign("cd", e_call("intval", vec![e_call("gmdate", vec![e_str("j"), e_var("timestamp")])])),
                                                                                                                                                            ],
                                                                                                                                                            vec![],
                                                                                                                                                            Some(vec![
                                                                                                                                                            s_assign("cd", e_call("intval", vec![e_call("date", vec![e_str("j"), e_var("timestamp")])])),
                                                                                                                                                        ]),
                                                                                                                                                        ),
                                                                                                                                                        s_assign("cs", e_binop(e_str(""), BinOp::Concat, e_var("cd"))),
                                                                                                                                                        s_if(
                                                                                                                                                            e_binop(e_call("strlen", vec![e_var("cs")]), BinOp::Lt, e_int(2)),
                                                                                                                                                            vec![
                                                                                                                                                                s_assign("cs", e_binop(e_str(" "), BinOp::Concat, e_var("cs"))),
                                                                                                                                                            ],
                                                                                                                                                            vec![],
                                                                                                                                                            None,
                                                                                                                                                        ),
                                                                                                                                                        s_assign("out", e_binop(e_binop(e_binop(e_var("out"), BinOp::Concat, e_str("D M ")), BinOp::Concat, e_var("cs")), BinOp::Concat, e_str(" H:i:s Y"))),
                                                                                                                                                    ],
                                                                                                                                                    vec![],
                                                                                                                                                    Some(vec![
                                                                                                                                                    s_if(
                                                                                                                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("n")),
                                                                                                                                                        vec![
                                                                                                                                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("\n"))),
                                                                                                                                                        ],
                                                                                                                                                        vec![],
                                                                                                                                                        Some(vec![
                                                                                                                                                        s_if(
                                                                                                                                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("t")),
                                                                                                                                                            vec![
                                                                                                                                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("\t"))),
                                                                                                                                                            ],
                                                                                                                                                            vec![],
                                                                                                                                                            Some(vec![
                                                                                                                                                            s_if(
                                                                                                                                                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("%")),
                                                                                                                                                                vec![
                                                                                                                                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("%"))),
                                                                                                                                                                ],
                                                                                                                                                                vec![],
                                                                                                                                                                Some(vec![
                                                                                                                                                                s_assign("sc", e_call("ord", vec![e_var("spec")])),
                                                                                                                                                                s_if(
                                                                                                                                                                    e_binop(e_binop(e_binop(e_var("sc"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("sc"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("sc"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("sc"), BinOp::LtEq, e_int(122)))),
                                                                                                                                                                    vec![
                                                                                                                                                                        s_assign("out", e_binop(e_binop(e_var("out"), BinOp::Concat, e_str("\\")), BinOp::Concat, e_var("spec"))),
                                                                                                                                                                    ],
                                                                                                                                                                    vec![],
                                                                                                                                                                    Some(vec![
                                                                                                                                                                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("spec"))),
                                                                                                                                                                ]),
                                                                                                                                                                ),
                                                                                                                                                            ]),
                                                                                                                                                            ),
                                                                                                                                                        ]),
                                                                                                                                                        ),
                                                                                                                                                    ]),
                                                                                                                                                    ),
                                                                                                                                                ]),
                                                                                                                                                ),
                                                                                                                                            ]),
                                                                                                                                            ),
                                                                                                                                        ]),
                                                                                                                                        ),
                                                                                                                                    ]),
                                                                                                                                    ),
                                                                                                                                ]),
                                                                                                                                ),
                                                                                                                            ]),
                                                                                                                            ),
                                                                                                                        ]),
                                                                                                                        ),
                                                                                                                    ]),
                                                                                                                    ),
                                                                                                                ]),
                                                                                                                ),
                                                                                                            ]),
                                                                                                            ),
                                                                                                        ]),
                                                                                                        ),
                                                                                                    ]),
                                                                                                    ),
                                                                                                ]),
                                                                                                ),
                                                                                            ]),
                                                                                            ),
                                                                                        ]),
                                                                                        ),
                                                                                    ]),
                                                                                    ),
                                                                                ]),
                                                                                ),
                                                                            ]),
                                                                            ),
                                                                        ]),
                                                                        ),
                                                                    ]),
                                                                    ),
                                                                ]),
                                                                ),
                                                            ]),
                                                            ),
                                                        ]),
                                                        ),
                                                    ]),
                                                    ),
                                                ]),
                                                ),
                                            ]),
                                            ),
                                        ]),
                                        ),
                                    ]),
                                    ),
                                ]),
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
            ]),
            ),
        ]),
        s_if(
            e_var("utc"),
            vec![
                s_return(e_call("gmdate", vec![e_var("out"), e_var("timestamp")])),
            ],
            vec![],
            None,
        ),
        s_return(e_call("date", vec![e_var("out"), e_var("timestamp")])),
    ])
}

/// `DateTime::__elephc_extract_micros` — transcribed method builder.
fn decl_class_datetime_method_46_elephc_extract_micros() -> MethodBuilder {
method("__elephc_extract_micros")
    .static_()
    .param("s", TypeExpr::Str)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_assign("__dot", e_call("strrpos", vec![e_var("s"), e_str(".")])),
        s_if(
            e_binop(e_binop(e_call("substr", vec![e_var("s"), e_int(0), e_int(1)]), BinOp::StrictEq, e_str("@")), BinOp::And, e_binop(e_var("__dot"), BinOp::StrictNotEq, e_bool(false))),
            vec![
                s_assign("__fd", e_call("substr", vec![e_var("s"), e_binop(e_var("__dot"), BinOp::Add, e_int(1))])),
                s_while(e_binop(e_call("strlen", vec![e_var("__fd")]), BinOp::Lt, e_int(6)), vec![
                    s_assign("__fd", e_binop(e_var("__fd"), BinOp::Concat, e_str("0"))),
                ]),
                s_assign("__micro", e_call("intval", vec![e_call("substr", vec![e_var("__fd"), e_int(0), e_int(6)])])),
                s_if(
                    e_binop(e_binop(e_call("substr", vec![e_var("s"), e_int(1), e_int(1)]), BinOp::StrictEq, e_str("-")), BinOp::And, e_binop(e_var("__micro"), BinOp::StrictNotEq, e_int(0))),
                    vec![
                        s_return(e_binop(e_int(1000000), BinOp::Sub, e_var("__micro"))),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_var("__micro")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_binop(e_var("__dot"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_var("__dot"), BinOp::GtEq, e_int(3))), BinOp::And, e_binop(e_call("substr", vec![e_var("s"), e_binop(e_var("__dot"), BinOp::Sub, e_int(3)), e_int(1)]), BinOp::StrictEq, e_str(":"))),
            vec![
                s_assign("__fd", e_str("")),
                s_assign("__k", e_binop(e_var("__dot"), BinOp::Add, e_int(1))),
                s_assign("__len", e_call("strlen", vec![e_var("s")])),
                s_while(e_binop(e_var("__k"), BinOp::Lt, e_var("__len")), vec![
                    s_assign("__c", e_call("substr", vec![e_var("s"), e_var("__k"), e_int(1)])),
                    s_if(
                        e_binop(e_binop(e_var("__c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("__c"), BinOp::LtEq, e_str("9"))),
                        vec![
                            s_assign("__fd", e_binop(e_var("__fd"), BinOp::Concat, e_var("__c"))),
                            s_assign("__k", e_binop(e_var("__k"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        Some(vec![
                        s_break(1),
                    ]),
                    ),
                ]),
                s_if(
                    e_binop(e_var("__fd"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_while(e_binop(e_call("strlen", vec![e_var("__fd")]), BinOp::Lt, e_int(6)), vec![
                            s_assign("__fd", e_binop(e_var("__fd"), BinOp::Concat, e_str("0"))),
                        ]),
                        s_return(e_call("intval", vec![e_call("substr", vec![e_var("__fd"), e_int(0), e_int(6)])])),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_return(e_int(0)),
    ])
}

/// `DateTime::__elephc_strip_micros` — transcribed method builder.
fn decl_class_datetime_method_47_elephc_strip_micros() -> MethodBuilder {
method("__elephc_strip_micros")
    .static_()
    .param("s", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("__dot", e_call("strrpos", vec![e_var("s"), e_str(".")])),
        s_if(
            e_binop(e_binop(e_binop(e_var("__dot"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_var("__dot"), BinOp::GtEq, e_int(3))), BinOp::And, e_binop(e_call("substr", vec![e_var("s"), e_binop(e_var("__dot"), BinOp::Sub, e_int(3)), e_int(1)]), BinOp::StrictEq, e_str(":"))),
            vec![
                s_assign("__k", e_binop(e_var("__dot"), BinOp::Add, e_int(1))),
                s_assign("__len", e_call("strlen", vec![e_var("s")])),
                s_while(e_binop(e_var("__k"), BinOp::Lt, e_var("__len")), vec![
                    s_assign("__c", e_call("substr", vec![e_var("s"), e_var("__k"), e_int(1)])),
                    s_if(
                        e_binop(e_binop(e_var("__c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("__c"), BinOp::LtEq, e_str("9"))),
                        vec![
                            s_assign("__k", e_binop(e_var("__k"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        Some(vec![
                        s_break(1),
                    ]),
                    ),
                ]),
                s_return(e_binop(e_call("substr", vec![e_var("s"), e_int(0), e_var("__dot")]), BinOp::Concat, e_call("substr", vec![e_var("s"), e_var("__k")]))),
            ],
            vec![],
            None,
        ),
        s_return(e_binop(e_var("s"), BinOp::Concat, e_str(""))),
    ])
}

/// `DateTime::__elephc_extract_constructor_zone` — transcribed method builder.
fn decl_class_datetime_method_48_elephc_extract_constructor_zone() -> MethodBuilder {
method("__elephc_extract_constructor_zone")
    .static_()
    .param("datetime", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("__display", e_str("")),
        s_assign("__base", e_binop(e_var("datetime"), BinOp::Concat, e_str(""))),
        s_assign("__len", e_call("strlen", vec![e_var("datetime")])),
        s_assign("__normalized", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_var("datetime")])),
        s_if(
            e_binop(e_var("__normalized"), BinOp::StrictNotEq, e_str("")),
            vec![
                s_assign("__display", e_binop(e_str(""), BinOp::Concat, e_var("__normalized"))),
                s_assign("__base", e_str("now")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_binop(e_var("__display"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_var("__len"), BinOp::GtEq, e_int(4))), BinOp::And, e_binop(e_call("strtoupper", vec![e_call("substr", vec![e_var("datetime"), e_binop(e_var("__len"), BinOp::Sub, e_int(4))])]), BinOp::StrictEq, e_str(" GMT"))),
            vec![
                s_assign("__display", e_str("GMT")),
                s_assign("__base", e_call("substr", vec![e_var("datetime"), e_int(0), e_binop(e_var("__len"), BinOp::Sub, e_int(4))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_var("__display"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_call("strpos", vec![e_var("datetime"), e_str(" GMT ")]), BinOp::StrictNotEq, e_bool(false))),
            vec![
                s_assign("__display", e_str("GMT")),
                s_assign("__base", e_call("str_replace", vec![e_str(" GMT "), e_str(" "), e_var("datetime")])),
            ],
            vec![],
            None,
        ),
        s_assign("__space", e_call("strrpos", vec![e_var("datetime"), e_str(" ")])),
        s_if(
            e_binop(e_binop(e_binop(e_var("__display"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_var("__space"), BinOp::StrictNotEq, e_bool(false))), BinOp::And, e_binop(e_binop(e_var("__space"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("__len"))),
            vec![
                s_assign("__candidate", e_call("substr", vec![e_var("datetime"), e_binop(e_var("__space"), BinOp::Add, e_int(1))])),
                s_assign("__normalized", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_var("__candidate")])),
                s_if(
                    e_binop(e_var("__normalized"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_assign("__display", e_binop(e_str(""), BinOp::Concat, e_var("__normalized"))),
                        s_assign("__base", e_call("substr", vec![e_var("datetime"), e_int(0), e_var("__space")])),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_var("__display"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_var("__len"), BinOp::Gt, e_int(1))),
            vec![
                s_assign("__last", e_call("strtoupper", vec![e_call("substr", vec![e_var("datetime"), e_binop(e_var("__len"), BinOp::Sub, e_int(1)), e_int(1)])])),
                s_assign("__lastCode", e_call("ord", vec![e_var("__last")])),
                s_assign("__previous", e_call("substr", vec![e_var("datetime"), e_binop(e_var("__len"), BinOp::Sub, e_int(2)), e_int(1)])),
                s_assign("__military", e_binop(e_binop(e_binop(e_var("__lastCode"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("__lastCode"), BinOp::LtEq, e_int(73))), BinOp::Or, e_binop(e_binop(e_var("__lastCode"), BinOp::GtEq, e_int(75)), BinOp::And, e_binop(e_var("__lastCode"), BinOp::LtEq, e_int(90))))),
                s_if(
                    e_binop(e_var("__military"), BinOp::And, e_call("ctype_digit", vec![e_var("__previous")])),
                    vec![
                        s_assign("__normalized", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_var("__last")])),
                        s_if(
                            e_binop(e_var("__normalized"), BinOp::StrictNotEq, e_str("")),
                            vec![
                                s_assign("__display", e_binop(e_str(""), BinOp::Concat, e_var("__normalized"))),
                                s_assign("__base", e_call("substr", vec![e_var("datetime"), e_int(0), e_binop(e_var("__len"), BinOp::Sub, e_int(1))])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("__display"), BinOp::StrictEq, e_str("")),
            vec![
                s_assign("__plus", e_call("strrpos", vec![e_var("datetime"), e_str("+")])),
                s_assign("__minus", e_call("strrpos", vec![e_var("datetime"), e_str("-")])),
                s_assign("__offset", e_var("__plus")),
                s_if(
                    e_binop(e_binop(e_var("__minus"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_binop(e_var("__offset"), BinOp::StrictEq, e_bool(false)), BinOp::Or, e_binop(e_var("__minus"), BinOp::Gt, e_var("__offset")))),
                    vec![
                        s_assign("__offset", e_var("__minus")),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_var("__offset"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_var("__offset"), BinOp::Gt, e_int(0))), BinOp::And, e_binop(e_call("strrpos", vec![e_call("substr", vec![e_var("datetime"), e_int(0), e_var("__offset")]), e_str(":")]), BinOp::StrictNotEq, e_bool(false))),
                    vec![
                        s_assign("__candidate", e_call("substr", vec![e_var("datetime"), e_var("__offset")])),
                        s_assign("__normalized", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_var("__candidate")])),
                        s_if(
                            e_binop(e_var("__normalized"), BinOp::StrictNotEq, e_str("")),
                            vec![
                                s_assign("__display", e_binop(e_str(""), BinOp::Concat, e_var("__normalized"))),
                                s_assign("__base", e_call("substr", vec![e_var("datetime"), e_int(0), e_var("__offset")])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_return(e_binop(e_binop(e_var("__display"), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("__base"))),
    ])
}

/// `DateTime::__elephc_extract_modify_micros` — transcribed method builder.
fn decl_class_datetime_method_49_elephc_extract_modify_micros() -> MethodBuilder {
method("__elephc_extract_modify_micros")
    .static_()
    .param("m", TypeExpr::Str)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_assign("__toks", e_call("explode", vec![e_str(" "), e_var("m")])),
        s_assign("__n", e_call("count", vec![e_var("__toks")])),
        s_assign("__sum", e_int(0)),
        s_assign("__i", e_int(0)),
        s_while(e_binop(e_var("__i"), BinOp::Lt, e_var("__n")), vec![
            s_assign("__t", e_call("strtolower", vec![e_index(e_var("__toks"), e_var("__i"))])),
            s_assign("__factor", e_int(0)),
            s_if(
                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("__t"), BinOp::StrictEq, e_str("microsecond")), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("microseconds"))), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("usec"))), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("usecs"))), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("µs"))), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("µsec"))), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("µsecs"))),
                vec![
                    s_assign("__factor", e_int(1)),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_binop(e_binop(e_binop(e_binop(e_var("__t"), BinOp::StrictEq, e_str("millisecond")), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("milliseconds"))), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("ms"))), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("msec"))), BinOp::Or, e_binop(e_var("__t"), BinOp::StrictEq, e_str("msecs"))),
                    vec![
                        s_assign("__factor", e_int(1000)),
                    ],
                    vec![],
                    None,
                ),
            ]),
            ),
            s_if(
                e_binop(e_binop(e_var("__factor"), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_var("__i"), BinOp::Gt, e_int(0))),
                vec![
                    s_assign("__sum", e_binop(e_var("__sum"), BinOp::Add, e_binop(e_call("intval", vec![e_index(e_var("__toks"), e_binop(e_var("__i"), BinOp::Sub, e_int(1)))]), BinOp::Mul, e_var("__factor")))),
                ],
                vec![],
                None,
            ),
            s_assign("__i", e_binop(e_var("__i"), BinOp::Add, e_int(1))),
        ]),
        s_return(e_var("__sum")),
    ])
}

/// `DateTime::__elephc_strip_modify_micros` — transcribed method builder.
fn decl_class_datetime_method_50_elephc_strip_modify_micros() -> MethodBuilder {
method("__elephc_strip_modify_micros")
    .static_()
    .param("m", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("__toks", e_call("explode", vec![e_str(" "), e_var("m")])),
        s_assign("__n", e_call("count", vec![e_var("__toks")])),
        s_assign("__out", e_str("")),
        s_assign("__i", e_int(0)),
        s_while(e_binop(e_var("__i"), BinOp::Lt, e_var("__n")), vec![
            s_assign("__unit", e_int(0)),
            s_if(
                e_binop(e_binop(e_var("__i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("__n")),
                vec![
                    s_assign("__nt", e_call("strtolower", vec![e_index(e_var("__toks"), e_binop(e_var("__i"), BinOp::Add, e_int(1)))])),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("__nt"), BinOp::StrictEq, e_str("microsecond")), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("microseconds"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("usec"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("usecs"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("µs"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("µsec"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("µsecs"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("millisecond"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("milliseconds"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("ms"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("msec"))), BinOp::Or, e_binop(e_var("__nt"), BinOp::StrictEq, e_str("msecs"))),
                        vec![
                            s_assign("__unit", e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("__unit"), BinOp::StrictEq, e_int(1)),
                vec![
                    s_assign("__i", e_binop(e_var("__i"), BinOp::Add, e_int(2))),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_var("__out"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_assign("__out", e_binop(e_var("__out"), BinOp::Concat, e_str(" "))),
                    ],
                    vec![],
                    None,
                ),
                s_assign("__out", e_binop(e_var("__out"), BinOp::Concat, e_index(e_var("__toks"), e_var("__i")))),
                s_assign("__i", e_binop(e_var("__i"), BinOp::Add, e_int(1))),
            ]),
            ),
        ]),
        s_return(e_var("__out")),
    ])
}

/// `DateTime::__elephc_malformed_time_message` — transcribed method builder.
fn decl_class_datetime_method_51_elephc_malformed_time_message() -> MethodBuilder {
method("__elephc_malformed_time_message")
    .static_()
    .param("context", TypeExpr::Str)
    .param("input", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("__parsed", e_static_call("DateTime", "__elephc_date_parse", vec![e_var("input")])),
        s_assign("__position", e_int(0)),
        s_assign("__message", e_str("Unknown or bad format")),
        s_if(
            e_binop(e_index(e_var("__parsed"), e_str("error_count")), BinOp::Gt, e_int(0)),
            vec![
                s_assign("__errors", e_index(e_var("__parsed"), e_str("errors"))),
                s_assign("__position", e_call("intval", vec![e_call("array_key_first", vec![e_var("__errors")])])),
                s_assign("__message", e_index(e_var("__errors"), e_var("__position"))),
            ],
            vec![],
            None,
        ),
        s_assign("__character", e_call("substr", vec![e_var("input"), e_var("__position"), e_int(1)])),
        s_if(
            e_binop(e_var("__character"), BinOp::StrictEq, e_str("")),
            vec![
                s_assign("__character", e_str(" ")),
            ],
            vec![],
            None,
        ),
        s_return(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("context"), BinOp::Concat, e_str("Failed to parse time string (")), BinOp::Concat, e_var("input")), BinOp::Concat, e_str(") at position ")), BinOp::Concat, e_var("__position")), BinOp::Concat, e_str(" (")), BinOp::Concat, e_var("__character")), BinOp::Concat, e_str("): ")), BinOp::Concat, e_var("__message"))),
    ])
}

/// `DateTime::__elephc_sun_rs` — transcribed method builder.
fn decl_class_datetime_method_52_elephc_sun_rs() -> MethodBuilder {
method("__elephc_sun_rs")
    .static_()
    .param("t_utc_sse", TypeExpr::Int)
    .param("lon", TypeExpr::Float)
    .param("lat", TypeExpr::Float)
    .param("altit", TypeExpr::Float)
    .param("limb", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("j2000", e_binop(e_binop(e_binop(e_var("t_utc_sse"), BinOp::Div, e_float(86400.0)), BinOp::Add, e_float(2440587.5)), BinOp::Sub, e_float(2451545.0))),
        s_assign("d", e_binop(e_binop(e_var("j2000"), BinOp::Add, e_int(2)), BinOp::Sub, e_binop(e_var("lon"), BinOp::Div, e_float(360.0)))),
        s_assign("gmst0", e_binop(e_binop(e_binop(e_float(180.0), BinOp::Add, e_float(356.047)), BinOp::Add, e_float(282.9404)), BinOp::Add, e_binop(e_binop(e_float(0.9856002585), BinOp::Add, e_float(4.70935e-5)), BinOp::Mul, e_var("d")))),
        s_assign("gmst0", e_binop(e_var("gmst0"), BinOp::Sub, e_binop(e_float(360.0), BinOp::Mul, e_call("floor", vec![e_binop(e_var("gmst0"), BinOp::Div, e_float(360.0))])))),
        s_assign("M", e_binop(e_float(356.047), BinOp::Add, e_binop(e_float(0.9856002585), BinOp::Mul, e_var("d")))),
        s_assign("M", e_binop(e_var("M"), BinOp::Sub, e_binop(e_float(360.0), BinOp::Mul, e_call("floor", vec![e_binop(e_var("M"), BinOp::Div, e_float(360.0))])))),
        s_assign("w", e_binop(e_float(282.9404), BinOp::Add, e_binop(e_float(4.70935e-5), BinOp::Mul, e_var("d")))),
        s_assign("e", e_binop(e_float(0.016709), BinOp::Sub, e_binop(e_float(1.151e-9), BinOp::Mul, e_var("d")))),
        s_assign("E", e_binop(e_var("M"), BinOp::Add, e_binop(e_binop(e_binop(e_var("e"), BinOp::Mul, e_binop(e_float(180.0), BinOp::Div, e_float(3.141592653589793))), BinOp::Mul, e_call("sin", vec![e_binop(e_binop(e_var("M"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))])), BinOp::Mul, e_binop(e_float(1.0), BinOp::Add, e_binop(e_var("e"), BinOp::Mul, e_call("cos", vec![e_binop(e_binop(e_var("M"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))])))))),
        s_assign("x", e_binop(e_call("cos", vec![e_binop(e_binop(e_var("E"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]), BinOp::Sub, e_var("e"))),
        s_assign("y", e_binop(e_call("sqrt", vec![e_binop(e_float(1.0), BinOp::Sub, e_binop(e_var("e"), BinOp::Mul, e_var("e")))]), BinOp::Mul, e_call("sin", vec![e_binop(e_binop(e_var("E"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]))),
        s_assign("sr", e_call("sqrt", vec![e_binop(e_binop(e_var("x"), BinOp::Mul, e_var("x")), BinOp::Add, e_binop(e_var("y"), BinOp::Mul, e_var("y")))])),
        s_assign("v", e_binop(e_binop(e_float(180.0), BinOp::Div, e_float(3.141592653589793)), BinOp::Mul, e_call("atan2", vec![e_var("y"), e_var("x")]))),
        s_assign("slon", e_binop(e_var("v"), BinOp::Add, e_var("w"))),
        s_if(
            e_binop(e_var("slon"), BinOp::GtEq, e_float(360.0)),
            vec![
                s_assign("slon", e_binop(e_var("slon"), BinOp::Sub, e_float(360.0))),
            ],
            vec![],
            None,
        ),
        s_assign("xx", e_binop(e_var("sr"), BinOp::Mul, e_call("cos", vec![e_binop(e_binop(e_var("slon"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]))),
        s_assign("yy", e_binop(e_var("sr"), BinOp::Mul, e_call("sin", vec![e_binop(e_binop(e_var("slon"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]))),
        s_assign("obl", e_binop(e_float(23.4393), BinOp::Sub, e_binop(e_float(3.563e-7), BinOp::Mul, e_var("d")))),
        s_assign("z", e_binop(e_var("yy"), BinOp::Mul, e_call("sin", vec![e_binop(e_binop(e_var("obl"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]))),
        s_assign("yy", e_binop(e_var("yy"), BinOp::Mul, e_call("cos", vec![e_binop(e_binop(e_var("obl"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]))),
        s_assign("sRA", e_binop(e_binop(e_float(180.0), BinOp::Div, e_float(3.141592653589793)), BinOp::Mul, e_call("atan2", vec![e_var("yy"), e_var("xx")]))),
        s_assign("sdec", e_binop(e_binop(e_float(180.0), BinOp::Div, e_float(3.141592653589793)), BinOp::Mul, e_call("atan2", vec![e_var("z"), e_call("sqrt", vec![e_binop(e_binop(e_var("xx"), BinOp::Mul, e_var("xx")), BinOp::Add, e_binop(e_var("yy"), BinOp::Mul, e_var("yy")))])]))),
        s_assign("sidtime", e_binop(e_binop(e_var("gmst0"), BinOp::Add, e_float(180.0)), BinOp::Add, e_var("lon"))),
        s_assign("sidtime", e_binop(e_var("sidtime"), BinOp::Sub, e_binop(e_float(360.0), BinOp::Mul, e_call("floor", vec![e_binop(e_var("sidtime"), BinOp::Div, e_float(360.0))])))),
        s_assign("diff", e_binop(e_var("sidtime"), BinOp::Sub, e_var("sRA"))),
        s_assign("diff", e_binop(e_var("diff"), BinOp::Sub, e_binop(e_float(360.0), BinOp::Mul, e_call("floor", vec![e_binop(e_binop(e_var("diff"), BinOp::Div, e_float(360.0)), BinOp::Add, e_float(0.5))])))),
        s_assign("tsouth", e_binop(e_float(12.0), BinOp::Sub, e_binop(e_var("diff"), BinOp::Div, e_float(15.0)))),
        s_assign("sradius", e_binop(e_float(0.2666), BinOp::Div, e_var("sr"))),
        s_if(
            e_binop(e_var("limb"), BinOp::NotEq, e_int(0)),
            vec![
                s_assign("altit", e_binop(e_var("altit"), BinOp::Sub, e_var("sradius"))),
            ],
            vec![],
            None,
        ),
        s_assign("cost", e_binop(e_binop(e_call("sin", vec![e_binop(e_binop(e_var("altit"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]), BinOp::Sub, e_binop(e_call("sin", vec![e_binop(e_binop(e_var("lat"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]), BinOp::Mul, e_call("sin", vec![e_binop(e_binop(e_var("sdec"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]))), BinOp::Div, e_binop(e_call("cos", vec![e_binop(e_binop(e_var("lat"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))]), BinOp::Mul, e_call("cos", vec![e_binop(e_binop(e_var("sdec"), BinOp::Mul, e_float(3.141592653589793)), BinOp::Div, e_float(180.0))])))),
        s_assign("rc", e_int(0)),
        s_assign("hr", e_float(0.0)),
        s_assign("hs", e_float(0.0)),
        s_if(
            e_binop(e_var("cost"), BinOp::GtEq, e_float(1.0)),
            vec![
                s_assign("rc", e_neg(e_int(1))),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_var("cost"), BinOp::LtEq, e_neg(e_float(1.0))),
                vec![
                    s_assign("rc", e_int(1)),
                ],
                vec![],
                Some(vec![
                s_assign("t", e_binop(e_binop(e_binop(e_float(180.0), BinOp::Div, e_float(3.141592653589793)), BinOp::Mul, e_call("acos", vec![e_var("cost")])), BinOp::Div, e_float(15.0))),
                s_assign("hr", e_binop(e_var("tsouth"), BinOp::Sub, e_var("t"))),
                s_assign("hs", e_binop(e_var("tsouth"), BinOp::Add, e_var("t"))),
            ]),
            ),
        ]),
        ),
        s_return(e_array_assoc(vec![(e_str("rc"), e_var("rc")), (e_str("hr"), e_var("hr")), (e_str("hs"), e_var("hs")), (e_str("ts"), e_var("tsouth"))])),
    ])
}

/// `DateTime::__elephc_sun_val` — transcribed method builder.
fn decl_class_datetime_method_53_elephc_sun_val() -> MethodBuilder {
method("__elephc_sun_val")
    .static_()
    .param("rc", TypeExpr::Int)
    .param("tsval", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_binop(e_var("rc"), BinOp::Eq, e_int(1)),
            vec![
                s_return(e_bool(true)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("rc"), BinOp::Eq, e_neg(e_int(1))),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_return(e_var("tsval")),
    ])
}

/// `DateTime::__elephc_date_sun_info` — transcribed method builder.
fn decl_class_datetime_method_54_elephc_date_sun_info() -> MethodBuilder {
method("__elephc_date_sun_info")
    .static_()
    .param("timestamp", TypeExpr::Int)
    .param("latitude", TypeExpr::Float)
    .param("longitude", TypeExpr::Float)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_not(e_call("is_finite", vec![e_var("latitude")])),
            vec![
                s_throw(e_new("ValueError", vec![e_str("date_sun_info(): Argument #2 ($latitude) must be finite")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_finite", vec![e_var("longitude")])),
            vec![
                s_throw(e_new("ValueError", vec![e_str("date_sun_info(): Argument #3 ($longitude) must be finite")])),
            ],
            vec![],
            None,
        ),
        s_assign("y", e_call("intval", vec![e_call("date", vec![e_str("Y"), e_var("timestamp")])])),
        s_assign("mo", e_call("intval", vec![e_call("date", vec![e_str("n"), e_var("timestamp")])])),
        s_assign("dy", e_call("intval", vec![e_call("date", vec![e_str("j"), e_var("timestamp")])])),
        s_assign("u", e_call("__elephc_gmmktime_raw", vec![e_int(0), e_int(0), e_int(0), e_var("mo"), e_var("dy"), e_var("y")])),
        s_assign("off", e_static_call("DateTime", "__elephc_sun_rs", vec![e_var("u"), e_var("longitude"), e_var("latitude"), e_binop(e_neg(e_float(35.0)), BinOp::Div, e_float(60.0)), e_int(1)])),
        s_assign("civ", e_static_call("DateTime", "__elephc_sun_rs", vec![e_var("u"), e_var("longitude"), e_var("latitude"), e_neg(e_float(6.0)), e_int(0)])),
        s_assign("nau", e_static_call("DateTime", "__elephc_sun_rs", vec![e_var("u"), e_var("longitude"), e_var("latitude"), e_neg(e_float(12.0)), e_int(0)])),
        s_assign("ast", e_static_call("DateTime", "__elephc_sun_rs", vec![e_var("u"), e_var("longitude"), e_var("latitude"), e_neg(e_float(18.0)), e_int(0)])),
        s_assign("sunrise", e_static_call("DateTime", "__elephc_sun_val", vec![e_index(e_var("off"), e_str("rc")), e_call("intval", vec![e_binop(e_binop(e_index(e_var("off"), e_str("hr")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])])),
        s_assign("sunset", e_static_call("DateTime", "__elephc_sun_val", vec![e_index(e_var("off"), e_str("rc")), e_call("intval", vec![e_binop(e_binop(e_index(e_var("off"), e_str("hs")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])])),
        s_assign("transit", e_call("intval", vec![e_binop(e_binop(e_index(e_var("off"), e_str("ts")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])),
        s_assign("cb", e_static_call("DateTime", "__elephc_sun_val", vec![e_index(e_var("civ"), e_str("rc")), e_call("intval", vec![e_binop(e_binop(e_index(e_var("civ"), e_str("hr")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])])),
        s_assign("ce", e_static_call("DateTime", "__elephc_sun_val", vec![e_index(e_var("civ"), e_str("rc")), e_call("intval", vec![e_binop(e_binop(e_index(e_var("civ"), e_str("hs")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])])),
        s_assign("nb", e_static_call("DateTime", "__elephc_sun_val", vec![e_index(e_var("nau"), e_str("rc")), e_call("intval", vec![e_binop(e_binop(e_index(e_var("nau"), e_str("hr")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])])),
        s_assign("ne", e_static_call("DateTime", "__elephc_sun_val", vec![e_index(e_var("nau"), e_str("rc")), e_call("intval", vec![e_binop(e_binop(e_index(e_var("nau"), e_str("hs")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])])),
        s_assign("ab", e_static_call("DateTime", "__elephc_sun_val", vec![e_index(e_var("ast"), e_str("rc")), e_call("intval", vec![e_binop(e_binop(e_index(e_var("ast"), e_str("hr")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])])),
        s_assign("ae", e_static_call("DateTime", "__elephc_sun_val", vec![e_index(e_var("ast"), e_str("rc")), e_call("intval", vec![e_binop(e_binop(e_index(e_var("ast"), e_str("hs")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])])),
        s_return(e_array_assoc(vec![(e_str("sunrise"), e_var("sunrise")), (e_str("sunset"), e_var("sunset")), (e_str("transit"), e_var("transit")), (e_str("civil_twilight_begin"), e_var("cb")), (e_str("civil_twilight_end"), e_var("ce")), (e_str("nautical_twilight_begin"), e_var("nb")), (e_str("nautical_twilight_end"), e_var("ne")), (e_str("astronomical_twilight_begin"), e_var("ab")), (e_str("astronomical_twilight_end"), e_var("ae"))])),
    ])
}

/// `DateTime::__elephc_date_sunfunc` — transcribed method builder.
fn decl_class_datetime_method_55_elephc_date_sunfunc() -> MethodBuilder {
method("__elephc_date_sunfunc")
    .static_()
    .param("which", TypeExpr::Int)
    .param_default("line", TypeExpr::Int, e_int(0))
    .param("timestamp", TypeExpr::Int)
    .param_default("returnFormat", TypeExpr::Int, e_int(1))
    .param_default("latitude", t_nullable(TypeExpr::Float), e_null())
    .param_default("longitude", t_nullable(TypeExpr::Float), e_null())
    .param_default("zenith", t_nullable(TypeExpr::Float), e_null())
    .param_default("utcOffset", t_nullable(TypeExpr::Float), e_null())
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_binop(e_var("which"), BinOp::Eq, e_int(0)),
            vec![
                s_if(
                    e_binop(e_var("line"), BinOp::Gt, e_int(0)),
                    vec![
                        s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: Function date_sunrise() is deprecated since 8.1, use date_sun_info() instead"), e_var("line"), e_const("E_DEPRECATED")])),
                    ],
                    vec![],
                    Some(vec![
                    s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: Function date_sunrise() is deprecated since 8.1, use date_sun_info() instead\n"), e_int(0), e_const("E_DEPRECATED")])),
                ]),
                ),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_var("line"), BinOp::Gt, e_int(0)),
                vec![
                    s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: Function date_sunset() is deprecated since 8.1, use date_sun_info() instead"), e_var("line"), e_const("E_DEPRECATED")])),
                ],
                vec![],
                Some(vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_str("\nDeprecated: Function date_sunset() is deprecated since 8.1, use date_sun_info() instead\n"), e_int(0), e_const("E_DEPRECATED")])),
            ]),
            ),
        ]),
        ),
        s_if(
            e_binop(e_binop(e_binop(e_var("returnFormat"), BinOp::StrictNotEq, e_int(0)), BinOp::And, e_binop(e_var("returnFormat"), BinOp::StrictNotEq, e_int(1))), BinOp::And, e_binop(e_var("returnFormat"), BinOp::StrictNotEq, e_int(2))),
            vec![
                s_if(
                    e_binop(e_var("which"), BinOp::Eq, e_int(0)),
                    vec![
                        s_throw(e_new("ValueError", vec![e_str("date_sunrise(): Argument #2 ($returnFormat) must be one of SUNFUNCS_RET_TIMESTAMP, SUNFUNCS_RET_STRING, or SUNFUNCS_RET_DOUBLE")])),
                    ],
                    vec![],
                    None,
                ),
                s_throw(e_new("ValueError", vec![e_str("date_sunset(): Argument #2 ($returnFormat) must be one of SUNFUNCS_RET_TIMESTAMP, SUNFUNCS_RET_STRING, or SUNFUNCS_RET_DOUBLE")])),
            ],
            vec![],
            None,
        ),
        s_assign("lat", e_ternary(e_binop(e_var("latitude"), BinOp::StrictEq, e_null()), e_float(31.7667), e_var("latitude"))),
        s_assign("lon", e_ternary(e_binop(e_var("longitude"), BinOp::StrictEq, e_null()), e_float(35.2333), e_var("longitude"))),
        s_assign("zen", e_ternary(e_binop(e_var("zenith"), BinOp::StrictEq, e_null()), e_binop(e_float(90.0), BinOp::Add, e_binop(e_float(50.0), BinOp::Div, e_float(60.0))), e_var("zenith"))),
        s_if(
            e_binop(e_binop(e_not(e_call("is_finite", vec![e_var("lat")])), BinOp::Or, e_not(e_call("is_finite", vec![e_var("lon")]))), BinOp::Or, e_not(e_call("is_finite", vec![e_var("zen")]))),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("offset", e_ternary(e_binop(e_var("utcOffset"), BinOp::StrictEq, e_null()), e_binop(e_call("intval", vec![e_call("date", vec![e_str("Z")])]), BinOp::Div, e_float(3600.0)), e_var("utcOffset"))),
        s_assign("y", e_call("intval", vec![e_call("date", vec![e_str("Y"), e_var("timestamp")])])),
        s_assign("mo", e_call("intval", vec![e_call("date", vec![e_str("n"), e_var("timestamp")])])),
        s_assign("dy", e_call("intval", vec![e_call("date", vec![e_str("j"), e_var("timestamp")])])),
        s_assign("u", e_call("__elephc_gmmktime_raw", vec![e_int(0), e_int(0), e_int(0), e_var("mo"), e_var("dy"), e_var("y")])),
        s_assign("r", e_static_call("DateTime", "__elephc_sun_rs", vec![e_var("u"), e_var("lon"), e_var("lat"), e_binop(e_float(90.0), BinOp::Sub, e_var("zen")), e_int(1)])),
        s_if(
            e_binop(e_index(e_var("r"), e_str("rc")), BinOp::NotEq, e_int(0)),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("returnFormat"), BinOp::Eq, e_int(0)),
            vec![
                s_if(
                    e_binop(e_var("which"), BinOp::Eq, e_int(0)),
                    vec![
                        s_return(e_call("intval", vec![e_binop(e_binop(e_index(e_var("r"), e_str("hr")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])),
                    ],
                    vec![],
                    None,
                ),
                s_return(e_call("intval", vec![e_binop(e_binop(e_index(e_var("r"), e_str("hs")), BinOp::Mul, e_int(3600)), BinOp::Add, e_var("u"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("which"), BinOp::Eq, e_int(0)),
            vec![
                s_assign("N", e_binop(e_index(e_var("r"), e_str("hr")), BinOp::Add, e_var("offset"))),
            ],
            vec![],
            Some(vec![
            s_assign("N", e_binop(e_index(e_var("r"), e_str("hs")), BinOp::Add, e_var("offset"))),
        ]),
        ),
        s_if(
            e_binop(e_binop(e_var("N"), BinOp::Gt, e_float(24.0)), BinOp::Or, e_binop(e_var("N"), BinOp::Lt, e_float(0.0))),
            vec![
                s_assign("N", e_binop(e_var("N"), BinOp::Sub, e_binop(e_call("floor", vec![e_binop(e_var("N"), BinOp::Div, e_float(24.0))]), BinOp::Mul, e_float(24.0)))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_binop(e_binop(e_var("N"), BinOp::LtEq, e_float(24.0)), BinOp::And, e_binop(e_var("N"), BinOp::GtEq, e_float(0.0)))),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("returnFormat"), BinOp::Eq, e_int(2)),
            vec![
                s_return(e_var("N")),
            ],
            vec![],
            None,
        ),
        s_assign("hh", e_call("intval", vec![e_var("N")])),
        s_assign("mm", e_call("intval", vec![e_binop(e_float(60.0), BinOp::Mul, e_binop(e_var("N"), BinOp::Sub, e_var("hh")))])),
        s_return(e_call("sprintf", vec![e_str("%02d:%02d"), e_var("hh"), e_var("mm")])),
    ])
}

/// `DateTime::__elephc_strptime` — transcribed method builder.
fn decl_class_datetime_method_56_elephc_strptime() -> MethodBuilder {
method("__elephc_strptime")
    .static_()
    .param("timestamp", TypeExpr::Str)
    .param("format", TypeExpr::Str)
    .returns(t_mixed())
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_str("Deprecated: Function strptime() is deprecated since 8.2, use date_parse_from_format() (for locale-independent parsing), or IntlDateFormatter::parse() (for locale-dependent parsing) instead\n"), e_int(0), e_const("E_DEPRECATED")])),
        s_assign("slen", e_call("strlen", vec![e_var("timestamp")])),
        s_assign("flen", e_call("strlen", vec![e_var("format")])),
        s_assign("sec", e_int(0)),
        s_assign("min", e_int(0)),
        s_assign("hour", e_int(0)),
        s_assign("mday", e_int(0)),
        s_assign("mon", e_int(0)),
        s_assign("year", e_int(0)),
        s_assign("gotY", e_bool(false)),
        s_assign("gotMon", e_bool(false)),
        s_assign("gotMday", e_bool(false)),
        s_assign("sp", e_int(0)),
        s_assign("fp", e_int(0)),
        s_assign("ok", e_bool(true)),
        s_while(e_binop(e_var("fp"), BinOp::Lt, e_var("flen")), vec![
            s_assign("fc", e_index(e_var("format"), e_var("fp"))),
            s_if(
                e_binop(e_var("fc"), BinOp::StrictEq, e_str("%")),
                vec![
                    s_assign("fp", e_binop(e_var("fp"), BinOp::Add, e_int(1))),
                    s_if(
                        e_binop(e_var("fp"), BinOp::GtEq, e_var("flen")),
                        vec![
                            s_assign("ok", e_bool(false)),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("spec", e_index(e_var("format"), e_var("fp"))),
                    s_assign("fp", e_binop(e_var("fp"), BinOp::Add, e_int(1))),
                    s_if(
                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("%")),
                        vec![
                            s_if(
                                e_binop(e_binop(e_var("sp"), BinOp::GtEq, e_var("slen")), BinOp::Or, e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictNotEq, e_str("%"))),
                                vec![
                                    s_assign("ok", e_bool(false)),
                                    s_break(1),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("n")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("t"))),
                            vec![
                                s_while(e_binop(e_binop(e_var("sp"), BinOp::Lt, e_var("slen")), BinOp::And, e_binop(e_binop(e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictEq, e_str("\n")))), vec![
                                    s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                ]),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("Y")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("y"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("m"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("d"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("e"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("H"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("M"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("S"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("j"))),
                                vec![
                                    s_if(
                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("e")),
                                        vec![
                                            s_while(e_binop(e_binop(e_var("sp"), BinOp::Lt, e_var("slen")), BinOp::And, e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictEq, e_str(" "))), vec![
                                                s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                            ]),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_assign("num", e_int(0)),
                                    s_assign("cnt", e_int(0)),
                                    s_assign("maxd", e_ternary(e_binop(e_var("spec"), BinOp::StrictEq, e_str("Y")), e_int(4), e_ternary(e_binop(e_var("spec"), BinOp::StrictEq, e_str("j")), e_int(3), e_int(2)))),
                                    s_while(e_binop(e_binop(e_binop(e_var("cnt"), BinOp::Lt, e_var("maxd")), BinOp::And, e_binop(e_var("sp"), BinOp::Lt, e_var("slen"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timestamp"), e_var("sp"))])), vec![
                                        s_assign("num", e_binop(e_binop(e_var("num"), BinOp::Mul, e_int(10)), BinOp::Add, e_binop(e_call("ord", vec![e_index(e_var("timestamp"), e_var("sp"))]), BinOp::Sub, e_int(48)))),
                                        s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                        s_assign("cnt", e_binop(e_var("cnt"), BinOp::Add, e_int(1))),
                                    ]),
                                    s_if(
                                        e_binop(e_var("cnt"), BinOp::StrictEq, e_int(0)),
                                        vec![
                                            s_assign("ok", e_bool(false)),
                                            s_break(1),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_if(
                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("Y")),
                                        vec![
                                            s_assign("year", e_var("num")),
                                            s_assign("gotY", e_bool(true)),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_if(
                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("y")),
                                            vec![
                                                s_assign("year", e_ternary(e_binop(e_var("num"), BinOp::Lt, e_int(69)), e_binop(e_int(2000), BinOp::Add, e_var("num")), e_binop(e_int(1900), BinOp::Add, e_var("num")))),
                                                s_assign("gotY", e_bool(true)),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_if(
                                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("m")),
                                                vec![
                                                    s_assign("mon", e_var("num")),
                                                    s_assign("gotMon", e_bool(true)),
                                                ],
                                                vec![],
                                                Some(vec![
                                                s_if(
                                                    e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("d")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("e"))),
                                                    vec![
                                                        s_assign("mday", e_var("num")),
                                                        s_assign("gotMday", e_bool(true)),
                                                    ],
                                                    vec![],
                                                    Some(vec![
                                                    s_if(
                                                        e_binop(e_var("spec"), BinOp::StrictEq, e_str("H")),
                                                        vec![
                                                            s_assign("hour", e_var("num")),
                                                        ],
                                                        vec![],
                                                        Some(vec![
                                                        s_if(
                                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("M")),
                                                            vec![
                                                                s_assign("min", e_var("num")),
                                                            ],
                                                            vec![],
                                                            Some(vec![
                                                            s_if(
                                                                e_binop(e_var("spec"), BinOp::StrictEq, e_str("S")),
                                                                vec![
                                                                    s_assign("sec", e_var("num")),
                                                                ],
                                                                vec![],
                                                                None,
                                                            ),
                                                        ]),
                                                        ),
                                                    ]),
                                                    ),
                                                ]),
                                                ),
                                            ]),
                                            ),
                                        ]),
                                        ),
                                    ]),
                                    ),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("B")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("b"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("h"))),
                                    vec![
                                        s_assign("sub", e_str("")),
                                        s_while(e_binop(e_var("sp"), BinOp::Lt, e_var("slen")), vec![
                                            s_assign("io", e_call("ord", vec![e_index(e_var("timestamp"), e_var("sp"))])),
                                            s_assign("a", e_binop(e_binop(e_binop(e_var("io"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("io"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("io"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("io"), BinOp::LtEq, e_int(122))))),
                                            s_if(
                                                e_not(e_var("a")),
                                                vec![
                                                    s_break(1),
                                                ],
                                                vec![],
                                                None,
                                            ),
                                            s_assign("sub", e_binop(e_var("sub"), BinOp::Concat, e_index(e_var("timestamp"), e_var("sp")))),
                                            s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                        ]),
                                        s_assign("low", e_call("strtolower", vec![e_var("sub")])),
                                        s_assign("mv", e_int(0)),
                                        s_if(
                                            e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("jan")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("january"))),
                                            vec![
                                                s_assign("mv", e_int(1)),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_if(
                                                e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("feb")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("february"))),
                                                vec![
                                                    s_assign("mv", e_int(2)),
                                                ],
                                                vec![],
                                                Some(vec![
                                                s_if(
                                                    e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("mar")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("march"))),
                                                    vec![
                                                        s_assign("mv", e_int(3)),
                                                    ],
                                                    vec![],
                                                    Some(vec![
                                                    s_if(
                                                        e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("apr")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("april"))),
                                                        vec![
                                                            s_assign("mv", e_int(4)),
                                                        ],
                                                        vec![],
                                                        Some(vec![
                                                        s_if(
                                                            e_binop(e_var("low"), BinOp::StrictEq, e_str("may")),
                                                            vec![
                                                                s_assign("mv", e_int(5)),
                                                            ],
                                                            vec![],
                                                            Some(vec![
                                                            s_if(
                                                                e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("jun")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("june"))),
                                                                vec![
                                                                    s_assign("mv", e_int(6)),
                                                                ],
                                                                vec![],
                                                                Some(vec![
                                                                s_if(
                                                                    e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("jul")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("july"))),
                                                                    vec![
                                                                        s_assign("mv", e_int(7)),
                                                                    ],
                                                                    vec![],
                                                                    Some(vec![
                                                                    s_if(
                                                                        e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("aug")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("august"))),
                                                                        vec![
                                                                            s_assign("mv", e_int(8)),
                                                                        ],
                                                                        vec![],
                                                                        Some(vec![
                                                                        s_if(
                                                                            e_binop(e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("sep")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("sept"))), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("september"))),
                                                                            vec![
                                                                                s_assign("mv", e_int(9)),
                                                                            ],
                                                                            vec![],
                                                                            Some(vec![
                                                                            s_if(
                                                                                e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("oct")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("october"))),
                                                                                vec![
                                                                                    s_assign("mv", e_int(10)),
                                                                                ],
                                                                                vec![],
                                                                                Some(vec![
                                                                                s_if(
                                                                                    e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("nov")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("november"))),
                                                                                    vec![
                                                                                        s_assign("mv", e_int(11)),
                                                                                    ],
                                                                                    vec![],
                                                                                    Some(vec![
                                                                                    s_if(
                                                                                        e_binop(e_binop(e_var("low"), BinOp::StrictEq, e_str("dec")), BinOp::Or, e_binop(e_var("low"), BinOp::StrictEq, e_str("december"))),
                                                                                        vec![
                                                                                            s_assign("mv", e_int(12)),
                                                                                        ],
                                                                                        vec![],
                                                                                        None,
                                                                                    ),
                                                                                ]),
                                                                                ),
                                                                            ]),
                                                                            ),
                                                                        ]),
                                                                        ),
                                                                    ]),
                                                                    ),
                                                                ]),
                                                                ),
                                                            ]),
                                                            ),
                                                        ]),
                                                        ),
                                                    ]),
                                                    ),
                                                ]),
                                                ),
                                            ]),
                                            ),
                                        ]),
                                        ),
                                        s_if(
                                            e_binop(e_var("mv"), BinOp::StrictEq, e_int(0)),
                                            vec![
                                                s_assign("ok", e_bool(false)),
                                                s_break(1),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                        s_assign("mon", e_var("mv")),
                                        s_assign("gotMon", e_bool(true)),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_if(
                                        e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("A")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("a"))),
                                        vec![
                                            s_while(e_binop(e_var("sp"), BinOp::Lt, e_var("slen")), vec![
                                                s_assign("io", e_call("ord", vec![e_index(e_var("timestamp"), e_var("sp"))])),
                                                s_assign("a", e_binop(e_binop(e_binop(e_var("io"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("io"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("io"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("io"), BinOp::LtEq, e_int(122))))),
                                                s_if(
                                                    e_not(e_var("a")),
                                                    vec![
                                                        s_break(1),
                                                    ],
                                                    vec![],
                                                    None,
                                                ),
                                                s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                            ]),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_if(
                                            e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("p")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("P"))),
                                            vec![
                                                s_assign("two", e_call("strtoupper", vec![e_call("substr", vec![e_var("timestamp"), e_var("sp"), e_int(2)])])),
                                                s_if(
                                                    e_binop(e_var("two"), BinOp::StrictEq, e_str("PM")),
                                                    vec![
                                                        s_if(
                                                            e_binop(e_var("hour"), BinOp::Lt, e_int(12)),
                                                            vec![
                                                                s_assign("hour", e_binop(e_var("hour"), BinOp::Add, e_int(12))),
                                                            ],
                                                            vec![],
                                                            None,
                                                        ),
                                                        s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(2))),
                                                    ],
                                                    vec![],
                                                    Some(vec![
                                                    s_if(
                                                        e_binop(e_var("two"), BinOp::StrictEq, e_str("AM")),
                                                        vec![
                                                            s_if(
                                                                e_binop(e_var("hour"), BinOp::StrictEq, e_int(12)),
                                                                vec![
                                                                    s_assign("hour", e_int(0)),
                                                                ],
                                                                vec![],
                                                                None,
                                                            ),
                                                            s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(2))),
                                                        ],
                                                        vec![],
                                                        Some(vec![
                                                        s_assign("ok", e_bool(false)),
                                                        s_break(1),
                                                    ]),
                                                    ),
                                                ]),
                                                ),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_if(
                                                e_binop(e_binop(e_binop(e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("u")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("w"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("U"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("W"))), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("V"))),
                                                vec![
                                                    s_assign("num", e_int(0)),
                                                    s_assign("cnt", e_int(0)),
                                                    s_assign("maxd", e_ternary(e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("u")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("w"))), e_int(1), e_int(2))),
                                                    s_while(e_binop(e_binop(e_binop(e_var("cnt"), BinOp::Lt, e_var("maxd")), BinOp::And, e_binop(e_var("sp"), BinOp::Lt, e_var("slen"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timestamp"), e_var("sp"))])), vec![
                                                        s_assign("num", e_binop(e_binop(e_var("num"), BinOp::Mul, e_int(10)), BinOp::Add, e_binop(e_call("ord", vec![e_index(e_var("timestamp"), e_var("sp"))]), BinOp::Sub, e_int(48)))),
                                                        s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                                        s_assign("cnt", e_binop(e_var("cnt"), BinOp::Add, e_int(1))),
                                                    ]),
                                                    s_if(
                                                        e_binop(e_var("cnt"), BinOp::StrictEq, e_int(0)),
                                                        vec![
                                                            s_assign("ok", e_bool(false)),
                                                            s_break(1),
                                                        ],
                                                        vec![],
                                                        None,
                                                    ),
                                                ],
                                                vec![],
                                                Some(vec![
                                                s_if(
                                                    e_binop(e_binop(e_var("spec"), BinOp::StrictEq, e_str("z")), BinOp::Or, e_binop(e_var("spec"), BinOp::StrictEq, e_str("Z"))),
                                                    vec![
                                                        s_if(
                                                            e_binop(e_var("spec"), BinOp::StrictEq, e_str("z")),
                                                            vec![
                                                                s_if(
                                                                    e_binop(e_binop(e_var("sp"), BinOp::Lt, e_var("slen")), BinOp::And, e_binop(e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictEq, e_str("+")), BinOp::Or, e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictEq, e_str("-")))),
                                                                    vec![
                                                                        s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                                                    ],
                                                                    vec![],
                                                                    None,
                                                                ),
                                                                s_assign("cnt", e_int(0)),
                                                                s_while(e_binop(e_binop(e_binop(e_var("cnt"), BinOp::Lt, e_int(4)), BinOp::And, e_binop(e_var("sp"), BinOp::Lt, e_var("slen"))), BinOp::And, e_binop(e_call("ctype_digit", vec![e_index(e_var("timestamp"), e_var("sp"))]), BinOp::Or, e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictEq, e_str(":")))), vec![
                                                                    s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                                                    s_assign("cnt", e_binop(e_var("cnt"), BinOp::Add, e_int(1))),
                                                                ]),
                                                            ],
                                                            vec![],
                                                            Some(vec![
                                                            s_while(e_binop(e_var("sp"), BinOp::Lt, e_var("slen")), vec![
                                                                s_assign("io", e_call("ord", vec![e_index(e_var("timestamp"), e_var("sp"))])),
                                                                s_assign("a", e_binop(e_binop(e_binop(e_var("io"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("io"), BinOp::LtEq, e_int(90))), BinOp::Or, e_binop(e_binop(e_var("io"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("io"), BinOp::LtEq, e_int(122))))),
                                                                s_if(
                                                                    e_not(e_var("a")),
                                                                    vec![
                                                                        s_break(1),
                                                                    ],
                                                                    vec![],
                                                                    None,
                                                                ),
                                                                s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                                                            ]),
                                                        ]),
                                                        ),
                                                    ],
                                                    vec![],
                                                    Some(vec![
                                                    s_assign("ok", e_bool(false)),
                                                    s_break(1),
                                                ]),
                                                ),
                                            ]),
                                            ),
                                        ]),
                                        ),
                                    ]),
                                    ),
                                ]),
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_var("fc"), BinOp::StrictEq, e_str(" ")),
                    vec![
                        s_while(e_binop(e_binop(e_var("sp"), BinOp::Lt, e_var("slen")), BinOp::And, e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictEq, e_str(" "))), vec![
                            s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                        ]),
                        s_assign("fp", e_binop(e_var("fp"), BinOp::Add, e_int(1))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_binop(e_var("sp"), BinOp::GtEq, e_var("slen")), BinOp::Or, e_binop(e_index(e_var("timestamp"), e_var("sp")), BinOp::StrictNotEq, e_var("fc"))),
                        vec![
                            s_assign("ok", e_bool(false)),
                            s_break(1),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("sp", e_binop(e_var("sp"), BinOp::Add, e_int(1))),
                    s_assign("fp", e_binop(e_var("fp"), BinOp::Add, e_int(1))),
                ]),
                ),
            ]),
            ),
        ]),
        s_if(
            e_not(e_var("ok")),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("wday", e_int(0)),
        s_assign("yday", e_int(0)),
        s_assign("tmMon", e_int(0)),
        s_assign("tmYear", e_int(0)),
        s_if(
            e_var("gotMon"),
            vec![
                s_assign("tmMon", e_binop(e_var("mon"), BinOp::Sub, e_int(1))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_var("gotY"),
            vec![
                s_assign("tmYear", e_binop(e_var("year"), BinOp::Sub, e_int(1900))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_var("gotY"), BinOp::And, e_var("gotMon")), BinOp::And, e_var("gotMday")),
            vec![
                s_assign("ts", e_call("__elephc_gmmktime_raw", vec![e_var("hour"), e_var("min"), e_var("sec"), e_var("mon"), e_var("mday"), e_var("year")])),
                s_assign("wday", e_call("intval", vec![e_call("gmdate", vec![e_str("w"), e_var("ts")])])),
                s_assign("yday", e_call("intval", vec![e_call("gmdate", vec![e_str("z"), e_var("ts")])])),
            ],
            vec![],
            None,
        ),
        s_return(e_array_assoc(vec![(e_str("tm_sec"), e_var("sec")), (e_str("tm_min"), e_var("min")), (e_str("tm_hour"), e_var("hour")), (e_str("tm_mday"), e_var("mday")), (e_str("tm_mon"), e_var("tmMon")), (e_str("tm_year"), e_var("tmYear")), (e_str("tm_wday"), e_var("wday")), (e_str("tm_yday"), e_var("yday")), (e_str("unparsed"), e_call("substr", vec![e_var("timestamp"), e_var("sp")]))])),
    ])
}

/// `DateTime::__elephc_timezone_name_from_abbr` — transcribed method builder.
fn decl_class_datetime_method_57_elephc_timezone_name_from_abbr() -> MethodBuilder {
method("__elephc_timezone_name_from_abbr")
    .static_()
    .param("abbr", TypeExpr::Str)
    .param_default("utcOffset", TypeExpr::Int, e_int(-1))
    .param_default("isDST", TypeExpr::Int, e_int(-1))
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("key", e_call("strtolower", vec![e_var("abbr")])),
        s_if(
            e_binop(e_binop(e_var("key"), BinOp::StrictEq, e_str("utc")), BinOp::Or, e_binop(e_var("key"), BinOp::StrictEq, e_str("gmt"))),
            vec![
                s_return(e_str("UTC")),
            ],
            vec![],
            None,
        ),
        s_assign("lines", e_call("explode", vec![e_str("\n"), e_call("elephc_tz_abbreviations", vec![])])),
        s_foreach(e_var("lines"), None, "line", vec![
            s_assign("parts", e_call("explode", vec![e_str("\t"), e_var("line")])),
            s_if(
                e_binop(e_index(e_var("parts"), e_int(0)), BinOp::StrictEq, e_var("key")),
                vec![
                    s_assign("rows", e_call("explode", vec![e_str(";"), e_index(e_var("parts"), e_int(1))])),
                    s_assign("first", e_str("")),
                    s_assign("firstIsNull", e_bool(false)),
                    s_assign("haveFirst", e_bool(false)),
                    s_foreach(e_var("rows"), None, "row", vec![
                        s_assign("columns", e_call("explode", vec![e_str(":"), e_var("row")])),
                        s_assign("zone", e_index(e_var("columns"), e_int(2))),
                        s_if(
                            e_not(e_var("haveFirst")),
                            vec![
                                s_assign("first", e_binop(e_str(""), BinOp::Concat, e_var("zone"))),
                                s_assign("firstIsNull", e_binop(e_var("zone"), BinOp::StrictEq, e_str("NULL"))),
                                s_assign("haveFirst", e_bool(true)),
                                s_if(
                                    e_binop(e_var("utcOffset"), BinOp::Eq, e_neg(e_int(1))),
                                    vec![
                                        s_return(e_ternary(e_var("firstIsNull"), e_bool(false), e_var("first"))),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_call("intval", vec![e_index(e_var("columns"), e_int(1))]), BinOp::Eq, e_var("utcOffset")),
                            vec![
                                s_return(e_ternary(e_binop(e_var("zone"), BinOp::StrictEq, e_str("NULL")), e_bool(false), e_binop(e_str(""), BinOp::Concat, e_var("zone")))),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_ternary(e_var("firstIsNull"), e_bool(false), e_var("first"))),
                ],
                vec![],
                None,
            ),
        ]),
        s_assign("fallback", e_array_assoc(vec![(e_str("-39600:0"), e_str("Pacific/Apia")), (e_str("-36000:0"), e_str("Pacific/Honolulu")), (e_str("-32400:0"), e_str("America/Anchorage")), (e_str("-28800:1"), e_str("America/Anchorage")), (e_str("-28800:0"), e_str("America/Los_Angeles")), (e_str("-25200:1"), e_str("America/Los_Angeles")), (e_str("-25200:0"), e_str("America/Denver")), (e_str("-21600:1"), e_str("America/Denver")), (e_str("-21600:0"), e_str("America/Chicago")), (e_str("-18000:1"), e_str("America/Chicago")), (e_str("-18000:0"), e_str("America/New_York")), (e_str("-16200:0"), e_str("America/Caracas")), (e_str("-14400:1"), e_str("America/New_York")), (e_str("-14400:0"), e_str("America/Halifax")), (e_str("-10800:1"), e_str("America/Halifax")), (e_str("-10800:0"), e_str("America/Sao_Paulo")), (e_str("-7200:1"), e_str("America/Sao_Paulo")), (e_str("-3600:0"), e_str("Atlantic/Azores")), (e_str("0:1"), e_str("Atlantic/Azores")), (e_str("0:0"), e_str("Europe/London")), (e_str("3600:1"), e_str("Europe/London")), (e_str("3600:0"), e_str("Europe/Paris")), (e_str("7200:1"), e_str("Europe/Paris")), (e_str("7200:0"), e_str("Europe/Helsinki")), (e_str("10800:1"), e_str("Europe/Helsinki")), (e_str("10800:0"), e_str("Europe/Moscow")), (e_str("14400:1"), e_str("Europe/Moscow")), (e_str("14400:0"), e_str("Asia/Dubai")), (e_str("18000:0"), e_str("Asia/Karachi")), (e_str("19800:0"), e_str("Asia/Kolkata")), (e_str("20700:0"), e_str("Asia/Katmandu")), (e_str("21600:1"), e_str("Asia/Yekaterinburg")), (e_str("25200:1"), e_str("Asia/Novosibirsk")), (e_str("25200:0"), e_str("Asia/Krasnoyarsk")), (e_str("28800:0"), e_str("Asia/Shanghai")), (e_str("28800:1"), e_str("Asia/Krasnoyarsk")), (e_str("32400:0"), e_str("Asia/Tokyo")), (e_str("36000:0"), e_str("Australia/Melbourne")), (e_str("37800:1"), e_str("Australia/Adelaide")), (e_str("39600:1"), e_str("Australia/Melbourne")), (e_str("43200:0"), e_str("Pacific/Auckland")), (e_str("46800:1"), e_str("Pacific/Auckland"))])),
        s_assign("fallbackKey", e_binop(e_binop(e_var("utcOffset"), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("isDST"))),
        s_return(e_ternary(e_call("isset", vec![e_index(e_var("fallback"), e_var("fallbackKey"))]), e_index(e_var("fallback"), e_var("fallbackKey")), e_bool(false))),
    ])
}

/// `DateTime::__elephc_argument_type_error` — transcribed method builder.
fn decl_class_datetime_method_58_elephc_argument_type_error() -> MethodBuilder {
method("__elephc_argument_type_error")
    .private()
    .static_()
    .param("value", t_mixed())
    .param("prefix", TypeExpr::Str)
    .returns(TypeExpr::Never)
    .body_exact(vec![
        s_if(
            e_call("is_object", vec![e_var("value")]),
            vec![
                s_assign("actual", e_call("get_class", vec![e_var("value")])),
            ],
            vec![
            (e_call("is_array", vec![e_var("value")]), vec![
                s_assign("actual", e_str("array")),
            ]),
            (e_call("is_int", vec![e_var("value")]), vec![
                s_assign("actual", e_str("int")),
            ]),
            (e_call("is_float", vec![e_var("value")]), vec![
                s_assign("actual", e_str("float")),
            ]),
            (e_call("is_bool", vec![e_var("value")]), vec![
                s_assign("actual", e_str("bool")),
            ]),
            (e_call("is_string", vec![e_var("value")]), vec![
                s_assign("actual", e_str("string")),
            ]),
            (e_call("is_null", vec![e_var("value")]), vec![
                s_assign("actual", e_str("null")),
            ]),
            (e_call("is_resource", vec![e_var("value")]), vec![
                s_assign("actual", e_str("resource")),
            ]),
        ],
            Some(vec![
            s_assign("actual", e_str("unknown")),
        ]),
        ),
        s_throw(e_new("TypeError", vec![e_binop(e_binop(e_var("prefix"), BinOp::Concat, e_var("actual")), BinOp::Concat, e_str(" given"))])),
    ])
}

/// `DateTime::__elephc_weak_string_argument` — transcribed method builder.
fn decl_class_datetime_method_59_elephc_weak_string_argument() -> MethodBuilder {
method("__elephc_weak_string_argument")
    .private()
    .static_()
    .param("value", t_mixed())
    .param("prefix", TypeExpr::Str)
    .param_default("fixedError", TypeExpr::Str, e_str(""))
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_if(
            e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_binop(e_call("is_object", vec![e_var("value")]), BinOp::And, e_not(e_instance_of(e_var("value"), "Stringable")))),
            vec![
                s_if(
                    e_binop(e_var("fixedError"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_throw(e_new("TypeError", vec![e_var("fixedError")])),
                    ],
                    vec![],
                    None,
                ),
                s_expr(e_static_call("DateTime", "__elephc_argument_type_error", vec![e_var("value"), e_var("prefix")])),
            ],
            vec![],
            None,
        ),
        s_return(e_cast(CastType::String, e_var("value"))),
    ])
}

/// `DateTime::__elephc_deprecated_string_constant` — transcribed method builder.
fn decl_class_datetime_method_60_elephc_deprecated_string_constant() -> MethodBuilder {
method("__elephc_deprecated_string_constant")
    .static_()
    .param("value", TypeExpr::Str)
    .param("message", TypeExpr::Str)
    .param("line", TypeExpr::Int)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_var("message"), e_var("line"), e_const("E_DEPRECATED")])),
        s_return(e_var("value")),
    ])
}

/// `DateTime::__elephc_deprecated_int_constant` — transcribed method builder.
fn decl_class_datetime_method_61_elephc_deprecated_int_constant() -> MethodBuilder {
method("__elephc_deprecated_int_constant")
    .static_()
    .param("value", TypeExpr::Int)
    .param("message", TypeExpr::Str)
    .param("line", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_var("message"), e_var("line"), e_const("E_DEPRECATED")])),
        s_return(e_var("value")),
    ])
}

/// `DateTime::__elephc_greg_to_sdn` — transcribed method builder.
fn decl_class_datetime_method_62_elephc_greg_to_sdn() -> MethodBuilder {
method("__elephc_greg_to_sdn")
    .static_()
    .param("iy", TypeExpr::Int)
    .param("im", TypeExpr::Int)
    .param("id", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("iy"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("iy"), BinOp::Lt, e_neg(e_int(4714)))), BinOp::Or, e_binop(e_var("im"), BinOp::LtEq, e_int(0))), BinOp::Or, e_binop(e_var("im"), BinOp::Gt, e_int(12))), BinOp::Or, e_binop(e_var("id"), BinOp::LtEq, e_int(0))), BinOp::Or, e_binop(e_var("id"), BinOp::Gt, e_int(31))),
            vec![
                s_return(e_int(0)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("iy"), BinOp::Eq, e_neg(e_int(4714))),
            vec![
                s_if(
                    e_binop(e_var("im"), BinOp::Lt, e_int(11)),
                    vec![
                        s_return(e_int(0)),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_var("im"), BinOp::Eq, e_int(11)), BinOp::And, e_binop(e_var("id"), BinOp::Lt, e_int(25))),
                    vec![
                        s_return(e_int(0)),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_assign("year", e_ternary(e_binop(e_var("iy"), BinOp::Lt, e_int(0)), e_binop(e_var("iy"), BinOp::Add, e_int(4801)), e_binop(e_var("iy"), BinOp::Add, e_int(4800)))),
        s_if(
            e_binop(e_var("im"), BinOp::Gt, e_int(2)),
            vec![
                s_assign("month", e_binop(e_var("im"), BinOp::Sub, e_int(3))),
            ],
            vec![],
            Some(vec![
            s_assign("month", e_binop(e_var("im"), BinOp::Add, e_int(9))),
            s_assign("year", e_binop(e_var("year"), BinOp::Sub, e_int(1))),
        ]),
        ),
        s_return(e_binop(e_binop(e_binop(e_binop(e_call("intdiv", vec![e_binop(e_call("intdiv", vec![e_var("year"), e_int(100)]), BinOp::Mul, e_int(146097)), e_int(4)]), BinOp::Add, e_call("intdiv", vec![e_binop(e_binop(e_var("year"), BinOp::Mod, e_int(100)), BinOp::Mul, e_int(1461)), e_int(4)])), BinOp::Add, e_call("intdiv", vec![e_binop(e_binop(e_var("month"), BinOp::Mul, e_int(153)), BinOp::Add, e_int(2)), e_int(5)])), BinOp::Add, e_var("id")), BinOp::Sub, e_int(32045))),
    ])
}

/// `DateTime::__elephc_sdn_to_greg` — transcribed method builder.
fn decl_class_datetime_method_63_elephc_sdn_to_greg() -> MethodBuilder {
method("__elephc_sdn_to_greg")
    .static_()
    .param("sdn", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_binop(e_var("sdn"), BinOp::LtEq, e_int(0)),
            vec![
                s_return(e_array_assoc(vec![(e_str("y"), e_int(0)), (e_str("m"), e_int(0)), (e_str("d"), e_int(0))])),
            ],
            vec![],
            None,
        ),
        s_assign("temp", e_binop(e_binop(e_binop(e_var("sdn"), BinOp::Add, e_int(32045)), BinOp::Mul, e_int(4)), BinOp::Sub, e_int(1))),
        s_assign("century", e_call("intdiv", vec![e_var("temp"), e_int(146097)])),
        s_assign("temp", e_binop(e_binop(e_call("intdiv", vec![e_binop(e_var("temp"), BinOp::Mod, e_int(146097)), e_int(4)]), BinOp::Mul, e_int(4)), BinOp::Add, e_int(3))),
        s_assign("year", e_binop(e_binop(e_var("century"), BinOp::Mul, e_int(100)), BinOp::Add, e_call("intdiv", vec![e_var("temp"), e_int(1461)]))),
        s_assign("doy", e_binop(e_call("intdiv", vec![e_binop(e_var("temp"), BinOp::Mod, e_int(1461)), e_int(4)]), BinOp::Add, e_int(1))),
        s_assign("temp", e_binop(e_binop(e_var("doy"), BinOp::Mul, e_int(5)), BinOp::Sub, e_int(3))),
        s_assign("month", e_call("intdiv", vec![e_var("temp"), e_int(153)])),
        s_assign("day", e_binop(e_call("intdiv", vec![e_binop(e_var("temp"), BinOp::Mod, e_int(153)), e_int(5)]), BinOp::Add, e_int(1))),
        s_if(
            e_binop(e_var("month"), BinOp::Lt, e_int(10)),
            vec![
                s_assign("month", e_binop(e_var("month"), BinOp::Add, e_int(3))),
            ],
            vec![],
            Some(vec![
            s_assign("year", e_binop(e_var("year"), BinOp::Add, e_int(1))),
            s_assign("month", e_binop(e_var("month"), BinOp::Sub, e_int(9))),
        ]),
        ),
        s_assign("year", e_binop(e_var("year"), BinOp::Sub, e_int(4800))),
        s_if(
            e_binop(e_var("year"), BinOp::LtEq, e_int(0)),
            vec![
                s_assign("year", e_binop(e_var("year"), BinOp::Sub, e_int(1))),
            ],
            vec![],
            None,
        ),
        s_return(e_array_assoc(vec![(e_str("y"), e_var("year")), (e_str("m"), e_var("month")), (e_str("d"), e_var("day"))])),
    ])
}

/// `DateTime::__elephc_jul_to_sdn` — transcribed method builder.
fn decl_class_datetime_method_64_elephc_jul_to_sdn() -> MethodBuilder {
method("__elephc_jul_to_sdn")
    .static_()
    .param("iy", TypeExpr::Int)
    .param("im", TypeExpr::Int)
    .param("id", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("iy"), BinOp::Eq, e_int(0)), BinOp::Or, e_binop(e_var("iy"), BinOp::Lt, e_neg(e_int(4713)))), BinOp::Or, e_binop(e_var("im"), BinOp::LtEq, e_int(0))), BinOp::Or, e_binop(e_var("im"), BinOp::Gt, e_int(12))), BinOp::Or, e_binop(e_var("id"), BinOp::LtEq, e_int(0))), BinOp::Or, e_binop(e_var("id"), BinOp::Gt, e_int(31))),
            vec![
                s_return(e_int(0)),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("iy"), BinOp::Eq, e_neg(e_int(4713))),
            vec![
                s_if(
                    e_binop(e_binop(e_var("im"), BinOp::Eq, e_int(1)), BinOp::And, e_binop(e_var("id"), BinOp::Eq, e_int(1))),
                    vec![
                        s_return(e_int(0)),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_assign("year", e_ternary(e_binop(e_var("iy"), BinOp::Lt, e_int(0)), e_binop(e_var("iy"), BinOp::Add, e_int(4801)), e_binop(e_var("iy"), BinOp::Add, e_int(4800)))),
        s_if(
            e_binop(e_var("im"), BinOp::Gt, e_int(2)),
            vec![
                s_assign("month", e_binop(e_var("im"), BinOp::Sub, e_int(3))),
            ],
            vec![],
            Some(vec![
            s_assign("month", e_binop(e_var("im"), BinOp::Add, e_int(9))),
            s_assign("year", e_binop(e_var("year"), BinOp::Sub, e_int(1))),
        ]),
        ),
        s_return(e_binop(e_binop(e_binop(e_call("intdiv", vec![e_binop(e_var("year"), BinOp::Mul, e_int(1461)), e_int(4)]), BinOp::Add, e_call("intdiv", vec![e_binop(e_binop(e_var("month"), BinOp::Mul, e_int(153)), BinOp::Add, e_int(2)), e_int(5)])), BinOp::Add, e_var("id")), BinOp::Sub, e_int(32083))),
    ])
}

/// `DateTime::__elephc_sdn_to_jul` — transcribed method builder.
fn decl_class_datetime_method_65_elephc_sdn_to_jul() -> MethodBuilder {
method("__elephc_sdn_to_jul")
    .static_()
    .param("sdn", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_binop(e_var("sdn"), BinOp::LtEq, e_int(0)),
            vec![
                s_return(e_array_assoc(vec![(e_str("y"), e_int(0)), (e_str("m"), e_int(0)), (e_str("d"), e_int(0))])),
            ],
            vec![],
            None,
        ),
        s_assign("temp", e_binop(e_binop(e_var("sdn"), BinOp::Mul, e_int(4)), BinOp::Add, e_binop(e_binop(e_int(32083), BinOp::Mul, e_int(4)), BinOp::Sub, e_int(1)))),
        s_assign("year", e_call("intdiv", vec![e_var("temp"), e_int(1461)])),
        s_assign("doy", e_binop(e_call("intdiv", vec![e_binop(e_var("temp"), BinOp::Mod, e_int(1461)), e_int(4)]), BinOp::Add, e_int(1))),
        s_assign("temp", e_binop(e_binop(e_var("doy"), BinOp::Mul, e_int(5)), BinOp::Sub, e_int(3))),
        s_assign("month", e_call("intdiv", vec![e_var("temp"), e_int(153)])),
        s_assign("day", e_binop(e_call("intdiv", vec![e_binop(e_var("temp"), BinOp::Mod, e_int(153)), e_int(5)]), BinOp::Add, e_int(1))),
        s_if(
            e_binop(e_var("month"), BinOp::Lt, e_int(10)),
            vec![
                s_assign("month", e_binop(e_var("month"), BinOp::Add, e_int(3))),
            ],
            vec![],
            Some(vec![
            s_assign("year", e_binop(e_var("year"), BinOp::Add, e_int(1))),
            s_assign("month", e_binop(e_var("month"), BinOp::Sub, e_int(9))),
        ]),
        ),
        s_assign("year", e_binop(e_var("year"), BinOp::Sub, e_int(4800))),
        s_if(
            e_binop(e_var("year"), BinOp::LtEq, e_int(0)),
            vec![
                s_assign("year", e_binop(e_var("year"), BinOp::Sub, e_int(1))),
            ],
            vec![],
            None,
        ),
        s_return(e_array_assoc(vec![(e_str("y"), e_var("year")), (e_str("m"), e_var("month")), (e_str("d"), e_var("day"))])),
    ])
}

/// `DateTime::__elephc_fr_to_sdn` — transcribed method builder.
fn decl_class_datetime_method_66_elephc_fr_to_sdn() -> MethodBuilder {
method("__elephc_fr_to_sdn")
    .static_()
    .param("y", TypeExpr::Int)
    .param("m", TypeExpr::Int)
    .param("d", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("y"), BinOp::Lt, e_int(1)), BinOp::Or, e_binop(e_var("y"), BinOp::Gt, e_int(14))), BinOp::Or, e_binop(e_var("m"), BinOp::Lt, e_int(1))), BinOp::Or, e_binop(e_var("m"), BinOp::Gt, e_int(13))), BinOp::Or, e_binop(e_var("d"), BinOp::Lt, e_int(1))), BinOp::Or, e_binop(e_var("d"), BinOp::Gt, e_int(30))),
            vec![
                s_return(e_int(0)),
            ],
            vec![],
            None,
        ),
        s_return(e_binop(e_binop(e_binop(e_call("intdiv", vec![e_binop(e_var("y"), BinOp::Mul, e_int(1461)), e_int(4)]), BinOp::Add, e_binop(e_binop(e_var("m"), BinOp::Sub, e_int(1)), BinOp::Mul, e_int(30))), BinOp::Add, e_var("d")), BinOp::Add, e_int(2375474))),
    ])
}

/// `DateTime::__elephc_sdn_to_fr` — transcribed method builder.
fn decl_class_datetime_method_67_elephc_sdn_to_fr() -> MethodBuilder {
method("__elephc_sdn_to_fr")
    .static_()
    .param("sdn", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_var("sdn"), BinOp::Lt, e_int(2375840)), BinOp::Or, e_binop(e_var("sdn"), BinOp::Gt, e_int(2380952))),
            vec![
                s_return(e_array_assoc(vec![(e_str("y"), e_int(0)), (e_str("m"), e_int(0)), (e_str("d"), e_int(0))])),
            ],
            vec![],
            None,
        ),
        s_assign("temp", e_binop(e_binop(e_binop(e_var("sdn"), BinOp::Sub, e_int(2375474)), BinOp::Mul, e_int(4)), BinOp::Sub, e_int(1))),
        s_assign("year", e_call("intdiv", vec![e_var("temp"), e_int(1461)])),
        s_assign("doy", e_call("intdiv", vec![e_binop(e_var("temp"), BinOp::Mod, e_int(1461)), e_int(4)])),
        s_assign("month", e_binop(e_call("intdiv", vec![e_var("doy"), e_int(30)]), BinOp::Add, e_int(1))),
        s_assign("day", e_binop(e_binop(e_var("doy"), BinOp::Mod, e_int(30)), BinOp::Add, e_int(1))),
        s_return(e_array_assoc(vec![(e_str("y"), e_var("year")), (e_str("m"), e_var("month")), (e_str("d"), e_var("day"))])),
    ])
}

/// `DateTime::__elephc_jew_tishri1` — transcribed method builder.
fn decl_class_datetime_method_68_elephc_jew_tishri1() -> MethodBuilder {
method("__elephc_jew_tishri1")
    .static_()
    .param("my", TypeExpr::Int)
    .param("moladDay", TypeExpr::Int)
    .param("moladHalakim", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_assign("tishri1", e_var("moladDay")),
        s_assign("dow", e_binop(e_var("tishri1"), BinOp::Mod, e_int(7))),
        s_assign("leap", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("my"), BinOp::Eq, e_int(2)), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(5))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(7))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(10))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(13))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(16))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(18)))),
        s_assign("lastLeap", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("my"), BinOp::Eq, e_int(3)), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(6))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(8))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(11))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(14))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(17))), BinOp::Or, e_binop(e_var("my"), BinOp::Eq, e_int(0)))),
        s_if(
            e_binop(e_binop(e_binop(e_var("moladHalakim"), BinOp::GtEq, e_int(19440)), BinOp::Or, e_binop(e_binop(e_not(e_var("leap")), BinOp::And, e_binop(e_var("dow"), BinOp::Eq, e_int(2))), BinOp::And, e_binop(e_var("moladHalakim"), BinOp::GtEq, e_int(9924)))), BinOp::Or, e_binop(e_binop(e_var("lastLeap"), BinOp::And, e_binop(e_var("dow"), BinOp::Eq, e_int(1))), BinOp::And, e_binop(e_var("moladHalakim"), BinOp::GtEq, e_int(16789)))),
            vec![
                s_assign("tishri1", e_binop(e_var("tishri1"), BinOp::Add, e_int(1))),
                s_assign("dow", e_binop(e_var("dow"), BinOp::Add, e_int(1))),
                s_if(
                    e_binop(e_var("dow"), BinOp::Eq, e_int(7)),
                    vec![
                        s_assign("dow", e_int(0)),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_binop(e_var("dow"), BinOp::Eq, e_int(3)), BinOp::Or, e_binop(e_var("dow"), BinOp::Eq, e_int(5))), BinOp::Or, e_binop(e_var("dow"), BinOp::Eq, e_int(0))),
            vec![
                s_assign("tishri1", e_binop(e_var("tishri1"), BinOp::Add, e_int(1))),
            ],
            vec![],
            None,
        ),
        s_return(e_var("tishri1")),
    ])
}

/// `DateTime::__elephc_jew_molad_cycle` — transcribed method builder.
fn decl_class_datetime_method_69_elephc_jew_molad_cycle() -> MethodBuilder {
method("__elephc_jew_molad_cycle")
    .static_()
    .param("mc", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("total", e_binop(e_int(31524), BinOp::Add, e_binop(e_var("mc"), BinOp::Mul, e_int(179876755)))),
        s_return(e_array_assoc(vec![(e_str("md"), e_call("intdiv", vec![e_var("total"), e_int(25920)])), (e_str("mh"), e_binop(e_var("total"), BinOp::Mod, e_int(25920)))])),
    ])
}

/// `DateTime::__elephc_jew_find_tishri_molad` — transcribed method builder.
fn decl_class_datetime_method_70_elephc_jew_find_tishri_molad() -> MethodBuilder {
method("__elephc_jew_find_tishri_molad")
    .static_()
    .param("inputDay", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("months", e_array(vec![e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(13)])),
        s_assign("mc", e_call("intdiv", vec![e_binop(e_var("inputDay"), BinOp::Add, e_int(310)), e_int(6940)])),
        s_assign("mm", e_static_call("DateTime", "__elephc_jew_molad_cycle", vec![e_var("mc")])),
        s_assign("md", e_index(e_var("mm"), e_str("md"))),
        s_assign("mh", e_index(e_var("mm"), e_str("mh"))),
        s_while(e_binop(e_var("md"), BinOp::Lt, e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_int(6940)), BinOp::Add, e_int(310))), vec![
            s_assign("mc", e_binop(e_var("mc"), BinOp::Add, e_int(1))),
            s_assign("mh", e_binop(e_var("mh"), BinOp::Add, e_int(179876755))),
            s_assign("md", e_binop(e_var("md"), BinOp::Add, e_call("intdiv", vec![e_var("mh"), e_int(25920)]))),
            s_assign("mh", e_binop(e_var("mh"), BinOp::Mod, e_int(25920))),
        ]),
        s_assign("my", e_int(0)),
        s_while(e_binop(e_var("my"), BinOp::Lt, e_int(18)), vec![
            s_if(
                e_binop(e_var("md"), BinOp::Gt, e_binop(e_var("inputDay"), BinOp::Sub, e_int(74))),
                vec![
                    s_break(1),
                ],
                vec![],
                None,
            ),
            s_assign("mh", e_binop(e_var("mh"), BinOp::Add, e_binop(e_int(765433), BinOp::Mul, e_index(e_var("months"), e_var("my"))))),
            s_assign("md", e_binop(e_var("md"), BinOp::Add, e_call("intdiv", vec![e_var("mh"), e_int(25920)]))),
            s_assign("mh", e_binop(e_var("mh"), BinOp::Mod, e_int(25920))),
            s_assign("my", e_binop(e_var("my"), BinOp::Add, e_int(1))),
        ]),
        s_return(e_array_assoc(vec![(e_str("mc"), e_var("mc")), (e_str("my"), e_var("my")), (e_str("md"), e_var("md")), (e_str("mh"), e_var("mh"))])),
    ])
}

/// `DateTime::__elephc_jew_find_start_year` — transcribed method builder.
fn decl_class_datetime_method_71_elephc_jew_find_start_year() -> MethodBuilder {
method("__elephc_jew_find_start_year")
    .static_()
    .param("year", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("offsets", e_array(vec![e_int(0), e_int(12), e_int(24), e_int(37), e_int(49), e_int(61), e_int(74), e_int(86), e_int(99), e_int(111), e_int(123), e_int(136), e_int(148), e_int(160), e_int(173), e_int(185), e_int(197), e_int(210), e_int(222)])),
        s_assign("mc", e_call("intdiv", vec![e_binop(e_var("year"), BinOp::Sub, e_int(1)), e_int(19)])),
        s_assign("my", e_binop(e_binop(e_var("year"), BinOp::Sub, e_int(1)), BinOp::Mod, e_int(19))),
        s_assign("mm", e_static_call("DateTime", "__elephc_jew_molad_cycle", vec![e_var("mc")])),
        s_assign("md", e_index(e_var("mm"), e_str("md"))),
        s_assign("mh", e_index(e_var("mm"), e_str("mh"))),
        s_assign("mh", e_binop(e_var("mh"), BinOp::Add, e_binop(e_int(765433), BinOp::Mul, e_index(e_var("offsets"), e_var("my"))))),
        s_assign("md", e_binop(e_var("md"), BinOp::Add, e_call("intdiv", vec![e_var("mh"), e_int(25920)]))),
        s_assign("mh", e_binop(e_var("mh"), BinOp::Mod, e_int(25920))),
        s_assign("t1", e_static_call("DateTime", "__elephc_jew_tishri1", vec![e_var("my"), e_var("md"), e_var("mh")])),
        s_return(e_array_assoc(vec![(e_str("mc"), e_var("mc")), (e_str("my"), e_var("my")), (e_str("md"), e_var("md")), (e_str("mh"), e_var("mh")), (e_str("t1"), e_var("t1"))])),
    ])
}

/// `DateTime::__elephc_jew_to_sdn` — transcribed method builder.
fn decl_class_datetime_method_72_elephc_jew_to_sdn() -> MethodBuilder {
method("__elephc_jew_to_sdn")
    .static_()
    .param("year", TypeExpr::Int)
    .param("month", TypeExpr::Int)
    .param("day", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_var("year"), BinOp::LtEq, e_int(0)), BinOp::Or, e_binop(e_var("day"), BinOp::LtEq, e_int(0))), BinOp::Or, e_binop(e_var("day"), BinOp::Gt, e_int(30))),
            vec![
                s_return(e_int(0)),
            ],
            vec![],
            None,
        ),
        s_assign("months", e_array(vec![e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(13)])),
        s_if(
            e_binop(e_binop(e_var("month"), BinOp::Eq, e_int(1)), BinOp::Or, e_binop(e_var("month"), BinOp::Eq, e_int(2))),
            vec![
                s_assign("s", e_static_call("DateTime", "__elephc_jew_find_start_year", vec![e_var("year")])),
                s_assign("t1", e_index(e_var("s"), e_str("t1"))),
                s_assign("sdn", e_ternary(e_binop(e_var("month"), BinOp::Eq, e_int(1)), e_binop(e_binop(e_var("t1"), BinOp::Add, e_var("day")), BinOp::Sub, e_int(1)), e_binop(e_binop(e_var("t1"), BinOp::Add, e_var("day")), BinOp::Add, e_int(29)))),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_var("month"), BinOp::Eq, e_int(3)),
                vec![
                    s_assign("s", e_static_call("DateTime", "__elephc_jew_find_start_year", vec![e_var("year")])),
                    s_assign("t1", e_index(e_var("s"), e_str("t1"))),
                    s_assign("md", e_index(e_var("s"), e_str("md"))),
                    s_assign("mh", e_index(e_var("s"), e_str("mh"))),
                    s_assign("my", e_index(e_var("s"), e_str("my"))),
                    s_assign("mh", e_binop(e_var("mh"), BinOp::Add, e_binop(e_int(765433), BinOp::Mul, e_index(e_var("months"), e_var("my"))))),
                    s_assign("md", e_binop(e_var("md"), BinOp::Add, e_call("intdiv", vec![e_var("mh"), e_int(25920)]))),
                    s_assign("mh", e_binop(e_var("mh"), BinOp::Mod, e_int(25920))),
                    s_assign("t1a", e_static_call("DateTime", "__elephc_jew_tishri1", vec![e_binop(e_binop(e_var("my"), BinOp::Add, e_int(1)), BinOp::Mod, e_int(19)), e_var("md"), e_var("mh")])),
                    s_assign("yl", e_binop(e_var("t1a"), BinOp::Sub, e_var("t1"))),
                    s_assign("sdn", e_ternary(e_binop(e_binop(e_var("yl"), BinOp::Eq, e_int(355)), BinOp::Or, e_binop(e_var("yl"), BinOp::Eq, e_int(385))), e_binop(e_binop(e_var("t1"), BinOp::Add, e_var("day")), BinOp::Add, e_int(59)), e_binop(e_binop(e_var("t1"), BinOp::Add, e_var("day")), BinOp::Add, e_int(58)))),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_binop(e_binop(e_var("month"), BinOp::Eq, e_int(4)), BinOp::Or, e_binop(e_var("month"), BinOp::Eq, e_int(5))), BinOp::Or, e_binop(e_var("month"), BinOp::Eq, e_int(6))),
                    vec![
                        s_assign("s", e_static_call("DateTime", "__elephc_jew_find_start_year", vec![e_binop(e_var("year"), BinOp::Add, e_int(1))])),
                        s_assign("t1a", e_index(e_var("s"), e_str("t1"))),
                        s_assign("lai", e_ternary(e_binop(e_index(e_var("months"), e_binop(e_binop(e_var("year"), BinOp::Sub, e_int(1)), BinOp::Mod, e_int(19))), BinOp::Eq, e_int(12)), e_int(29), e_int(59))),
                        s_if(
                            e_binop(e_var("month"), BinOp::Eq, e_int(4)),
                            vec![
                                s_assign("sdn", e_binop(e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_var("lai")), BinOp::Sub, e_int(237))),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("month"), BinOp::Eq, e_int(5)),
                                vec![
                                    s_assign("sdn", e_binop(e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_var("lai")), BinOp::Sub, e_int(208))),
                                ],
                                vec![],
                                Some(vec![
                                s_assign("sdn", e_binop(e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_var("lai")), BinOp::Sub, e_int(178))),
                            ]),
                            ),
                        ]),
                        ),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("s", e_static_call("DateTime", "__elephc_jew_find_start_year", vec![e_binop(e_var("year"), BinOp::Add, e_int(1))])),
                    s_assign("t1a", e_index(e_var("s"), e_str("t1"))),
                    s_if(
                        e_binop(e_var("month"), BinOp::Eq, e_int(7)),
                        vec![
                            s_assign("sdn", e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_int(207))),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("month"), BinOp::Eq, e_int(8)),
                            vec![
                                s_assign("sdn", e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_int(178))),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("month"), BinOp::Eq, e_int(9)),
                                vec![
                                    s_assign("sdn", e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_int(148))),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_var("month"), BinOp::Eq, e_int(10)),
                                    vec![
                                        s_assign("sdn", e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_int(119))),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_if(
                                        e_binop(e_var("month"), BinOp::Eq, e_int(11)),
                                        vec![
                                            s_assign("sdn", e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_int(89))),
                                        ],
                                        vec![],
                                        Some(vec![
                                        s_if(
                                            e_binop(e_var("month"), BinOp::Eq, e_int(12)),
                                            vec![
                                                s_assign("sdn", e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_int(60))),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_if(
                                                e_binop(e_var("month"), BinOp::Eq, e_int(13)),
                                                vec![
                                                    s_assign("sdn", e_binop(e_binop(e_var("t1a"), BinOp::Add, e_var("day")), BinOp::Sub, e_int(30))),
                                                ],
                                                vec![],
                                                Some(vec![
                                                s_return(e_int(0)),
                                            ]),
                                            ),
                                        ]),
                                        ),
                                    ]),
                                    ),
                                ]),
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
            ]),
            ),
        ]),
        ),
        s_return(e_binop(e_var("sdn"), BinOp::Add, e_int(347997))),
    ])
}

/// `DateTime::__elephc_sdn_to_jew` — transcribed method builder.
fn decl_class_datetime_method_73_elephc_sdn_to_jew() -> MethodBuilder {
method("__elephc_sdn_to_jew")
    .static_()
    .param("sdn", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_var("sdn"), BinOp::LtEq, e_int(347997)), BinOp::Or, e_binop(e_var("sdn"), BinOp::Gt, e_int(324542846))),
            vec![
                s_return(e_array_assoc(vec![(e_str("y"), e_int(0)), (e_str("m"), e_int(0)), (e_str("d"), e_int(0))])),
            ],
            vec![],
            None,
        ),
        s_assign("months", e_array(vec![e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(13)])),
        s_assign("inputDay", e_binop(e_var("sdn"), BinOp::Sub, e_int(347997))),
        s_assign("f", e_static_call("DateTime", "__elephc_jew_find_tishri_molad", vec![e_var("inputDay")])),
        s_assign("mc", e_index(e_var("f"), e_str("mc"))),
        s_assign("my", e_index(e_var("f"), e_str("my"))),
        s_assign("day", e_index(e_var("f"), e_str("md"))),
        s_assign("hal", e_index(e_var("f"), e_str("mh"))),
        s_assign("t1", e_static_call("DateTime", "__elephc_jew_tishri1", vec![e_var("my"), e_var("day"), e_var("hal")])),
        s_assign("t1a", e_int(0)),
        s_assign("py", e_int(0)),
        s_assign("pm", e_int(0)),
        s_assign("pd", e_int(0)),
        s_if(
            e_binop(e_var("inputDay"), BinOp::GtEq, e_var("t1")),
            vec![
                s_assign("py", e_binop(e_binop(e_binop(e_var("mc"), BinOp::Mul, e_int(19)), BinOp::Add, e_var("my")), BinOp::Add, e_int(1))),
                s_if(
                    e_binop(e_var("inputDay"), BinOp::Lt, e_binop(e_var("t1"), BinOp::Add, e_int(59))),
                    vec![
                        s_if(
                            e_binop(e_var("inputDay"), BinOp::Lt, e_binop(e_var("t1"), BinOp::Add, e_int(30))),
                            vec![
                                s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(1)), (e_str("d"), e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(1)))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(2)), (e_str("d"), e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Sub, e_int(29)))])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("hal", e_binop(e_var("hal"), BinOp::Add, e_binop(e_int(765433), BinOp::Mul, e_index(e_var("months"), e_var("my"))))),
                s_assign("day", e_binop(e_var("day"), BinOp::Add, e_call("intdiv", vec![e_var("hal"), e_int(25920)]))),
                s_assign("hal", e_binop(e_var("hal"), BinOp::Mod, e_int(25920))),
                s_assign("t1a", e_static_call("DateTime", "__elephc_jew_tishri1", vec![e_binop(e_binop(e_var("my"), BinOp::Add, e_int(1)), BinOp::Mod, e_int(19)), e_var("day"), e_var("hal")])),
            ],
            vec![],
            Some(vec![
            s_assign("py", e_binop(e_binop(e_var("mc"), BinOp::Mul, e_int(19)), BinOp::Add, e_var("my"))),
            s_if(
                e_binop(e_var("inputDay"), BinOp::GtEq, e_binop(e_var("t1"), BinOp::Sub, e_int(177))),
                vec![
                    s_if(
                        e_binop(e_var("inputDay"), BinOp::Gt, e_binop(e_var("t1"), BinOp::Sub, e_int(30))),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(13)), (e_str("d"), e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(30)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("inputDay"), BinOp::Gt, e_binop(e_var("t1"), BinOp::Sub, e_int(60))),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(12)), (e_str("d"), e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(60)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("inputDay"), BinOp::Gt, e_binop(e_var("t1"), BinOp::Sub, e_int(89))),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(11)), (e_str("d"), e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(89)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("inputDay"), BinOp::Gt, e_binop(e_var("t1"), BinOp::Sub, e_int(119))),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(10)), (e_str("d"), e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(119)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("inputDay"), BinOp::Gt, e_binop(e_var("t1"), BinOp::Sub, e_int(148))),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(9)), (e_str("d"), e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(148)))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(8)), (e_str("d"), e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(178)))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_index(e_var("months"), e_binop(e_binop(e_var("py"), BinOp::Sub, e_int(1)), BinOp::Mod, e_int(19))), BinOp::Eq, e_int(13)),
                vec![
                    s_assign("pm", e_int(7)),
                    s_assign("pd", e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(207))),
                    s_if(
                        e_binop(e_var("pd"), BinOp::Gt, e_int(0)),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_var("pm")), (e_str("d"), e_var("pd"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("pm", e_binop(e_var("pm"), BinOp::Sub, e_int(1))),
                    s_assign("pd", e_binop(e_var("pd"), BinOp::Add, e_int(30))),
                    s_if(
                        e_binop(e_var("pd"), BinOp::Gt, e_int(0)),
                        vec![
                            s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_var("pm")), (e_str("d"), e_var("pd"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("pm", e_binop(e_var("pm"), BinOp::Sub, e_int(1))),
                    s_assign("pd", e_binop(e_var("pd"), BinOp::Add, e_int(30))),
                ],
                vec![],
                Some(vec![
                s_assign("pm", e_int(7)),
                s_assign("pd", e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Add, e_int(207))),
                s_if(
                    e_binop(e_var("pd"), BinOp::Gt, e_int(0)),
                    vec![
                        s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_var("pm")), (e_str("d"), e_var("pd"))])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("pm", e_binop(e_var("pm"), BinOp::Sub, e_int(2))),
                s_assign("pd", e_binop(e_var("pd"), BinOp::Add, e_int(30))),
            ]),
            ),
            s_if(
                e_binop(e_var("pd"), BinOp::Gt, e_int(0)),
                vec![
                    s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_var("pm")), (e_str("d"), e_var("pd"))])),
                ],
                vec![],
                None,
            ),
            s_assign("pm", e_binop(e_var("pm"), BinOp::Sub, e_int(1))),
            s_assign("pd", e_binop(e_var("pd"), BinOp::Add, e_int(29))),
            s_if(
                e_binop(e_var("pd"), BinOp::Gt, e_int(0)),
                vec![
                    s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_var("pm")), (e_str("d"), e_var("pd"))])),
                ],
                vec![],
                None,
            ),
            s_assign("t1a", e_var("t1")),
            s_assign("f2", e_static_call("DateTime", "__elephc_jew_find_tishri_molad", vec![e_binop(e_var("day"), BinOp::Sub, e_int(365))])),
            s_assign("mc", e_index(e_var("f2"), e_str("mc"))),
            s_assign("my", e_index(e_var("f2"), e_str("my"))),
            s_assign("day", e_index(e_var("f2"), e_str("md"))),
            s_assign("hal", e_index(e_var("f2"), e_str("mh"))),
            s_assign("t1", e_static_call("DateTime", "__elephc_jew_tishri1", vec![e_var("my"), e_var("day"), e_var("hal")])),
        ]),
        ),
        s_assign("yl", e_binop(e_var("t1a"), BinOp::Sub, e_var("t1"))),
        s_assign("day", e_binop(e_binop(e_var("inputDay"), BinOp::Sub, e_var("t1")), BinOp::Sub, e_int(29))),
        s_if(
            e_binop(e_binop(e_var("yl"), BinOp::Eq, e_int(355)), BinOp::Or, e_binop(e_var("yl"), BinOp::Eq, e_int(385))),
            vec![
                s_if(
                    e_binop(e_var("day"), BinOp::LtEq, e_int(30)),
                    vec![
                        s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(2)), (e_str("d"), e_var("day"))])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("day", e_binop(e_var("day"), BinOp::Sub, e_int(30))),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_var("day"), BinOp::LtEq, e_int(29)),
                vec![
                    s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(2)), (e_str("d"), e_var("day"))])),
                ],
                vec![],
                None,
            ),
            s_assign("day", e_binop(e_var("day"), BinOp::Sub, e_int(29))),
        ]),
        ),
        s_return(e_array_assoc(vec![(e_str("y"), e_var("py")), (e_str("m"), e_int(3)), (e_str("d"), e_var("day"))])),
    ])
}

/// `DateTime::__elephc_jew_month_name` — transcribed method builder.
fn decl_class_datetime_method_74_elephc_jew_month_name() -> MethodBuilder {
method("__elephc_jew_month_name")
    .static_()
    .param("year", TypeExpr::Int)
    .param("month", TypeExpr::Int)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("months", e_array(vec![e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(12), e_int(13), e_int(12), e_int(13)])),
        s_assign("leapYear", e_binop(e_index(e_var("months"), e_binop(e_binop(e_var("year"), BinOp::Sub, e_int(1)), BinOp::Mod, e_int(19))), BinOp::Eq, e_int(13))),
        s_assign("leap", e_array(vec![e_str(""), e_str("Tishri"), e_str("Heshvan"), e_str("Kislev"), e_str("Tevet"), e_str("Shevat"), e_str("Adar I"), e_str("Adar II"), e_str("Nisan"), e_str("Iyyar"), e_str("Sivan"), e_str("Tammuz"), e_str("Av"), e_str("Elul")])),
        s_assign("reg", e_array(vec![e_str(""), e_str("Tishri"), e_str("Heshvan"), e_str("Kislev"), e_str("Tevet"), e_str("Shevat"), e_str(""), e_str("Adar"), e_str("Nisan"), e_str("Iyyar"), e_str("Sivan"), e_str("Tammuz"), e_str("Av"), e_str("Elul")])),
        s_return(e_ternary(e_var("leapYear"), e_index(e_var("leap"), e_var("month")), e_index(e_var("reg"), e_var("month")))),
    ])
}

/// `DateTime::__elephc_easter_calc` — transcribed method builder.
fn decl_class_datetime_method_75_elephc_easter_calc() -> MethodBuilder {
method("__elephc_easter_calc")
    .static_()
    .param("year", TypeExpr::Int)
    .param("method", TypeExpr::Int)
    .param("gm", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_assign("golden", e_binop(e_binop(e_var("year"), BinOp::Mod, e_int(19)), BinOp::Add, e_int(1))),
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_var("year"), BinOp::LtEq, e_int(1582)), BinOp::And, e_binop(e_var("method"), BinOp::NotEq, e_int(2))), BinOp::Or, e_binop(e_binop(e_binop(e_binop(e_var("year"), BinOp::GtEq, e_int(1583)), BinOp::And, e_binop(e_var("year"), BinOp::LtEq, e_int(1752))), BinOp::And, e_binop(e_var("method"), BinOp::NotEq, e_int(1))), BinOp::And, e_binop(e_var("method"), BinOp::NotEq, e_int(2)))), BinOp::Or, e_binop(e_var("method"), BinOp::Eq, e_int(3))),
            vec![
                s_assign("dom", e_binop(e_binop(e_binop(e_var("year"), BinOp::Add, e_call("intdiv", vec![e_var("year"), e_int(4)])), BinOp::Add, e_int(5)), BinOp::Mod, e_int(7))),
                s_if(
                    e_binop(e_var("dom"), BinOp::Lt, e_int(0)),
                    vec![
                        s_assign("dom", e_binop(e_var("dom"), BinOp::Add, e_int(7))),
                    ],
                    vec![],
                    None,
                ),
                s_assign("pfm", e_binop(e_binop(e_binop(e_int(3), BinOp::Sub, e_binop(e_int(11), BinOp::Mul, e_var("golden"))), BinOp::Sub, e_int(7)), BinOp::Mod, e_int(30))),
                s_if(
                    e_binop(e_var("pfm"), BinOp::Lt, e_int(0)),
                    vec![
                        s_assign("pfm", e_binop(e_var("pfm"), BinOp::Add, e_int(30))),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            Some(vec![
            s_assign("dom", e_binop(e_binop(e_binop(e_binop(e_var("year"), BinOp::Add, e_call("intdiv", vec![e_var("year"), e_int(4)])), BinOp::Sub, e_call("intdiv", vec![e_var("year"), e_int(100)])), BinOp::Add, e_call("intdiv", vec![e_var("year"), e_int(400)])), BinOp::Mod, e_int(7))),
            s_if(
                e_binop(e_var("dom"), BinOp::Lt, e_int(0)),
                vec![
                    s_assign("dom", e_binop(e_var("dom"), BinOp::Add, e_int(7))),
                ],
                vec![],
                None,
            ),
            s_assign("solar", e_binop(e_call("intdiv", vec![e_binop(e_var("year"), BinOp::Sub, e_int(1600)), e_int(100)]), BinOp::Sub, e_call("intdiv", vec![e_binop(e_var("year"), BinOp::Sub, e_int(1600)), e_int(400)]))),
            s_assign("lunar", e_call("intdiv", vec![e_binop(e_call("intdiv", vec![e_binop(e_var("year"), BinOp::Sub, e_int(1400)), e_int(100)]), BinOp::Mul, e_int(8)), e_int(25)])),
            s_assign("pfm", e_binop(e_binop(e_binop(e_binop(e_int(3), BinOp::Sub, e_binop(e_int(11), BinOp::Mul, e_var("golden"))), BinOp::Add, e_var("solar")), BinOp::Sub, e_var("lunar")), BinOp::Mod, e_int(30))),
            s_if(
                e_binop(e_var("pfm"), BinOp::Lt, e_int(0)),
                vec![
                    s_assign("pfm", e_binop(e_var("pfm"), BinOp::Add, e_int(30))),
                ],
                vec![],
                None,
            ),
        ]),
        ),
        s_if(
            e_binop(e_binop(e_var("pfm"), BinOp::Eq, e_int(29)), BinOp::Or, e_binop(e_binop(e_var("pfm"), BinOp::Eq, e_int(28)), BinOp::And, e_binop(e_var("golden"), BinOp::Gt, e_int(11)))),
            vec![
                s_assign("pfm", e_binop(e_var("pfm"), BinOp::Sub, e_int(1))),
            ],
            vec![],
            None,
        ),
        s_assign("tmp", e_binop(e_binop(e_binop(e_int(4), BinOp::Sub, e_var("pfm")), BinOp::Sub, e_var("dom")), BinOp::Mod, e_int(7))),
        s_if(
            e_binop(e_var("tmp"), BinOp::Lt, e_int(0)),
            vec![
                s_assign("tmp", e_binop(e_var("tmp"), BinOp::Add, e_int(7))),
            ],
            vec![],
            None,
        ),
        s_assign("easter", e_binop(e_binop(e_var("pfm"), BinOp::Add, e_var("tmp")), BinOp::Add, e_int(1))),
        s_if(
            e_binop(e_var("gm"), BinOp::NotEq, e_int(0)),
            vec![
                s_if(
                    e_binop(e_var("easter"), BinOp::Lt, e_int(11)),
                    vec![
                        s_assign("mon", e_int(3)),
                        s_assign("mday", e_binop(e_var("easter"), BinOp::Add, e_int(21))),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("mon", e_int(4)),
                    s_assign("mday", e_binop(e_var("easter"), BinOp::Sub, e_int(10))),
                ]),
                ),
                s_return(e_call("__elephc_mktime_raw", vec![e_int(0), e_int(0), e_int(0), e_var("mon"), e_var("mday"), e_var("year")])),
            ],
            vec![],
            None,
        ),
        s_return(e_var("easter")),
    ])
}

/// `DateTime::__elephc_cal_to_jd` — transcribed method builder.
fn decl_class_datetime_method_76_elephc_cal_to_jd() -> MethodBuilder {
method("__elephc_cal_to_jd")
    .static_()
    .param("calendar", TypeExpr::Int)
    .param("month", TypeExpr::Int)
    .param("day", TypeExpr::Int)
    .param("year", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(0)),
            vec![
                s_return(e_static_call("DateTime", "__elephc_greg_to_sdn", vec![e_var("year"), e_var("month"), e_var("day")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(1)),
            vec![
                s_return(e_static_call("DateTime", "__elephc_jul_to_sdn", vec![e_var("year"), e_var("month"), e_var("day")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(2)),
            vec![
                s_return(e_static_call("DateTime", "__elephc_jew_to_sdn", vec![e_var("year"), e_var("month"), e_var("day")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(3)),
            vec![
                s_return(e_static_call("DateTime", "__elephc_fr_to_sdn", vec![e_var("year"), e_var("month"), e_var("day")])),
            ],
            vec![],
            None,
        ),
        s_return(e_int(0)),
    ])
}

/// `DateTime::__elephc_gregoriantojd` — transcribed method builder.
fn decl_class_datetime_method_77_elephc_gregoriantojd() -> MethodBuilder {
method("__elephc_gregoriantojd")
    .static_()
    .param("month", TypeExpr::Int)
    .param("day", TypeExpr::Int)
    .param("year", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_static_call("DateTime", "__elephc_greg_to_sdn", vec![e_var("year"), e_var("month"), e_var("day")])),
    ])
}

/// `DateTime::__elephc_jdtogregorian` — transcribed method builder.
fn decl_class_datetime_method_78_elephc_jdtogregorian() -> MethodBuilder {
method("__elephc_jdtogregorian")
    .static_()
    .param("jd", TypeExpr::Int)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_greg", vec![e_var("jd")])),
        s_return(e_binop(e_binop(e_binop(e_binop(e_index(e_var("r"), e_str("m")), BinOp::Concat, e_str("/")), BinOp::Concat, e_index(e_var("r"), e_str("d"))), BinOp::Concat, e_str("/")), BinOp::Concat, e_index(e_var("r"), e_str("y")))),
    ])
}

/// `DateTime::__elephc_juliantojd` — transcribed method builder.
fn decl_class_datetime_method_79_elephc_juliantojd() -> MethodBuilder {
method("__elephc_juliantojd")
    .static_()
    .param("month", TypeExpr::Int)
    .param("day", TypeExpr::Int)
    .param("year", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_static_call("DateTime", "__elephc_jul_to_sdn", vec![e_var("year"), e_var("month"), e_var("day")])),
    ])
}

/// `DateTime::__elephc_jdtojulian` — transcribed method builder.
fn decl_class_datetime_method_80_elephc_jdtojulian() -> MethodBuilder {
method("__elephc_jdtojulian")
    .static_()
    .param("jd", TypeExpr::Int)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_jul", vec![e_var("jd")])),
        s_return(e_binop(e_binop(e_binop(e_binop(e_index(e_var("r"), e_str("m")), BinOp::Concat, e_str("/")), BinOp::Concat, e_index(e_var("r"), e_str("d"))), BinOp::Concat, e_str("/")), BinOp::Concat, e_index(e_var("r"), e_str("y")))),
    ])
}

/// `DateTime::__elephc_frenchtojd` — transcribed method builder.
fn decl_class_datetime_method_81_elephc_frenchtojd() -> MethodBuilder {
method("__elephc_frenchtojd")
    .static_()
    .param("month", TypeExpr::Int)
    .param("day", TypeExpr::Int)
    .param("year", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_static_call("DateTime", "__elephc_fr_to_sdn", vec![e_var("year"), e_var("month"), e_var("day")])),
    ])
}

/// `DateTime::__elephc_jdtofrench` — transcribed method builder.
fn decl_class_datetime_method_82_elephc_jdtofrench() -> MethodBuilder {
method("__elephc_jdtofrench")
    .static_()
    .param("jd", TypeExpr::Int)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_fr", vec![e_var("jd")])),
        s_return(e_binop(e_binop(e_binop(e_binop(e_index(e_var("r"), e_str("m")), BinOp::Concat, e_str("/")), BinOp::Concat, e_index(e_var("r"), e_str("d"))), BinOp::Concat, e_str("/")), BinOp::Concat, e_index(e_var("r"), e_str("y")))),
    ])
}

/// `DateTime::__elephc_jewishtojd` — transcribed method builder.
fn decl_class_datetime_method_83_elephc_jewishtojd() -> MethodBuilder {
method("__elephc_jewishtojd")
    .static_()
    .param("month", TypeExpr::Int)
    .param("day", TypeExpr::Int)
    .param("year", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_static_call("DateTime", "__elephc_jew_to_sdn", vec![e_var("year"), e_var("month"), e_var("day")])),
    ])
}

/// `DateTime::__elephc_jdtojewish` — transcribed method builder.
fn decl_class_datetime_method_84_elephc_jdtojewish() -> MethodBuilder {
method("__elephc_jdtojewish")
    .static_()
    .param("jd", TypeExpr::Int)
    .param_default("hebrew", TypeExpr::Bool, e_bool(false))
    .param_default("flags", TypeExpr::Int, e_int(0))
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_jew", vec![e_var("jd")])),
        s_return(e_binop(e_binop(e_binop(e_binop(e_index(e_var("r"), e_str("m")), BinOp::Concat, e_str("/")), BinOp::Concat, e_index(e_var("r"), e_str("d"))), BinOp::Concat, e_str("/")), BinOp::Concat, e_index(e_var("r"), e_str("y")))),
    ])
}

/// `DateTime::__elephc_easter_days` — transcribed method builder.
fn decl_class_datetime_method_85_elephc_easter_days() -> MethodBuilder {
method("__elephc_easter_days")
    .static_()
    .param("year", TypeExpr::Int)
    .param_default("mode", TypeExpr::Int, e_int(0))
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_static_call("DateTime", "__elephc_easter_calc", vec![e_var("year"), e_var("mode"), e_int(0)])),
    ])
}

/// `DateTime::__elephc_easter_date` — transcribed method builder.
fn decl_class_datetime_method_86_elephc_easter_date() -> MethodBuilder {
method("__elephc_easter_date")
    .static_()
    .param("year", TypeExpr::Int)
    .param_default("mode", TypeExpr::Int, e_int(0))
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_static_call("DateTime", "__elephc_easter_calc", vec![e_var("year"), e_var("mode"), e_int(1)])),
    ])
}

/// `DateTime::__elephc_unixtojd` — transcribed method builder.
fn decl_class_datetime_method_87_elephc_unixtojd() -> MethodBuilder {
method("__elephc_unixtojd")
    .static_()
    .param_default("timestamp", TypeExpr::Int, e_int(0))
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_assign("y", e_call("intval", vec![e_call("gmdate", vec![e_str("Y"), e_var("timestamp")])])),
        s_assign("m", e_call("intval", vec![e_call("gmdate", vec![e_str("n"), e_var("timestamp")])])),
        s_assign("d", e_call("intval", vec![e_call("gmdate", vec![e_str("j"), e_var("timestamp")])])),
        s_return(e_static_call("DateTime", "__elephc_greg_to_sdn", vec![e_var("y"), e_var("m"), e_var("d")])),
    ])
}

/// `DateTime::__elephc_jdtounix` — transcribed method builder.
fn decl_class_datetime_method_88_elephc_jdtounix() -> MethodBuilder {
method("__elephc_jdtounix")
    .static_()
    .param("jd", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_return(e_binop(e_binop(e_var("jd"), BinOp::Sub, e_int(2440588)), BinOp::Mul, e_int(86400))),
    ])
}

/// `DateTime::__elephc_jddayofweek` — transcribed method builder.
fn decl_class_datetime_method_89_elephc_jddayofweek() -> MethodBuilder {
method("__elephc_jddayofweek")
    .static_()
    .param("jd", TypeExpr::Int)
    .param_default("mode", TypeExpr::Int, e_int(0))
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("d", e_binop(e_binop(e_binop(e_var("jd"), BinOp::Mod, e_int(7)), BinOp::Add, e_int(8)), BinOp::Mod, e_int(7))),
        s_assign("long", e_array(vec![e_str("Sunday"), e_str("Monday"), e_str("Tuesday"), e_str("Wednesday"), e_str("Thursday"), e_str("Friday"), e_str("Saturday")])),
        s_assign("short", e_array(vec![e_str("Sun"), e_str("Mon"), e_str("Tue"), e_str("Wed"), e_str("Thu"), e_str("Fri"), e_str("Sat")])),
        s_if(
            e_binop(e_var("mode"), BinOp::Eq, e_int(1)),
            vec![
                s_return(e_index(e_var("long"), e_var("d"))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("mode"), BinOp::Eq, e_int(2)),
            vec![
                s_return(e_index(e_var("short"), e_var("d"))),
            ],
            vec![],
            None,
        ),
        s_return(e_var("d")),
    ])
}

/// `DateTime::__elephc_jdmonthname` — transcribed method builder.
fn decl_class_datetime_method_90_elephc_jdmonthname() -> MethodBuilder {
method("__elephc_jdmonthname")
    .static_()
    .param("jd", TypeExpr::Int)
    .param("mode", TypeExpr::Int)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_assign("gregShort", e_array(vec![e_str(""), e_str("Jan"), e_str("Feb"), e_str("Mar"), e_str("Apr"), e_str("May"), e_str("Jun"), e_str("Jul"), e_str("Aug"), e_str("Sep"), e_str("Oct"), e_str("Nov"), e_str("Dec")])),
        s_assign("gregLong", e_array(vec![e_str(""), e_str("January"), e_str("February"), e_str("March"), e_str("April"), e_str("May"), e_str("June"), e_str("July"), e_str("August"), e_str("September"), e_str("October"), e_str("November"), e_str("December")])),
        s_assign("french", e_array(vec![e_str(""), e_str("Vendemiaire"), e_str("Brumaire"), e_str("Frimaire"), e_str("Nivose"), e_str("Pluviose"), e_str("Ventose"), e_str("Germinal"), e_str("Floreal"), e_str("Prairial"), e_str("Messidor"), e_str("Thermidor"), e_str("Fructidor"), e_str("Extra")])),
        s_if(
            e_binop(e_var("mode"), BinOp::Eq, e_int(1)),
            vec![
                s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_greg", vec![e_var("jd")])),
                s_return(e_index(e_var("gregLong"), e_index(e_var("r"), e_str("m")))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("mode"), BinOp::Eq, e_int(2)),
            vec![
                s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_jul", vec![e_var("jd")])),
                s_return(e_index(e_var("gregShort"), e_index(e_var("r"), e_str("m")))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("mode"), BinOp::Eq, e_int(3)),
            vec![
                s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_jul", vec![e_var("jd")])),
                s_return(e_index(e_var("gregLong"), e_index(e_var("r"), e_str("m")))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("mode"), BinOp::Eq, e_int(4)),
            vec![
                s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_jew", vec![e_var("jd")])),
                s_return(e_ternary(e_binop(e_index(e_var("r"), e_str("y")), BinOp::Gt, e_int(0)), e_static_call("DateTime", "__elephc_jew_month_name", vec![e_index(e_var("r"), e_str("y")), e_index(e_var("r"), e_str("m"))]), e_str(""))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("mode"), BinOp::Eq, e_int(5)),
            vec![
                s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_fr", vec![e_var("jd")])),
                s_return(e_index(e_var("french"), e_index(e_var("r"), e_str("m")))),
            ],
            vec![],
            None,
        ),
        s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_greg", vec![e_var("jd")])),
        s_return(e_index(e_var("gregShort"), e_index(e_var("r"), e_str("m")))),
    ])
}

/// `DateTime::__elephc_cal_days_in_month` — transcribed method builder.
fn decl_class_datetime_method_91_elephc_cal_days_in_month() -> MethodBuilder {
method("__elephc_cal_days_in_month")
    .static_()
    .param("calendar", TypeExpr::Int)
    .param("month", TypeExpr::Int)
    .param("year", TypeExpr::Int)
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_assign("start", e_static_call("DateTime", "__elephc_cal_to_jd", vec![e_var("calendar"), e_var("month"), e_int(1), e_var("year")])),
        s_assign("next", e_static_call("DateTime", "__elephc_cal_to_jd", vec![e_var("calendar"), e_binop(e_var("month"), BinOp::Add, e_int(1)), e_int(1), e_var("year")])),
        s_if(
            e_binop(e_var("next"), BinOp::Eq, e_int(0)),
            vec![
                s_if(
                    e_binop(e_var("year"), BinOp::Eq, e_neg(e_int(1))),
                    vec![
                        s_assign("next", e_static_call("DateTime", "__elephc_cal_to_jd", vec![e_var("calendar"), e_int(1), e_int(1), e_int(1)])),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("next", e_static_call("DateTime", "__elephc_cal_to_jd", vec![e_var("calendar"), e_int(1), e_int(1), e_binop(e_var("year"), BinOp::Add, e_int(1))])),
                    s_if(
                        e_binop(e_binop(e_var("calendar"), BinOp::Eq, e_int(3)), BinOp::And, e_binop(e_var("next"), BinOp::Eq, e_int(0))),
                        vec![
                            s_assign("next", e_int(2380953)),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                ),
            ],
            vec![],
            None,
        ),
        s_return(e_binop(e_var("next"), BinOp::Sub, e_var("start"))),
    ])
}

/// `DateTime::__elephc_cal_from_jd` — transcribed method builder.
fn decl_class_datetime_method_92_elephc_cal_from_jd() -> MethodBuilder {
method("__elephc_cal_from_jd")
    .static_()
    .param("jd", TypeExpr::Int)
    .param("calendar", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("gregShort", e_array(vec![e_str(""), e_str("Jan"), e_str("Feb"), e_str("Mar"), e_str("Apr"), e_str("May"), e_str("Jun"), e_str("Jul"), e_str("Aug"), e_str("Sep"), e_str("Oct"), e_str("Nov"), e_str("Dec")])),
        s_assign("gregLong", e_array(vec![e_str(""), e_str("January"), e_str("February"), e_str("March"), e_str("April"), e_str("May"), e_str("June"), e_str("July"), e_str("August"), e_str("September"), e_str("October"), e_str("November"), e_str("December")])),
        s_assign("french", e_array(vec![e_str(""), e_str("Vendemiaire"), e_str("Brumaire"), e_str("Frimaire"), e_str("Nivose"), e_str("Pluviose"), e_str("Ventose"), e_str("Germinal"), e_str("Floreal"), e_str("Prairial"), e_str("Messidor"), e_str("Thermidor"), e_str("Fructidor"), e_str("Extra")])),
        s_assign("dayLong", e_array(vec![e_str("Sunday"), e_str("Monday"), e_str("Tuesday"), e_str("Wednesday"), e_str("Thursday"), e_str("Friday"), e_str("Saturday")])),
        s_assign("dayShort", e_array(vec![e_str("Sun"), e_str("Mon"), e_str("Tue"), e_str("Wed"), e_str("Thu"), e_str("Fri"), e_str("Sat")])),
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(1)),
            vec![
                s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_jul", vec![e_var("jd")])),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_var("calendar"), BinOp::Eq, e_int(2)),
                vec![
                    s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_jew", vec![e_var("jd")])),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_var("calendar"), BinOp::Eq, e_int(3)),
                    vec![
                        s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_fr", vec![e_var("jd")])),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("r", e_static_call("DateTime", "__elephc_sdn_to_greg", vec![e_var("jd")])),
                ]),
                ),
            ]),
            ),
        ]),
        ),
        s_assign("y", e_index(e_var("r"), e_str("y"))),
        s_assign("m", e_index(e_var("r"), e_str("m"))),
        s_assign("d", e_index(e_var("r"), e_str("d"))),
        s_assign("dow", e_binop(e_binop(e_binop(e_var("jd"), BinOp::Mod, e_int(7)), BinOp::Add, e_int(8)), BinOp::Mod, e_int(7))),
        s_if(
            e_binop(e_binop(e_var("calendar"), BinOp::Eq, e_int(2)), BinOp::And, e_binop(e_var("y"), BinOp::LtEq, e_int(0))),
            vec![
                s_assign("abMonth", e_str("")),
                s_assign("monthName", e_str("")),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_var("calendar"), BinOp::Eq, e_int(2)),
                vec![
                    s_assign("abMonth", e_static_call("DateTime", "__elephc_jew_month_name", vec![e_var("y"), e_var("m")])),
                    s_assign("monthName", e_var("abMonth")),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_var("calendar"), BinOp::Eq, e_int(1)),
                    vec![
                        s_assign("abMonth", e_index(e_var("gregShort"), e_var("m"))),
                        s_assign("monthName", e_index(e_var("gregLong"), e_var("m"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("calendar"), BinOp::Eq, e_int(3)),
                        vec![
                            s_assign("abMonth", e_index(e_var("french"), e_var("m"))),
                            s_assign("monthName", e_index(e_var("french"), e_var("m"))),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("abMonth", e_index(e_var("gregShort"), e_var("m"))),
                        s_assign("monthName", e_index(e_var("gregLong"), e_var("m"))),
                    ]),
                    ),
                ]),
                ),
            ]),
            ),
        ]),
        ),
        s_return(e_array_assoc(vec![(e_str("date"), e_binop(e_binop(e_binop(e_binop(e_var("m"), BinOp::Concat, e_str("/")), BinOp::Concat, e_var("d")), BinOp::Concat, e_str("/")), BinOp::Concat, e_var("y"))), (e_str("month"), e_var("m")), (e_str("day"), e_var("d")), (e_str("year"), e_var("y")), (e_str("dow"), e_var("dow")), (e_str("abbrevdayname"), e_index(e_var("dayShort"), e_var("dow"))), (e_str("dayname"), e_index(e_var("dayLong"), e_var("dow"))), (e_str("abbrevmonth"), e_var("abMonth")), (e_str("monthname"), e_var("monthName"))])),
    ])
}

/// `DateTime::__elephc_cal_info` — transcribed method builder.
fn decl_class_datetime_method_93_elephc_cal_info() -> MethodBuilder {
method("__elephc_cal_info")
    .static_()
    .param_default("calendar", TypeExpr::Int, e_int(-1))
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("greg", e_array_assoc(vec![(e_str("months"), e_array_assoc(vec![(e_int(1), e_str("January")), (e_int(2), e_str("February")), (e_int(3), e_str("March")), (e_int(4), e_str("April")), (e_int(5), e_str("May")), (e_int(6), e_str("June")), (e_int(7), e_str("July")), (e_int(8), e_str("August")), (e_int(9), e_str("September")), (e_int(10), e_str("October")), (e_int(11), e_str("November")), (e_int(12), e_str("December"))])), (e_str("abbrevmonths"), e_array_assoc(vec![(e_int(1), e_str("Jan")), (e_int(2), e_str("Feb")), (e_int(3), e_str("Mar")), (e_int(4), e_str("Apr")), (e_int(5), e_str("May")), (e_int(6), e_str("Jun")), (e_int(7), e_str("Jul")), (e_int(8), e_str("Aug")), (e_int(9), e_str("Sep")), (e_int(10), e_str("Oct")), (e_int(11), e_str("Nov")), (e_int(12), e_str("Dec"))])), (e_str("maxdaysinmonth"), e_int(31)), (e_str("calname"), e_str("Gregorian")), (e_str("calsymbol"), e_str("CAL_GREGORIAN"))])),
        s_assign("jul", e_array_assoc(vec![(e_str("months"), e_array_assoc(vec![(e_int(1), e_str("January")), (e_int(2), e_str("February")), (e_int(3), e_str("March")), (e_int(4), e_str("April")), (e_int(5), e_str("May")), (e_int(6), e_str("June")), (e_int(7), e_str("July")), (e_int(8), e_str("August")), (e_int(9), e_str("September")), (e_int(10), e_str("October")), (e_int(11), e_str("November")), (e_int(12), e_str("December"))])), (e_str("abbrevmonths"), e_array_assoc(vec![(e_int(1), e_str("Jan")), (e_int(2), e_str("Feb")), (e_int(3), e_str("Mar")), (e_int(4), e_str("Apr")), (e_int(5), e_str("May")), (e_int(6), e_str("Jun")), (e_int(7), e_str("Jul")), (e_int(8), e_str("Aug")), (e_int(9), e_str("Sep")), (e_int(10), e_str("Oct")), (e_int(11), e_str("Nov")), (e_int(12), e_str("Dec"))])), (e_str("maxdaysinmonth"), e_int(31)), (e_str("calname"), e_str("Julian")), (e_str("calsymbol"), e_str("CAL_JULIAN"))])),
        s_assign("jew", e_array_assoc(vec![(e_str("months"), e_array_assoc(vec![(e_int(1), e_str("Tishri")), (e_int(2), e_str("Heshvan")), (e_int(3), e_str("Kislev")), (e_int(4), e_str("Tevet")), (e_int(5), e_str("Shevat")), (e_int(6), e_str("Adar I")), (e_int(7), e_str("Adar II")), (e_int(8), e_str("Nisan")), (e_int(9), e_str("Iyyar")), (e_int(10), e_str("Sivan")), (e_int(11), e_str("Tammuz")), (e_int(12), e_str("Av")), (e_int(13), e_str("Elul"))])), (e_str("abbrevmonths"), e_array_assoc(vec![(e_int(1), e_str("Tishri")), (e_int(2), e_str("Heshvan")), (e_int(3), e_str("Kislev")), (e_int(4), e_str("Tevet")), (e_int(5), e_str("Shevat")), (e_int(6), e_str("Adar I")), (e_int(7), e_str("Adar II")), (e_int(8), e_str("Nisan")), (e_int(9), e_str("Iyyar")), (e_int(10), e_str("Sivan")), (e_int(11), e_str("Tammuz")), (e_int(12), e_str("Av")), (e_int(13), e_str("Elul"))])), (e_str("maxdaysinmonth"), e_int(30)), (e_str("calname"), e_str("Jewish")), (e_str("calsymbol"), e_str("CAL_JEWISH"))])),
        s_assign("fr", e_array_assoc(vec![(e_str("months"), e_array_assoc(vec![(e_int(1), e_str("Vendemiaire")), (e_int(2), e_str("Brumaire")), (e_int(3), e_str("Frimaire")), (e_int(4), e_str("Nivose")), (e_int(5), e_str("Pluviose")), (e_int(6), e_str("Ventose")), (e_int(7), e_str("Germinal")), (e_int(8), e_str("Floreal")), (e_int(9), e_str("Prairial")), (e_int(10), e_str("Messidor")), (e_int(11), e_str("Thermidor")), (e_int(12), e_str("Fructidor")), (e_int(13), e_str("Extra"))])), (e_str("abbrevmonths"), e_array_assoc(vec![(e_int(1), e_str("Vendemiaire")), (e_int(2), e_str("Brumaire")), (e_int(3), e_str("Frimaire")), (e_int(4), e_str("Nivose")), (e_int(5), e_str("Pluviose")), (e_int(6), e_str("Ventose")), (e_int(7), e_str("Germinal")), (e_int(8), e_str("Floreal")), (e_int(9), e_str("Prairial")), (e_int(10), e_str("Messidor")), (e_int(11), e_str("Thermidor")), (e_int(12), e_str("Fructidor")), (e_int(13), e_str("Extra"))])), (e_str("maxdaysinmonth"), e_int(30)), (e_str("calname"), e_str("French")), (e_str("calsymbol"), e_str("CAL_FRENCH"))])),
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(0)),
            vec![
                s_return(e_var("greg")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(1)),
            vec![
                s_return(e_var("jul")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(2)),
            vec![
                s_return(e_var("jew")),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("calendar"), BinOp::Eq, e_int(3)),
            vec![
                s_return(e_var("fr")),
            ],
            vec![],
            None,
        ),
        s_return(e_array_assoc(vec![(e_int(0), e_var("greg")), (e_int(1), e_var("jul")), (e_int(2), e_var("jew")), (e_int(3), e_var("fr"))])),
    ])
}

/// `DateTime::__elephc_is_initialized` — transcribed method builder.
fn decl_class_datetime_method_94_elephc_is_initialized() -> MethodBuilder {
method("__elephc_is_initialized")
    .final_()
    .returns(TypeExpr::Bool)
    .body_exact(vec![
        s_return(e_this_prop("__elephc_initialized")),
    ])
}

/// `DateTime::__elephc_assert_initialized` — transcribed method builder.
fn decl_class_datetime_method_95_elephc_assert_initialized() -> MethodBuilder {
method("__elephc_assert_initialized")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_assign("objectClass", e_call("get_class", vec![e_this()])),
                s_assign("inheritance", e_ternary(e_binop(e_var("objectClass"), BinOp::StrictEq, e_str("DateTime")), e_str(""), e_str(" (inheriting DateTime)"))),
                s_throw(e_new("DateObjectError", vec![e_binop(e_binop(e_binop(e_str("Object of type "), BinOp::Concat, e_var("objectClass")), BinOp::Concat, e_var("inheritance")), BinOp::Concat, e_str(" has not been correctly initialized by calling parent::__construct() in its constructor"))])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateTime::__elephc_assert_comparable` — transcribed method builder.
fn decl_class_datetime_method_96_elephc_assert_comparable() -> MethodBuilder {
method("__elephc_assert_comparable")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Trying to compare an incomplete DateTime or DateTimeImmutable object")])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateTime::__elephc_compare` — transcribed method builder.
fn decl_class_datetime_method_97_elephc_compare() -> MethodBuilder {
method("__elephc_compare")
    .final_()
    .param("other", t_class("DateTimeInterface"))
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_comparable", vec![])),
        s_expr(e_method_call(e_var("other"), "__elephc_assert_comparable", vec![])),
        s_assign("leftTimestamp", e_method_call(e_this(), "getTimestamp", vec![])),
        s_assign("rightTimestamp", e_method_call(e_var("other"), "getTimestamp", vec![])),
        s_if(
            e_binop(e_var("leftTimestamp"), BinOp::Lt, e_var("rightTimestamp")),
            vec![
                s_return(e_neg(e_int(1))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("leftTimestamp"), BinOp::Gt, e_var("rightTimestamp")),
            vec![
                s_return(e_int(1)),
            ],
            vec![],
            None,
        ),
        s_assign("leftMicrosecond", e_method_call(e_this(), "getMicrosecond", vec![])),
        s_assign("rightMicrosecond", e_method_call(e_var("other"), "getMicrosecond", vec![])),
        s_if(
            e_binop(e_var("leftMicrosecond"), BinOp::Lt, e_var("rightMicrosecond")),
            vec![
                s_return(e_neg(e_int(1))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("leftMicrosecond"), BinOp::Gt, e_var("rightMicrosecond")),
            vec![
                s_return(e_int(1)),
            ],
            vec![],
            None,
        ),
        s_return(e_int(0)),
    ])
}

/// `DateTime` — transcribed from the PHP form.
fn decl_class_datetime() -> Stmt {
    class("DateTime")
        .implements("DateTimeInterface")
        .private_prop("__elephc_initialized", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("timestamp", TypeExpr::Int, Some(e_int(0)))
        .private_prop("timezone_name", TypeExpr::Str, Some(e_str("UTC")))
        .private_prop("microsecond", TypeExpr::Int, Some(e_int(0)))
        .private_prop("__elephc_civil_override", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("__elephc_civil_year", TypeExpr::Int, Some(e_int(1970)))
        .private_prop("__elephc_civil_month", TypeExpr::Int, Some(e_int(1)))
        .private_prop("__elephc_civil_day", TypeExpr::Int, Some(e_int(1)))
        .static_prop("lastErrorCount", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastErrorPosition", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastErrorMessage", TypeExpr::Str, Some(e_str("")))
        .static_prop("lastWarningCount", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastWarningPosition", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastWarningMessage", TypeExpr::Str, Some(e_str("")))
        .static_prop("lastParseResult", t_mixed(), Some(e_str("")))
        .private_prop("__elephc_arguments", t_mixed(), Some(e_null()))
        .private_prop("__elephc_seen_named_argument", TypeExpr::Bool, Some(e_bool(false)))
        .method(decl_class_datetime_method_0_construct())
        .method(decl_class_datetime_method_1_gettimestamp())
        .method(decl_class_datetime_method_2_getmicrosecond())
        .method(decl_class_datetime_method_3_elephc_set_microsecond_raw())
        .method(decl_class_datetime_method_4_gettimezone())
        .method(decl_class_datetime_method_5_format())
        .method(decl_class_datetime_method_6_getoffset())
        .method(decl_class_datetime_method_7_diff())
        .method(decl_class_datetime_method_8_settimestamp())
        .method(decl_class_datetime_method_9_setmicrosecond())
        .method(decl_class_datetime_method_10_settime())
        .method(decl_class_datetime_method_11_setdate())
        .method(decl_class_datetime_method_12_settimezone())
        .method(decl_class_datetime_method_13_add())
        .method(decl_class_datetime_method_14_sub())
        .method(decl_class_datetime_method_15_modify())
        .method(decl_class_datetime_method_16_createfromformat())
        .method(decl_class_datetime_method_17_getlasterrors())
        .method(decl_class_datetime_method_18_createfromtimestamp())
        .method(decl_class_datetime_method_19_createfrominterface())
        .method(decl_class_datetime_method_20_createfromimmutable())
        .method(decl_class_datetime_method_21_setisodate())
        .method(decl_class_datetime_method_22_elephc_date_parse_from_format())
        .method(decl_class_datetime_method_23_elephc_date_parse())
        .method(decl_class_datetime_method_24_elephc_gettimeofday())
        .method(decl_class_datetime_method_25_elephc_idate())
        .method(decl_class_datetime_method_26_elephc_timezone_type())
        .method(decl_class_datetime_method_27_elephc_runtime_timezone_name())
        .method(decl_class_datetime_method_28_elephc_date_create())
        .method(decl_class_datetime_method_29_wakeup())
        .method(decl_class_datetime_method_30_serialize())
        .method(decl_class_datetime_method_31_unserialize())
        .method(decl_class_datetime_method_32_set_state())
        .method(decl_class_datetime_method_33_elephc_debug_dump())
        .method(decl_class_datetime_method_34_elephc_print_r_dump())
        .method(decl_class_datetime_method_35_elephc_clone_for_period())
        .method(decl_class_datetime_method_36_elephc_clone_for_period_storage())
        .method(decl_class_datetime_method_37_elephc_begin_argument_array())
        .method(decl_class_datetime_method_38_elephc_append_one_argument())
        .method(decl_class_datetime_method_39_elephc_append_argument_chunk())
        .method(decl_class_datetime_method_40_elephc_finish_argument_array())
        .method(decl_class_datetime_method_41_elephc_date_modify())
        .method(decl_class_datetime_method_42_elephc_date_timestamp_set())
        .method(decl_class_datetime_method_43_elephc_date_add())
        .method(decl_class_datetime_method_44_elephc_date_sub())
        .method(decl_class_datetime_method_45_elephc_strftime())
        .method(decl_class_datetime_method_46_elephc_extract_micros())
        .method(decl_class_datetime_method_47_elephc_strip_micros())
        .method(decl_class_datetime_method_48_elephc_extract_constructor_zone())
        .method(decl_class_datetime_method_49_elephc_extract_modify_micros())
        .method(decl_class_datetime_method_50_elephc_strip_modify_micros())
        .method(decl_class_datetime_method_51_elephc_malformed_time_message())
        .method(decl_class_datetime_method_52_elephc_sun_rs())
        .method(decl_class_datetime_method_53_elephc_sun_val())
        .method(decl_class_datetime_method_54_elephc_date_sun_info())
        .method(decl_class_datetime_method_55_elephc_date_sunfunc())
        .method(decl_class_datetime_method_56_elephc_strptime())
        .method(decl_class_datetime_method_57_elephc_timezone_name_from_abbr())
        .method(decl_class_datetime_method_58_elephc_argument_type_error())
        .method(decl_class_datetime_method_59_elephc_weak_string_argument())
        .method(decl_class_datetime_method_60_elephc_deprecated_string_constant())
        .method(decl_class_datetime_method_61_elephc_deprecated_int_constant())
        .method(decl_class_datetime_method_62_elephc_greg_to_sdn())
        .method(decl_class_datetime_method_63_elephc_sdn_to_greg())
        .method(decl_class_datetime_method_64_elephc_jul_to_sdn())
        .method(decl_class_datetime_method_65_elephc_sdn_to_jul())
        .method(decl_class_datetime_method_66_elephc_fr_to_sdn())
        .method(decl_class_datetime_method_67_elephc_sdn_to_fr())
        .method(decl_class_datetime_method_68_elephc_jew_tishri1())
        .method(decl_class_datetime_method_69_elephc_jew_molad_cycle())
        .method(decl_class_datetime_method_70_elephc_jew_find_tishri_molad())
        .method(decl_class_datetime_method_71_elephc_jew_find_start_year())
        .method(decl_class_datetime_method_72_elephc_jew_to_sdn())
        .method(decl_class_datetime_method_73_elephc_sdn_to_jew())
        .method(decl_class_datetime_method_74_elephc_jew_month_name())
        .method(decl_class_datetime_method_75_elephc_easter_calc())
        .method(decl_class_datetime_method_76_elephc_cal_to_jd())
        .method(decl_class_datetime_method_77_elephc_gregoriantojd())
        .method(decl_class_datetime_method_78_elephc_jdtogregorian())
        .method(decl_class_datetime_method_79_elephc_juliantojd())
        .method(decl_class_datetime_method_80_elephc_jdtojulian())
        .method(decl_class_datetime_method_81_elephc_frenchtojd())
        .method(decl_class_datetime_method_82_elephc_jdtofrench())
        .method(decl_class_datetime_method_83_elephc_jewishtojd())
        .method(decl_class_datetime_method_84_elephc_jdtojewish())
        .method(decl_class_datetime_method_85_elephc_easter_days())
        .method(decl_class_datetime_method_86_elephc_easter_date())
        .method(decl_class_datetime_method_87_elephc_unixtojd())
        .method(decl_class_datetime_method_88_elephc_jdtounix())
        .method(decl_class_datetime_method_89_elephc_jddayofweek())
        .method(decl_class_datetime_method_90_elephc_jdmonthname())
        .method(decl_class_datetime_method_91_elephc_cal_days_in_month())
        .method(decl_class_datetime_method_92_elephc_cal_from_jd())
        .method(decl_class_datetime_method_93_elephc_cal_info())
        .method(decl_class_datetime_method_94_elephc_is_initialized())
        .method(decl_class_datetime_method_95_elephc_assert_initialized())
        .method(decl_class_datetime_method_96_elephc_assert_comparable())
        .method(decl_class_datetime_method_97_elephc_compare())
        .build()
}

/// `DateTimeImmutable::__construct` — transcribed method builder.
fn decl_class_datetimeimmutable_method_0_construct() -> MethodBuilder {
method("__construct")
    .param_default("datetime", TypeExpr::Str, e_str("now"))
    .param_default("timezone", t_nullable(t_class("DateTimeZone")), e_null())
    .body_exact(vec![
        s_assign("__originalDateTime", e_binop(e_var("datetime"), BinOp::Concat, e_str(""))),
        s_if(
            e_binop(e_binop(e_var("__originalDateTime"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("__originalDateTime"), BinOp::StrictEq, e_str("now"))),
            vec![
                s_static_prop_assign("DateTime", "lastParseResult", e_str("")),
            ],
            vec![],
            Some(vec![
            s_assign("__parseResult", e_static_call("DateTime", "__elephc_date_parse", vec![e_var("__originalDateTime")])),
            s_if(
                e_binop(e_binop(e_index(e_var("__parseResult"), e_str("error_count")), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_index(e_var("__parseResult"), e_str("warning_count")), BinOp::StrictEq, e_int(0))),
                vec![
                    s_static_prop_assign("DateTime", "lastParseResult", e_str("")),
                ],
                vec![],
                Some(vec![
                s_static_prop_assign("DateTime", "lastParseResult", e_var("__parseResult")),
            ]),
            ),
        ]),
        ),
        s_if(
            e_binop(e_var("datetime"), BinOp::StrictEq, e_str("")),
            vec![
                s_assign("datetime", e_str("now")),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "microsecond", e_static_call("DateTime", "__elephc_extract_micros", vec![e_var("datetime")])),
        s_assign("datetime", e_static_call("DateTime", "__elephc_strip_micros", vec![e_var("datetime")])),
        s_if(
            e_binop(e_call("substr", vec![e_var("__originalDateTime"), e_int(0), e_int(1)]), BinOp::StrictEq, e_str("@")),
            vec![
                s_assign("__ts", e_call("strtotime", vec![e_var("datetime")])),
                s_if(
                    e_binop(e_var("__ts"), BinOp::StrictEq, e_bool(false)),
                    vec![
                        s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str(""), e_var("__originalDateTime")])])),
                    ],
                    vec![],
                    None,
                ),
                s_prop_assign(e_this(), "timestamp", e_var("__ts")),
                s_prop_assign(e_this(), "timezone_name", e_str("+00:00")),
                s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_assign("__zoneData", e_call("explode", vec![e_str("\t"), e_static_call("DateTime", "__elephc_extract_constructor_zone", vec![e_var("datetime")])])),
        s_assign("__detectedZone", e_index(e_var("__zoneData"), e_int(0))),
        s_assign("datetime", e_index(e_var("__zoneData"), e_int(1))),
        s_if(
            e_binop(e_var("__detectedZone"), BinOp::StrictNotEq, e_str("")),
            vec![
                s_if(
                    e_binop(e_var("datetime"), BinOp::StrictEq, e_str("now")),
                    vec![
                        s_assign("__ts", e_call("microtime", vec![e_bool(true)])),
                        s_prop_assign(e_this(), "timestamp", e_call("intval", vec![e_var("__ts")])),
                        s_prop_assign(e_this(), "microsecond", e_call("intval", vec![e_binop(e_binop(e_var("__ts"), BinOp::Sub, e_this_prop("timestamp")), BinOp::Mul, e_int(1000000))])),
                        s_if(
                            e_binop(e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("__detectedZone")]), BinOp::StrictNotEq, e_int(3)),
                            vec![
                                s_assign("__saved", e_call("date_default_timezone_get", vec![])),
                                s_assign("__wall", e_call("date", vec![e_str("Y-m-d H:i:s"), e_this_prop("timestamp")])),
                                s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("__detectedZone")])])),
                                s_prop_assign(e_this(), "timestamp", e_call("strtotime", vec![e_var("__wall")])),
                                s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("__saved", e_call("date_default_timezone_get", vec![])),
                    s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("__detectedZone")])])),
                    s_assign("__ts", e_call("strtotime", vec![e_var("datetime")])),
                    s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
                    s_if(
                        e_binop(e_var("__ts"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str(""), e_var("__originalDateTime")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "timestamp", e_var("__ts")),
                ]),
                ),
                s_prop_assign(e_this(), "timezone_name", e_var("__detectedZone")),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_var("timezone"), BinOp::StrictEq, e_null()),
                vec![
                    s_if(
                        e_binop(e_var("datetime"), BinOp::StrictEq, e_str("now")),
                        vec![
                            s_assign("__ts", e_call("microtime", vec![e_bool(true)])),
                            s_prop_assign(e_this(), "timestamp", e_call("intval", vec![e_var("__ts")])),
                            s_prop_assign(e_this(), "microsecond", e_call("intval", vec![e_binop(e_binop(e_var("__ts"), BinOp::Sub, e_this_prop("timestamp")), BinOp::Mul, e_int(1000000))])),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("__ts", e_call("strtotime", vec![e_var("datetime")])),
                        s_if(
                            e_binop(e_var("__ts"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str(""), e_var("__originalDateTime")])])),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "timestamp", e_var("__ts")),
                    ]),
                    ),
                    s_prop_assign(e_this(), "timezone_name", e_call("date_default_timezone_get", vec![])),
                ],
                vec![],
                Some(vec![
                s_assign("tzname", e_method_call(e_var("timezone"), "getName", vec![])),
                s_if(
                    e_binop(e_var("datetime"), BinOp::StrictEq, e_str("now")),
                    vec![
                        s_assign("__ts", e_call("microtime", vec![e_bool(true)])),
                        s_prop_assign(e_this(), "timestamp", e_call("intval", vec![e_var("__ts")])),
                        s_prop_assign(e_this(), "microsecond", e_call("intval", vec![e_binop(e_binop(e_var("__ts"), BinOp::Sub, e_this_prop("timestamp")), BinOp::Mul, e_int(1000000))])),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("saved", e_call("date_default_timezone_get", vec![])),
                    s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("tzname")])])),
                    s_assign("__ts", e_call("strtotime", vec![e_var("datetime")])),
                    s_if(
                        e_binop(e_var("__ts"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_expr(e_call("date_default_timezone_set", vec![e_var("saved")])),
                            s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str(""), e_var("__originalDateTime")])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "timestamp", e_var("__ts")),
                    s_expr(e_call("date_default_timezone_set", vec![e_var("saved")])),
                ]),
                ),
                s_prop_assign(e_this(), "timezone_name", e_var("tzname")),
            ]),
            ),
        ]),
        ),
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
    ])
}

/// `DateTimeImmutable::getTimestamp` — transcribed method builder.
fn decl_class_datetimeimmutable_method_1_gettimestamp() -> MethodBuilder {
method("getTimestamp")
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_this_prop("timestamp")),
    ])
}

/// `DateTimeImmutable::getMicrosecond` — transcribed method builder.
fn decl_class_datetimeimmutable_method_2_getmicrosecond() -> MethodBuilder {
method("getMicrosecond")
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_this_prop("microsecond")),
    ])
}

/// `DateTimeImmutable::__elephc_set_microsecond_raw` — transcribed method builder.
fn decl_class_datetimeimmutable_method_3_elephc_set_microsecond_raw() -> MethodBuilder {
method("__elephc_set_microsecond_raw")
    .param("microsecond", TypeExpr::Int)
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_prop_assign(e_this(), "microsecond", e_var("microsecond")),
    ])
}

/// `DateTimeImmutable::getTimezone` — transcribed method builder.
fn decl_class_datetimeimmutable_method_4_gettimezone() -> MethodBuilder {
method("getTimezone")
    .returns(t_class("DateTimeZone"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_new("DateTimeZone", vec![e_this_prop("timezone_name")])),
    ])
}

/// `DateTimeImmutable::format` — transcribed method builder.
fn decl_class_datetimeimmutable_method_5_format() -> MethodBuilder {
method("format")
    .param("format", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("saved", e_call("date_default_timezone_get", vec![])),
        s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_this_prop("timezone_name")])])),
        s_if(
            e_this_prop("__elephc_civil_override"),
            vec![
                s_assign("civil", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_this_prop("timezone_name"), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("__elephc_civil_year")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("__elephc_civil_month")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_this_prop("__elephc_civil_day"))),
                s_assign("r", e_call("elephc_tz_format_civil", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_var("format"), e_call("strlen", vec![e_var("format")]), e_var("civil"), e_call("strlen", vec![e_var("civil")])])),
                s_expr(e_call("date_default_timezone_set", vec![e_var("saved")])),
                s_return(e_var("r")),
            ],
            vec![],
            None,
        ),
        s_assign("us", e_this_prop("microsecond")),
        s_assign("fmt", e_str("")),
        s_assign("flen", e_call("strlen", vec![e_var("format")])),
        s_assign("k", e_int(0)),
        s_while(e_binop(e_var("k"), BinOp::Lt, e_var("flen")), vec![
            s_assign("ch", e_index(e_var("format"), e_var("k"))),
            s_if(
                e_binop(e_var("ch"), BinOp::StrictEq, e_str("\\")),
                vec![
                    s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("ch"))),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_if(
                        e_binop(e_var("k"), BinOp::Lt, e_var("flen")),
                        vec![
                            s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_index(e_var("format"), e_var("k")))),
                            s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
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
                e_binop(e_var("ch"), BinOp::StrictEq, e_str("u")),
                vec![
                    s_assign("s", e_binop(e_str(""), BinOp::Concat, e_var("us"))),
                    s_while(e_binop(e_call("strlen", vec![e_var("s")]), BinOp::Lt, e_int(6)), vec![
                        s_assign("s", e_binop(e_str("0"), BinOp::Concat, e_var("s"))),
                    ]),
                    s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("s"))),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("ch"), BinOp::StrictEq, e_str("v")),
                vec![
                    s_assign("ms", e_call("intdiv", vec![e_var("us"), e_int(1000)])),
                    s_assign("s", e_binop(e_str(""), BinOp::Concat, e_var("ms"))),
                    s_while(e_binop(e_call("strlen", vec![e_var("s")]), BinOp::Lt, e_int(3)), vec![
                        s_assign("s", e_binop(e_str("0"), BinOp::Concat, e_var("s"))),
                    ]),
                    s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("s"))),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("T")), BinOp::And, e_binop(e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("timezone_name")]), BinOp::StrictEq, e_int(1))),
                vec![
                    s_assign("zoneLiteral", e_binop(e_binop(e_str("GMT"), BinOp::Concat, e_call("substr", vec![e_this_prop("timezone_name"), e_int(0), e_int(3)])), BinOp::Concat, e_call("substr", vec![e_this_prop("timezone_name"), e_int(4), e_int(2)]))),
                    s_assign("zoneLength", e_call("strlen", vec![e_var("zoneLiteral")])),
                    s_assign("zoneIndex", e_int(0)),
                    s_while(e_binop(e_var("zoneIndex"), BinOp::Lt, e_var("zoneLength")), vec![
                        s_assign("fmt", e_binop(e_binop(e_var("fmt"), BinOp::Concat, e_str("\\")), BinOp::Concat, e_index(e_var("zoneLiteral"), e_var("zoneIndex")))),
                        s_assign("zoneIndex", e_binop(e_var("zoneIndex"), BinOp::Add, e_int(1))),
                    ]),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("e")), BinOp::Or, e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("T")), BinOp::And, e_binop(e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("timezone_name")]), BinOp::StrictEq, e_int(2)))),
                vec![
                    s_assign("zoneLiteral", e_this_prop("timezone_name")),
                    s_assign("zoneLength", e_call("strlen", vec![e_var("zoneLiteral")])),
                    s_assign("zoneIndex", e_int(0)),
                    s_while(e_binop(e_var("zoneIndex"), BinOp::Lt, e_var("zoneLength")), vec![
                        s_assign("fmt", e_binop(e_binop(e_var("fmt"), BinOp::Concat, e_str("\\")), BinOp::Concat, e_index(e_var("zoneLiteral"), e_var("zoneIndex")))),
                        s_assign("zoneIndex", e_binop(e_var("zoneIndex"), BinOp::Add, e_int(1))),
                    ]),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("X")), BinOp::Or, e_binop(e_var("ch"), BinOp::StrictEq, e_str("x"))),
                vec![
                    s_assign("year", e_call("intval", vec![e_call("date", vec![e_str("Y"), e_this_prop("timestamp")])])),
                    s_if(
                        e_binop(e_var("year"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("year", e_neg(e_var("year"))),
                            s_assign("sign", e_str("-")),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("sign", e_str("+")),
                    ]),
                    ),
                    s_assign("s", e_binop(e_str(""), BinOp::Concat, e_var("year"))),
                    s_while(e_binop(e_call("strlen", vec![e_var("s")]), BinOp::Lt, e_int(4)), vec![
                        s_assign("s", e_binop(e_str("0"), BinOp::Concat, e_var("s"))),
                    ]),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("ch"), BinOp::StrictEq, e_str("x")), BinOp::And, e_binop(e_var("sign"), BinOp::StrictEq, e_str("+"))), BinOp::And, e_binop(e_call("strlen", vec![e_var("s")]), BinOp::LtEq, e_int(4))),
                        vec![
                            s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("s"))),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("fmt", e_binop(e_binop(e_var("fmt"), BinOp::Concat, e_var("sign")), BinOp::Concat, e_var("s"))),
                    ]),
                    ),
                    s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
                    s_continue(1),
                ],
                vec![],
                None,
            ),
            s_assign("fmt", e_binop(e_var("fmt"), BinOp::Concat, e_var("ch"))),
            s_assign("k", e_binop(e_var("k"), BinOp::Add, e_int(1))),
        ]),
        s_assign("r", e_call("date", vec![e_var("fmt"), e_this_prop("timestamp")])),
        s_expr(e_call("date_default_timezone_set", vec![e_var("saved")])),
        s_return(e_var("r")),
    ])
}

/// `DateTimeImmutable::getOffset` — transcribed method builder.
fn decl_class_datetimeimmutable_method_6_getoffset() -> MethodBuilder {
method("getOffset")
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__saved", e_call("date_default_timezone_get", vec![])),
        s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_this_prop("timezone_name")])])),
        s_assign("__off", e_call("intval", vec![e_call("date", vec![e_str("Z"), e_this_prop("timestamp")])])),
        s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
        s_return(e_var("__off")),
    ])
}

/// `DateTimeImmutable::diff` — transcribed method builder.
fn decl_class_datetimeimmutable_method_7_diff() -> MethodBuilder {
method("diff")
    .param("targetObject", t_class("DateTimeInterface"))
    .param_default("absolute", TypeExpr::Bool, e_bool(false))
    .returns(t_class("DateInterval"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("leftTimestamp", e_this_prop("timestamp")),
        s_assign("leftMicrosecond", e_this_prop("microsecond")),
        s_assign("leftTimezone", e_this_prop("timezone_name")),
        s_assign("rightTimestamp", e_method_call(e_var("targetObject"), "getTimestamp", vec![])),
        s_assign("rightMicrosecond", e_method_call(e_var("targetObject"), "getMicrosecond", vec![])),
        s_assign("rightTimezone", e_method_call(e_var("targetObject"), "format", vec![e_str("e")])),
        s_assign("parsed", e_call("__elephc_timelib_diff", vec![e_var("leftTimestamp"), e_var("leftMicrosecond"), e_var("leftTimezone"), e_var("rightTimestamp"), e_var("rightMicrosecond"), e_var("rightTimezone")])),
        s_assign("interval", e_new("DateInterval", vec![e_str("PT0S")])),
        s_prop_assign(e_var("interval"), "y", e_index(e_var("parsed"), e_str("y"))),
        s_prop_assign(e_var("interval"), "m", e_index(e_var("parsed"), e_str("m"))),
        s_prop_assign(e_var("interval"), "d", e_index(e_var("parsed"), e_str("d"))),
        s_prop_assign(e_var("interval"), "h", e_index(e_var("parsed"), e_str("h"))),
        s_prop_assign(e_var("interval"), "i", e_index(e_var("parsed"), e_str("i"))),
        s_prop_assign(e_var("interval"), "s", e_index(e_var("parsed"), e_str("s"))),
        s_prop_assign(e_var("interval"), "f", e_binop(e_index(e_var("parsed"), e_str("us")), BinOp::Div, e_float(1000000.0))),
        s_prop_assign(e_var("interval"), "invert", e_index(e_var("parsed"), e_str("invert"))),
        s_if(
            e_var("absolute"),
            vec![
                s_prop_assign(e_var("interval"), "invert", e_int(0)),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_var("interval"), "days", e_index(e_var("parsed"), e_str("days"))),
        s_expr(e_method_call(e_var("interval"), "__elephc_mark_civil", vec![])),
        s_return(e_var("interval")),
    ])
}

/// `DateTimeImmutable::setTimestamp` — transcribed method builder.
fn decl_class_datetimeimmutable_method_8_settimestamp() -> MethodBuilder {
method("setTimestamp")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::setTimestamp() does not modify the object itself"))])
    .param("timestamp", TypeExpr::Int)
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__new", e_new("DateTimeImmutable", vec![])),
        s_prop_assign(e_var("__new"), "timestamp", e_var("timestamp")),
        s_prop_assign(e_var("__new"), "timezone_name", e_this_prop("timezone_name")),
        s_prop_assign(e_var("__new"), "microsecond", e_int(0)),
        s_return(e_var("__new")),
    ])
}

/// `DateTimeImmutable::setMicrosecond` — transcribed method builder.
fn decl_class_datetimeimmutable_method_9_setmicrosecond() -> MethodBuilder {
method("setMicrosecond")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::setMicrosecond() does not modify the object itself"))])
    .param("microsecond", TypeExpr::Int)
    .returns(t_class("static"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_binop(e_binop(e_var("microsecond"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("microsecond"), BinOp::Gt, e_int(999999))),
            vec![
                s_throw(e_new("DateRangeError", vec![e_binop(e_binop(e_str("DateTimeImmutable::setMicrosecond(): Argument #1 ($microsecond) must be between 0 and 999999, "), BinOp::Concat, e_var("microsecond")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("__new", e_call("__elephc_new_instance_without_constructor", vec![e_static_class()])),
        s_expr(e_method_call(e_var("__new"), "__unserialize", vec![e_method_call(e_this(), "__serialize", vec![])])),
        s_expr(e_method_call(e_var("__new"), "__elephc_set_microsecond_raw", vec![e_var("microsecond")])),
        s_return(e_var("__new")),
    ])
}

/// `DateTimeImmutable::setTime` — transcribed method builder.
fn decl_class_datetimeimmutable_method_10_settime() -> MethodBuilder {
method("setTime")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::setTime() does not modify the object itself"))])
    .param("hour", TypeExpr::Int)
    .param("minute", TypeExpr::Int)
    .param_default("second", TypeExpr::Int, e_int(0))
    .param_default("microsecond", TypeExpr::Int, e_int(0))
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__payload", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("T\t"), BinOp::Concat, e_var("hour")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("minute")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("second")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("microsecond"))),
        s_assign("__parsed", e_call("__elephc_timelib_set_civil", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_var("__payload")])),
        s_assign("__new", e_new("DateTimeImmutable", vec![])),
        s_prop_assign(e_var("__new"), "timestamp", e_index(e_var("__parsed"), e_str("timestamp"))),
        s_prop_assign(e_var("__new"), "timezone_name", e_this_prop("timezone_name")),
        s_prop_assign(e_var("__new"), "microsecond", e_index(e_var("__parsed"), e_str("microsecond"))),
        s_return(e_var("__new")),
    ])
}

/// `DateTimeImmutable::setDate` — transcribed method builder.
fn decl_class_datetimeimmutable_method_11_setdate() -> MethodBuilder {
method("setDate")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::setDate() does not modify the object itself"))])
    .param("year", TypeExpr::Int)
    .param("month", TypeExpr::Int)
    .param("day", TypeExpr::Int)
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__payload", e_binop(e_binop(e_binop(e_binop(e_binop(e_str("D\t"), BinOp::Concat, e_var("year")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("month")), BinOp::Concat, e_str("\t")), BinOp::Concat, e_var("day"))),
        s_assign("__parsed", e_call("__elephc_timelib_set_civil", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_var("__payload")])),
        s_assign("__new", e_new("DateTimeImmutable", vec![])),
        s_prop_assign(e_var("__new"), "timestamp", e_index(e_var("__parsed"), e_str("timestamp"))),
        s_prop_assign(e_var("__new"), "timezone_name", e_this_prop("timezone_name")),
        s_prop_assign(e_var("__new"), "microsecond", e_index(e_var("__parsed"), e_str("microsecond"))),
        s_return(e_var("__new")),
    ])
}

/// `DateTimeImmutable::setTimezone` — transcribed method builder.
fn decl_class_datetimeimmutable_method_12_settimezone() -> MethodBuilder {
method("setTimezone")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::setTimezone() does not modify the object itself"))])
    .param("timezone", t_class("DateTimeZone"))
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__new", e_new("DateTimeImmutable", vec![])),
        s_prop_assign(e_var("__new"), "timestamp", e_this_prop("timestamp")),
        s_prop_assign(e_var("__new"), "timezone_name", e_method_call(e_var("timezone"), "getName", vec![])),
        s_return(e_var("__new")),
    ])
}

/// `DateTimeImmutable::add` — transcribed method builder.
fn decl_class_datetimeimmutable_method_13_add() -> MethodBuilder {
method("add")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::add() does not modify the object itself"))])
    .param("interval", t_mixed())
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_not(e_instance_of(e_var("interval"), "DateInterval")),
            vec![
                s_assign("__actual", e_call("gettype", vec![e_var("interval")])),
                s_if(
                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("boolean")),
                    vec![
                        s_assign("__actual", e_ternary(e_var("interval"), e_str("true"), e_str("false"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("__actual"), BinOp::StrictEq, e_str("integer")),
                        vec![
                            s_assign("__actual", e_str("int")),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("__actual"), BinOp::StrictEq, e_str("double")),
                            vec![
                                s_assign("__actual", e_str("float")),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("__actual"), BinOp::StrictEq, e_str("NULL")),
                                vec![
                                    s_assign("__actual", e_str("null")),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("object")),
                                    vec![
                                        s_assign("__actual", e_call("get_class", vec![e_var("interval")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("DateTimeImmutable::add(): Argument #1 ($interval) must be of type DateInterval, "), BinOp::Concat, e_var("__actual")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("__interval_result", e_call("__elephc_timelib_apply_interval", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_method_call(e_var("interval"), "__elephc_payload", vec![]), e_bool(false)])),
        s_if(
            e_index(e_var("__interval_result"), e_str("warning")),
            vec![
                s_throw(e_new("DateInvalidOperationException", vec![e_str("DateTimeImmutable::sub(): Only non-special relative time specifications are supported for subtraction")])),
            ],
            vec![],
            None,
        ),
        s_assign("__new", e_new("DateTimeImmutable", vec![])),
        s_prop_assign(e_var("__new"), "timestamp", e_index(e_var("__interval_result"), e_str("timestamp"))),
        s_prop_assign(e_var("__new"), "timezone_name", e_this_prop("timezone_name")),
        s_prop_assign(e_var("__new"), "microsecond", e_index(e_var("__interval_result"), e_str("microsecond"))),
        s_return(e_var("__new")),
    ])
}

/// `DateTimeImmutable::sub` — transcribed method builder.
fn decl_class_datetimeimmutable_method_14_sub() -> MethodBuilder {
method("sub")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::sub() does not modify the object itself"))])
    .param("interval", t_mixed())
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_not(e_instance_of(e_var("interval"), "DateInterval")),
            vec![
                s_assign("__actual", e_call("gettype", vec![e_var("interval")])),
                s_if(
                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("boolean")),
                    vec![
                        s_assign("__actual", e_ternary(e_var("interval"), e_str("true"), e_str("false"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("__actual"), BinOp::StrictEq, e_str("integer")),
                        vec![
                            s_assign("__actual", e_str("int")),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("__actual"), BinOp::StrictEq, e_str("double")),
                            vec![
                                s_assign("__actual", e_str("float")),
                            ],
                            vec![],
                            Some(vec![
                            s_if(
                                e_binop(e_var("__actual"), BinOp::StrictEq, e_str("NULL")),
                                vec![
                                    s_assign("__actual", e_str("null")),
                                ],
                                vec![],
                                Some(vec![
                                s_if(
                                    e_binop(e_var("__actual"), BinOp::StrictEq, e_str("object")),
                                    vec![
                                        s_assign("__actual", e_call("get_class", vec![e_var("interval")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            ),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("DateTimeImmutable::sub(): Argument #1 ($interval) must be of type DateInterval, "), BinOp::Concat, e_var("__actual")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("__interval_result", e_call("__elephc_timelib_apply_interval", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_method_call(e_var("interval"), "__elephc_payload", vec![]), e_bool(true)])),
        s_if(
            e_index(e_var("__interval_result"), e_str("warning")),
            vec![
                s_throw(e_new("DateInvalidOperationException", vec![e_str("DateTimeImmutable::sub(): Only non-special relative time specifications are supported for subtraction")])),
            ],
            vec![],
            None,
        ),
        s_assign("__new", e_new("DateTimeImmutable", vec![])),
        s_prop_assign(e_var("__new"), "timestamp", e_index(e_var("__interval_result"), e_str("timestamp"))),
        s_prop_assign(e_var("__new"), "timezone_name", e_this_prop("timezone_name")),
        s_prop_assign(e_var("__new"), "microsecond", e_index(e_var("__interval_result"), e_str("microsecond"))),
        s_return(e_var("__new")),
    ])
}

/// `DateTimeImmutable::modify` — transcribed method builder.
fn decl_class_datetimeimmutable_method_15_modify() -> MethodBuilder {
method("modify")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::modify() does not modify the object itself"))])
    .param("modifier", TypeExpr::Str)
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_if(
            e_binop(e_var("modifier"), BinOp::StrictEq, e_str("")),
            vec![
                s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str("DateTimeImmutable::modify(): "), e_var("modifier")])])),
            ],
            vec![],
            None,
        ),
        s_assign("__modified", e_call("__elephc_timelib_modify", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_this_prop("timezone_name")]), e_var("modifier")])),
        s_static_prop_assign("DateTime", "lastParseResult", e_index(e_var("__modified"), e_str("parse"))),
        s_if(
            e_binop(e_index(e_var("__modified"), e_str("status")), BinOp::StrictNotEq, e_str("O")),
            vec![
                s_throw(e_new("DateMalformedStringException", vec![e_static_call("DateTime", "__elephc_malformed_time_message", vec![e_str("DateTimeImmutable::modify(): "), e_var("modifier")])])),
            ],
            vec![],
            None,
        ),
        s_assign("__ts", e_index(e_var("__modified"), e_str("timestamp"))),
        s_assign("__micro", e_index(e_var("__modified"), e_str("microsecond"))),
        s_assign("__timezone", e_ternary(e_index(e_var("__modified"), e_str("reset_to_utc")), e_str("+00:00"), e_this_prop("timezone_name"))),
        s_assign("__new", e_new("DateTimeImmutable", vec![])),
        s_prop_assign(e_var("__new"), "timestamp", e_var("__ts")),
        s_prop_assign(e_var("__new"), "timezone_name", e_var("__timezone")),
        s_prop_assign(e_var("__new"), "microsecond", e_var("__micro")),
        s_return(e_var("__new")),
    ])
}

/// `DateTimeImmutable::createFromFormat` — transcribed method builder.
fn decl_class_datetimeimmutable_method_16_createfromformat() -> MethodBuilder {
method("createFromFormat")
    .static_()
    .param("format", TypeExpr::Str)
    .param("datetime", TypeExpr::Str)
    .param_default("timezone", t_nullable(t_class("DateTimeZone")), e_null())
    .returns(t_union(vec![t_class("DateTimeImmutable"), TypeExpr::False]))
    .body_exact(vec![
        s_if(
            e_call("str_contains", vec![e_var("format"), e_call("chr", vec![e_int(0)])]),
            vec![
                s_throw(e_new("ValueError", vec![e_str("DateTimeImmutable::createFromFormat(): Argument #1 ($format) must not contain any null bytes")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("str_contains", vec![e_var("datetime"), e_call("chr", vec![e_int(0)])]),
            vec![
                s_throw(e_new("ValueError", vec![e_str("DateTimeImmutable::createFromFormat(): Argument #2 ($datetime) must not contain any null bytes")])),
            ],
            vec![],
            None,
        ),
        s_assign("timezoneName", e_call("date_default_timezone_get", vec![])),
        s_if(
            e_binop(e_var("timezone"), BinOp::StrictNotEq, e_null()),
            vec![
                s_assign("timezoneName", e_method_call(e_var("timezone"), "getName", vec![])),
            ],
            vec![],
            None,
        ),
        s_assign("parsed", e_call("__elephc_timelib_create_from_format", vec![e_var("format"), e_var("datetime"), e_var("timezoneName")])),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("error_count")), BinOp::Gt, e_int(0)),
            vec![
                s_static_prop_assign("DateTime", "lastParseResult", e_index(e_var("parsed"), e_str("__elephc_serialized"))),
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("object", e_new("DateTimeImmutable", vec![])),
        s_assign("object", e_method_call(e_var("object"), "setTimestamp", vec![e_index(e_var("parsed"), e_str("__elephc_timestamp"))])),
        s_assign("microsecond", e_int(0)),
        s_if(
            e_binop(e_index(e_var("parsed"), e_str("fraction")), BinOp::StrictNotEq, e_bool(false)),
            vec![
                s_assign("microsecond", e_call("intval", vec![e_call("round", vec![e_binop(e_index(e_var("parsed"), e_str("fraction")), BinOp::Mul, e_float(1000000.0))])])),
            ],
            vec![],
            None,
        ),
        s_assign("object", e_method_call(e_var("object"), "setMicrosecond", vec![e_var("microsecond")])),
        s_if(
            e_index(e_var("parsed"), e_str("is_localtime")),
            vec![
                s_assign("zoneType", e_index(e_var("parsed"), e_str("zone_type"))),
                s_if(
                    e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(1)),
                    vec![
                        s_prop_assign(e_var("object"), "timezone_name", e_call("__elephc_timelib_offset_name", vec![e_index(e_var("parsed"), e_str("zone"))])),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_prop_assign(e_var("object"), "timezone_name", e_index(e_var("parsed"), e_str("tz_abbr"))),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(3)),
                            vec![
                                s_prop_assign(e_var("object"), "timezone_name", e_index(e_var("parsed"), e_str("tz_id"))),
                            ],
                            vec![],
                            Some(vec![
                            s_prop_assign(e_var("object"), "timezone_name", e_var("timezoneName")),
                        ]),
                        ),
                    ]),
                    ),
                ]),
                ),
            ],
            vec![],
            Some(vec![
            s_prop_assign(e_var("object"), "timezone_name", e_var("timezoneName")),
        ]),
        ),
        s_static_prop_assign("DateTime", "lastParseResult", e_index(e_var("parsed"), e_str("__elephc_serialized"))),
        s_if(
            e_binop(e_static_class(), BinOp::StrictEq, e_named_class("DateTimeImmutable")),
            vec![
                s_return(e_var("object")),
            ],
            vec![],
            None,
        ),
        s_assign("result", e_call("__elephc_new_instance_without_constructor", vec![e_static_class()])),
        s_expr(e_method_call(e_var("result"), "__unserialize", vec![e_method_call(e_var("object"), "__serialize", vec![])])),
        s_return(e_var("result")),
    ])
}

/// `DateTimeImmutable::getLastErrors` — transcribed method builder.
fn decl_class_datetimeimmutable_method_17_getlasterrors() -> MethodBuilder {
method("getLastErrors")
    .static_()
    .returns(t_union(vec![t_array(), TypeExpr::False]))
    .body_exact(vec![
        s_assign("lastResult", e_static_prop("DateTime", "lastParseResult")),
        s_if(
            e_binop(e_var("lastResult"), BinOp::StrictEq, e_str("")),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("parsed", e_ternary(e_call("is_array", vec![e_var("lastResult")]), e_var("lastResult"), e_call("__elephc_timelib_decode_parse_result", vec![e_var("lastResult")]))),
        s_if(
            e_binop(e_binop(e_index(e_var("parsed"), e_str("error_count")), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_index(e_var("parsed"), e_str("warning_count")), BinOp::StrictEq, e_int(0))),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_return(e_array_assoc(vec![(e_str("warning_count"), e_index(e_var("parsed"), e_str("warning_count"))), (e_str("warnings"), e_index(e_var("parsed"), e_str("warnings"))), (e_str("error_count"), e_index(e_var("parsed"), e_str("error_count"))), (e_str("errors"), e_index(e_var("parsed"), e_str("errors")))])),
    ])
}

/// `DateTimeImmutable::createFromTimestamp` — transcribed method builder.
fn decl_class_datetimeimmutable_method_18_createfromtimestamp() -> MethodBuilder {
method("createFromTimestamp")
    .static_()
    .param("timestamp", t_union(vec![TypeExpr::Int, TypeExpr::Float]))
    .returns(t_class("static"))
    .body_exact(vec![
        s_if(
            e_binop(e_call("is_float", vec![e_var("timestamp")]), BinOp::And, e_binop(e_binop(e_not(e_call("is_finite", vec![e_var("timestamp")])), BinOp::Or, e_binop(e_var("timestamp"), BinOp::Lt, e_neg(e_float(9.223372036854776e18)))), BinOp::Or, e_binop(e_var("timestamp"), BinOp::GtEq, e_float(9.223372036854776e18)))),
            vec![
                s_if(
                    e_call("is_nan", vec![e_var("timestamp")]),
                    vec![
                        s_assign("given", e_str("NAN")),
                    ],
                    vec![
                    (e_binop(e_var("timestamp"), BinOp::StrictEq, e_float(f64::INFINITY)), vec![
                        s_assign("given", e_str("INF")),
                    ]),
                    (e_binop(e_var("timestamp"), BinOp::StrictEq, e_neg(e_float(f64::INFINITY))), vec![
                        s_assign("given", e_str("-INF")),
                    ]),
                ],
                    Some(vec![
                    s_assign("given", e_call("sprintf", vec![e_str("%.6g"), e_var("timestamp")])),
                ]),
                ),
                s_throw(e_new("DateRangeError", vec![e_binop(e_binop(e_binop(e_binop(e_static_class(), BinOp::Concat, e_str("::createFromTimestamp(): Argument #1 ($timestamp) must be a finite number between ")), BinOp::Concat, e_str("-9223372036854775808 and 9223372036854775807.999999, ")), BinOp::Concat, e_var("given")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("secs", e_call("intval", vec![e_call("floor", vec![e_var("timestamp")])])),
        s_assign("microseconds", e_call("intval", vec![e_call("round", vec![e_binop(e_binop(e_var("timestamp"), BinOp::Sub, e_var("secs")), BinOp::Mul, e_int(1000000))])])),
        s_if(
            e_binop(e_var("microseconds"), BinOp::GtEq, e_int(1000000)),
            vec![
                s_assign("secs", e_binop(e_var("secs"), BinOp::Add, e_int(1))),
                s_assign("microseconds", e_binop(e_var("microseconds"), BinOp::Sub, e_int(1000000))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_static_class(), BinOp::StrictEq, e_named_class("DateTimeImmutable")),
            vec![
                s_assign("baseResult", e_new("DateTimeImmutable", vec![e_binop(e_str("@"), BinOp::Concat, e_var("secs"))])),
                s_prop_assign(e_var("baseResult"), "microsecond", e_var("microseconds")),
                s_return(e_var("baseResult")),
            ],
            vec![],
            None,
        ),
        s_assign("subclassResult", e_call("__elephc_new_instance_without_constructor", vec![e_static_class()])),
        s_expr(e_method_call(e_var("subclassResult"), "__unserialize", vec![e_array_assoc(vec![(e_str("date"), e_binop(e_binop(e_call("gmdate", vec![e_str("x-m-d H:i:s"), e_var("secs")]), BinOp::Concat, e_str(".")), BinOp::Concat, e_call("sprintf", vec![e_str("%06d"), e_var("microseconds")]))), (e_str("timezone_type"), e_int(1)), (e_str("timezone"), e_str("+00:00"))])])),
        s_return(e_var("subclassResult")),
    ])
}

/// `DateTimeImmutable::createFromInterface` — transcribed method builder.
fn decl_class_datetimeimmutable_method_19_createfrominterface() -> MethodBuilder {
method("createFromInterface")
    .static_()
    .param("object", t_class("DateTimeInterface"))
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_assign("className", e_static_class()),
        s_assign("timezone", e_method_call(e_var("object"), "format", vec![e_str("e")])),
        s_assign("data", e_array_assoc(vec![(e_str("date"), e_method_call(e_var("object"), "format", vec![e_str("x-m-d H:i:s.u")])), (e_str("timezone_type"), e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("timezone")])), (e_str("timezone"), e_var("timezone"))])),
        s_if(
            e_binop(e_var("className"), BinOp::StrictEq, e_named_class("DateTimeImmutable")),
            vec![
                s_assign("baseResult", e_new("DateTimeImmutable", vec![])),
                s_expr(e_method_call(e_var("baseResult"), "__unserialize", vec![e_var("data")])),
                s_return(e_var("baseResult")),
            ],
            vec![],
            None,
        ),
        s_assign("subclassResult", e_call("__elephc_new_instance_without_constructor", vec![e_var("className")])),
        s_expr(e_method_call(e_var("subclassResult"), "__unserialize", vec![e_var("data")])),
        s_return(e_var("subclassResult")),
    ])
}

/// `DateTimeImmutable::createFromMutable` — transcribed method builder.
fn decl_class_datetimeimmutable_method_20_createfrommutable() -> MethodBuilder {
method("createFromMutable")
    .static_()
    .param("object", t_class("DateTime"))
    .returns(t_class("static"))
    .body_exact(vec![
        s_assign("actualClass", e_object_class_name(e_var("object"))),
        s_if(
            e_not(e_instance_of(e_var("object"), "DateTime")),
            vec![
                s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("DateTimeImmutable::createFromMutable(): Argument #1 ($object) must be of type DateTime, "), BinOp::Concat, e_var("actualClass")), BinOp::Concat, e_str(" given"))])),
            ],
            vec![],
            None,
        ),
        s_assign("className", e_static_class()),
        s_assign("timezone", e_method_call(e_var("object"), "format", vec![e_str("e")])),
        s_assign("data", e_array_assoc(vec![(e_str("date"), e_method_call(e_var("object"), "format", vec![e_str("x-m-d H:i:s.u")])), (e_str("timezone_type"), e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("timezone")])), (e_str("timezone"), e_var("timezone"))])),
        s_if(
            e_binop(e_var("className"), BinOp::StrictEq, e_named_class("DateTimeImmutable")),
            vec![
                s_assign("baseResult", e_new("DateTimeImmutable", vec![])),
                s_expr(e_method_call(e_var("baseResult"), "__unserialize", vec![e_var("data")])),
                s_return(e_var("baseResult")),
            ],
            vec![],
            None,
        ),
        s_assign("subclassResult", e_call("__elephc_new_instance_without_constructor", vec![e_var("className")])),
        s_expr(e_method_call(e_var("subclassResult"), "__unserialize", vec![e_var("data")])),
        s_return(e_var("subclassResult")),
    ])
}

/// `DateTimeImmutable::setISODate` — transcribed method builder.
fn decl_class_datetimeimmutable_method_21_setisodate() -> MethodBuilder {
method("setISODate")
    .attr("\\NoDiscard", vec![e_named_arg("message", e_str("as DateTimeImmutable::setISODate() does not modify the object itself"))])
    .param("year", TypeExpr::Int)
    .param("week", TypeExpr::Int)
    .param_default("dayOfWeek", TypeExpr::Int, e_int(1))
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("parsed", e_call("__elephc_timelib_set_iso_date", vec![e_this_prop("timestamp"), e_this_prop("microsecond"), e_this_prop("timezone_name"), e_var("year"), e_var("week"), e_var("dayOfWeek")])),
        s_assign("timestamp", e_index(e_var("parsed"), e_str("timestamp"))),
        s_assign("microsecond", e_index(e_var("parsed"), e_str("microsecond"))),
        s_assign("civilYear", e_index(e_var("parsed"), e_str("year"))),
        s_assign("civilMonth", e_index(e_var("parsed"), e_str("month"))),
        s_assign("civilDay", e_index(e_var("parsed"), e_str("day"))),
        s_assign("result", e_new("DateTimeImmutable", vec![])),
        s_prop_assign(e_var("result"), "timestamp", e_var("timestamp")),
        s_prop_assign(e_var("result"), "timezone_name", e_this_prop("timezone_name")),
        s_prop_assign(e_var("result"), "microsecond", e_var("microsecond")),
        s_prop_assign(e_var("result"), "__elephc_civil_override", e_bool(true)),
        s_prop_assign(e_var("result"), "__elephc_civil_year", e_var("civilYear")),
        s_prop_assign(e_var("result"), "__elephc_civil_month", e_var("civilMonth")),
        s_prop_assign(e_var("result"), "__elephc_civil_day", e_var("civilDay")),
        s_return(e_var("result")),
    ])
}

/// `DateTimeImmutable::__elephc_date_create` — transcribed method builder.
fn decl_class_datetimeimmutable_method_22_elephc_date_create() -> MethodBuilder {
method("__elephc_date_create")
    .static_()
    .param_default("datetime", TypeExpr::Str, e_str("now"))
    .param_default("timezone", t_nullable(t_class("DateTimeZone")), e_null())
    .returns(t_mixed())
    .body_exact(vec![
        s_try(vec![
            s_if(
                e_binop(e_var("timezone"), BinOp::StrictEq, e_null()),
                vec![
                    s_return(e_new("DateTimeImmutable", vec![e_var("datetime")])),
                ],
                vec![],
                None,
            ),
            s_try(vec![
                s_expr(e_method_call(e_var("timezone"), "__elephc_assert_initialized", vec![])),
            ], vec![
                (vec!["\\DateObjectError"], Some("e"), vec![
                    s_throw(e_new_fq("Error", vec![e_str("The DateTimeZone object has not been correctly initialized by its constructor")])),
                ]),
            ], None),
            s_return(e_new("DateTimeImmutable", vec![e_var("datetime"), e_var("timezone")])),
        ], vec![
            (vec!["\\DateMalformedStringException"], Some("e"), vec![
                s_return(e_bool(false)),
            ]),
        ], None),
    ])
}

/// `DateTimeImmutable::__wakeup` — transcribed method builder.
fn decl_class_datetimeimmutable_method_23_wakeup() -> MethodBuilder {
method("__wakeup")
    .attr("\\Deprecated", vec![e_named_arg("since", e_str("8.5")), e_named_arg("message", e_str("this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()"))])
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_str("Deprecated: Method DateTimeImmutable::__wakeup() is deprecated since 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()\n"), e_int(0), e_const("E_DEPRECATED")])),
        s_if(
            e_binop(e_str("DateTimeImmutable"), BinOp::StrictNotEq, e_str("DateInterval")),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeImmutable object")])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateTimeImmutable::__serialize` — transcribed method builder.
fn decl_class_datetimeimmutable_method_24_serialize() -> MethodBuilder {
method("__serialize")
    .returns(t_array())
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__tz", e_cast(CastType::String, e_this_prop("timezone_name"))),
        s_assign("__saved", e_call("date_default_timezone_get", vec![])),
        s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("__tz")])])),
        s_assign("__date", e_call("date", vec![e_str("x-m-d H:i:s"), e_this_prop("timestamp")])),
        s_assign("__us", e_call("str_pad", vec![e_cast(CastType::String, e_this_prop("microsecond")), e_int(6), e_str("0"), e_int(1)])),
        s_assign("__date", e_binop(e_binop(e_var("__date"), BinOp::Concat, e_str(".")), BinOp::Concat, e_var("__us"))),
        s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
        s_return(e_array_assoc(vec![(e_str("date"), e_var("__date")), (e_str("timezone_type"), e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("__tz")])), (e_str("timezone"), e_var("__tz"))])),
    ])
}

/// `DateTimeImmutable::__unserialize` — transcribed method builder.
fn decl_class_datetimeimmutable_method_25_unserialize() -> MethodBuilder {
method("__unserialize")
    .param("data", t_array())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_binop(e_binop(e_not(e_call("array_key_exists", vec![e_str("date"), e_var("data")])), BinOp::Or, e_not(e_call("array_key_exists", vec![e_str("timezone_type"), e_var("data")]))), BinOp::Or, e_not(e_call("array_key_exists", vec![e_str("timezone"), e_var("data")]))), BinOp::Or, e_not(e_call("is_string", vec![e_index(e_var("data"), e_str("date"))]))), BinOp::Or, e_not(e_call("is_int", vec![e_index(e_var("data"), e_str("timezone_type"))]))), BinOp::Or, e_not(e_call("is_string", vec![e_index(e_var("data"), e_str("timezone"))]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeImmutable object")])),
            ],
            vec![],
            None,
        ),
        s_assign("__date", e_index(e_var("data"), e_str("date"))),
        s_assign("__tz", e_index(e_var("data"), e_str("timezone"))),
        s_assign("__tzType", e_index(e_var("data"), e_str("timezone_type"))),
        s_assign("__normalizedTz", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_var("__tz")])),
        s_if(
            e_binop(e_binop(e_var("__normalizedTz"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_var("__tzType"), BinOp::StrictNotEq, e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("__normalizedTz")]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeImmutable object")])),
            ],
            vec![],
            None,
        ),
        s_assign("__tz", e_var("__normalizedTz")),
        s_prop_assign(e_this(), "microsecond", e_static_call("DateTime", "__elephc_extract_micros", vec![e_var("__date")])),
        s_assign("__dateWithoutMicros", e_static_call("DateTime", "__elephc_strip_micros", vec![e_var("__date")])),
        s_assign("__saved", e_call("date_default_timezone_get", vec![])),
        s_if(
            e_binop(e_var("__tzType"), BinOp::StrictEq, e_int(1)),
            vec![
                s_expr(e_call("date_default_timezone_set", vec![e_str("UTC")])),
                s_assign("__timestamp", e_call("strtotime", vec![e_var("__dateWithoutMicros")])),
                s_assign("__offsetSeconds", e_binop(e_binop(e_call("intval", vec![e_call("substr", vec![e_var("__tz"), e_int(1), e_int(2)])]), BinOp::Mul, e_int(3600)), BinOp::Add, e_binop(e_call("intval", vec![e_call("substr", vec![e_var("__tz"), e_int(4), e_int(2)])]), BinOp::Mul, e_int(60)))),
                s_if(
                    e_binop(e_call("strlen", vec![e_var("__tz")]), BinOp::StrictEq, e_int(9)),
                    vec![
                        s_assign("__offsetSeconds", e_binop(e_var("__offsetSeconds"), BinOp::Add, e_call("intval", vec![e_call("substr", vec![e_var("__tz"), e_int(7), e_int(2)])]))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_index(e_var("__tz"), e_int(0)), BinOp::StrictEq, e_str("-")),
                    vec![
                        s_assign("__offsetSeconds", e_neg(e_var("__offsetSeconds"))),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("__timestamp"), BinOp::StrictNotEq, e_bool(false)),
                    vec![
                        s_assign("__timestamp", e_binop(e_var("__timestamp"), BinOp::Sub, e_var("__offsetSeconds"))),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            Some(vec![
            s_if(
                e_not(e_error_suppress(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_var("__tz")])]))),
                vec![
                    s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
                    s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeImmutable object")])),
                ],
                vec![],
                None,
            ),
            s_assign("__timestamp", e_call("strtotime", vec![e_var("__dateWithoutMicros")])),
        ]),
        ),
        s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
        s_if(
            e_binop(e_var("__timestamp"), BinOp::StrictEq, e_bool(false)),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeImmutable object")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "timestamp", e_var("__timestamp")),
        s_prop_assign(e_this(), "timezone_name", e_var("__tz")),
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
    ])
}

/// `DateTimeImmutable::__set_state` — transcribed method builder.
fn decl_class_datetimeimmutable_method_26_set_state() -> MethodBuilder {
method("__set_state")
    .static_()
    .param("array", t_array())
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_assign("__d", e_new("DateTimeImmutable", vec![])),
        s_expr(e_method_call(e_var("__d"), "__unserialize", vec![e_var("array")])),
        s_return(e_var("__d")),
    ])
}

/// `DateTimeImmutable::__elephc_debug_dump` — transcribed method builder.
fn decl_class_datetimeimmutable_method_27_elephc_debug_dump() -> MethodBuilder {
method("__elephc_debug_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_call("__elephc_var_dump_indent", vec![e_int(0)])])),
        s_assign("field_pad", e_binop(e_var("pad"), BinOp::Concat, e_str("  "))),
        s_assign("property_count", e_call("__elephc_var_dump_object_property_count", vec![e_this()])),
        s_echo(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("pad"), BinOp::Concat, e_str("object(")), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(")#")), BinOp::Concat, e_call("spl_object_id", vec![e_this()])), BinOp::Concat, e_str(" (")), BinOp::Concat, e_binop(e_var("property_count"), BinOp::Add, e_int(3))), BinOp::Concat, e_str(") {\n"))),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
        s_expr(e_call("__elephc_var_dump_object_properties", vec![e_this()])),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"date\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_method_call(e_this(), "format", vec![e_str("x-m-d H:i:s.u")])])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"timezone_type\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("timezone_name")])])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"timezone\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("timezone_name")])),
        s_echo(e_binop(e_var("pad"), BinOp::Concat, e_str("}\n"))),
    ])
}

/// `DateTimeImmutable::__elephc_print_r_dump` — transcribed method builder.
fn decl_class_datetimeimmutable_method_28_elephc_print_r_dump() -> MethodBuilder {
method("__elephc_print_r_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_echo(e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str(" Object\n(\n"))),
        s_expr(e_call("__elephc_print_r_object_properties", vec![e_this()])),
        s_echo(e_binop(e_binop(e_str("    [date] => "), BinOp::Concat, e_method_call(e_this(), "format", vec![e_str("x-m-d H:i:s.u")])), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [timezone_type] => "), BinOp::Concat, e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("timezone_name")])), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [timezone] => "), BinOp::Concat, e_this_prop("timezone_name")), BinOp::Concat, e_str("\n"))),
        s_echo(e_str(")\n")),
    ])
}

/// `DateTimeImmutable::__elephc_clone_for_period` — transcribed method builder.
fn decl_class_datetimeimmutable_method_29_elephc_clone_for_period() -> MethodBuilder {
method("__elephc_clone_for_period")
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_clone(e_this())),
    ])
}

/// `DateTimeImmutable::__elephc_clone_for_period_storage` — transcribed method builder.
fn decl_class_datetimeimmutable_method_30_elephc_clone_for_period_storage() -> MethodBuilder {
method("__elephc_clone_for_period_storage")
    .returns(t_class("DateTimeImmutable"))
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_call("__elephc_object_clone_internal", vec![e_this()])),
    ])
}

/// `DateTimeImmutable::__elephc_begin_argument_array` — transcribed method builder.
fn decl_class_datetimeimmutable_method_31_elephc_begin_argument_array() -> MethodBuilder {
method("__elephc_begin_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_arguments", e_array(vec![])),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DateTimeImmutable::__elephc_append_one_argument` — transcribed method builder.
fn decl_class_datetimeimmutable_method_32_elephc_append_one_argument() -> MethodBuilder {
method("__elephc_append_one_argument")
    .private()
    .param("key", t_mixed())
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_if(
            e_call("is_int", vec![e_var("key")]),
            vec![
                s_if(
                    e_this_prop("__elephc_seen_named_argument"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Cannot use positional argument after named argument during unpacking")])),
                    ],
                    vec![],
                    None,
                ),
                s_array_push("arguments", e_var("value")),
                s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_string", vec![e_var("key")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Keys must be of type int|string during argument unpacking")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(true)),
        s_if(
            e_not(e_binop(e_binop(e_var("key"), BinOp::StrictEq, e_str("datetime")), BinOp::Or, e_binop(e_var("key"), BinOp::StrictEq, e_str("timezone")))),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_str("Unknown named parameter $"), BinOp::Concat, e_var("key"))])),
            ],
            vec![],
            None,
        ),
        s_assign("parameterIndex", e_neg(e_int(1))),
        s_if(
            e_binop(e_var("key"), BinOp::StrictEq, e_str("datetime")),
            vec![
                s_assign("parameterIndex", e_int(0)),
            ],
            vec![
            (e_binop(e_var("key"), BinOp::StrictEq, e_str("timezone")), vec![
                s_assign("parameterIndex", e_int(1)),
            ]),
        ],
            None,
        ),
        s_assign("positionalCount", e_int(0)),
        s_foreach(e_var("arguments"), Some("existingKey"), "existingValue", vec![
            s_if(
                e_call("is_int", vec![e_var("existingKey")]),
                vec![
                    s_expr(e_post_inc("positionalCount")),
                ],
                vec![],
                None,
            ),
        ]),
        s_if(
            e_binop(e_var("parameterIndex"), BinOp::Lt, e_var("positionalCount")),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("array_key_exists", vec![e_var("key"), e_var("arguments")]),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_array_assign("arguments", e_var("key"), e_var("value")),
        s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
    ])
}

/// `DateTimeImmutable::__elephc_append_argument_chunk` — transcribed method builder.
fn decl_class_datetimeimmutable_method_33_elephc_append_argument_chunk() -> MethodBuilder {
method("__elephc_append_argument_chunk")
    .private()
    .param("kind", TypeExpr::Int)
    .param("name", TypeExpr::Str)
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
            vec![
                s_if(
                    e_not(e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_instance_of(e_var("value"), "Traversable"))),
                    vec![
                        s_expr(e_static_call("DateTime", "__elephc_argument_type_error", vec![e_var("value"), e_str("Only arrays and Traversables can be unpacked, ")])),
                    ],
                    vec![],
                    None,
                ),
                s_foreach(e_var("value"), Some("key"), "unpackedValue", vec![
                    s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("key"), e_var("unpackedValue")])),
                ]),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
            vec![
                s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("name"), e_var("value")])),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_int(0), e_var("value")])),
    ])
}

/// `DateTimeImmutable::__elephc_finish_argument_array` — transcribed method builder.
fn decl_class_datetimeimmutable_method_34_elephc_finish_argument_array() -> MethodBuilder {
method("__elephc_finish_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_assign("datetime", e_str("now")),
        s_assign("timezone", e_null()),
        s_assign("hasDatetime", e_bool(false)),
        s_assign("hasTimezone", e_bool(false)),
        s_assign("nextPosition", e_int(0)),
        s_foreach(e_var("arguments"), Some("key"), "value", vec![
            s_if(
                e_call("is_int", vec![e_var("key")]),
                vec![
                    s_if(
                        e_binop(e_var("nextPosition"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_assign("datetime", e_var("value")),
                            s_assign("hasDatetime", e_bool(true)),
                        ],
                        vec![
                        (e_binop(e_var("nextPosition"), BinOp::StrictEq, e_int(1)), vec![
                            s_assign("timezone", e_var("value")),
                            s_assign("hasTimezone", e_bool(true)),
                        ]),
                    ],
                        Some(vec![
                        s_throw(e_new("ArgumentCountError", vec![e_binop(e_binop(e_str("DateTimeImmutable::__construct() expects at most 2 arguments, "), BinOp::Concat, e_call("count", vec![e_var("arguments")])), BinOp::Concat, e_str(" given"))])),
                    ]),
                    ),
                    s_expr(e_post_inc("nextPosition")),
                ],
                vec![
                (e_binop(e_var("key"), BinOp::StrictEq, e_str("datetime")), vec![
                    s_if(
                        e_var("hasDatetime"),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Named parameter $datetime overwrites previous argument")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("datetime", e_var("value")),
                    s_assign("hasDatetime", e_bool(true)),
                ]),
            ],
                Some(vec![
                s_if(
                    e_var("hasTimezone"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Named parameter $timezone overwrites previous argument")])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("timezone", e_var("value")),
                s_assign("hasTimezone", e_bool(true)),
            ]),
            ),
        ]),
        s_assign("datetime", e_static_call("DateTime", "__elephc_weak_string_argument", vec![e_var("datetime"), e_str("DateTimeImmutable::__construct(): Argument #1 ($datetime) must be of type string, "), e_str("")])),
        s_if(
            e_binop(e_not(e_call("is_null", vec![e_var("timezone")])), BinOp::And, e_not(e_instance_of(e_var("timezone"), "DateTimeZone"))),
            vec![
                s_expr(e_static_call("DateTime", "__elephc_argument_type_error", vec![e_var("timezone"), e_str("DateTimeImmutable::__construct(): Argument #2 ($timezone) must be of type ?DateTimeZone, ")])),
            ],
            vec![],
            None,
        ),
        s_expr(e_method_call(e_this(), "__construct", vec![e_var("datetime"), e_var("timezone")])),
        s_prop_assign(e_this(), "__elephc_arguments", e_null()),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DateTimeImmutable::__elephc_is_initialized` — transcribed method builder.
fn decl_class_datetimeimmutable_method_35_elephc_is_initialized() -> MethodBuilder {
method("__elephc_is_initialized")
    .final_()
    .returns(TypeExpr::Bool)
    .body_exact(vec![
        s_return(e_this_prop("__elephc_initialized")),
    ])
}

/// `DateTimeImmutable::__elephc_assert_initialized` — transcribed method builder.
fn decl_class_datetimeimmutable_method_36_elephc_assert_initialized() -> MethodBuilder {
method("__elephc_assert_initialized")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_assign("objectClass", e_call("get_class", vec![e_this()])),
                s_assign("inheritance", e_ternary(e_binop(e_var("objectClass"), BinOp::StrictEq, e_str("DateTimeImmutable")), e_str(""), e_str(" (inheriting DateTimeImmutable)"))),
                s_throw(e_new("DateObjectError", vec![e_binop(e_binop(e_binop(e_str("Object of type "), BinOp::Concat, e_var("objectClass")), BinOp::Concat, e_var("inheritance")), BinOp::Concat, e_str(" has not been correctly initialized by calling parent::__construct() in its constructor"))])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateTimeImmutable::__elephc_assert_comparable` — transcribed method builder.
fn decl_class_datetimeimmutable_method_37_elephc_assert_comparable() -> MethodBuilder {
method("__elephc_assert_comparable")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Trying to compare an incomplete DateTime or DateTimeImmutable object")])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateTimeImmutable::__elephc_compare` — transcribed method builder.
fn decl_class_datetimeimmutable_method_38_elephc_compare() -> MethodBuilder {
method("__elephc_compare")
    .final_()
    .param("other", t_class("DateTimeInterface"))
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_comparable", vec![])),
        s_expr(e_method_call(e_var("other"), "__elephc_assert_comparable", vec![])),
        s_assign("leftTimestamp", e_method_call(e_this(), "getTimestamp", vec![])),
        s_assign("rightTimestamp", e_method_call(e_var("other"), "getTimestamp", vec![])),
        s_if(
            e_binop(e_var("leftTimestamp"), BinOp::Lt, e_var("rightTimestamp")),
            vec![
                s_return(e_neg(e_int(1))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("leftTimestamp"), BinOp::Gt, e_var("rightTimestamp")),
            vec![
                s_return(e_int(1)),
            ],
            vec![],
            None,
        ),
        s_assign("leftMicrosecond", e_method_call(e_this(), "getMicrosecond", vec![])),
        s_assign("rightMicrosecond", e_method_call(e_var("other"), "getMicrosecond", vec![])),
        s_if(
            e_binop(e_var("leftMicrosecond"), BinOp::Lt, e_var("rightMicrosecond")),
            vec![
                s_return(e_neg(e_int(1))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("leftMicrosecond"), BinOp::Gt, e_var("rightMicrosecond")),
            vec![
                s_return(e_int(1)),
            ],
            vec![],
            None,
        ),
        s_return(e_int(0)),
    ])
}

/// `DateTimeImmutable` — transcribed from the PHP form.
fn decl_class_datetimeimmutable() -> Stmt {
    class("DateTimeImmutable")
        .implements("DateTimeInterface")
        .private_prop("__elephc_initialized", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("timestamp", TypeExpr::Int, Some(e_int(0)))
        .private_prop("timezone_name", TypeExpr::Str, Some(e_str("UTC")))
        .private_prop("microsecond", TypeExpr::Int, Some(e_int(0)))
        .private_prop("__elephc_civil_override", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("__elephc_civil_year", TypeExpr::Int, Some(e_int(1970)))
        .private_prop("__elephc_civil_month", TypeExpr::Int, Some(e_int(1)))
        .private_prop("__elephc_civil_day", TypeExpr::Int, Some(e_int(1)))
        .static_prop("lastErrorCount", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastErrorPosition", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastErrorMessage", TypeExpr::Str, Some(e_str("")))
        .static_prop("lastWarningCount", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastWarningPosition", TypeExpr::Int, Some(e_int(0)))
        .static_prop("lastWarningMessage", TypeExpr::Str, Some(e_str("")))
        .static_prop("lastParseResult", t_mixed(), Some(e_str("")))
        .private_prop("__elephc_arguments", t_mixed(), Some(e_null()))
        .private_prop("__elephc_seen_named_argument", TypeExpr::Bool, Some(e_bool(false)))
        .method(decl_class_datetimeimmutable_method_0_construct())
        .method(decl_class_datetimeimmutable_method_1_gettimestamp())
        .method(decl_class_datetimeimmutable_method_2_getmicrosecond())
        .method(decl_class_datetimeimmutable_method_3_elephc_set_microsecond_raw())
        .method(decl_class_datetimeimmutable_method_4_gettimezone())
        .method(decl_class_datetimeimmutable_method_5_format())
        .method(decl_class_datetimeimmutable_method_6_getoffset())
        .method(decl_class_datetimeimmutable_method_7_diff())
        .method(decl_class_datetimeimmutable_method_8_settimestamp())
        .method(decl_class_datetimeimmutable_method_9_setmicrosecond())
        .method(decl_class_datetimeimmutable_method_10_settime())
        .method(decl_class_datetimeimmutable_method_11_setdate())
        .method(decl_class_datetimeimmutable_method_12_settimezone())
        .method(decl_class_datetimeimmutable_method_13_add())
        .method(decl_class_datetimeimmutable_method_14_sub())
        .method(decl_class_datetimeimmutable_method_15_modify())
        .method(decl_class_datetimeimmutable_method_16_createfromformat())
        .method(decl_class_datetimeimmutable_method_17_getlasterrors())
        .method(decl_class_datetimeimmutable_method_18_createfromtimestamp())
        .method(decl_class_datetimeimmutable_method_19_createfrominterface())
        .method(decl_class_datetimeimmutable_method_20_createfrommutable())
        .method(decl_class_datetimeimmutable_method_21_setisodate())
        .method(decl_class_datetimeimmutable_method_22_elephc_date_create())
        .method(decl_class_datetimeimmutable_method_23_wakeup())
        .method(decl_class_datetimeimmutable_method_24_serialize())
        .method(decl_class_datetimeimmutable_method_25_unserialize())
        .method(decl_class_datetimeimmutable_method_26_set_state())
        .method(decl_class_datetimeimmutable_method_27_elephc_debug_dump())
        .method(decl_class_datetimeimmutable_method_28_elephc_print_r_dump())
        .method(decl_class_datetimeimmutable_method_29_elephc_clone_for_period())
        .method(decl_class_datetimeimmutable_method_30_elephc_clone_for_period_storage())
        .method(decl_class_datetimeimmutable_method_31_elephc_begin_argument_array())
        .method(decl_class_datetimeimmutable_method_32_elephc_append_one_argument())
        .method(decl_class_datetimeimmutable_method_33_elephc_append_argument_chunk())
        .method(decl_class_datetimeimmutable_method_34_elephc_finish_argument_array())
        .method(decl_class_datetimeimmutable_method_35_elephc_is_initialized())
        .method(decl_class_datetimeimmutable_method_36_elephc_assert_initialized())
        .method(decl_class_datetimeimmutable_method_37_elephc_assert_comparable())
        .method(decl_class_datetimeimmutable_method_38_elephc_compare())
        .build()
}

/// `DateTimeZone::__elephc_normalize_timezone` — transcribed method builder.
fn decl_class_datetimezone_method_0_elephc_normalize_timezone() -> MethodBuilder {
method("__elephc_normalize_timezone")
    .static_()
    .param("timezone", TypeExpr::Str)
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_call("strtoupper", vec![e_var("timezone")]), BinOp::StrictEq, e_str("UTC")), BinOp::Or, e_binop(e_call("strtoupper", vec![e_var("timezone")]), BinOp::StrictEq, e_str("GMT"))),
            vec![
                s_return(e_call("strtoupper", vec![e_var("timezone")])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_binop(e_call("strlen", vec![e_var("timezone")]), BinOp::GtEq, e_int(5)), BinOp::And, e_binop(e_call("substr", vec![e_var("timezone"), e_int(0), e_int(3)]), BinOp::StrictEq, e_str("GMT"))), BinOp::And, e_binop(e_binop(e_index(e_var("timezone"), e_int(3)), BinOp::StrictEq, e_str("+")), BinOp::Or, e_binop(e_index(e_var("timezone"), e_int(3)), BinOp::StrictEq, e_str("-")))),
            vec![
                s_assign("timezone", e_call("substr", vec![e_var("timezone"), e_int(3)])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_binop(e_call("strlen", vec![e_var("timezone")]), BinOp::GtEq, e_int(2)), BinOp::And, e_binop(e_binop(e_index(e_var("timezone"), e_int(0)), BinOp::StrictEq, e_str("+")), BinOp::Or, e_binop(e_index(e_var("timezone"), e_int(0)), BinOp::StrictEq, e_str("-")))),
            vec![
                s_assign("len", e_call("strlen", vec![e_var("timezone")])),
                s_assign("hours", e_int(0)),
                s_assign("minutes", e_int(0)),
                s_assign("seconds", e_int(0)),
                s_assign("ok", e_bool(false)),
                s_assign("digits", e_call("substr", vec![e_var("timezone"), e_int(1)])),
                s_if(
                    e_binop(e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(2)), BinOp::Or, e_binop(e_var("len"), BinOp::StrictEq, e_int(3))), BinOp::And, e_call("ctype_digit", vec![e_var("digits")])),
                    vec![
                        s_assign("hours", e_call("intval", vec![e_var("digits")])),
                        s_assign("ok", e_bool(true)),
                    ],
                    vec![
                    (e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(4)), BinOp::And, e_call("ctype_digit", vec![e_var("digits")])), vec![
                        s_assign("hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(1)])])),
                        s_assign("minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(2), e_int(2)])])),
                        s_assign("ok", e_bool(true)),
                    ]),
                    (e_binop(e_binop(e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(4)), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(2)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(1))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(3))])), vec![
                        s_assign("hours", e_call("intval", vec![e_index(e_var("timezone"), e_int(1))])),
                        s_assign("minutes", e_call("intval", vec![e_index(e_var("timezone"), e_int(3))])),
                        s_assign("ok", e_bool(true)),
                    ]),
                    (e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(5)), BinOp::And, e_call("ctype_digit", vec![e_var("digits")])), vec![
                        s_assign("hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                        s_assign("minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(3), e_int(2)])])),
                        s_assign("ok", e_bool(true)),
                    ]),
                    (e_binop(e_binop(e_binop(e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(5)), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(2)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(1))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(3))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(4))])), vec![
                        s_assign("hours", e_call("intval", vec![e_index(e_var("timezone"), e_int(1))])),
                        s_assign("minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(3), e_int(2)])])),
                        s_assign("ok", e_bool(true)),
                    ]),
                    (e_binop(e_binop(e_binop(e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(5)), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(3)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(1))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(2))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(4))])), vec![
                        s_assign("hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                        s_assign("minutes", e_call("intval", vec![e_index(e_var("timezone"), e_int(4))])),
                        s_assign("ok", e_bool(true)),
                    ]),
                    (e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(6)), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(3)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(1))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(2))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(4))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(5))])), vec![
                        s_assign("hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                        s_assign("minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(4), e_int(2)])])),
                        s_assign("ok", e_bool(true)),
                    ]),
                    (e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(7)), BinOp::And, e_call("ctype_digit", vec![e_var("digits")])), vec![
                        s_assign("hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                        s_assign("minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(3), e_int(2)])])),
                        s_assign("seconds", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(5), e_int(2)])])),
                        s_assign("ok", e_bool(true)),
                    ]),
                    (e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("len"), BinOp::StrictEq, e_int(9)), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(3)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(6)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(1))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(2))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(4))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(5))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(7))])), BinOp::And, e_call("ctype_digit", vec![e_index(e_var("timezone"), e_int(8))])), vec![
                        s_assign("hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                        s_assign("minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(4), e_int(2)])])),
                        s_assign("seconds", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(7), e_int(2)])])),
                        s_assign("ok", e_bool(true)),
                    ]),
                ],
                    None,
                ),
                s_if(
                    e_var("ok"),
                    vec![
                        s_assign("total", e_binop(e_binop(e_binop(e_var("hours"), BinOp::Mul, e_int(3600)), BinOp::Add, e_binop(e_var("minutes"), BinOp::Mul, e_int(60))), BinOp::Add, e_var("seconds"))),
                        s_assign("hours", e_call("intdiv", vec![e_var("total"), e_int(3600)])),
                        s_assign("remaining", e_binop(e_var("total"), BinOp::Mod, e_int(3600))),
                        s_assign("minutes", e_call("intdiv", vec![e_var("remaining"), e_int(60)])),
                        s_assign("seconds", e_binop(e_var("remaining"), BinOp::Mod, e_int(60))),
                        s_if(
                            e_binop(e_var("hours"), BinOp::GtEq, e_int(100)),
                            vec![
                                s_return(e_str("")),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("sign", e_ternary(e_binop(e_var("total"), BinOp::StrictEq, e_int(0)), e_str("+"), e_index(e_var("timezone"), e_int(0)))),
                        s_assign("hh", e_binop(e_ternary(e_binop(e_var("hours"), BinOp::Lt, e_int(10)), e_str("0"), e_str("")), BinOp::Concat, e_cast(CastType::String, e_var("hours")))),
                        s_assign("mm", e_binop(e_ternary(e_binop(e_var("minutes"), BinOp::Lt, e_int(10)), e_str("0"), e_str("")), BinOp::Concat, e_cast(CastType::String, e_var("minutes")))),
                        s_assign("name", e_binop(e_binop(e_binop(e_var("sign"), BinOp::Concat, e_var("hh")), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("mm"))),
                        s_if(
                            e_binop(e_var("seconds"), BinOp::StrictNotEq, e_int(0)),
                            vec![
                                s_assign("ss", e_binop(e_ternary(e_binop(e_var("seconds"), BinOp::Lt, e_int(10)), e_str("0"), e_str("")), BinOp::Concat, e_cast(CastType::String, e_var("seconds")))),
                                s_assign("name", e_binop(e_binop(e_var("name"), BinOp::Concat, e_str(":")), BinOp::Concat, e_var("ss"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_var("name")),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("in_array", vec![e_call("strtolower", vec![e_var("timezone")]), e_array(vec![e_str("africa/abidjan"), e_str("africa/accra"), e_str("africa/addis_ababa"), e_str("africa/algiers"), e_str("africa/asmara"), e_str("africa/asmera"), e_str("africa/bamako"), e_str("africa/bangui"), e_str("africa/banjul"), e_str("africa/bissau"), e_str("africa/blantyre"), e_str("africa/brazzaville"), e_str("africa/bujumbura"), e_str("africa/cairo"), e_str("africa/casablanca"), e_str("africa/ceuta"), e_str("africa/conakry"), e_str("africa/dakar"), e_str("africa/dar_es_salaam"), e_str("africa/djibouti"), e_str("africa/douala"), e_str("africa/el_aaiun"), e_str("africa/freetown"), e_str("africa/gaborone"), e_str("africa/harare"), e_str("africa/johannesburg"), e_str("africa/juba"), e_str("africa/kampala"), e_str("africa/khartoum"), e_str("africa/kigali"), e_str("africa/kinshasa"), e_str("africa/lagos"), e_str("africa/libreville"), e_str("africa/lome"), e_str("africa/luanda"), e_str("africa/lubumbashi"), e_str("africa/lusaka"), e_str("africa/malabo"), e_str("africa/maputo"), e_str("africa/maseru"), e_str("africa/mbabane"), e_str("africa/mogadishu"), e_str("africa/monrovia"), e_str("africa/nairobi"), e_str("africa/ndjamena"), e_str("africa/niamey"), e_str("africa/nouakchott"), e_str("africa/ouagadougou"), e_str("africa/porto-novo"), e_str("africa/sao_tome"), e_str("africa/timbuktu"), e_str("africa/tripoli"), e_str("africa/tunis"), e_str("africa/windhoek"), e_str("america/adak"), e_str("america/anchorage"), e_str("america/anguilla"), e_str("america/antigua"), e_str("america/araguaina"), e_str("america/argentina/buenos_aires"), e_str("america/argentina/catamarca"), e_str("america/argentina/comodrivadavia"), e_str("america/argentina/cordoba"), e_str("america/argentina/jujuy"), e_str("america/argentina/la_rioja"), e_str("america/argentina/mendoza"), e_str("america/argentina/rio_gallegos"), e_str("america/argentina/salta"), e_str("america/argentina/san_juan"), e_str("america/argentina/san_luis"), e_str("america/argentina/tucuman"), e_str("america/argentina/ushuaia"), e_str("america/aruba"), e_str("america/asuncion"), e_str("america/atikokan"), e_str("america/atka"), e_str("america/bahia"), e_str("america/bahia_banderas"), e_str("america/barbados"), e_str("america/belem"), e_str("america/belize"), e_str("america/blanc-sablon"), e_str("america/boa_vista"), e_str("america/bogota"), e_str("america/boise"), e_str("america/buenos_aires"), e_str("america/cambridge_bay"), e_str("america/campo_grande"), e_str("america/cancun"), e_str("america/caracas"), e_str("america/catamarca"), e_str("america/cayenne"), e_str("america/cayman"), e_str("america/chicago"), e_str("america/chihuahua"), e_str("america/ciudad_juarez"), e_str("america/coral_harbour"), e_str("america/cordoba"), e_str("america/costa_rica"), e_str("america/coyhaique"), e_str("america/creston"), e_str("america/cuiaba"), e_str("america/curacao"), e_str("america/danmarkshavn"), e_str("america/dawson"), e_str("america/dawson_creek"), e_str("america/denver"), e_str("america/detroit"), e_str("america/dominica"), e_str("america/edmonton"), e_str("america/eirunepe"), e_str("america/el_salvador"), e_str("america/ensenada"), e_str("america/fort_nelson"), e_str("america/fort_wayne"), e_str("america/fortaleza"), e_str("america/glace_bay"), e_str("america/godthab"), e_str("america/goose_bay"), e_str("america/grand_turk"), e_str("america/grenada"), e_str("america/guadeloupe"), e_str("america/guatemala"), e_str("america/guayaquil"), e_str("america/guyana"), e_str("america/halifax"), e_str("america/havana"), e_str("america/hermosillo"), e_str("america/indiana/indianapolis"), e_str("america/indiana/knox"), e_str("america/indiana/marengo"), e_str("america/indiana/petersburg"), e_str("america/indiana/tell_city"), e_str("america/indiana/vevay"), e_str("america/indiana/vincennes"), e_str("america/indiana/winamac"), e_str("america/indianapolis"), e_str("america/inuvik"), e_str("america/iqaluit"), e_str("america/jamaica"), e_str("america/jujuy"), e_str("america/juneau"), e_str("america/kentucky/louisville"), e_str("america/kentucky/monticello"), e_str("america/knox_in"), e_str("america/kralendijk"), e_str("america/la_paz"), e_str("america/lima"), e_str("america/los_angeles"), e_str("america/louisville"), e_str("america/lower_princes"), e_str("america/maceio"), e_str("america/managua"), e_str("america/manaus"), e_str("america/marigot"), e_str("america/martinique"), e_str("america/matamoros"), e_str("america/mazatlan"), e_str("america/mendoza"), e_str("america/menominee"), e_str("america/merida"), e_str("america/metlakatla"), e_str("america/mexico_city"), e_str("america/miquelon"), e_str("america/moncton"), e_str("america/monterrey"), e_str("america/montevideo"), e_str("america/montreal"), e_str("america/montserrat"), e_str("america/nassau"), e_str("america/new_york"), e_str("america/nipigon"), e_str("america/nome"), e_str("america/noronha"), e_str("america/north_dakota/beulah"), e_str("america/north_dakota/center"), e_str("america/north_dakota/new_salem"), e_str("america/nuuk"), e_str("america/ojinaga"), e_str("america/panama"), e_str("america/pangnirtung"), e_str("america/paramaribo"), e_str("america/phoenix"), e_str("america/port-au-prince"), e_str("america/port_of_spain"), e_str("america/porto_acre"), e_str("america/porto_velho"), e_str("america/puerto_rico"), e_str("america/punta_arenas"), e_str("america/rainy_river"), e_str("america/rankin_inlet"), e_str("america/recife"), e_str("america/regina"), e_str("america/resolute"), e_str("america/rio_branco"), e_str("america/rosario"), e_str("america/santa_isabel"), e_str("america/santarem"), e_str("america/santiago"), e_str("america/santo_domingo"), e_str("america/sao_paulo"), e_str("america/scoresbysund"), e_str("america/shiprock"), e_str("america/sitka"), e_str("america/st_barthelemy"), e_str("america/st_johns"), e_str("america/st_kitts"), e_str("america/st_lucia"), e_str("america/st_thomas"), e_str("america/st_vincent"), e_str("america/swift_current"), e_str("america/tegucigalpa"), e_str("america/thule"), e_str("america/thunder_bay"), e_str("america/tijuana"), e_str("america/toronto"), e_str("america/tortola"), e_str("america/vancouver"), e_str("america/virgin"), e_str("america/whitehorse"), e_str("america/winnipeg"), e_str("america/yakutat"), e_str("america/yellowknife"), e_str("antarctica/casey"), e_str("antarctica/davis"), e_str("antarctica/dumontdurville"), e_str("antarctica/macquarie"), e_str("antarctica/mawson"), e_str("antarctica/mcmurdo"), e_str("antarctica/palmer"), e_str("antarctica/rothera"), e_str("antarctica/south_pole"), e_str("antarctica/syowa"), e_str("antarctica/troll"), e_str("antarctica/vostok"), e_str("arctic/longyearbyen"), e_str("asia/aden"), e_str("asia/almaty"), e_str("asia/amman"), e_str("asia/anadyr"), e_str("asia/aqtau"), e_str("asia/aqtobe"), e_str("asia/ashgabat"), e_str("asia/ashkhabad"), e_str("asia/atyrau"), e_str("asia/baghdad"), e_str("asia/bahrain"), e_str("asia/baku"), e_str("asia/bangkok"), e_str("asia/barnaul"), e_str("asia/beirut"), e_str("asia/bishkek"), e_str("asia/brunei"), e_str("asia/calcutta"), e_str("asia/chita"), e_str("asia/choibalsan"), e_str("asia/chongqing"), e_str("asia/chungking"), e_str("asia/colombo"), e_str("asia/dacca"), e_str("asia/damascus"), e_str("asia/dhaka"), e_str("asia/dili"), e_str("asia/dubai"), e_str("asia/dushanbe"), e_str("asia/famagusta"), e_str("asia/gaza"), e_str("asia/harbin"), e_str("asia/hebron"), e_str("asia/ho_chi_minh"), e_str("asia/hong_kong"), e_str("asia/hovd"), e_str("asia/irkutsk"), e_str("asia/istanbul"), e_str("asia/jakarta"), e_str("asia/jayapura"), e_str("asia/jerusalem"), e_str("asia/kabul"), e_str("asia/kamchatka"), e_str("asia/karachi"), e_str("asia/kashgar"), e_str("asia/kathmandu"), e_str("asia/katmandu"), e_str("asia/khandyga"), e_str("asia/kolkata"), e_str("asia/krasnoyarsk"), e_str("asia/kuala_lumpur"), e_str("asia/kuching"), e_str("asia/kuwait"), e_str("asia/macao"), e_str("asia/macau"), e_str("asia/magadan"), e_str("asia/makassar"), e_str("asia/manila"), e_str("asia/muscat"), e_str("asia/nicosia"), e_str("asia/novokuznetsk"), e_str("asia/novosibirsk"), e_str("asia/omsk"), e_str("asia/oral"), e_str("asia/phnom_penh"), e_str("asia/pontianak"), e_str("asia/pyongyang"), e_str("asia/qatar"), e_str("asia/qostanay"), e_str("asia/qyzylorda"), e_str("asia/rangoon"), e_str("asia/riyadh"), e_str("asia/saigon"), e_str("asia/sakhalin"), e_str("asia/samarkand"), e_str("asia/seoul"), e_str("asia/shanghai"), e_str("asia/singapore"), e_str("asia/srednekolymsk"), e_str("asia/taipei"), e_str("asia/tashkent"), e_str("asia/tbilisi"), e_str("asia/tehran"), e_str("asia/tel_aviv"), e_str("asia/thimbu"), e_str("asia/thimphu"), e_str("asia/tokyo"), e_str("asia/tomsk"), e_str("asia/ujung_pandang"), e_str("asia/ulaanbaatar"), e_str("asia/ulan_bator"), e_str("asia/urumqi"), e_str("asia/ust-nera"), e_str("asia/vientiane"), e_str("asia/vladivostok"), e_str("asia/yakutsk"), e_str("asia/yangon"), e_str("asia/yekaterinburg"), e_str("asia/yerevan"), e_str("atlantic/azores"), e_str("atlantic/bermuda"), e_str("atlantic/canary"), e_str("atlantic/cape_verde"), e_str("atlantic/faeroe"), e_str("atlantic/faroe"), e_str("atlantic/jan_mayen"), e_str("atlantic/madeira"), e_str("atlantic/reykjavik"), e_str("atlantic/south_georgia"), e_str("atlantic/st_helena"), e_str("atlantic/stanley"), e_str("australia/act"), e_str("australia/adelaide"), e_str("australia/brisbane"), e_str("australia/broken_hill"), e_str("australia/canberra"), e_str("australia/currie"), e_str("australia/darwin"), e_str("australia/eucla"), e_str("australia/hobart"), e_str("australia/lhi"), e_str("australia/lindeman"), e_str("australia/lord_howe"), e_str("australia/melbourne"), e_str("australia/north"), e_str("australia/nsw"), e_str("australia/perth"), e_str("australia/queensland"), e_str("australia/south"), e_str("australia/sydney"), e_str("australia/tasmania"), e_str("australia/victoria"), e_str("australia/west"), e_str("australia/yancowinna"), e_str("brazil/acre"), e_str("brazil/denoronha"), e_str("brazil/east"), e_str("brazil/west"), e_str("canada/atlantic"), e_str("canada/central"), e_str("canada/eastern"), e_str("canada/mountain"), e_str("canada/newfoundland"), e_str("canada/pacific"), e_str("canada/saskatchewan"), e_str("canada/yukon"), e_str("cet"), e_str("chile/continental"), e_str("chile/easterisland"), e_str("cst6cdt"), e_str("cuba"), e_str("eet"), e_str("egypt"), e_str("eire"), e_str("est"), e_str("est5edt"), e_str("etc/gmt"), e_str("etc/gmt+0"), e_str("etc/gmt+1"), e_str("etc/gmt+10"), e_str("etc/gmt+11"), e_str("etc/gmt+12"), e_str("etc/gmt+2"), e_str("etc/gmt+3"), e_str("etc/gmt+4"), e_str("etc/gmt+5"), e_str("etc/gmt+6"), e_str("etc/gmt+7"), e_str("etc/gmt+8"), e_str("etc/gmt+9"), e_str("etc/gmt-0"), e_str("etc/gmt-1"), e_str("etc/gmt-10"), e_str("etc/gmt-11"), e_str("etc/gmt-12"), e_str("etc/gmt-13"), e_str("etc/gmt-14"), e_str("etc/gmt-2"), e_str("etc/gmt-3"), e_str("etc/gmt-4"), e_str("etc/gmt-5"), e_str("etc/gmt-6"), e_str("etc/gmt-7"), e_str("etc/gmt-8"), e_str("etc/gmt-9"), e_str("etc/gmt0"), e_str("etc/greenwich"), e_str("etc/uct"), e_str("etc/universal"), e_str("etc/utc"), e_str("etc/zulu"), e_str("europe/amsterdam"), e_str("europe/andorra"), e_str("europe/astrakhan"), e_str("europe/athens"), e_str("europe/belfast"), e_str("europe/belgrade"), e_str("europe/berlin"), e_str("europe/bratislava"), e_str("europe/brussels"), e_str("europe/bucharest"), e_str("europe/budapest"), e_str("europe/busingen"), e_str("europe/chisinau"), e_str("europe/copenhagen"), e_str("europe/dublin"), e_str("europe/gibraltar"), e_str("europe/guernsey"), e_str("europe/helsinki"), e_str("europe/isle_of_man"), e_str("europe/istanbul"), e_str("europe/jersey"), e_str("europe/kaliningrad"), e_str("europe/kiev"), e_str("europe/kirov"), e_str("europe/kyiv"), e_str("europe/lisbon"), e_str("europe/ljubljana"), e_str("europe/london"), e_str("europe/luxembourg"), e_str("europe/madrid"), e_str("europe/malta"), e_str("europe/mariehamn"), e_str("europe/minsk"), e_str("europe/monaco"), e_str("europe/moscow"), e_str("europe/nicosia"), e_str("europe/oslo"), e_str("europe/paris"), e_str("europe/podgorica"), e_str("europe/prague"), e_str("europe/riga"), e_str("europe/rome"), e_str("europe/samara"), e_str("europe/san_marino"), e_str("europe/sarajevo"), e_str("europe/saratov"), e_str("europe/simferopol"), e_str("europe/skopje"), e_str("europe/sofia"), e_str("europe/stockholm"), e_str("europe/tallinn"), e_str("europe/tirane"), e_str("europe/tiraspol"), e_str("europe/ulyanovsk"), e_str("europe/uzhgorod"), e_str("europe/vaduz"), e_str("europe/vatican"), e_str("europe/vienna"), e_str("europe/vilnius"), e_str("europe/volgograd"), e_str("europe/warsaw"), e_str("europe/zagreb"), e_str("europe/zaporozhye"), e_str("europe/zurich"), e_str("factory"), e_str("gb"), e_str("gb-eire"), e_str("gmt"), e_str("gmt+0"), e_str("gmt-0"), e_str("gmt0"), e_str("greenwich"), e_str("hongkong"), e_str("hst"), e_str("iceland"), e_str("indian/antananarivo"), e_str("indian/chagos"), e_str("indian/christmas"), e_str("indian/cocos"), e_str("indian/comoro"), e_str("indian/kerguelen"), e_str("indian/mahe"), e_str("indian/maldives"), e_str("indian/mauritius"), e_str("indian/mayotte"), e_str("indian/reunion"), e_str("iran"), e_str("israel"), e_str("jamaica"), e_str("japan"), e_str("kwajalein"), e_str("libya"), e_str("met"), e_str("mexico/bajanorte"), e_str("mexico/bajasur"), e_str("mexico/general"), e_str("mst"), e_str("mst7mdt"), e_str("navajo"), e_str("nz"), e_str("nz-chat"), e_str("pacific/apia"), e_str("pacific/auckland"), e_str("pacific/bougainville"), e_str("pacific/chatham"), e_str("pacific/chuuk"), e_str("pacific/easter"), e_str("pacific/efate"), e_str("pacific/enderbury"), e_str("pacific/fakaofo"), e_str("pacific/fiji"), e_str("pacific/funafuti"), e_str("pacific/galapagos"), e_str("pacific/gambier"), e_str("pacific/guadalcanal"), e_str("pacific/guam"), e_str("pacific/honolulu"), e_str("pacific/johnston"), e_str("pacific/kanton"), e_str("pacific/kiritimati"), e_str("pacific/kosrae"), e_str("pacific/kwajalein"), e_str("pacific/majuro"), e_str("pacific/marquesas"), e_str("pacific/midway"), e_str("pacific/nauru"), e_str("pacific/niue"), e_str("pacific/norfolk"), e_str("pacific/noumea"), e_str("pacific/pago_pago"), e_str("pacific/palau"), e_str("pacific/pitcairn"), e_str("pacific/pohnpei"), e_str("pacific/ponape"), e_str("pacific/port_moresby"), e_str("pacific/rarotonga"), e_str("pacific/saipan"), e_str("pacific/samoa"), e_str("pacific/tahiti"), e_str("pacific/tarawa"), e_str("pacific/tongatapu"), e_str("pacific/truk"), e_str("pacific/wake"), e_str("pacific/wallis"), e_str("pacific/yap"), e_str("poland"), e_str("portugal"), e_str("prc"), e_str("pst8pdt"), e_str("roc"), e_str("rok"), e_str("singapore"), e_str("turkey"), e_str("uct"), e_str("universal"), e_str("us/alaska"), e_str("us/aleutian"), e_str("us/arizona"), e_str("us/central"), e_str("us/east-indiana"), e_str("us/eastern"), e_str("us/hawaii"), e_str("us/indiana-starke"), e_str("us/michigan"), e_str("us/mountain"), e_str("us/pacific"), e_str("us/samoa"), e_str("utc"), e_str("w-su"), e_str("wet"), e_str("zulu")]), e_bool(true)]),
            vec![
                s_return(e_binop(e_var("timezone"), BinOp::Concat, e_str(""))),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_call("strlen", vec![e_var("timezone")]), BinOp::StrictEq, e_int(1)),
            vec![
                s_assign("upper", e_call("strtoupper", vec![e_var("timezone")])),
                s_assign("code", e_call("ord", vec![e_var("upper")])),
                s_if(
                    e_binop(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(73))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(75)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(90)))),
                    vec![
                        s_return(e_var("upper")),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("in_array", vec![e_call("strtolower", vec![e_var("timezone")]), e_array(vec![e_str("acdt"), e_str("acst"), e_str("addt"), e_str("adt"), e_str("aedt"), e_str("aest"), e_str("ahdt"), e_str("ahst"), e_str("akdt"), e_str("akst"), e_str("amt"), e_str("apt"), e_str("ast"), e_str("awdt"), e_str("awst"), e_str("awt"), e_str("bdst"), e_str("bdt"), e_str("bmt"), e_str("bst"), e_str("cast"), e_str("cat"), e_str("cddt"), e_str("cdt"), e_str("cemt"), e_str("cest"), e_str("cet"), e_str("cmt"), e_str("cpt"), e_str("cst"), e_str("cwt"), e_str("chst"), e_str("dmt"), e_str("eat"), e_str("eddt"), e_str("edt"), e_str("eest"), e_str("eet"), e_str("emt"), e_str("ept"), e_str("est"), e_str("ewt"), e_str("ffmt"), e_str("fmt"), e_str("gdt"), e_str("gmt"), e_str("gst"), e_str("hdt"), e_str("hkst"), e_str("hkt"), e_str("hmt"), e_str("hpt"), e_str("hst"), e_str("hwt"), e_str("iddt"), e_str("idt"), e_str("imt"), e_str("ist"), e_str("jdt"), e_str("jmt"), e_str("jst"), e_str("kdt"), e_str("kmt"), e_str("kst"), e_str("lst"), e_str("mddt"), e_str("mdst"), e_str("mdt"), e_str("mest"), e_str("met"), e_str("mmt"), e_str("mpt"), e_str("msd"), e_str("msk"), e_str("mst"), e_str("mwt"), e_str("nddt"), e_str("ndt"), e_str("npt"), e_str("nst"), e_str("nwt"), e_str("nzdt"), e_str("nzmt"), e_str("nzst"), e_str("pddt"), e_str("pdt"), e_str("pkst"), e_str("pkt"), e_str("plmt"), e_str("pmt"), e_str("ppmt"), e_str("ppt"), e_str("pst"), e_str("pwt"), e_str("qmt"), e_str("rmt"), e_str("sast"), e_str("sdmt"), e_str("sjmt"), e_str("smt"), e_str("sst"), e_str("tbmt"), e_str("tmt"), e_str("uct"), e_str("utc"), e_str("wast"), e_str("wat"), e_str("wemt"), e_str("west"), e_str("wet"), e_str("wib"), e_str("wita"), e_str("wit"), e_str("wmt"), e_str("yddt"), e_str("ydt"), e_str("ypt"), e_str("yst"), e_str("ywt"), e_str("a"), e_str("b"), e_str("c"), e_str("d"), e_str("e"), e_str("f"), e_str("g"), e_str("h"), e_str("i"), e_str("k"), e_str("l"), e_str("m"), e_str("n"), e_str("o"), e_str("p"), e_str("q"), e_str("r"), e_str("s"), e_str("t"), e_str("u"), e_str("v"), e_str("w"), e_str("x"), e_str("y"), e_str("z")]), e_bool(true)]),
            vec![
                s_return(e_call("strtoupper", vec![e_var("timezone")])),
            ],
            vec![],
            None,
        ),
        s_return(e_str("")),
    ])
}

/// `DateTimeZone::__construct` — transcribed method builder.
fn decl_class_datetimezone_method_1_construct() -> MethodBuilder {
method("__construct")
    .param("timezone", TypeExpr::Str)
    .body_exact(vec![
        s_if(
            e_call("str_contains", vec![e_var("timezone"), e_call("chr", vec![e_int(0)])]),
            vec![
                s_throw(e_new("ValueError", vec![e_str("DateTimeZone::__construct(): Argument #1 ($timezone) must not contain any null bytes")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
        s_assign("__normalized", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_var("timezone")])),
        s_if(
            e_binop(e_var("__normalized"), BinOp::StrictNotEq, e_str("")),
            vec![
                s_prop_assign(e_this(), "name", e_var("__normalized")),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_assign("__length", e_call("strlen", vec![e_var("timezone")])),
        s_assign("__offsetOutOfRange", e_bool(false)),
        s_if(
            e_binop(e_binop(e_var("__length"), BinOp::StrictEq, e_int(5)), BinOp::And, e_call("ctype_digit", vec![e_call("substr", vec![e_var("timezone"), e_int(1)])])),
            vec![
                s_assign("__hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                s_assign("__minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(3), e_int(2)])])),
                s_assign("__offsetOutOfRange", e_binop(e_binop(e_binop(e_var("__hours"), BinOp::Mul, e_int(3600)), BinOp::Add, e_binop(e_var("__minutes"), BinOp::Mul, e_int(60))), BinOp::GtEq, e_binop(e_int(100), BinOp::Mul, e_int(3600)))),
            ],
            vec![],
            Some(vec![
            s_if(
                e_binop(e_binop(e_var("__length"), BinOp::StrictEq, e_int(6)), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(3)), BinOp::StrictEq, e_str(":"))),
                vec![
                    s_assign("__hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                    s_assign("__minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(4), e_int(2)])])),
                    s_assign("__offsetOutOfRange", e_binop(e_binop(e_binop(e_var("__hours"), BinOp::Mul, e_int(3600)), BinOp::Add, e_binop(e_var("__minutes"), BinOp::Mul, e_int(60))), BinOp::GtEq, e_binop(e_int(100), BinOp::Mul, e_int(3600)))),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_binop(e_var("__length"), BinOp::StrictEq, e_int(7)), BinOp::And, e_call("ctype_digit", vec![e_call("substr", vec![e_var("timezone"), e_int(1)])])),
                    vec![
                        s_assign("__hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                        s_assign("__minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(3), e_int(2)])])),
                        s_assign("__seconds", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(5), e_int(2)])])),
                        s_assign("__offsetOutOfRange", e_binop(e_binop(e_binop(e_binop(e_var("__hours"), BinOp::Mul, e_int(3600)), BinOp::Add, e_binop(e_var("__minutes"), BinOp::Mul, e_int(60))), BinOp::Add, e_var("__seconds")), BinOp::GtEq, e_binop(e_int(100), BinOp::Mul, e_int(3600)))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("__length"), BinOp::StrictEq, e_int(9)), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(3)), BinOp::StrictEq, e_str(":"))), BinOp::And, e_binop(e_index(e_var("timezone"), e_int(6)), BinOp::StrictEq, e_str(":"))),
                        vec![
                            s_assign("__hours", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(1), e_int(2)])])),
                            s_assign("__minutes", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(4), e_int(2)])])),
                            s_assign("__seconds", e_call("intval", vec![e_call("substr", vec![e_var("timezone"), e_int(7), e_int(2)])])),
                            s_assign("__offsetOutOfRange", e_binop(e_binop(e_binop(e_binop(e_var("__hours"), BinOp::Mul, e_int(3600)), BinOp::Add, e_binop(e_var("__minutes"), BinOp::Mul, e_int(60))), BinOp::Add, e_var("__seconds")), BinOp::GtEq, e_binop(e_int(100), BinOp::Mul, e_int(3600)))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                ),
            ]),
            ),
        ]),
        ),
        s_if(
            e_var("__offsetOutOfRange"),
            vec![
                s_throw(e_new("DateInvalidTimeZoneException", vec![e_binop(e_binop(e_str("DateTimeZone::__construct(): Timezone offset is out of range ("), BinOp::Concat, e_var("timezone")), BinOp::Concat, e_str(")"))])),
            ],
            vec![],
            None,
        ),
        s_throw(e_new("DateInvalidTimeZoneException", vec![e_binop(e_binop(e_str("DateTimeZone::__construct(): Unknown or bad timezone ("), BinOp::Concat, e_var("timezone")), BinOp::Concat, e_str(")"))])),
    ])
}

/// `DateTimeZone::__elephc_timezone_open` — transcribed method builder.
fn decl_class_datetimezone_method_2_elephc_timezone_open() -> MethodBuilder {
method("__elephc_timezone_open")
    .static_()
    .param("timezone", t_mixed())
    .param("sourceLine", TypeExpr::Int)
    .returns(t_mixed())
    .body_exact(vec![
        s_assign("timezone", e_cast(CastType::String, e_var("timezone"))),
        s_if(
            e_call("str_contains", vec![e_var("timezone"), e_call("chr", vec![e_int(0)])]),
            vec![
                s_throw(e_new("ValueError", vec![e_str("timezone_open(): Argument #1 ($timezone) must not contain any null bytes")])),
            ],
            vec![],
            None,
        ),
        s_try(vec![
            s_return(e_new("DateTimeZone", vec![e_var("timezone")])),
        ], vec![
            (vec!["DateInvalidTimeZoneException"], Some("exception"), vec![
                s_expr(e_call("__elephc_diag_warning", vec![e_binop(e_binop(e_str("\nWarning: timezone_open(): Unknown or bad timezone ("), BinOp::Concat, e_var("timezone")), BinOp::Concat, e_str(")")), e_var("sourceLine")])),
                s_return(e_bool(false)),
            ]),
        ], None),
    ])
}

/// `DateTimeZone::getName` — transcribed method builder.
fn decl_class_datetimezone_method_3_getname() -> MethodBuilder {
method("getName")
    .returns(TypeExpr::Str)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_this_prop("name")),
    ])
}

/// `DateTimeZone::getOffset` — transcribed method builder.
fn decl_class_datetimezone_method_4_getoffset() -> MethodBuilder {
method("getOffset")
    .param("datetime", t_class("DateTimeInterface"))
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("__saved", e_call("date_default_timezone_get", vec![])),
        s_expr(e_call("date_default_timezone_set", vec![e_static_call("DateTime", "__elephc_runtime_timezone_name", vec![e_this_prop("name")])])),
        s_assign("__off", e_call("intval", vec![e_call("date", vec![e_str("Z"), e_method_call(e_var("datetime"), "getTimestamp", vec![])])])),
        s_expr(e_call("date_default_timezone_set", vec![e_var("__saved")])),
        s_return(e_var("__off")),
    ])
}

/// `DateTimeZone::listIdentifiers` — transcribed method builder.
fn decl_class_datetimezone_method_5_listidentifiers() -> MethodBuilder {
method("listIdentifiers")
    .static_()
    .param_default("timezoneGroup", TypeExpr::Int, e_int(2047))
    .param_default("countryCode", t_nullable(TypeExpr::Str), e_null())
    .body_exact(vec![
        s_return(e_array(vec![e_str("Africa/Abidjan"), e_str("Africa/Accra"), e_str("Africa/Addis_Ababa"), e_str("Africa/Algiers"), e_str("Africa/Asmara"), e_str("Africa/Bamako"), e_str("Africa/Bangui"), e_str("Africa/Banjul"), e_str("Africa/Bissau"), e_str("Africa/Blantyre"), e_str("Africa/Brazzaville"), e_str("Africa/Bujumbura"), e_str("Africa/Cairo"), e_str("Africa/Casablanca"), e_str("Africa/Ceuta"), e_str("Africa/Conakry"), e_str("Africa/Dakar"), e_str("Africa/Dar_es_Salaam"), e_str("Africa/Djibouti"), e_str("Africa/Douala"), e_str("Africa/El_Aaiun"), e_str("Africa/Freetown"), e_str("Africa/Gaborone"), e_str("Africa/Harare"), e_str("Africa/Johannesburg"), e_str("Africa/Juba"), e_str("Africa/Kampala"), e_str("Africa/Khartoum"), e_str("Africa/Kigali"), e_str("Africa/Kinshasa"), e_str("Africa/Lagos"), e_str("Africa/Libreville"), e_str("Africa/Lome"), e_str("Africa/Luanda"), e_str("Africa/Lubumbashi"), e_str("Africa/Lusaka"), e_str("Africa/Malabo"), e_str("Africa/Maputo"), e_str("Africa/Maseru"), e_str("Africa/Mbabane"), e_str("Africa/Mogadishu"), e_str("Africa/Monrovia"), e_str("Africa/Nairobi"), e_str("Africa/Ndjamena"), e_str("Africa/Niamey"), e_str("Africa/Nouakchott"), e_str("Africa/Ouagadougou"), e_str("Africa/Porto-Novo"), e_str("Africa/Sao_Tome"), e_str("Africa/Tripoli"), e_str("Africa/Tunis"), e_str("Africa/Windhoek"), e_str("America/Adak"), e_str("America/Anchorage"), e_str("America/Anguilla"), e_str("America/Antigua"), e_str("America/Araguaina"), e_str("America/Argentina/Buenos_Aires"), e_str("America/Argentina/Catamarca"), e_str("America/Argentina/Cordoba"), e_str("America/Argentina/Jujuy"), e_str("America/Argentina/La_Rioja"), e_str("America/Argentina/Mendoza"), e_str("America/Argentina/Rio_Gallegos"), e_str("America/Argentina/Salta"), e_str("America/Argentina/San_Juan"), e_str("America/Argentina/San_Luis"), e_str("America/Argentina/Tucuman"), e_str("America/Argentina/Ushuaia"), e_str("America/Aruba"), e_str("America/Asuncion"), e_str("America/Atikokan"), e_str("America/Bahia"), e_str("America/Bahia_Banderas"), e_str("America/Barbados"), e_str("America/Belem"), e_str("America/Belize"), e_str("America/Blanc-Sablon"), e_str("America/Boa_Vista"), e_str("America/Bogota"), e_str("America/Boise"), e_str("America/Cambridge_Bay"), e_str("America/Campo_Grande"), e_str("America/Cancun"), e_str("America/Caracas"), e_str("America/Cayenne"), e_str("America/Cayman"), e_str("America/Chicago"), e_str("America/Chihuahua"), e_str("America/Ciudad_Juarez"), e_str("America/Costa_Rica"), e_str("America/Coyhaique"), e_str("America/Creston"), e_str("America/Cuiaba"), e_str("America/Curacao"), e_str("America/Danmarkshavn"), e_str("America/Dawson"), e_str("America/Dawson_Creek"), e_str("America/Denver"), e_str("America/Detroit"), e_str("America/Dominica"), e_str("America/Edmonton"), e_str("America/Eirunepe"), e_str("America/El_Salvador"), e_str("America/Fort_Nelson"), e_str("America/Fortaleza"), e_str("America/Glace_Bay"), e_str("America/Goose_Bay"), e_str("America/Grand_Turk"), e_str("America/Grenada"), e_str("America/Guadeloupe"), e_str("America/Guatemala"), e_str("America/Guayaquil"), e_str("America/Guyana"), e_str("America/Halifax"), e_str("America/Havana"), e_str("America/Hermosillo"), e_str("America/Indiana/Indianapolis"), e_str("America/Indiana/Knox"), e_str("America/Indiana/Marengo"), e_str("America/Indiana/Petersburg"), e_str("America/Indiana/Tell_City"), e_str("America/Indiana/Vevay"), e_str("America/Indiana/Vincennes"), e_str("America/Indiana/Winamac"), e_str("America/Inuvik"), e_str("America/Iqaluit"), e_str("America/Jamaica"), e_str("America/Juneau"), e_str("America/Kentucky/Louisville"), e_str("America/Kentucky/Monticello"), e_str("America/Kralendijk"), e_str("America/La_Paz"), e_str("America/Lima"), e_str("America/Los_Angeles"), e_str("America/Lower_Princes"), e_str("America/Maceio"), e_str("America/Managua"), e_str("America/Manaus"), e_str("America/Marigot"), e_str("America/Martinique"), e_str("America/Matamoros"), e_str("America/Mazatlan"), e_str("America/Menominee"), e_str("America/Merida"), e_str("America/Metlakatla"), e_str("America/Mexico_City"), e_str("America/Miquelon"), e_str("America/Moncton"), e_str("America/Monterrey"), e_str("America/Montevideo"), e_str("America/Montserrat"), e_str("America/Nassau"), e_str("America/New_York"), e_str("America/Nome"), e_str("America/Noronha"), e_str("America/North_Dakota/Beulah"), e_str("America/North_Dakota/Center"), e_str("America/North_Dakota/New_Salem"), e_str("America/Nuuk"), e_str("America/Ojinaga"), e_str("America/Panama"), e_str("America/Paramaribo"), e_str("America/Phoenix"), e_str("America/Port-au-Prince"), e_str("America/Port_of_Spain"), e_str("America/Porto_Velho"), e_str("America/Puerto_Rico"), e_str("America/Punta_Arenas"), e_str("America/Rankin_Inlet"), e_str("America/Recife"), e_str("America/Regina"), e_str("America/Resolute"), e_str("America/Rio_Branco"), e_str("America/Santarem"), e_str("America/Santiago"), e_str("America/Santo_Domingo"), e_str("America/Sao_Paulo"), e_str("America/Scoresbysund"), e_str("America/Sitka"), e_str("America/St_Barthelemy"), e_str("America/St_Johns"), e_str("America/St_Kitts"), e_str("America/St_Lucia"), e_str("America/St_Thomas"), e_str("America/St_Vincent"), e_str("America/Swift_Current"), e_str("America/Tegucigalpa"), e_str("America/Thule"), e_str("America/Tijuana"), e_str("America/Toronto"), e_str("America/Tortola"), e_str("America/Vancouver"), e_str("America/Whitehorse"), e_str("America/Winnipeg"), e_str("America/Yakutat"), e_str("Antarctica/Casey"), e_str("Antarctica/Davis"), e_str("Antarctica/DumontDUrville"), e_str("Antarctica/Macquarie"), e_str("Antarctica/Mawson"), e_str("Antarctica/McMurdo"), e_str("Antarctica/Palmer"), e_str("Antarctica/Rothera"), e_str("Antarctica/Syowa"), e_str("Antarctica/Troll"), e_str("Antarctica/Vostok"), e_str("Arctic/Longyearbyen"), e_str("Asia/Aden"), e_str("Asia/Almaty"), e_str("Asia/Amman"), e_str("Asia/Anadyr"), e_str("Asia/Aqtau"), e_str("Asia/Aqtobe"), e_str("Asia/Ashgabat"), e_str("Asia/Atyrau"), e_str("Asia/Baghdad"), e_str("Asia/Bahrain"), e_str("Asia/Baku"), e_str("Asia/Bangkok"), e_str("Asia/Barnaul"), e_str("Asia/Beirut"), e_str("Asia/Bishkek"), e_str("Asia/Brunei"), e_str("Asia/Chita"), e_str("Asia/Colombo"), e_str("Asia/Damascus"), e_str("Asia/Dhaka"), e_str("Asia/Dili"), e_str("Asia/Dubai"), e_str("Asia/Dushanbe"), e_str("Asia/Famagusta"), e_str("Asia/Gaza"), e_str("Asia/Hebron"), e_str("Asia/Ho_Chi_Minh"), e_str("Asia/Hong_Kong"), e_str("Asia/Hovd"), e_str("Asia/Irkutsk"), e_str("Asia/Jakarta"), e_str("Asia/Jayapura"), e_str("Asia/Jerusalem"), e_str("Asia/Kabul"), e_str("Asia/Kamchatka"), e_str("Asia/Karachi"), e_str("Asia/Kathmandu"), e_str("Asia/Khandyga"), e_str("Asia/Kolkata"), e_str("Asia/Krasnoyarsk"), e_str("Asia/Kuala_Lumpur"), e_str("Asia/Kuching"), e_str("Asia/Kuwait"), e_str("Asia/Macau"), e_str("Asia/Magadan"), e_str("Asia/Makassar"), e_str("Asia/Manila"), e_str("Asia/Muscat"), e_str("Asia/Nicosia"), e_str("Asia/Novokuznetsk"), e_str("Asia/Novosibirsk"), e_str("Asia/Omsk"), e_str("Asia/Oral"), e_str("Asia/Phnom_Penh"), e_str("Asia/Pontianak"), e_str("Asia/Pyongyang"), e_str("Asia/Qatar"), e_str("Asia/Qostanay"), e_str("Asia/Qyzylorda"), e_str("Asia/Riyadh"), e_str("Asia/Sakhalin"), e_str("Asia/Samarkand"), e_str("Asia/Seoul"), e_str("Asia/Shanghai"), e_str("Asia/Singapore"), e_str("Asia/Srednekolymsk"), e_str("Asia/Taipei"), e_str("Asia/Tashkent"), e_str("Asia/Tbilisi"), e_str("Asia/Tehran"), e_str("Asia/Thimphu"), e_str("Asia/Tokyo"), e_str("Asia/Tomsk"), e_str("Asia/Ulaanbaatar"), e_str("Asia/Urumqi"), e_str("Asia/Ust-Nera"), e_str("Asia/Vientiane"), e_str("Asia/Vladivostok"), e_str("Asia/Yakutsk"), e_str("Asia/Yangon"), e_str("Asia/Yekaterinburg"), e_str("Asia/Yerevan"), e_str("Atlantic/Azores"), e_str("Atlantic/Bermuda"), e_str("Atlantic/Canary"), e_str("Atlantic/Cape_Verde"), e_str("Atlantic/Faroe"), e_str("Atlantic/Madeira"), e_str("Atlantic/Reykjavik"), e_str("Atlantic/South_Georgia"), e_str("Atlantic/St_Helena"), e_str("Atlantic/Stanley"), e_str("Australia/Adelaide"), e_str("Australia/Brisbane"), e_str("Australia/Broken_Hill"), e_str("Australia/Darwin"), e_str("Australia/Eucla"), e_str("Australia/Hobart"), e_str("Australia/Lindeman"), e_str("Australia/Lord_Howe"), e_str("Australia/Melbourne"), e_str("Australia/Perth"), e_str("Australia/Sydney"), e_str("Europe/Amsterdam"), e_str("Europe/Andorra"), e_str("Europe/Astrakhan"), e_str("Europe/Athens"), e_str("Europe/Belgrade"), e_str("Europe/Berlin"), e_str("Europe/Bratislava"), e_str("Europe/Brussels"), e_str("Europe/Bucharest"), e_str("Europe/Budapest"), e_str("Europe/Busingen"), e_str("Europe/Chisinau"), e_str("Europe/Copenhagen"), e_str("Europe/Dublin"), e_str("Europe/Gibraltar"), e_str("Europe/Guernsey"), e_str("Europe/Helsinki"), e_str("Europe/Isle_of_Man"), e_str("Europe/Istanbul"), e_str("Europe/Jersey"), e_str("Europe/Kaliningrad"), e_str("Europe/Kirov"), e_str("Europe/Kyiv"), e_str("Europe/Lisbon"), e_str("Europe/Ljubljana"), e_str("Europe/London"), e_str("Europe/Luxembourg"), e_str("Europe/Madrid"), e_str("Europe/Malta"), e_str("Europe/Mariehamn"), e_str("Europe/Minsk"), e_str("Europe/Monaco"), e_str("Europe/Moscow"), e_str("Europe/Oslo"), e_str("Europe/Paris"), e_str("Europe/Podgorica"), e_str("Europe/Prague"), e_str("Europe/Riga"), e_str("Europe/Rome"), e_str("Europe/Samara"), e_str("Europe/San_Marino"), e_str("Europe/Sarajevo"), e_str("Europe/Saratov"), e_str("Europe/Simferopol"), e_str("Europe/Skopje"), e_str("Europe/Sofia"), e_str("Europe/Stockholm"), e_str("Europe/Tallinn"), e_str("Europe/Tirane"), e_str("Europe/Ulyanovsk"), e_str("Europe/Vaduz"), e_str("Europe/Vatican"), e_str("Europe/Vienna"), e_str("Europe/Vilnius"), e_str("Europe/Volgograd"), e_str("Europe/Warsaw"), e_str("Europe/Zagreb"), e_str("Europe/Zurich"), e_str("Indian/Antananarivo"), e_str("Indian/Chagos"), e_str("Indian/Christmas"), e_str("Indian/Cocos"), e_str("Indian/Comoro"), e_str("Indian/Kerguelen"), e_str("Indian/Mahe"), e_str("Indian/Maldives"), e_str("Indian/Mauritius"), e_str("Indian/Mayotte"), e_str("Indian/Reunion"), e_str("Pacific/Apia"), e_str("Pacific/Auckland"), e_str("Pacific/Bougainville"), e_str("Pacific/Chatham"), e_str("Pacific/Chuuk"), e_str("Pacific/Easter"), e_str("Pacific/Efate"), e_str("Pacific/Fakaofo"), e_str("Pacific/Fiji"), e_str("Pacific/Funafuti"), e_str("Pacific/Galapagos"), e_str("Pacific/Gambier"), e_str("Pacific/Guadalcanal"), e_str("Pacific/Guam"), e_str("Pacific/Honolulu"), e_str("Pacific/Kanton"), e_str("Pacific/Kiritimati"), e_str("Pacific/Kosrae"), e_str("Pacific/Kwajalein"), e_str("Pacific/Majuro"), e_str("Pacific/Marquesas"), e_str("Pacific/Midway"), e_str("Pacific/Nauru"), e_str("Pacific/Niue"), e_str("Pacific/Norfolk"), e_str("Pacific/Noumea"), e_str("Pacific/Pago_Pago"), e_str("Pacific/Palau"), e_str("Pacific/Pitcairn"), e_str("Pacific/Pohnpei"), e_str("Pacific/Port_Moresby"), e_str("Pacific/Rarotonga"), e_str("Pacific/Saipan"), e_str("Pacific/Tahiti"), e_str("Pacific/Tarawa"), e_str("Pacific/Tongatapu"), e_str("Pacific/Wake"), e_str("Pacific/Wallis"), e_str("UTC")])),
    ])
}

/// `DateTimeZone::__elephc_compare` — transcribed method builder.
fn decl_class_datetimezone_method_6_elephc_compare() -> MethodBuilder {
method("__elephc_compare")
    .final_()
    .param("other", t_class("DateTimeZone"))
    .returns(TypeExpr::Int)
    .body_exact(vec![
        s_if(
            e_binop(e_not(e_this_prop("__elephc_initialized")), BinOp::Or, e_not(e_prop(e_var("other"), "__elephc_initialized"))),
            vec![
                s_throw(e_new("DateObjectError", vec![e_str("Trying to compare uninitialized DateTimeZone objects")])),
            ],
            vec![],
            None,
        ),
        s_assign("leftType", e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("name")])),
        s_assign("rightType", e_static_call("DateTime", "__elephc_timezone_type", vec![e_prop(e_var("other"), "name")])),
        s_if(
            e_binop(e_var("leftType"), BinOp::StrictNotEq, e_var("rightType")),
            vec![
                s_throw(e_new("DateException", vec![e_str("Cannot compare two different kinds of DateTimeZone objects")])),
            ],
            vec![],
            None,
        ),
        s_return(e_ternary(e_binop(e_this_prop("name"), BinOp::StrictEq, e_prop(e_var("other"), "name")), e_int(0), e_int(1))),
    ])
}

/// `DateTimeZone::__elephc_begin_argument_array` — transcribed method builder.
fn decl_class_datetimezone_method_7_elephc_begin_argument_array() -> MethodBuilder {
method("__elephc_begin_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_prop_assign(e_this(), "__elephc_arguments", e_array(vec![])),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DateTimeZone::__elephc_append_one_argument` — transcribed method builder.
fn decl_class_datetimezone_method_8_elephc_append_one_argument() -> MethodBuilder {
method("__elephc_append_one_argument")
    .private()
    .param("key", t_mixed())
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_if(
            e_call("is_int", vec![e_var("key")]),
            vec![
                s_if(
                    e_this_prop("__elephc_seen_named_argument"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Cannot use positional argument after named argument during unpacking")])),
                    ],
                    vec![],
                    None,
                ),
                s_array_push("arguments", e_var("value")),
                s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_not(e_call("is_string", vec![e_var("key")])),
            vec![
                s_throw(e_new("Error", vec![e_str("Keys must be of type int|string during argument unpacking")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(true)),
        s_if(
            e_not(e_binop(e_var("key"), BinOp::StrictEq, e_str("timezone"))),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_str("Unknown named parameter $"), BinOp::Concat, e_var("key"))])),
            ],
            vec![],
            None,
        ),
        s_assign("parameterIndex", e_neg(e_int(1))),
        s_if(
            e_binop(e_var("key"), BinOp::StrictEq, e_str("timezone")),
            vec![
                s_assign("parameterIndex", e_int(0)),
            ],
            vec![],
            None,
        ),
        s_assign("positionalCount", e_int(0)),
        s_foreach(e_var("arguments"), Some("existingKey"), "existingValue", vec![
            s_if(
                e_call("is_int", vec![e_var("existingKey")]),
                vec![
                    s_expr(e_post_inc("positionalCount")),
                ],
                vec![],
                None,
            ),
        ]),
        s_if(
            e_binop(e_var("parameterIndex"), BinOp::Lt, e_var("positionalCount")),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_call("array_key_exists", vec![e_var("key"), e_var("arguments")]),
            vec![
                s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Named parameter $"), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" overwrites previous argument"))])),
            ],
            vec![],
            None,
        ),
        s_array_assign("arguments", e_var("key"), e_var("value")),
        s_prop_assign(e_this(), "__elephc_arguments", e_var("arguments")),
    ])
}

/// `DateTimeZone::__elephc_append_argument_chunk` — transcribed method builder.
fn decl_class_datetimezone_method_9_elephc_append_argument_chunk() -> MethodBuilder {
method("__elephc_append_argument_chunk")
    .private()
    .param("kind", TypeExpr::Int)
    .param("name", TypeExpr::Str)
    .param("value", t_mixed())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
            vec![
                s_if(
                    e_not(e_binop(e_call("is_array", vec![e_var("value")]), BinOp::Or, e_instance_of(e_var("value"), "Traversable"))),
                    vec![
                        s_expr(e_static_call("DateTime", "__elephc_argument_type_error", vec![e_var("value"), e_str("Only arrays and Traversables can be unpacked, ")])),
                    ],
                    vec![],
                    None,
                ),
                s_foreach(e_var("value"), Some("key"), "unpackedValue", vec![
                    s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("key"), e_var("unpackedValue")])),
                ]),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
            vec![
                s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_var("name"), e_var("value")])),
                s_return_void(),
            ],
            vec![],
            None,
        ),
        s_expr(e_method_call(e_this(), "__elephc_append_one_argument", vec![e_int(0), e_var("value")])),
    ])
}

/// `DateTimeZone::__elephc_finish_argument_array` — transcribed method builder.
fn decl_class_datetimezone_method_10_elephc_finish_argument_array() -> MethodBuilder {
method("__elephc_finish_argument_array")
    .private()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_assign("arguments", e_this_prop("__elephc_arguments")),
        s_assign("hasTimezone", e_bool(false)),
        s_assign("nextPosition", e_int(0)),
        s_foreach(e_var("arguments"), Some("key"), "value", vec![
            s_if(
                e_call("is_int", vec![e_var("key")]),
                vec![
                    s_if(
                        e_binop(e_var("nextPosition"), BinOp::Gt, e_int(0)),
                        vec![
                            s_throw(e_new("ArgumentCountError", vec![e_binop(e_binop(e_str("DateTimeZone::__construct() expects exactly 1 argument, "), BinOp::Concat, e_call("count", vec![e_var("arguments")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("timezone", e_var("value")),
                    s_assign("hasTimezone", e_bool(true)),
                    s_expr(e_post_inc("nextPosition")),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_var("hasTimezone"),
                    vec![
                        s_throw(e_new("Error", vec![e_str("Named parameter $timezone overwrites previous argument")])),
                    ],
                    vec![],
                    None,
                ),
                s_assign("timezone", e_var("value")),
                s_assign("hasTimezone", e_bool(true)),
            ]),
            ),
        ]),
        s_if(
            e_not(e_var("hasTimezone")),
            vec![
                s_throw(e_new("ArgumentCountError", vec![e_str("DateTimeZone::__construct() expects exactly 1 argument, 0 given")])),
            ],
            vec![],
            None,
        ),
        s_assign("timezone", e_static_call("DateTime", "__elephc_weak_string_argument", vec![e_var("timezone"), e_str("DateTimeZone::__construct(): Argument #1 ($timezone) must be of type string, "), e_str("")])),
        s_expr(e_method_call(e_this(), "__construct", vec![e_var("timezone")])),
        s_prop_assign(e_this(), "__elephc_arguments", e_null()),
        s_prop_assign(e_this(), "__elephc_seen_named_argument", e_bool(false)),
    ])
}

/// `DateTimeZone::__wakeup` — transcribed method builder.
fn decl_class_datetimezone_method_11_wakeup() -> MethodBuilder {
method("__wakeup")
    .attr("\\Deprecated", vec![e_named_arg("since", e_str("8.5")), e_named_arg("message", e_str("this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()"))])
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_call("__elephc_diag_warning", vec![e_str("Deprecated: Method DateTimeZone::__wakeup() is deprecated since 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()\n"), e_int(0), e_const("E_DEPRECATED")])),
        s_if(
            e_binop(e_str("DateTimeZone"), BinOp::StrictNotEq, e_str("DateInterval")),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeZone object")])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateTimeZone::__serialize` — transcribed method builder.
fn decl_class_datetimezone_method_12_serialize() -> MethodBuilder {
method("__serialize")
    .returns(t_array())
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_return(e_array_assoc(vec![(e_str("timezone_type"), e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("name")])), (e_str("timezone"), e_this_prop("name"))])),
    ])
}

/// `DateTimeZone::__unserialize` — transcribed method builder.
fn decl_class_datetimezone_method_13_unserialize() -> MethodBuilder {
method("__unserialize")
    .param("data", t_array())
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_not(e_call("array_key_exists", vec![e_str("timezone_type"), e_var("data")])), BinOp::Or, e_not(e_call("array_key_exists", vec![e_str("timezone"), e_var("data")]))), BinOp::Or, e_not(e_call("is_int", vec![e_index(e_var("data"), e_str("timezone_type"))]))), BinOp::Or, e_not(e_call("is_string", vec![e_index(e_var("data"), e_str("timezone"))]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeZone object")])),
            ],
            vec![],
            None,
        ),
        s_assign("__normalized", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_index(e_var("data"), e_str("timezone"))])),
        s_if(
            e_binop(e_binop(e_var("__normalized"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_index(e_var("data"), e_str("timezone_type")), BinOp::StrictNotEq, e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("__normalized")]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeZone object")])),
            ],
            vec![],
            None,
        ),
        s_prop_assign(e_this(), "name", e_var("__normalized")),
        s_prop_assign(e_this(), "__elephc_initialized", e_bool(true)),
    ])
}

/// `DateTimeZone::__set_state` — transcribed method builder.
fn decl_class_datetimezone_method_14_set_state() -> MethodBuilder {
method("__set_state")
    .static_()
    .param("array", t_array())
    .returns(t_class("DateTimeZone"))
    .body_exact(vec![
        s_if(
            e_binop(e_binop(e_binop(e_not(e_call("array_key_exists", vec![e_str("timezone_type"), e_var("array")])), BinOp::Or, e_not(e_call("array_key_exists", vec![e_str("timezone"), e_var("array")]))), BinOp::Or, e_not(e_call("is_int", vec![e_index(e_var("array"), e_str("timezone_type"))]))), BinOp::Or, e_not(e_call("is_string", vec![e_index(e_var("array"), e_str("timezone"))]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeZone object")])),
            ],
            vec![],
            None,
        ),
        s_assign("__normalized", e_static_call("DateTimeZone", "__elephc_normalize_timezone", vec![e_index(e_var("array"), e_str("timezone"))])),
        s_if(
            e_binop(e_binop(e_var("__normalized"), BinOp::StrictEq, e_str("")), BinOp::Or, e_binop(e_index(e_var("array"), e_str("timezone_type")), BinOp::StrictNotEq, e_static_call("DateTime", "__elephc_timezone_type", vec![e_var("__normalized")]))),
            vec![
                s_throw(e_new("Error", vec![e_str("Invalid serialization data for DateTimeZone object")])),
            ],
            vec![],
            None,
        ),
        s_return(e_new("DateTimeZone", vec![e_var("__normalized")])),
    ])
}

/// `DateTimeZone::__elephc_debug_dump` — transcribed method builder.
fn decl_class_datetimezone_method_15_elephc_debug_dump() -> MethodBuilder {
method("__elephc_debug_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_call("__elephc_var_dump_indent", vec![e_int(0)])])),
        s_assign("field_pad", e_binop(e_var("pad"), BinOp::Concat, e_str("  "))),
        s_assign("property_count", e_call("__elephc_var_dump_object_property_count", vec![e_this()])),
        s_echo(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("pad"), BinOp::Concat, e_str("object(")), BinOp::Concat, e_call("get_class", vec![e_this()])), BinOp::Concat, e_str(")#")), BinOp::Concat, e_call("spl_object_id", vec![e_this()])), BinOp::Concat, e_str(" (")), BinOp::Concat, e_binop(e_var("property_count"), BinOp::Add, e_int(2))), BinOp::Concat, e_str(") {\n"))),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_int(2)])),
        s_expr(e_call("__elephc_var_dump_object_properties", vec![e_this()])),
        s_expr(e_call("__elephc_var_dump_indent", vec![e_neg(e_int(2))])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"timezone_type\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("name")])])),
        s_echo(e_binop(e_var("field_pad"), BinOp::Concat, e_str("[\"timezone\"]=>\n"))),
        s_echo(e_var("field_pad")),
        s_expr(e_call("var_dump", vec![e_this_prop("name")])),
        s_echo(e_binop(e_var("pad"), BinOp::Concat, e_str("}\n"))),
    ])
}

/// `DateTimeZone::__elephc_print_r_dump` — transcribed method builder.
fn decl_class_datetimezone_method_16_elephc_print_r_dump() -> MethodBuilder {
method("__elephc_print_r_dump")
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_echo(e_binop(e_call("get_class", vec![e_this()]), BinOp::Concat, e_str(" Object\n(\n"))),
        s_expr(e_call("__elephc_print_r_object_properties", vec![e_this()])),
        s_echo(e_binop(e_binop(e_str("    [timezone_type] => "), BinOp::Concat, e_static_call("DateTime", "__elephc_timezone_type", vec![e_this_prop("name")])), BinOp::Concat, e_str("\n"))),
        s_echo(e_binop(e_binop(e_str("    [timezone] => "), BinOp::Concat, e_this_prop("name")), BinOp::Concat, e_str("\n"))),
        s_echo(e_str(")\n")),
    ])
}

/// `DateTimeZone::getLocation` — transcribed method builder.
fn decl_class_datetimezone_method_17_getlocation() -> MethodBuilder {
method("getLocation")
    .returns(t_mixed())
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("raw", e_call("elephc_tz_location", vec![e_this_prop("name")])),
        s_if(
            e_binop(e_var("raw"), BinOp::StrictEq, e_str("")),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("f", e_call("explode", vec![e_str("\t"), e_var("raw")])),
        s_return(e_array_assoc(vec![(e_str("country_code"), e_index(e_var("f"), e_int(0))), (e_str("latitude"), e_cast(CastType::Float, e_index(e_var("f"), e_int(1)))), (e_str("longitude"), e_cast(CastType::Float, e_index(e_var("f"), e_int(2)))), (e_str("comments"), e_index(e_var("f"), e_int(3)))])),
    ])
}

/// `DateTimeZone::getTransitions` — transcribed method builder.
fn decl_class_datetimezone_method_18_gettransitions() -> MethodBuilder {
method("getTransitions")
    .param_default("timestampBegin", TypeExpr::Int, e_int(-9223372036854775808))
    .param_default("timestampEnd", TypeExpr::Int, e_int(2147483647))
    .returns(t_mixed())
    .body_exact(vec![
        s_expr(e_method_call(e_this(), "__elephc_assert_initialized", vec![])),
        s_assign("raw", e_call("elephc_tz_transitions", vec![e_this_prop("name")])),
        s_if(
            e_binop(e_var("raw"), BinOp::StrictEq, e_str("")),
            vec![
                s_return(e_bool(false)),
            ],
            vec![],
            None,
        ),
        s_assign("lines", e_call("explode", vec![e_str("\n"), e_var("raw")])),
        s_assign("lineCount", e_call("count", vec![e_var("lines")])),
        s_assign("result", e_array(vec![])),
        s_assign("resultIndex", e_int(0)),
        s_assign("activeFound", e_bool(false)),
        s_assign("activeTs", e_int(0)),
        s_assign("activeOffset", e_int(0)),
        s_assign("activeDst", e_bool(false)),
        s_assign("activeAbbr", e_str("")),
        s_assign("activeTime", e_str("")),
        s_assign("i", e_int(0)),
        s_while(e_binop(e_var("i"), BinOp::Lt, e_var("lineCount")), vec![
            s_assign("g", e_call("explode", vec![e_str("\t"), e_index(e_var("lines"), e_var("i"))])),
            s_assign("ts", e_cast(CastType::Int, e_index(e_var("g"), e_int(0)))),
            s_if(
                e_binop(e_var("ts"), BinOp::LtEq, e_var("timestampBegin")),
                vec![
                    s_assign("activeFound", e_bool(true)),
                    s_assign("activeTs", e_var("ts")),
                    s_assign("activeOffset", e_cast(CastType::Int, e_index(e_var("g"), e_int(1)))),
                    s_assign("activeDst", e_binop(e_index(e_var("g"), e_int(2)), BinOp::StrictEq, e_str("1"))),
                    s_assign("activeAbbr", e_index(e_var("g"), e_int(3))),
                    s_assign("activeTime", e_index(e_var("g"), e_int(4))),
                ],
                vec![],
                None,
            ),
            s_assign("i", e_call("intval", vec![e_binop(e_var("i"), BinOp::Add, e_int(1))])),
        ]),
        s_if(
            e_var("activeFound"),
            vec![
                s_array_assign("result", e_var("resultIndex"), e_array_assoc(vec![(e_str("ts"), e_ternary(e_binop(e_var("timestampBegin"), BinOp::LtEq, e_var("activeTs")), e_var("activeTs"), e_var("timestampBegin"))), (e_str("time"), e_ternary(e_binop(e_var("timestampBegin"), BinOp::LtEq, e_var("activeTs")), e_var("activeTime"), e_call("gmdate", vec![e_str("Y-m-d\\TH:i:sP"), e_var("timestampBegin")]))), (e_str("offset"), e_var("activeOffset")), (e_str("isdst"), e_var("activeDst")), (e_str("abbr"), e_var("activeAbbr"))])),
                s_assign("resultIndex", e_call("intval", vec![e_binop(e_var("resultIndex"), BinOp::Add, e_int(1))])),
            ],
            vec![],
            None,
        ),
        s_assign("i", e_int(0)),
        s_while(e_binop(e_var("i"), BinOp::Lt, e_var("lineCount")), vec![
            s_assign("g", e_call("explode", vec![e_str("\t"), e_index(e_var("lines"), e_var("i"))])),
            s_assign("ts", e_cast(CastType::Int, e_index(e_var("g"), e_int(0)))),
            s_if(
                e_binop(e_binop(e_var("ts"), BinOp::Gt, e_var("timestampBegin")), BinOp::And, e_binop(e_var("ts"), BinOp::LtEq, e_var("timestampEnd"))),
                vec![
                    s_array_assign("result", e_var("resultIndex"), e_array_assoc(vec![(e_str("ts"), e_var("ts")), (e_str("time"), e_index(e_var("g"), e_int(4))), (e_str("offset"), e_cast(CastType::Int, e_index(e_var("g"), e_int(1)))), (e_str("isdst"), e_binop(e_index(e_var("g"), e_int(2)), BinOp::StrictEq, e_str("1"))), (e_str("abbr"), e_index(e_var("g"), e_int(3)))])),
                    s_assign("resultIndex", e_call("intval", vec![e_binop(e_var("resultIndex"), BinOp::Add, e_int(1))])),
                ],
                vec![],
                None,
            ),
            s_assign("i", e_call("intval", vec![e_binop(e_var("i"), BinOp::Add, e_int(1))])),
        ]),
        s_return(e_call("array_slice", vec![e_var("result"), e_int(0), e_var("resultIndex")])),
    ])
}

/// `DateTimeZone::listAbbreviations` — transcribed method builder.
fn decl_class_datetimezone_method_19_listabbreviations() -> MethodBuilder {
method("listAbbreviations")
    .static_()
    .returns(t_array())
    .body_exact(vec![
        s_assign("raw", e_call("elephc_tz_abbreviations", vec![])),
        s_assign("lines", e_call("explode", vec![e_str("\n"), e_var("raw")])),
        s_assign("result", e_array(vec![])),
        s_foreach(e_var("lines"), None, "line", vec![
            s_assign("parts", e_call("explode", vec![e_str("\t"), e_var("line")])),
            s_assign("abbr", e_index(e_var("parts"), e_int(0))),
            s_assign("rows", e_call("explode", vec![e_str(";"), e_index(e_var("parts"), e_int(1))])),
            s_assign("arr", e_array(vec![])),
            s_foreach(e_var("rows"), None, "row", vec![
                s_assign("c", e_call("explode", vec![e_str(":"), e_var("row")])),
                s_assign("id", e_index(e_var("c"), e_int(2))),
                s_array_push("arr", e_array_assoc(vec![(e_str("dst"), e_binop(e_index(e_var("c"), e_int(0)), BinOp::StrictEq, e_str("1"))), (e_str("offset"), e_cast(CastType::Int, e_index(e_var("c"), e_int(1)))), (e_str("timezone_id"), e_ternary(e_binop(e_var("id"), BinOp::StrictEq, e_str("NULL")), e_null(), e_var("id")))])),
            ]),
            s_array_assign("result", e_var("abbr"), e_var("arr")),
        ]),
        s_return(e_var("result")),
    ])
}

/// `DateTimeZone::__elephc_assert_initialized` — transcribed method builder.
fn decl_class_datetimezone_method_20_elephc_assert_initialized() -> MethodBuilder {
method("__elephc_assert_initialized")
    .final_()
    .returns(TypeExpr::Void)
    .body_exact(vec![
        s_if(
            e_not(e_this_prop("__elephc_initialized")),
            vec![
                s_assign("objectClass", e_call("get_class", vec![e_this()])),
                s_assign("inheritance", e_ternary(e_binop(e_var("objectClass"), BinOp::StrictEq, e_str("DateTimeZone")), e_str(""), e_str(" (inheriting DateTimeZone)"))),
                s_throw(e_new("DateObjectError", vec![e_binop(e_binop(e_binop(e_str("Object of type "), BinOp::Concat, e_var("objectClass")), BinOp::Concat, e_var("inheritance")), BinOp::Concat, e_str(" has not been correctly initialized by calling parent::__construct() in its constructor"))])),
            ],
            vec![],
            None,
        ),
    ])
}

/// `DateTimeZone` — transcribed from the PHP form.
fn decl_class_datetimezone() -> Stmt {
    class("DateTimeZone")
        .constant_full("AFRICA", e_int(1), Some(TypeExpr::Int), vec![])
        .constant_full("AMERICA", e_int(2), Some(TypeExpr::Int), vec![])
        .constant_full("ANTARCTICA", e_int(4), Some(TypeExpr::Int), vec![])
        .constant_full("ARCTIC", e_int(8), Some(TypeExpr::Int), vec![])
        .constant_full("ASIA", e_int(16), Some(TypeExpr::Int), vec![])
        .constant_full("ATLANTIC", e_int(32), Some(TypeExpr::Int), vec![])
        .constant_full("AUSTRALIA", e_int(64), Some(TypeExpr::Int), vec![])
        .constant_full("EUROPE", e_int(128), Some(TypeExpr::Int), vec![])
        .constant_full("INDIAN", e_int(256), Some(TypeExpr::Int), vec![])
        .constant_full("PACIFIC", e_int(512), Some(TypeExpr::Int), vec![])
        .constant_full("UTC", e_int(1024), Some(TypeExpr::Int), vec![])
        .constant_full("ALL", e_int(2047), Some(TypeExpr::Int), vec![])
        .constant_full("ALL_WITH_BC", e_int(4095), Some(TypeExpr::Int), vec![])
        .constant_full("PER_COUNTRY", e_int(4096), Some(TypeExpr::Int), vec![])
        .private_prop("name", TypeExpr::Str, Some(e_str("UTC")))
        .private_prop("__elephc_initialized", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("__elephc_arguments", t_mixed(), Some(e_null()))
        .private_prop("__elephc_seen_named_argument", TypeExpr::Bool, Some(e_bool(false)))
        .method(decl_class_datetimezone_method_0_elephc_normalize_timezone())
        .method(decl_class_datetimezone_method_1_construct())
        .method(decl_class_datetimezone_method_2_elephc_timezone_open())
        .method(decl_class_datetimezone_method_3_getname())
        .method(decl_class_datetimezone_method_4_getoffset())
        .method(decl_class_datetimezone_method_5_listidentifiers())
        .method(decl_class_datetimezone_method_6_elephc_compare())
        .method(decl_class_datetimezone_method_7_elephc_begin_argument_array())
        .method(decl_class_datetimezone_method_8_elephc_append_one_argument())
        .method(decl_class_datetimezone_method_9_elephc_append_argument_chunk())
        .method(decl_class_datetimezone_method_10_elephc_finish_argument_array())
        .method(decl_class_datetimezone_method_11_wakeup())
        .method(decl_class_datetimezone_method_12_serialize())
        .method(decl_class_datetimezone_method_13_unserialize())
        .method(decl_class_datetimezone_method_14_set_state())
        .method(decl_class_datetimezone_method_15_elephc_debug_dump())
        .method(decl_class_datetimezone_method_16_elephc_print_r_dump())
        .method(decl_class_datetimezone_method_17_getlocation())
        .method(decl_class_datetimezone_method_18_gettransitions())
        .method(decl_class_datetimezone_method_19_listabbreviations())
        .method(decl_class_datetimezone_method_20_elephc_assert_initialized())
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn generated_datetime_declarations() -> Program {
    vec![
            decl_stmt_bootstrap_1(),
            decl_class_dateinterval(),
            decl_class_dateperiod(),
            decl_class_datetime(),
            decl_class_datetimeimmutable(),
            decl_class_datetimezone(),
    ]
}
