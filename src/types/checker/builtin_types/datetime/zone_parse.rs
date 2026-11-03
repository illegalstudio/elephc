//! Purpose:
//! Synthetic `DateTime` helpers that split a trailing timezone token from a datetime string
//! and resolve it to the name and offset PHP stores on the object.
//!
//! Called from:
//! - `crate::types::checker::builtin_types::datetime::injection` adds the helpers to
//!   `DateTime`; `DateTime::__construct` calls `__elephc_construct_state`.
//!
//! Key details:
//! - Abbreviations resolve to PHP's tzdb fixed offsets (the first `listAbbreviations()` row),
//!   embedded at compile time from `elephc_tz::abbreviations()`.
//! - The splitter accepts `Z`, `±HH[:MM]` / `±HHMM`, `GMT±...`, `UTC[±...]`, tzdb
//!   abbreviations, and `Continent/City` identifiers.
//! - Fixed offsets are applied by parsing the stripped wall time in UTC and subtracting the
//!   offset; named zones are applied through `date_default_timezone_set`, so DST is honoured.

use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_array, e_array_assoc, e_binop, e_bool, e_call, e_cast, e_concat, e_concat_all, e_index, e_int, e_method_call,
    e_null, e_static_call, e_str, e_ternary, e_var, s_assign, s_if, s_return, s_throw, s_while,
};

use super::ast::method;

/// Builds `DateTime::__elephc_date_malformed_message(string $datetime): string`.
///
/// PHP's parser names the failure position and reason. The common alphabetic-only case
/// (`new DateTime("foobar")`) is a missing timezone; the short form remains for the parser
/// failures this port does not classify yet.
pub(super) fn datetime_date_malformed_message() -> crate::parser::ast::ClassMethod {
    let var = e_var;
    let call = |name: &str, args: Vec<crate::parser::ast::Expr>| e_call(name, args);
    let body = vec![
        s_assign(
            "message",
            e_concat_all(vec![
                e_str("Failed to parse time string ("),
                var("datetime"),
                e_str(")"),
            ]),
        ),
        s_if(
            e_call(
                "ctype_alpha",
                vec![call("substr", vec![var("datetime"), e_int(0), e_int(1)])],
            ),
            vec![s_assign(
                "message",
                e_concat_all(vec![
                    e_str("Failed to parse time string ("),
                    var("datetime"),
                    e_str(") at position 0 ("),
                    call("substr", vec![var("datetime"), e_int(0), e_int(1)]),
                    e_str("): The timezone could not be found in the database"),
                ]),
            )],
            vec![],
            None,
        ),
        s_return(var("message")),
    ];
    let mut built = method(
        "__elephc_date_malformed_message",
        vec![("datetime".to_string(), Some(TypeExpr::Str), None, false)],
        Some(TypeExpr::Str),
        body,
    );
    built.is_static = true;
    built
}

