//! Purpose:
//! Generated direct AST for timelib bridge declarations and decoding helpers.
//!
//! Called from:
//! - `crate::tz_prelude::inject_if_used()`.
//!
//! Key details:
//! - Generated from a test-only PHP oracle; production performs no PHP parsing.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `elephc_tz_mktime` — transcribed from the PHP form.
fn decl_extern_elephc_tz_mktime() -> Stmt {
    extern_fn("elephc_tz_mktime", "elephc_tz")
        .param("hour", CType::Int)
        .param("minute", CType::Int)
        .param("second", CType::Int)
        .param("month", CType::Int)
        .param("day", CType::Int)
        .param("year", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tz_gmmktime` — transcribed from the PHP form.
fn decl_extern_elephc_tz_gmmktime() -> Stmt {
    extern_fn("elephc_tz_gmmktime", "elephc_tz")
        .param("hour", CType::Int)
        .param("minute", CType::Int)
        .param("second", CType::Int)
        .param("month", CType::Int)
        .param("day", CType::Int)
        .param("year", CType::Int)
        .returns(CType::Int)
        .build()
}

/// `elephc_tz_format_civil` — transcribed from the PHP form.
fn decl_extern_elephc_tz_format_civil() -> Stmt {
    extern_fn("elephc_tz_format_civil", "elephc_tz")
        .param("timestamp", CType::Int)
        .param("microsecond", CType::Int)
        .param("format", CType::Str)
        .param("format_length", CType::Int)
        .param("payload", CType::Str)
        .param("payload_length", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_date_parse` — transcribed from the PHP form.
fn decl_extern_elephc_tz_date_parse() -> Stmt {
    extern_fn("elephc_tz_date_parse", "elephc_tz")
        .param("datetime", CType::Str)
        .param("datetime_length", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_date_parse_from_format` — transcribed from the PHP form.
fn decl_extern_elephc_tz_date_parse_from_format() -> Stmt {
    extern_fn("elephc_tz_date_parse_from_format", "elephc_tz")
        .param("format", CType::Str)
        .param("format_length", CType::Int)
        .param("datetime", CType::Str)
        .param("datetime_length", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_create_from_format` — transcribed from the PHP form.
fn decl_extern_elephc_tz_create_from_format() -> Stmt {
    extern_fn("elephc_tz_create_from_format", "elephc_tz")
        .param("format", CType::Str)
        .param("format_length", CType::Int)
        .param("datetime", CType::Str)
        .param("datetime_length", CType::Int)
        .param("base_timestamp", CType::Int)
        .param("timezone", CType::Str)
        .param("timezone_length", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_interval_parse` — transcribed from the PHP form.
fn decl_extern_elephc_tz_interval_parse() -> Stmt {
    extern_fn("elephc_tz_interval_parse", "elephc_tz")
        .param("input", CType::Str)
        .param("input_length", CType::Int)
        .param("relative", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_period_parse` — transcribed from the PHP form.
fn decl_extern_elephc_tz_period_parse() -> Stmt {
    extern_fn("elephc_tz_period_parse", "elephc_tz")
        .param("input", CType::Str)
        .param("input_length", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_apply_interval` — transcribed from the PHP form.
fn decl_extern_elephc_tz_apply_interval() -> Stmt {
    extern_fn("elephc_tz_apply_interval", "elephc_tz")
        .param("timestamp", CType::Int)
        .param("microsecond", CType::Int)
        .param("timezone", CType::Str)
        .param("timezone_length", CType::Int)
        .param("payload", CType::Str)
        .param("payload_length", CType::Int)
        .param("subtract", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_modify` — transcribed from the PHP form.
fn decl_extern_elephc_tz_modify() -> Stmt {
    extern_fn("elephc_tz_modify", "elephc_tz")
        .param("timestamp", CType::Int)
        .param("microsecond", CType::Int)
        .param("timezone", CType::Str)
        .param("timezone_length", CType::Int)
        .param("modifier", CType::Str)
        .param("modifier_length", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_set_civil` — transcribed from the PHP form.
fn decl_extern_elephc_tz_set_civil() -> Stmt {
    extern_fn("elephc_tz_set_civil", "elephc_tz")
        .param("timestamp", CType::Int)
        .param("microsecond", CType::Int)
        .param("timezone", CType::Str)
        .param("timezone_length", CType::Int)
        .param("payload", CType::Str)
        .param("payload_length", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_set_iso_date` — transcribed from the PHP form.
fn decl_extern_elephc_tz_set_iso_date() -> Stmt {
    extern_fn("elephc_tz_set_iso_date", "elephc_tz")
        .param("timestamp", CType::Int)
        .param("microsecond", CType::Int)
        .param("timezone", CType::Str)
        .param("timezone_length", CType::Int)
        .param("year", CType::Int)
        .param("week", CType::Int)
        .param("day", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `elephc_tz_diff` — transcribed from the PHP form.
fn decl_extern_elephc_tz_diff() -> Stmt {
    extern_fn("elephc_tz_diff", "elephc_tz")
        .param("left_timestamp", CType::Int)
        .param("left_microsecond", CType::Int)
        .param("left_timezone", CType::Str)
        .param("left_timezone_length", CType::Int)
        .param("right_timestamp", CType::Int)
        .param("right_microsecond", CType::Int)
        .param("right_timezone", CType::Str)
        .param("right_timezone_length", CType::Int)
        .returns(CType::Str)
        .build()
}

/// `__elephc_timelib_optional_int` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_optional_int() -> Stmt {
    function("__elephc_timelib_optional_int")
        .param("value", TypeExpr::Int)
        .param("unset", TypeExpr::Int)
        .returns(t_mixed())
        .body_exact(vec![
            s_if(
                e_binop(e_var("value"), BinOp::StrictEq, e_var("unset")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_var("value")),
        ])
        .build()
}

/// `__elephc_timelib_optional_fraction` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_optional_fraction() -> Stmt {
    function("__elephc_timelib_optional_fraction")
        .param("microsecond", TypeExpr::Int)
        .param("unset", TypeExpr::Int)
        .returns(t_mixed())
        .body_exact(vec![
            s_if(
                e_binop(e_var("microsecond"), BinOp::StrictEq, e_var("unset")),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_var("microsecond"), BinOp::Div, e_float(1000000.0))),
        ])
        .build()
}

/// `__elephc_timelib_decode_parse_result` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_decode_parse_result() -> Stmt {
    function("__elephc_timelib_decode_parse_result")
        .param("serialized", TypeExpr::Str)
        .body_exact(vec![
            s_assign("lines", e_call("explode", vec![e_str("\n"), e_var("serialized")])),
            s_assign("header", e_call("explode", vec![e_str("\t"), e_index(e_var("lines"), e_int(0))])),
            s_assign("unset", e_neg(e_int(9999999))),
            s_assign("year", e_call("intval", vec![e_index(e_var("header"), e_int(1))])),
            s_assign("month", e_call("intval", vec![e_index(e_var("header"), e_int(2))])),
            s_assign("day", e_call("intval", vec![e_index(e_var("header"), e_int(3))])),
            s_assign("hour", e_call("intval", vec![e_index(e_var("header"), e_int(4))])),
            s_assign("minute", e_call("intval", vec![e_index(e_var("header"), e_int(5))])),
            s_assign("second", e_call("intval", vec![e_index(e_var("header"), e_int(6))])),
            s_assign("microsecond", e_call("intval", vec![e_index(e_var("header"), e_int(7))])),
            s_assign("warnings", e_array_assoc(vec![(e_str(""), e_str(""))])),
            s_expr(e_call("unset", vec![e_index(e_var("warnings"), e_str(""))])),
            s_assign("errors", e_array_assoc(vec![(e_str(""), e_str(""))])),
            s_expr(e_call("unset", vec![e_index(e_var("errors"), e_str(""))])),
            s_assign("relative", e_array(vec![])),
            s_assign("hasRelative", e_bool(false)),
            s_assign("lineCount", e_call("count", vec![e_var("lines")])),
            s_assign("lineIndex", e_int(1)),
            s_while(e_binop(e_var("lineIndex"), BinOp::Lt, e_var("lineCount")), vec![
                s_assign("parts", e_call("explode", vec![e_str("\t"), e_index(e_var("lines"), e_var("lineIndex"))])),
                s_if(
                    e_binop(e_index(e_var("parts"), e_int(0)), BinOp::StrictEq, e_str("W")),
                    vec![
                        s_array_assign("warnings", e_call("intval", vec![e_index(e_var("parts"), e_int(1))]), e_index(e_var("parts"), e_int(2))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_index(e_var("parts"), e_int(0)), BinOp::StrictEq, e_str("E")),
                        vec![
                            s_array_assign("errors", e_call("intval", vec![e_index(e_var("parts"), e_int(1))]), e_index(e_var("parts"), e_int(2))),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_index(e_var("parts"), e_int(0)), BinOp::StrictEq, e_str("R")),
                            vec![
                                s_assign("hasRelative", e_bool(true)),
                                s_assign("relative", e_array_assoc(vec![(e_str("year"), e_call("intval", vec![e_index(e_var("parts"), e_int(1))])), (e_str("month"), e_call("intval", vec![e_index(e_var("parts"), e_int(2))])), (e_str("day"), e_call("intval", vec![e_index(e_var("parts"), e_int(3))])), (e_str("hour"), e_call("intval", vec![e_index(e_var("parts"), e_int(4))])), (e_str("minute"), e_call("intval", vec![e_index(e_var("parts"), e_int(5))])), (e_str("second"), e_call("intval", vec![e_index(e_var("parts"), e_int(6))]))])),
                                s_if(
                                    e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(7))]), BinOp::StrictNotEq, e_var("unset")),
                                    vec![
                                        s_array_assign("relative", e_str("weekday"), e_call("intval", vec![e_index(e_var("parts"), e_int(7))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(8))]), BinOp::StrictNotEq, e_var("unset")),
                                    vec![
                                        s_array_assign("relative", e_str("weekdays"), e_call("intval", vec![e_index(e_var("parts"), e_int(8))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(9))]), BinOp::StrictEq, e_int(1)),
                                    vec![
                                        s_array_assign("relative", e_str("first_day_of_month"), e_bool(true)),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_if(
                                        e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(9))]), BinOp::StrictEq, e_int(2)),
                                        vec![
                                            s_array_assign("relative", e_str("last_day_of_month"), e_bool(true)),
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
                    ]),
                    ),
                ]),
                ),
                s_assign("lineIndex", e_binop(e_var("lineIndex"), BinOp::Add, e_int(1))),
            ]),
            s_assign("result", e_array_assoc(vec![(e_str("year"), e_call("__elephc_timelib_optional_int", vec![e_var("year"), e_var("unset")])), (e_str("month"), e_call("__elephc_timelib_optional_int", vec![e_var("month"), e_var("unset")])), (e_str("day"), e_call("__elephc_timelib_optional_int", vec![e_var("day"), e_var("unset")])), (e_str("hour"), e_call("__elephc_timelib_optional_int", vec![e_var("hour"), e_var("unset")])), (e_str("minute"), e_call("__elephc_timelib_optional_int", vec![e_var("minute"), e_var("unset")])), (e_str("second"), e_call("__elephc_timelib_optional_int", vec![e_var("second"), e_var("unset")])), (e_str("fraction"), e_call("__elephc_timelib_optional_fraction", vec![e_var("microsecond"), e_var("unset")])), (e_str("warning_count"), e_call("intval", vec![e_index(e_var("header"), e_int(14))])), (e_str("warnings"), e_var("warnings")), (e_str("error_count"), e_call("intval", vec![e_index(e_var("header"), e_int(15))])), (e_str("errors"), e_var("errors")), (e_str("is_localtime"), e_binop(e_call("intval", vec![e_index(e_var("header"), e_int(8))]), BinOp::StrictNotEq, e_int(0)))])),
            s_if(
                e_index(e_var("result"), e_str("is_localtime")),
                vec![
                    s_assign("zoneType", e_call("intval", vec![e_index(e_var("header"), e_int(9))])),
                    s_array_assign("result", e_str("zone_type"), e_var("zoneType")),
                    s_if(
                        e_binop(e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(1)), BinOp::Or, e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(2))),
                        vec![
                            s_array_assign("result", e_str("zone"), e_call("intval", vec![e_index(e_var("header"), e_int(10))])),
                            s_array_assign("result", e_str("is_dst"), e_binop(e_call("intval", vec![e_index(e_var("header"), e_int(11))]), BinOp::StrictNotEq, e_int(0))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(2)), BinOp::Or, e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(3))), BinOp::And, e_binop(e_index(e_var("header"), e_int(12)), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_array_assign("result", e_str("tz_abbr"), e_index(e_var("header"), e_int(12))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("zoneType"), BinOp::StrictEq, e_int(3)), BinOp::And, e_binop(e_index(e_var("header"), e_int(13)), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_array_assign("result", e_str("tz_id"), e_index(e_var("header"), e_int(13))),
                        ],
                        vec![],
                        None,
                    ),
                ],
                vec![],
                None,
            ),
            s_if(
                e_var("hasRelative"),
                vec![
                    s_array_assign("result", e_str("relative"), e_var("relative")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_timelib_date_parse` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_date_parse() -> Stmt {
    function("__elephc_timelib_date_parse")
        .param("datetime", TypeExpr::Str)
        .body_exact(vec![
            s_return(e_call("__elephc_timelib_decode_parse_result", vec![e_call("elephc_tz_date_parse", vec![e_var("datetime"), e_call("strlen", vec![e_var("datetime")])])])),
        ])
        .build()
}

/// `__elephc_timelib_date_parse_from_format` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_date_parse_from_format() -> Stmt {
    function("__elephc_timelib_date_parse_from_format")
        .param("format", TypeExpr::Str)
        .param("datetime", TypeExpr::Str)
        .body_exact(vec![
            s_return(e_call("__elephc_timelib_decode_parse_result", vec![e_call("elephc_tz_date_parse_from_format", vec![e_var("format"), e_call("strlen", vec![e_var("format")]), e_var("datetime"), e_call("strlen", vec![e_var("datetime")])])])),
        ])
        .build()
}

/// `__elephc_timelib_create_from_format` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_create_from_format() -> Stmt {
    function("__elephc_timelib_create_from_format")
        .param("format", TypeExpr::Str)
        .param("datetime", TypeExpr::Str)
        .param("timezone", TypeExpr::Str)
        .body_exact(vec![
            s_assign("serialized", e_call("elephc_tz_create_from_format", vec![e_var("format"), e_call("strlen", vec![e_var("format")]), e_var("datetime"), e_call("strlen", vec![e_var("datetime")]), e_call("time", vec![]), e_var("timezone"), e_call("strlen", vec![e_var("timezone")])])),
            s_assign("result", e_call("__elephc_timelib_decode_parse_result", vec![e_var("serialized")])),
            s_assign("header", e_call("explode", vec![e_str("\t"), e_index(e_call("explode", vec![e_str("\n"), e_var("serialized")]), e_int(0))])),
            s_array_assign("result", e_str("__elephc_timestamp"), e_call("intval", vec![e_index(e_var("header"), e_int(17))])),
            s_array_assign("result", e_str("__elephc_serialized"), e_var("serialized")),
            s_return(e_var("result")),
        ])
        .build()
}

/// `__elephc_timelib_decode_interval_result` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_decode_interval_result() -> Stmt {
    function("__elephc_timelib_decode_interval_result")
        .param("serialized", TypeExpr::Str)
        .body_exact(vec![
            s_assign("parts", e_call("explode", vec![e_str("\t"), e_var("serialized")])),
            s_if(
                e_binop(e_index(e_var("parts"), e_int(0)), BinOp::StrictEq, e_str("O")),
                vec![
                    s_return(e_array_assoc(vec![(e_str("status"), e_str("O")), (e_str("y"), e_call("intval", vec![e_index(e_var("parts"), e_int(1))])), (e_str("m"), e_call("intval", vec![e_index(e_var("parts"), e_int(2))])), (e_str("d"), e_call("intval", vec![e_index(e_var("parts"), e_int(3))])), (e_str("h"), e_call("intval", vec![e_index(e_var("parts"), e_int(4))])), (e_str("i"), e_call("intval", vec![e_index(e_var("parts"), e_int(5))])), (e_str("s"), e_call("intval", vec![e_index(e_var("parts"), e_int(6))])), (e_str("us"), e_call("intval", vec![e_index(e_var("parts"), e_int(7))])), (e_str("invert"), e_call("intval", vec![e_index(e_var("parts"), e_int(8))])), (e_str("days"), e_call("intval", vec![e_index(e_var("parts"), e_int(9))])), (e_str("position"), e_int(0)), (e_str("character"), e_str("")), (e_str("message"), e_str(""))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_binop(e_index(e_var("parts"), e_int(0)), BinOp::StrictEq, e_str("E")), BinOp::And, e_binop(e_call("count", vec![e_var("parts")]), BinOp::GtEq, e_int(4))),
                vec![
                    s_return(e_array_assoc(vec![(e_str("status"), e_str("E")), (e_str("y"), e_int(0)), (e_str("m"), e_int(0)), (e_str("d"), e_int(0)), (e_str("h"), e_int(0)), (e_str("i"), e_int(0)), (e_str("s"), e_int(0)), (e_str("us"), e_int(0)), (e_str("invert"), e_int(0)), (e_str("days"), e_neg(e_int(9999999))), (e_str("position"), e_call("intval", vec![e_index(e_var("parts"), e_int(1))])), (e_str("character"), e_call("chr", vec![e_call("intval", vec![e_index(e_var("parts"), e_int(2))])])), (e_str("message"), e_index(e_var("parts"), e_int(3)))])),
                ],
                vec![],
                None,
            ),
            s_return(e_array_assoc(vec![(e_str("status"), e_index(e_var("parts"), e_int(0))), (e_str("y"), e_int(0)), (e_str("m"), e_int(0)), (e_str("d"), e_int(0)), (e_str("h"), e_int(0)), (e_str("i"), e_int(0)), (e_str("s"), e_int(0)), (e_str("us"), e_int(0)), (e_str("invert"), e_int(0)), (e_str("days"), e_neg(e_int(9999999))), (e_str("position"), e_int(0)), (e_str("character"), e_str("")), (e_str("message"), e_str(""))])),
        ])
        .build()
}

/// `__elephc_timelib_interval_parse` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_interval_parse() -> Stmt {
    function("__elephc_timelib_interval_parse")
        .param("input", TypeExpr::Str)
        .param("relative", TypeExpr::Bool)
        .body_exact(vec![
            s_return(e_call("__elephc_timelib_decode_interval_result", vec![e_call("elephc_tz_interval_parse", vec![e_var("input"), e_call("strlen", vec![e_var("input")]), e_ternary(e_var("relative"), e_int(1), e_int(0))])])),
        ])
        .build()
}

/// `__elephc_timelib_interval_restore_parse` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_interval_restore_parse() -> Stmt {
    function("__elephc_timelib_interval_restore_parse")
        .param("input", TypeExpr::Str)
        .body_exact(vec![
            s_return(e_call("__elephc_timelib_decode_interval_result", vec![e_call("elephc_tz_interval_parse", vec![e_var("input"), e_call("strlen", vec![e_var("input")]), e_int(2)])])),
        ])
        .build()
}

/// `__elephc_timelib_period_parse` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_period_parse() -> Stmt {
    function("__elephc_timelib_period_parse")
        .param("input", TypeExpr::Str)
        .body_exact(vec![
            s_assign("parts", e_call("explode", vec![e_str("\t"), e_call("elephc_tz_period_parse", vec![e_var("input"), e_call("strlen", vec![e_var("input")])])])),
            s_if(
                e_binop(e_index(e_var("parts"), e_int(0)), BinOp::StrictEq, e_str("P")),
                vec![
                    s_return(e_array_assoc(vec![(e_str("status"), e_str("P")), (e_str("has_start"), e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(1))]), BinOp::StrictNotEq, e_int(0))), (e_str("start"), e_call("intval", vec![e_index(e_var("parts"), e_int(2))])), (e_str("has_end"), e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(3))]), BinOp::StrictNotEq, e_int(0))), (e_str("end"), e_call("intval", vec![e_index(e_var("parts"), e_int(4))])), (e_str("has_interval"), e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(5))]), BinOp::StrictNotEq, e_int(0))), (e_str("recurrences"), e_call("intval", vec![e_index(e_var("parts"), e_int(6))])), (e_str("y"), e_call("intval", vec![e_index(e_var("parts"), e_int(7))])), (e_str("m"), e_call("intval", vec![e_index(e_var("parts"), e_int(8))])), (e_str("d"), e_call("intval", vec![e_index(e_var("parts"), e_int(9))])), (e_str("h"), e_call("intval", vec![e_index(e_var("parts"), e_int(10))])), (e_str("i"), e_call("intval", vec![e_index(e_var("parts"), e_int(11))])), (e_str("s"), e_call("intval", vec![e_index(e_var("parts"), e_int(12))])), (e_str("us"), e_call("intval", vec![e_index(e_var("parts"), e_int(13))]))])),
                ],
                vec![],
                None,
            ),
            s_return(e_array_assoc(vec![(e_str("status"), e_index(e_var("parts"), e_int(0))), (e_str("has_start"), e_bool(false)), (e_str("start"), e_int(0)), (e_str("has_end"), e_bool(false)), (e_str("end"), e_int(0)), (e_str("has_interval"), e_bool(false)), (e_str("recurrences"), e_int(0)), (e_str("y"), e_int(0)), (e_str("m"), e_int(0)), (e_str("d"), e_int(0)), (e_str("h"), e_int(0)), (e_str("i"), e_int(0)), (e_str("s"), e_int(0)), (e_str("us"), e_int(0))])),
        ])
        .build()
}

/// `__elephc_timelib_apply_interval` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_apply_interval() -> Stmt {
    function("__elephc_timelib_apply_interval")
        .param("timestamp", TypeExpr::Int)
        .param("microsecond", TypeExpr::Int)
        .param("timezone", TypeExpr::Str)
        .param("payload", TypeExpr::Str)
        .param("subtract", TypeExpr::Bool)
        .body_exact(vec![
            s_assign("parts", e_call("explode", vec![e_str("\t"), e_call("elephc_tz_apply_interval", vec![e_var("timestamp"), e_var("microsecond"), e_var("timezone"), e_call("strlen", vec![e_var("timezone")]), e_var("payload"), e_call("strlen", vec![e_var("payload")]), e_ternary(e_var("subtract"), e_int(1), e_int(0))])])),
            s_return(e_array_assoc(vec![(e_str("timestamp"), e_call("intval", vec![e_index(e_var("parts"), e_int(0))])), (e_str("microsecond"), e_call("intval", vec![e_index(e_var("parts"), e_int(1))])), (e_str("warning"), e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(2))]), BinOp::StrictNotEq, e_int(0)))])),
        ])
        .build()
}

/// `__elephc_timelib_modify` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_modify() -> Stmt {
    function("__elephc_timelib_modify")
        .param("timestamp", TypeExpr::Int)
        .param("microsecond", TypeExpr::Int)
        .param("timezone", TypeExpr::Str)
        .param("modifier", TypeExpr::Str)
        .body_exact(vec![
            s_assign("serialized", e_call("elephc_tz_modify", vec![e_var("timestamp"), e_var("microsecond"), e_var("timezone"), e_call("strlen", vec![e_var("timezone")]), e_var("modifier"), e_call("strlen", vec![e_var("modifier")])])),
            s_assign("lineBreak", e_call("strpos", vec![e_var("serialized"), e_str("\n")])),
            s_assign("metadataLine", e_ternary(e_binop(e_var("lineBreak"), BinOp::StrictEq, e_bool(false)), e_var("serialized"), e_call("substr", vec![e_var("serialized"), e_int(0), e_var("lineBreak")]))),
            s_assign("parse", e_ternary(e_binop(e_var("lineBreak"), BinOp::StrictEq, e_bool(false)), e_str(""), e_call("substr", vec![e_var("serialized"), e_binop(e_var("lineBreak"), BinOp::Add, e_int(1))]))),
            s_assign("parts", e_call("explode", vec![e_str("\t"), e_var("metadataLine")])),
            s_if(
                e_binop(e_binop(e_call("count", vec![e_var("parts")]), BinOp::GtEq, e_int(4)), BinOp::And, e_binop(e_index(e_var("parts"), e_int(0)), BinOp::StrictEq, e_str("O"))),
                vec![
                    s_return(e_array_assoc(vec![(e_str("status"), e_str("O")), (e_str("timestamp"), e_call("intval", vec![e_index(e_var("parts"), e_int(1))])), (e_str("microsecond"), e_call("intval", vec![e_index(e_var("parts"), e_int(2))])), (e_str("reset_to_utc"), e_binop(e_call("intval", vec![e_index(e_var("parts"), e_int(3))]), BinOp::StrictNotEq, e_int(0))), (e_str("parse"), e_var("parse"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_array_assoc(vec![(e_str("status"), e_str("E")), (e_str("timestamp"), e_int(0)), (e_str("microsecond"), e_int(0)), (e_str("reset_to_utc"), e_bool(false)), (e_str("parse"), e_var("parse"))])),
        ])
        .build()
}

/// `__elephc_timelib_set_civil` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_set_civil() -> Stmt {
    function("__elephc_timelib_set_civil")
        .param("timestamp", TypeExpr::Int)
        .param("microsecond", TypeExpr::Int)
        .param("timezone", TypeExpr::Str)
        .param("payload", TypeExpr::Str)
        .body_exact(vec![
            s_assign("parts", e_call("explode", vec![e_str("\t"), e_call("elephc_tz_set_civil", vec![e_var("timestamp"), e_var("microsecond"), e_var("timezone"), e_call("strlen", vec![e_var("timezone")]), e_var("payload"), e_call("strlen", vec![e_var("payload")])])])),
            s_return(e_array_assoc(vec![(e_str("timestamp"), e_call("intval", vec![e_index(e_var("parts"), e_int(0))])), (e_str("microsecond"), e_call("intval", vec![e_index(e_var("parts"), e_int(1))]))])),
        ])
        .build()
}

/// `__elephc_timelib_set_iso_date` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_set_iso_date() -> Stmt {
    function("__elephc_timelib_set_iso_date")
        .param("timestamp", TypeExpr::Int)
        .param("microsecond", TypeExpr::Int)
        .param("timezone", TypeExpr::Str)
        .param("year", TypeExpr::Int)
        .param("week", TypeExpr::Int)
        .param("day", TypeExpr::Int)
        .body_exact(vec![
            s_assign("parts", e_call("explode", vec![e_str("\t"), e_call("elephc_tz_set_iso_date", vec![e_var("timestamp"), e_var("microsecond"), e_var("timezone"), e_call("strlen", vec![e_var("timezone")]), e_var("year"), e_var("week"), e_var("day")])])),
            s_return(e_array_assoc(vec![(e_str("timestamp"), e_call("intval", vec![e_index(e_var("parts"), e_int(0))])), (e_str("microsecond"), e_call("intval", vec![e_index(e_var("parts"), e_int(1))])), (e_str("year"), e_call("intval", vec![e_index(e_var("parts"), e_int(2))])), (e_str("month"), e_call("intval", vec![e_index(e_var("parts"), e_int(3))])), (e_str("day"), e_call("intval", vec![e_index(e_var("parts"), e_int(4))]))])),
        ])
        .build()
}

/// `__elephc_timelib_diff` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_diff() -> Stmt {
    function("__elephc_timelib_diff")
        .param("leftTimestamp", TypeExpr::Int)
        .param("leftMicrosecond", TypeExpr::Int)
        .param("leftTimezone", TypeExpr::Str)
        .param("rightTimestamp", TypeExpr::Int)
        .param("rightMicrosecond", TypeExpr::Int)
        .param("rightTimezone", TypeExpr::Str)
        .body_exact(vec![
            s_return(e_call("__elephc_timelib_decode_interval_result", vec![e_call("elephc_tz_diff", vec![e_var("leftTimestamp"), e_var("leftMicrosecond"), e_var("leftTimezone"), e_call("strlen", vec![e_var("leftTimezone")]), e_var("rightTimestamp"), e_var("rightMicrosecond"), e_var("rightTimezone"), e_call("strlen", vec![e_var("rightTimezone")])])])),
        ])
        .build()
}

/// `__elephc_timelib_offset_name` — transcribed from the PHP form.
fn decl_fn_elephc_timelib_offset_name() -> Stmt {
    function("__elephc_timelib_offset_name")
        .param("offset", TypeExpr::Int)
        .body_exact(vec![
            s_assign("sign", e_str("+")),
            s_if(
                e_binop(e_var("offset"), BinOp::Lt, e_int(0)),
                vec![
                    s_assign("sign", e_str("-")),
                    s_assign("offset", e_binop(e_int(0), BinOp::Sub, e_var("offset"))),
                ],
                vec![],
                None,
            ),
            s_assign("hours", e_call("intdiv", vec![e_var("offset"), e_int(3600)])),
            s_assign("minutes", e_call("intdiv", vec![e_binop(e_var("offset"), BinOp::Mod, e_int(3600)), e_int(60)])),
            s_assign("seconds", e_binop(e_var("offset"), BinOp::Mod, e_int(60))),
            s_if(
                e_binop(e_var("seconds"), BinOp::StrictNotEq, e_int(0)),
                vec![
                    s_return(e_call("sprintf", vec![e_str("%s%02d:%02d:%02d"), e_var("sign"), e_var("hours"), e_var("minutes"), e_var("seconds")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("sprintf", vec![e_str("%s%02d:%02d"), e_var("sign"), e_var("hours"), e_var("minutes")])),
        ])
        .build()
}

/// `__elephc_timezone_argument_type` — transcribed from the PHP form.
fn decl_fn_elephc_timezone_argument_type() -> Stmt {
    function("__elephc_timezone_argument_type")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body_exact(vec![
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![
                    s_return(e_call("get_class", vec![e_var("value")])),
                ],
                vec![],
                None,
            ),
            s_assign("type", e_call("gettype", vec![e_var("value")])),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("integer")),
                vec![
                    s_return(e_str("int")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("double")),
                vec![
                    s_return(e_str("float")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("boolean")),
                vec![
                    s_return(e_str("bool")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_str("NULL")),
                vec![
                    s_return(e_str("null")),
                ],
                vec![],
                None,
            ),
            s_return(e_var("type")),
        ])
        .build()
}

/// `timezone_offset_get` — transcribed from the PHP form.
fn decl_fn_timezone_offset_get() -> Stmt {
    function("timezone_offset_get")
        .param("object", t_mixed())
        .param("datetime", t_mixed())
        .body_exact(vec![
            s_if(
                e_not(e_instance_of(e_var("object"), "DateTimeZone")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("timezone_offset_get(): Argument #1 ($object) must be of type DateTimeZone, "), BinOp::Concat, e_call("__elephc_timezone_argument_type", vec![e_var("object")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_instance_of(e_var("datetime"), "DateTimeInterface")),
                vec![
                    s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("timezone_offset_get(): Argument #2 ($datetime) must be of type DateTimeInterface, "), BinOp::Concat, e_call("__elephc_timezone_argument_type", vec![e_var("datetime")])), BinOp::Concat, e_str(" given"))])),
                ],
                vec![],
                None,
            ),
            s_return(e_method_call(e_var("object"), "getOffset", vec![e_var("datetime")])),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn timelib_declarations() -> Program {
    vec![
            decl_extern_elephc_tz_mktime(),
            decl_extern_elephc_tz_gmmktime(),
            decl_extern_elephc_tz_format_civil(),
            decl_extern_elephc_tz_date_parse(),
            decl_extern_elephc_tz_date_parse_from_format(),
            decl_extern_elephc_tz_create_from_format(),
            decl_extern_elephc_tz_interval_parse(),
            decl_extern_elephc_tz_period_parse(),
            decl_extern_elephc_tz_apply_interval(),
            decl_extern_elephc_tz_modify(),
            decl_extern_elephc_tz_set_civil(),
            decl_extern_elephc_tz_set_iso_date(),
            decl_extern_elephc_tz_diff(),
            decl_fn_elephc_timelib_optional_int(),
            decl_fn_elephc_timelib_optional_fraction(),
            decl_fn_elephc_timelib_decode_parse_result(),
            decl_fn_elephc_timelib_date_parse(),
            decl_fn_elephc_timelib_date_parse_from_format(),
            decl_fn_elephc_timelib_create_from_format(),
            decl_fn_elephc_timelib_decode_interval_result(),
            decl_fn_elephc_timelib_interval_parse(),
            decl_fn_elephc_timelib_interval_restore_parse(),
            decl_fn_elephc_timelib_period_parse(),
            decl_fn_elephc_timelib_apply_interval(),
            decl_fn_elephc_timelib_modify(),
            decl_fn_elephc_timelib_set_civil(),
            decl_fn_elephc_timelib_set_iso_date(),
            decl_fn_elephc_timelib_diff(),
            decl_fn_elephc_timelib_offset_name(),
            decl_fn_elephc_timezone_argument_type(),
            decl_fn_timezone_offset_get(),
    ]
}