/// Builds `DateTime::__elephc_construct_state(string $datetime, ?DateTimeZone $timezone): array`.
///
/// Returns `[timestamp, timezone_name]`. A timezone inside the string wins over the
/// `$timezone` argument, matching PHP.
pub(super) fn datetime_construct_state() -> crate::parser::ast::ClassMethod {
    let var = e_var;
    let call = |name: &str, args: Vec<crate::parser::ast::Expr>| e_call(name, args);
    let static_call = |method: &str, args: Vec<crate::parser::ast::Expr>| {
        e_static_call("DateTime", method, args)
    };
    let assign = s_assign;
    let failed_throw = || {
        s_throw(crate::synthetic_class::e_new(
            "DateMalformedStringException",
            vec![e_static_call(
                "DateTime",
                "__elephc_date_malformed_message",
                vec![var("datetime")],
            )],
        ))
    };

    let body = vec![
        assign("zone", static_call("__elephc_split_trailing_zone", vec![var("datetime")])),
        s_if(
            e_binop(e_index(var("zone"), e_int(1)), BinOp::StrictNotEq, e_str("")),
            vec![
                assign("saved", call("date_default_timezone_get", vec![])),
                s_if(
                    e_binop(e_index(var("zone"), e_int(3)), BinOp::StrictEq, e_int(1)),
                    vec![
                        s_expr(call(
                            "date_default_timezone_set",
                            vec![e_index(var("zone"), e_int(1))],
                        )),
                        assign(
                            "ts",
                            call("strtotime", vec![e_index(var("zone"), e_int(0))]),
                        ),
                        s_expr(call("date_default_timezone_set", vec![var("saved")])),
                    ],
                    vec![],
                    Some(vec![
                        s_expr(call("date_default_timezone_set", vec![e_str("UTC")])),
                        assign(
                            "ts",
                            call("strtotime", vec![e_index(var("zone"), e_int(0))]),
                        ),
                        s_expr(call("date_default_timezone_set", vec![var("saved")])),
                        s_if(
                            e_binop(var("ts"), BinOp::StrictNotEq, e_bool(false)),
                            vec![assign(
                                "ts",
                                e_binop(
                                    var("ts"),
                                    BinOp::Sub,
                                    e_index(var("zone"), e_int(2)),
                                ),
                            )],
                            vec![],
                            None,
                        ),
                    ]),
                ),
                s_if(
                    e_binop(var("ts"), BinOp::StrictEq, e_bool(false)),
                    vec![failed_throw()],
                    vec![],
                    None,
                ),
                s_return(e_array(vec![
                    var("ts"),
                    e_index(var("zone"), e_int(1)),
                    e_index(var("zone"), e_int(2)),
                    e_binop(e_index(var("zone"), e_int(3)), BinOp::StrictEq, e_int(0)),
                ])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(var("timezone"), BinOp::StrictNotEq, e_null()),
            vec![
                assign(
                    "name",
                    e_method_call(var("timezone"), "getName", vec![]),
                ),
                s_if(
                    e_binop(var("datetime"), BinOp::StrictEq, e_str("now")),
                    vec![assign("ts", call("time", vec![]))],
                    vec![],
                    Some(vec![
                        assign("saved", call("date_default_timezone_get", vec![])),
                        s_expr(call(
                            "date_default_timezone_set",
                            vec![var("name")],
                        )),
                        assign("ts", call("strtotime", vec![var("datetime")])),
                        s_expr(call("date_default_timezone_set", vec![var("saved")])),
                        s_if(
                            e_binop(var("ts"), BinOp::StrictEq, e_bool(false)),
                            vec![failed_throw()],
                            vec![],
                            None,
                        ),
                    ]),
                ),
                s_return(e_array(vec![
                    var("ts"),
                    var("name"),
                    crate::synthetic_class::e_prop(var("timezone"), "__elephc_fixed_offset"),
                    crate::synthetic_class::e_prop(var("timezone"), "__elephc_fixed"),
                ])),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(var("datetime"), BinOp::StrictEq, e_str("now")),
            vec![assign("ts", call("time", vec![]))],
            vec![],
            Some(vec![
                assign("ts", call("strtotime", vec![var("datetime")])),
                s_if(
                    e_binop(var("ts"), BinOp::StrictEq, e_bool(false)),
                    vec![failed_throw()],
                    vec![],
                    None,
                ),
            ]),
        ),
        s_return(e_array(vec![
            var("ts"),
            call("date_default_timezone_get", vec![]),
            e_int(0),
            e_bool(false),
        ])),
    ];
    let mut built = method(
        "__elephc_construct_state",
        vec![
            ("datetime".to_string(), Some(TypeExpr::Str), None, false),
            (
                "timezone".to_string(),
                Some(t_nullable_datetimezone()),
                Some(e_null()),
                false,
            ),
        ],
        Some(t_array_of(t_mixed())),
        body,
    );
    built.is_static = true;
    built
}

/// Builds `DateTime::__elephc_split_trailing_zone(string $datetime): array`.
///
/// Returns `[stripped, name, offset, kind]`; an empty `name` means the string carries no
/// zone. `kind` is `0` for a fixed offset and `1` for an IANA identifier.
pub(super) fn datetime_split_trailing_zone() -> crate::parser::ast::ClassMethod {
    let var = e_var;
    let call = |name: &str, args: Vec<crate::parser::ast::Expr>| e_call(name, args);
    let static_call = |method: &str, args: Vec<crate::parser::ast::Expr>| {
        e_static_call("DateTime", method, args)
    };
    let char_at = |name: &str, index: crate::parser::ast::Expr| {
        call("substr", vec![var(name), index, e_int(1)])
    };
    let body = vec![
        s_assign(
            "none",
            e_array(vec![e_str(""), e_str(""), e_int(0), e_int(0)]),
        ),
        s_assign("end", call("strlen", vec![var("datetime")])),
        s_while(
            e_binop(var("end"), BinOp::Gt, e_int(0)),
            vec![
                s_assign(
                    "c",
                    call(
                        "substr",
                        vec![
                            var("datetime"),
                            e_binop(var("end"), BinOp::Sub, e_int(1)),
                            e_int(1),
                        ],
                    ),
                ),
                s_if(
                    e_binop(
                        e_binop(var("c"), BinOp::StrictEq, e_str(" ")),
                        BinOp::Or,
                        e_binop(
                            e_binop(var("c"), BinOp::StrictEq, e_str("\t")),
                            BinOp::Or,
                            e_binop(
                                e_binop(var("c"), BinOp::StrictEq, e_str("\n")),
                                BinOp::Or,
                                e_binop(var("c"), BinOp::StrictEq, e_str("\r")),
                            ),
                        ),
                    ),
                    vec![],
                    vec![],
                    Some(vec![s_break(1)]),
                ),
                s_assign("end", e_binop(var("end"), BinOp::Sub, e_int(1))),
            ],
        ),
        s_if(
            e_binop(var("end"), BinOp::StrictEq, e_int(0)),
            vec![s_return(var("none"))],
            vec![],
            None,
        ),
        s_assign("start", var("end")),
        s_while(
            e_binop(var("start"), BinOp::Gt, e_int(0)),
            vec![
                s_assign(
                    "c",
                    call(
                        "substr",
                        vec![
                            var("datetime"),
                            e_binop(var("start"), BinOp::Sub, e_int(1)),
                            e_int(1),
                        ],
                    ),
                ),
                s_if(
                    e_binop(
                        e_binop(var("c"), BinOp::StrictEq, e_str(" ")),
                        BinOp::Or,
                        e_binop(
                            e_binop(var("c"), BinOp::StrictEq, e_str("\t")),
                            BinOp::Or,
                            e_binop(
                                e_binop(var("c"), BinOp::StrictEq, e_str("\n")),
                                BinOp::Or,
                                e_binop(var("c"), BinOp::StrictEq, e_str("\r")),
                            ),
                        ),
                    ),
                    vec![s_break(1)],
                    vec![],
                    None,
                ),
                s_assign("start", e_binop(var("start"), BinOp::Sub, e_int(1))),
            ],
        ),
        s_assign(
            "token",
            call(
                "substr",
                vec![
                    var("datetime"),
                    var("start"),
                    e_binop(var("end"), BinOp::Sub, var("start")),
                ],
            ),
        ),
        s_assign("before", call("substr", vec![var("datetime"), e_int(0), var("start")])),
        s_assign(
            "zone",
            static_call("__elephc_parse_zone_token", vec![var("token")]),
        ),
        s_if(
            e_binop(var("zone"), BinOp::StrictNotEq, e_bool(false)),
            vec![s_return(e_array(vec![
                call("rtrim", vec![var("before")]),
                e_index(var("zone"), e_int(0)),
                e_index(var("zone"), e_int(1)),
                e_index(var("zone"), e_int(2)),
            ]))],
            vec![],
            None,
        ),
        s_assign("sign", e_bool(false)),
        s_assign("plus", call("strrpos", vec![var("token"), e_str("+")])),
        s_assign("minus", call("strrpos", vec![var("token"), e_str("-")])),
        s_if(
            e_binop(var("plus"), BinOp::StrictNotEq, e_bool(false)),
            vec![s_assign("sign", var("plus"))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_binop(var("minus"), BinOp::StrictNotEq, e_bool(false)),
                BinOp::And,
                e_binop(
                    e_binop(var("sign"), BinOp::StrictEq, e_bool(false)),
                    BinOp::Or,
                    e_binop(var("minus"), BinOp::Gt, var("sign")),
                ),
            ),
            vec![s_assign("sign", var("minus"))],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_binop(var("sign"), BinOp::StrictNotEq, e_bool(false)),
                BinOp::And,
                e_binop(var("sign"), BinOp::Gt, e_int(0)),
            ),
            vec![
                s_assign(
                    "head",
                    call("substr", vec![var("token"), e_int(0), var("sign")]),
                ),
                s_if(
                    e_binop(
                        e_binop(
                            call("strpos", vec![var("head"), e_str(":")]),
                            BinOp::StrictNotEq,
                            e_bool(false),
                        ),
                        BinOp::Or,
                        e_binop(
                            call("strpos", vec![var("head"), e_str("T")]),
                            BinOp::StrictNotEq,
                            e_bool(false),
                        ),
                    ),
                    vec![
                        s_assign(
                            "zone",
                            static_call(
                                "__elephc_parse_zone_token",
                                vec![call("substr", vec![var("token"), var("sign")])],
                            ),
                        ),
                        s_if(
                            e_binop(var("zone"), BinOp::StrictNotEq, e_bool(false)),
                            vec![s_return(e_array(vec![
                                e_concat(var("before"), var("head")),
                                e_index(var("zone"), e_int(0)),
                                e_index(var("zone"), e_int(1)),
                                e_index(var("zone"), e_int(2)),
                            ]))],
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
        s_assign("last", char_at("token", e_neg_one())),
        s_if(
            e_binop(
                e_binop(var("last"), BinOp::StrictEq, e_str("Z")),
                BinOp::Or,
                e_binop(var("last"), BinOp::StrictEq, e_str("z")),
            ),
            vec![
                s_assign(
                    "head",
                    call(
                        "substr",
                        vec![var("token"), e_int(0), e_neg_one()],
                    ),
                ),
                s_if(
                    e_binop(
                        call("strpos", vec![var("head"), e_str(":")]),
                        BinOp::StrictNotEq,
                        e_bool(false),
                    ),
                    vec![s_return(e_array(vec![
                        e_concat(var("before"), var("head")),
                        e_str("Z"),
                        e_int(0),
                        e_int(0),
                    ]))],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_return(var("none")),
    ];
    let mut built = method(
        "__elephc_split_trailing_zone",
        vec![("datetime".to_string(), Some(TypeExpr::Str), None, false)],
        Some(t_array_of(t_mixed())),
        body,
    );
    built.is_static = true;
    built
}

/// Builds `DateTime::__elephc_parse_zone_token(string $token): array|false`.
///
/// Resolves one whitespace-free token: `Z`, `GMT±...`, `UTC[±...]`, a numeric offset, a
/// tzdb abbreviation, or an IANA identifier. Returns `[name, offset, kind]` or `false`.
pub(super) fn datetime_parse_zone_token() -> crate::parser::ast::ClassMethod {
    let var = e_var;
    let call = |name: &str, args: Vec<crate::parser::ast::Expr>| e_call(name, args);
    let static_call = |method: &str, args: Vec<crate::parser::ast::Expr>| {
        e_static_call("DateTime", method, args)
    };
    let body = vec![
        s_assign("len", call("strlen", vec![var("token")])),
        s_if(
            e_binop(var("len"), BinOp::StrictEq, e_int(0)),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_assign(
            "last",
            call("substr", vec![var("token"), e_neg_one(), e_int(1)]),
        ),
        s_if(
            e_binop(
                e_binop(var("last"), BinOp::StrictEq, e_str("Z")),
                BinOp::Or,
                e_binop(var("last"), BinOp::StrictEq, e_str("z")),
            ),
            vec![s_if(
                e_binop(var("len"), BinOp::StrictEq, e_int(1)),
                vec![s_return(e_array(vec![
                    e_str("Z"),
                    e_int(0),
                    e_int(0),
                ]))],
                vec![],
                None,
            )],
            vec![],
            None,
        ),
        s_if(
            e_binop(var("len"), BinOp::GtEq, e_int(3)),
            vec![
                s_assign(
                    "head",
                    call(
                        "strtoupper",
                        vec![call("substr", vec![var("token"), e_int(0), e_int(3)])],
                    ),
                ),
                s_if(
                    e_binop(
                        e_binop(var("head"), BinOp::StrictEq, e_str("GMT")),
                        BinOp::Or,
                        e_binop(var("head"), BinOp::StrictEq, e_str("UTC")),
                    ),
                    vec![
                        s_if(
                            e_binop(var("len"), BinOp::StrictEq, e_int(3)),
                            vec![s_return(e_array(vec![
                                var("head"),
                                e_int(0),
                                e_int(0),
                            ]))],
                            vec![],
                            None,
                        ),
                        s_assign(
                            "rest",
                            call("substr", vec![var("token"), e_int(3)]),
                        ),
                        s_assign(
                            "first",
                            call("substr", vec![var("rest"), e_int(0), e_int(1)]),
                        ),
                        s_if(
                            e_binop(
                                e_binop(var("first"), BinOp::StrictEq, e_str("+")),
                                BinOp::Or,
                                e_binop(var("first"), BinOp::StrictEq, e_str("-")),
                            ),
                            vec![
                                s_assign(
                                    "parts",
                                    static_call("__elephc_zone_offset_parts", vec![var("rest")]),
                                ),
                                s_if(
                                    e_binop(var("parts"), BinOp::StrictEq, e_bool(false)),
                                    vec![s_return(e_bool(false))],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_binop(var("head"), BinOp::StrictEq, e_str("UTC")),
                                    vec![s_return(e_array(vec![
                                        e_str("UTC"),
                                        e_int(0),
                                        e_int(0),
                                    ]))],
                                    vec![],
                                    None,
                                ),
                                s_return(e_array(vec![
                                    e_index(var("parts"), e_int(1)),
                                    e_index(var("parts"), e_int(0)),
                                    e_int(0),
                                ])),
                            ],
                            vec![],
                            Some(vec![s_return(e_bool(false))]),
                        ),
                    ],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_assign(
            "first",
            call("substr", vec![var("token"), e_int(0), e_int(1)]),
        ),
        s_if(
            e_binop(
                e_binop(var("first"), BinOp::StrictEq, e_str("+")),
                BinOp::Or,
                e_binop(var("first"), BinOp::StrictEq, e_str("-")),
            ),
            vec![
                s_assign(
                    "parts",
                    static_call("__elephc_zone_offset_parts", vec![var("token")]),
                ),
                s_if(
                    e_binop(var("parts"), BinOp::StrictEq, e_bool(false)),
                    vec![s_return(e_bool(false))],
                    vec![],
                    None,
                ),
                s_return(e_array(vec![
                    e_index(var("parts"), e_int(1)),
                    e_index(var("parts"), e_int(0)),
                    e_int(0),
                ])),
            ],
            vec![],
            None,
        ),
        s_if(
            call("ctype_alpha", vec![var("token")]),
            vec![
                s_assign("key", call("strtoupper", vec![var("token")])),
                s_assign(
                    "table",
                    static_call("__elephc_abbreviation_offsets", vec![]),
                ),
                s_if(
                    call("isset", vec![e_index(var("table"), var("key"))]),
                    vec![s_return(e_array(vec![
                        var("key"),
                        e_index(var("table"), var("key")),
                        e_int(0),
                    ]))],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                call("strpos", vec![var("token"), e_str("/")]),
                BinOp::StrictNotEq,
                e_bool(false),
            ),
            vec![
                s_assign("ids", static_call("__elephc_timezone_ids", vec![])),
                s_if(
                    call(
                        "in_array",
                        vec![var("token"), var("ids"), e_bool(true)],
                    ),
                    vec![s_return(e_array(vec![
                        var("token"),
                        e_int(0),
                        e_int(1),
                    ]))],
                    vec![],
                    None,
                ),
            ],
            vec![],
            None,
        ),
        s_return(e_bool(false)),
    ];
    let mut built = method(
        "__elephc_parse_zone_token",
        vec![("token".to_string(), Some(TypeExpr::Str), None, false)],
        Some(t_mixed()),
        body,
    );
    built.is_static = true;
    built
}

/// Builds `DateTime::__elephc_zone_offset_parts(string $text): array|false`.
///
/// Parses `±H[:MM]`, `±HH[:MM]`, `±HHMM`, and `±HMM`, returning `[offset_seconds, ±HH:MM]`.
pub(super) fn datetime_zone_offset_parts() -> crate::parser::ast::ClassMethod {
    let var = e_var;
    let call = |name: &str, args: Vec<crate::parser::ast::Expr>| e_call(name, args);
    let len_call = |name: &str| call("strlen", vec![var(name)]);
    let body = vec![
        s_if(
            e_binop(len_call("text"), BinOp::Lt, e_int(2)),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_assign("sign", call("substr", vec![var("text"), e_int(0), e_int(1)])),
        s_if(
            e_binop(
                e_binop(var("sign"), BinOp::StrictNotEq, e_str("+")),
                BinOp::And,
                e_binop(var("sign"), BinOp::StrictNotEq, e_str("-")),
            ),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_assign("body", call("substr", vec![var("text"), e_int(1)])),
        s_assign("blen", len_call("body")),
        s_assign("colon", call("strpos", vec![var("body"), e_str(":")])),
        s_if(
            e_binop(var("colon"), BinOp::StrictNotEq, e_bool(false)),
            vec![
                s_assign(
                    "h",
                    call("substr", vec![var("body"), e_int(0), var("colon")]),
                ),
                s_assign(
                    "m",
                    call(
                        "substr",
                        vec![var("body"), e_binop(var("colon"), BinOp::Add, e_int(1))],
                    ),
                ),
                s_if(
                    e_binop(
                        e_binop(len_call("m"), BinOp::StrictNotEq, e_int(2)),
                        BinOp::Or,
                        e_binop(
                            call("ctype_digit", vec![var("h")]),
                            BinOp::StrictEq,
                            e_bool(false),
                        ),
                    ),
                    vec![s_return(e_bool(false))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(
                        call("ctype_digit", vec![var("m")]),
                        BinOp::StrictEq,
                        e_bool(false),
                    ),
                    vec![s_return(e_bool(false))],
                    vec![],
                    None,
                ),
            ],
            vec![],
            Some(vec![
                s_if(
                    e_binop(
                        call("ctype_digit", vec![var("body")]),
                        BinOp::StrictEq,
                        e_bool(false),
                    ),
                    vec![s_return(e_bool(false))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(
                        e_binop(var("blen"), BinOp::StrictEq, e_int(1)),
                        BinOp::Or,
                        e_binop(var("blen"), BinOp::StrictEq, e_int(2)),
                    ),
                    vec![
                        s_assign("h", var("body")),
                        s_assign("m", e_str("0")),
                    ],
                    vec![],
                    Some(vec![
                        s_if(
                            e_binop(var("blen"), BinOp::StrictEq, e_int(3)),
                            vec![
                                s_assign(
                                    "h",
                                    call("substr", vec![var("body"), e_int(0), e_int(1)]),
                                ),
                                s_assign(
                                    "m",
                                    call("substr", vec![var("body"), e_int(1), e_int(2)]),
                                ),
                            ],
                            vec![],
                            Some(vec![
                                s_if(
                                    e_binop(var("blen"), BinOp::StrictEq, e_int(4)),
                                    vec![
                                        s_assign(
                                            "h",
                                            call(
                                                "substr",
                                                vec![var("body"), e_int(0), e_int(2)],
                                            ),
                                        ),
                                        s_assign(
                                            "m",
                                            call(
                                                "substr",
                                                vec![var("body"), e_int(2), e_int(2)],
                                            ),
                                        ),
                                    ],
                                    vec![],
                                    Some(vec![s_return(e_bool(false))]),
                                ),
                            ]),
                        ),
                    ]),
                ),
            ]),
        ),
        s_assign("hours", e_cast(CastType::Int, var("h"))),
        s_assign("minutes", e_cast(CastType::Int, var("m"))),
        s_if(
            e_binop(
                e_binop(var("minutes"), BinOp::Gt, e_int(59)),
                BinOp::Or,
                e_binop(
                    e_binop(var("hours"), BinOp::Gt, e_int(24)),
                    BinOp::Or,
                    e_binop(
                        e_binop(var("hours"), BinOp::StrictEq, e_int(24)),
                        BinOp::And,
                        e_binop(var("minutes"), BinOp::StrictNotEq, e_int(0)),
                    ),
                ),
            ),
            vec![s_return(e_bool(false))],
            vec![],
            None,
        ),
        s_assign(
            "offset",
            e_binop(
                e_binop(
                    e_binop(var("hours"), BinOp::Mul, e_int(3600)),
                    BinOp::Add,
                    e_binop(var("minutes"), BinOp::Mul, e_int(60)),
                ),
                BinOp::Mul,
                e_ternary(
                    e_binop(var("sign"), BinOp::StrictEq, e_str("-")),
                    e_int(-1),
                    e_int(1),
                ),
            ),
        ),
        s_assign("hs", e_cast(CastType::String, var("hours"))),
        s_if(
            e_binop(len_call("hs"), BinOp::Lt, e_int(2)),
            vec![s_assign("hs", e_concat(e_str("0"), var("hs")))],
            vec![],
            None,
        ),
        s_assign("ms", e_cast(CastType::String, var("minutes"))),
        s_if(
            e_binop(len_call("ms"), BinOp::Lt, e_int(2)),
            vec![s_assign("ms", e_concat(e_str("0"), var("ms")))],
            vec![],
            None,
        ),
        s_return(e_array(vec![
            var("offset"),
            e_concat(
                e_concat(e_concat(var("sign"), var("hs")), e_str(":")),
                var("ms"),
            ),
        ])),
    ];
    let mut built = method(
        "__elephc_zone_offset_parts",
        vec![("text".to_string(), Some(TypeExpr::Str), None, false)],
        Some(t_mixed()),
        body,
    );
    built.is_static = true;
    built
}

/// Builds `DateTime::__elephc_abbreviation_offsets(): array`.
///
/// The table is PHP's `listAbbreviations()` order reduced to the first row's fixed offset.
/// `abbreviations.data` is the bridge crate's embedded tzdb snapshot; the compiler reads it
/// directly so a date-only program does not link `libelephc_tz.a`.
pub(super) fn datetime_abbreviation_offsets() -> crate::parser::ast::ClassMethod {
    let entries = abbreviation_entries()
        .into_iter()
        .map(|(abbreviation, offset)| (e_str(&abbreviation), e_int(offset)))
        .collect();
    let mut built = method(
        "__elephc_abbreviation_offsets",
        Vec::new(),
        Some(TypeExpr::AssocArray {
            key: Box::new(TypeExpr::Str),
            value: Box::new(TypeExpr::Int),
        }),
        vec![s_return(e_array_assoc(entries))],
    );
    built.is_static = true;
    built
}

/// Returns each abbreviation with the fixed offset of its first tzdb row, uppercased.
fn abbreviation_entries() -> Vec<(String, i64)> {
    crate::date_time_zone::abbreviation_offsets()
}

/// Builds `DateTime::__elephc_timezone_ids(): array`.
pub(super) fn datetime_timezone_ids() -> crate::parser::ast::ClassMethod {
    let entries = super::super::timezone_ids::TIMEZONE_IDENTIFIERS
        .iter()
        .map(|id| e_str(id))
        .collect();
    let mut built = method(
        "__elephc_timezone_ids",
        Vec::new(),
        Some(t_array_of(TypeExpr::Str)),
        vec![s_return(e_array(entries))],
    );
    built.is_static = true;
    built
}

/// `?DateTimeZone` for the internal helper's second parameter.
fn t_nullable_datetimezone() -> TypeExpr {
    crate::synthetic_class::t_nullable(crate::synthetic_class::t_class("DateTimeZone"))
}

/// `array<T>` for the internal helpers' return type.
fn t_array_of(inner: TypeExpr) -> TypeExpr {
    TypeExpr::Array(Box::new(inner))
}

/// `mixed` for the helpers that return a tuple or `false`.
fn t_mixed() -> TypeExpr {
    crate::synthetic_class::t_mixed()
}

/// The `-1` subscript used by `substr($s, -1, 1)`.
fn e_neg_one() -> crate::parser::ast::Expr {
    crate::synthetic_class::e_neg(crate::synthetic_class::e_int(1))
}

/// `s_expr`, imported lazily to keep the item list above readable.
fn s_expr(value: crate::parser::ast::Expr) -> Stmt {
    crate::synthetic_class::s_expr(value)
}

/// `s_break`, imported lazily to keep the item list above readable.
fn s_break(levels: usize) -> Stmt {
    crate::synthetic_class::s_break(levels)
}


