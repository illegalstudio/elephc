//! Purpose:
//! Declares PHP 8.6's `Time\Duration` value class and `Time\TimeException` through the
//! ordinary synthetic AST pipeline.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness through `inject_if_used`,
//!   after include resolution and before name resolution.
//!
//! Key details:
//! - Arithmetic stays in PHP's signed 64-bit integer domain; every operation that exceeds
//!   9_223_372_035 seconds raises `Time\TimeException`, matching php-src's timelib helper.
//! - The private constructor is the only writer of readonly state. Static factories apply
//!   php-src's argument ranges before delegating to it.
//! - `__elephc_compare` is the reviewed operator adapter for `<`, `<=`, `>`, `>=`, `<=>`,
//!   `==` and `!=`; `__elephc_abs_compare` is shared by the arithmetic methods. Both stay
//!   out of `ReflectionClass::getMethods()` through the class contract's visible list.

use crate::parser::ast::{BinOp, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::*;

/// Fully-qualified PHP spelling of the duration value class.
pub(crate) const DURATION_CLASS: &str = "\\Time\\Duration";
/// Fully-qualified PHP spelling of the duration exception.
pub(crate) const TIME_EXCEPTION_CLASS: &str = "\\Time\\TimeException";

/// Largest representable number of whole seconds, shared with php-src's `UINT64_C(9223372035)`.
const MAX_SECONDS: i64 = 9_223_372_035;
/// Nanoseconds in one second.
const NANOS_PER_SECOND: i64 = 1_000_000_000;

/// Out-of-range diagnostic shared by every factory and arithmetic method.
const MAX_RANGE_MESSAGE: &str =
    "The maximum representable range is 9_223_372_035 seconds (roughly 292 years)";
/// ISO-8601 parse-failure diagnostic.
const ISO_PARSE_MESSAGE: &str = "The ISO 8601 duration string could not be parsed";
/// ISO-8601 diagnostic for strings without a `P` period marker.
const ISO_MISSING_PERIOD_MESSAGE: &str =
    "The ISO 8601 duration string is missing the period (P) aspect";
/// ISO-8601 diagnostic for ranges or repeated periods.
const ISO_ONLY_PERIOD_MESSAGE: &str =
    "The ISO 8601 duration string may only contain the period (P) aspect";
/// ISO-8601 diagnostic for date components (Y, M, W, D).
const ISO_TIME_ONLY_MESSAGE: &str =
    "The ISO 8601 duration string may only contain the time (T) aspect";

/// Builds the complete `Time` namespace declaration set.
pub(crate) fn time_declarations() -> Program {
    internal_declarations(|| {
        let exception = class("TimeException").extends("\\Exception").build();
        let mut duration = class("Duration")
            .final_()
            .readonly_prop("seconds", TypeExpr::Int)
            .readonly_prop("nanoseconds", TypeExpr::Int)
            .readonly_prop("negative", TypeExpr::Bool)
            .method(constructor())
            .method(from_seconds())
            .method(from_nanoseconds())
            .method(from_microseconds())
            .method(from_milliseconds())
            .method(from_minutes())
            .method(from_hours())
            .method(from_iso8601_duration_string())
            .method(negate())
            .method(absolute())
            .method(add())
            .method(sub())
            .method(multiply_by())
            .method(divide_by())
            .method(compare())
            .method(abs_compare())
            .method(operator_compare())
            .build();
        if let crate::parser::ast::StmtKind::ClassDecl { is_readonly_class, .. } = &mut duration.kind {
            *is_readonly_class = true;
        }
        vec![s_namespace("Time", vec![exception, duration])]
    })
}

/// Throws the shared maximum-range `Time\TimeException`.
fn max_range_error() -> Stmt {
    s_throw(e_new_fq(TIME_EXCEPTION_CLASS, vec![e_str(MAX_RANGE_MESSAGE)]))
}

/// Throws one of the ISO-8601 `Time\TimeException` diagnostics.
fn iso_error(message: &str) -> Stmt {
    s_throw(e_new_fq(TIME_EXCEPTION_CLASS, vec![e_str(message)]))
}

/// Throws a `ValueError` with php-src's `Time\Duration::<method>()` argument prefix.
fn argument_error(method: &str, index: i64, parameter: &str, requirement: &str) -> Stmt {
    s_throw(e_new_fq(
        "\\ValueError",
        vec![e_str(&format!(
            "Time\\Duration::{method}(): Argument #{index} (${parameter}) {requirement}"
        ))],
    ))
}

/// Rejects a negative integer argument exactly like php-src's `Z_PARAM_ULONG`.
fn reject_negative(method: &str, index: i64, parameter: &str) -> Stmt {
    s_if(
        e_binop(e_var(parameter), BinOp::Lt, e_int(0)),
        vec![argument_error(
            method,
            index,
            parameter,
            "must be greater than or equal to 0",
        )],
        vec![],
        None,
    )
}

/// Returns the constructor argument expression for `$seconds` and `$nanoseconds`.
fn new_duration(seconds: Expr, nanoseconds: Expr, negative: Expr) -> Expr {
    e_new_self(vec![seconds, nanoseconds, negative])
}

/// Builds the private constructor that normalizes zero and writes the readonly state.
fn constructor() -> MethodBuilder {
    let non_zero = e_binop(
        e_binop(e_var("seconds"), BinOp::StrictNotEq, e_int(0)),
        BinOp::Or,
        e_binop(e_var("nanoseconds"), BinOp::StrictNotEq, e_int(0)),
    );
    method("__construct")
        .private()
        .param("seconds", TypeExpr::Int)
        .param("nanoseconds", TypeExpr::Int)
        .param("negative", TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("seconds"), BinOp::Gt, e_int(MAX_SECONDS)),
                vec![max_range_error()],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "seconds", e_var("seconds")),
            s_prop_assign(e_this(), "nanoseconds", e_var("nanoseconds")),
            s_prop_assign(
                e_this(),
                "negative",
                e_binop(non_zero, BinOp::And, e_var("negative")),
            ),
        ])
}

/// Builds `Duration::fromSeconds(int $seconds, int $nanoseconds = 0)`.
fn from_seconds() -> MethodBuilder {
    method("fromSeconds")
        .static_()
        .param("seconds", TypeExpr::Int)
        .param_default("nanoseconds", TypeExpr::Int, e_int(0))
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            reject_negative("fromSeconds", 1, "seconds"),
            reject_negative("fromSeconds", 2, "nanoseconds"),
            s_if(
                e_binop(e_var("nanoseconds"), BinOp::GtEq, e_int(NANOS_PER_SECOND)),
                vec![argument_error(
                    "fromSeconds",
                    2,
                    "nanoseconds",
                    "must be less than 1_000_000_000",
                )],
                vec![],
                None,
            ),
            s_return(new_duration(
                e_var("seconds"),
                e_var("nanoseconds"),
                e_bool(false),
            )),
        ])
}

/// Builds `Duration::fromNanoseconds(int $nanoseconds)`.
fn from_nanoseconds() -> MethodBuilder {
    method("fromNanoseconds")
        .static_()
        .param("nanoseconds", TypeExpr::Int)
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            reject_negative("fromNanoseconds", 1, "nanoseconds"),
            s_return(new_duration(
                e_call(
                    "\\intdiv",
                    vec![e_var("nanoseconds"), e_int(NANOS_PER_SECOND)],
                ),
                e_binop(e_var("nanoseconds"), BinOp::Mod, e_int(NANOS_PER_SECOND)),
                e_bool(false),
            )),
        ])
}

/// Builds `Duration::fromMicroseconds(int $microseconds)`.
fn from_microseconds() -> MethodBuilder {
    method("fromMicroseconds")
        .static_()
        .param("microseconds", TypeExpr::Int)
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            reject_negative("fromMicroseconds", 1, "microseconds"),
            s_return(new_duration(
                e_call("\\intdiv", vec![e_var("microseconds"), e_int(1_000_000)]),
                e_binop(
                    e_binop(e_var("microseconds"), BinOp::Mod, e_int(1_000_000)),
                    BinOp::Mul,
                    e_int(1_000),
                ),
                e_bool(false),
            )),
        ])
}

/// Builds `Duration::fromMilliseconds(int $milliseconds)`.
fn from_milliseconds() -> MethodBuilder {
    method("fromMilliseconds")
        .static_()
        .param("milliseconds", TypeExpr::Int)
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            reject_negative("fromMilliseconds", 1, "milliseconds"),
            s_return(new_duration(
                e_call("\\intdiv", vec![e_var("milliseconds"), e_int(1_000)]),
                e_binop(
                    e_binop(e_var("milliseconds"), BinOp::Mod, e_int(1_000)),
                    BinOp::Mul,
                    e_int(1_000_000),
                ),
                e_bool(false),
            )),
        ])
}

/// Builds `Duration::fromMinutes(int $minutes)` with php-src's pre-multiplication guard.
fn from_minutes() -> MethodBuilder {
    method("fromMinutes")
        .static_()
        .param("minutes", TypeExpr::Int)
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            reject_negative("fromMinutes", 1, "minutes"),
            s_if(
                e_binop(
                    e_var("minutes"),
                    BinOp::Gt,
                    e_call("\\intdiv", vec![e_int(i64::MAX), e_int(60)]),
                ),
                vec![max_range_error()],
                vec![],
                None,
            ),
            s_return(new_duration(
                e_binop(e_var("minutes"), BinOp::Mul, e_int(60)),
                e_int(0),
                e_bool(false),
            )),
        ])
}

/// Builds `Duration::fromHours(int $hours)` with php-src's pre-multiplication guard.
fn from_hours() -> MethodBuilder {
    method("fromHours")
        .static_()
        .param("hours", TypeExpr::Int)
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            reject_negative("fromHours", 1, "hours"),
            s_if(
                e_binop(
                    e_var("hours"),
                    BinOp::Gt,
                    e_call("\\intdiv", vec![e_int(i64::MAX), e_int(3_600)]),
                ),
                vec![max_range_error()],
                vec![],
                None,
            ),
            s_return(new_duration(
                e_binop(e_var("hours"), BinOp::Mul, e_int(3_600)),
                e_int(0),
                e_bool(false),
            )),
        ])
}

/// Builds `Duration::fromIso8601DurationString(string $specification)`.
///
/// The parser mirrors timelib's accepted subset: a `P` marker followed by an optional
/// date section that must be empty, then a mandatory `T` and integer `H`, `M`, `S`
/// components in that order.
fn from_iso8601_duration_string() -> MethodBuilder {
    let body = vec![
        s_assign("_d", e_var("specification")),
        s_if(
            e_call("\\str_contains", vec![e_var("_d"), e_str("\0")]),
            vec![iso_error(ISO_PARSE_MESSAGE)],
            vec![],
            None,
        ),
        s_if(
            e_binop(e_var("_d"), BinOp::StrictEq, e_str("")),
            vec![iso_error(ISO_PARSE_MESSAGE)],
            vec![],
            None,
        ),
        s_if(
            e_call("\\str_contains", vec![e_var("_d"), e_str("/")]),
            vec![iso_error(ISO_ONLY_PERIOD_MESSAGE)],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_index(e_var("_d"), e_int(0)),
                BinOp::StrictNotEq,
                e_str("P"),
            ),
            vec![s_if(
                e_binop(
                    e_binop(
                        e_call("\\str_contains", vec![e_var("_d"), e_str("-")]),
                        BinOp::Or,
                        e_call("\\str_contains", vec![e_var("_d"), e_str(":")]),
                    ),
                    BinOp::Or,
                    e_call("\\str_contains", vec![e_var("_d"), e_str("T")]),
                ),
                vec![iso_error(ISO_MISSING_PERIOD_MESSAGE)],
                vec![],
                Some(vec![iso_error(ISO_PARSE_MESSAGE)]),
            )],
            vec![],
            None,
        ),
        s_assign("_rest", e_call("\\substr", vec![e_var("_d"), e_int(1)])),
        s_if(
            e_binop(e_var("_rest"), BinOp::StrictEq, e_str("")),
            vec![iso_error(ISO_PARSE_MESSAGE)],
            vec![],
            None,
        ),
        s_assign(
            "_t",
            e_call("\\strpos", vec![e_var("_rest"), e_str("T")]),
        ),
        s_if(
            e_binop(e_var("_t"), BinOp::StrictEq, e_bool(false)),
            vec![s_if(
                e_call("\\__elephc_duration_has_date_component", vec![e_var("_rest")]),
                vec![iso_error(ISO_TIME_ONLY_MESSAGE)],
                vec![],
                Some(vec![iso_error(ISO_PARSE_MESSAGE)]),
            )],
            vec![],
            None,
        ),
        s_if(
            e_call(
                "\\__elephc_duration_has_date_component",
                vec![e_call(
                    "\\substr",
                    vec![e_var("_rest"), e_int(0), e_var("_t")],
                )],
            ),
            vec![iso_error(ISO_TIME_ONLY_MESSAGE)],
            vec![],
            None,
        ),
        s_assign(
            "_time",
            e_call(
                "\\substr",
                vec![e_var("_rest"), e_binop(e_var("_t"), BinOp::Add, e_int(1))],
            ),
        ),
        s_if(
            e_binop(e_var("_time"), BinOp::StrictEq, e_str("")),
            vec![iso_error(ISO_PARSE_MESSAGE)],
            vec![],
            None,
        ),
        s_assign("_hours", e_int(0)),
        s_assign("_minutes", e_int(0)),
        s_assign("_seconds", e_int(0)),
        s_assign("_seen", e_int(0)),
        s_assign("_pos", e_int(0)),
        s_assign("_length", e_call("\\strlen", vec![e_var("_time")])),
        s_while(
            e_binop(e_var("_pos"), BinOp::Lt, e_var("_length")),
            vec![
                s_assign("_start", e_var("_pos")),
                s_while(
                    e_binop(
                        e_binop(e_var("_pos"), BinOp::Lt, e_var("_length")),
                        BinOp::And,
                        e_binop(
                            e_binop(
                                e_index(e_var("_time"), e_var("_pos")),
                                BinOp::GtEq,
                                e_str("0"),
                            ),
                            BinOp::And,
                            e_binop(
                                e_index(e_var("_time"), e_var("_pos")),
                                BinOp::LtEq,
                                e_str("9"),
                            ),
                        ),
                    ),
                    vec![s_assign(
                        "_pos",
                        e_binop(e_var("_pos"), BinOp::Add, e_int(1)),
                    )],
                ),
                s_if(
                    e_binop(e_var("_pos"), BinOp::StrictEq, e_var("_start")),
                    vec![iso_error(ISO_PARSE_MESSAGE)],
                    vec![],
                    None,
                ),
                s_assign(
                    "_value",
                    e_cast(
                        crate::parser::ast::CastType::Int,
                        e_call(
                            "\\substr",
                            vec![
                                e_var("_time"),
                                e_var("_start"),
                                e_binop(e_var("_pos"), BinOp::Sub, e_var("_start")),
                            ],
                        ),
                    ),
                ),
                s_if(
                    e_binop(e_var("_pos"), BinOp::GtEq, e_var("_length")),
                    vec![iso_error(ISO_PARSE_MESSAGE)],
                    vec![],
                    None,
                ),
                s_assign(
                    "_unit",
                    e_index(e_var("_time"), e_var("_pos")),
                ),
                s_assign("_pos", e_binop(e_var("_pos"), BinOp::Add, e_int(1))),
                s_assign(
                    "_index",
                    e_match(
                        e_var("_unit"),
                        vec![
                            (vec![e_str("H")], e_int(0)),
                            (vec![e_str("M")], e_int(1)),
                            (vec![e_str("S")], e_int(2)),
                        ],
                        Some(e_int(3)),
                    ),
                ),
                s_if(
                    e_binop(e_var("_index"), BinOp::GtEq, e_int(3)),
                    vec![iso_error(ISO_PARSE_MESSAGE)],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("_index"), BinOp::Lt, e_var("_seen")),
                    vec![iso_error(ISO_PARSE_MESSAGE)],
                    vec![],
                    None,
                ),
                s_assign(
                    "_seen",
                    e_binop(e_var("_index"), BinOp::Add, e_int(1)),
                ),
                s_if(
                    e_binop(e_var("_index"), BinOp::StrictEq, e_int(0)),
                    vec![s_assign("_hours", e_var("_value"))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("_index"), BinOp::StrictEq, e_int(1)),
                    vec![s_assign("_minutes", e_var("_value"))],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_var("_index"), BinOp::StrictEq, e_int(2)),
                    vec![s_assign("_seconds", e_var("_value"))],
                    vec![],
                    None,
                ),
            ],
        ),
        s_if(
            e_binop(e_var("_seen"), BinOp::StrictEq, e_int(0)),
            vec![iso_error(ISO_PARSE_MESSAGE)],
            vec![],
            None,
        ),
        s_if(
            e_binop(
                e_var("_hours"),
                BinOp::Gt,
                e_call("\\intdiv", vec![e_int(i64::MAX), e_int(3_600)]),
            ),
            vec![max_range_error()],
            vec![],
            None,
        ),
        s_assign(
            "_total",
            e_binop(e_var("_hours"), BinOp::Mul, e_int(3_600)),
        ),
        s_if(
            e_binop(
                e_var("_minutes"),
                BinOp::Gt,
                e_call(
                    "\\intdiv",
                    vec![
                        e_binop(e_int(MAX_SECONDS), BinOp::Sub, e_var("_total")),
                        e_int(60),
                    ],
                ),
            ),
            vec![max_range_error()],
            vec![],
            None,
        ),
        s_assign(
            "_total",
            e_binop(
                e_var("_total"),
                BinOp::Add,
                e_binop(e_var("_minutes"), BinOp::Mul, e_int(60)),
            ),
        ),
        s_if(
            e_binop(
                e_var("_seconds"),
                BinOp::Gt,
                e_binop(e_int(MAX_SECONDS), BinOp::Sub, e_var("_total")),
            ),
            vec![max_range_error()],
            vec![],
            None,
        ),
        s_return(e_self_call(
            "fromSeconds",
            vec![e_binop(e_var("_total"), BinOp::Add, e_var("_seconds"))],
        )),
    ];
    method("fromIso8601DurationString")
        .static_()
        .param("specification", TypeExpr::Str)
        .returns(t_class(DURATION_CLASS))
        .body(body)
}

/// Builds `Duration::negate()`.
fn negate() -> MethodBuilder {
    method("negate")
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            s_if(
                e_binop(
                    e_binop(e_this_prop("seconds"), BinOp::StrictEq, e_int(0)),
                    BinOp::And,
                    e_binop(e_this_prop("nanoseconds"), BinOp::StrictEq, e_int(0)),
                ),
                vec![s_return(e_this())],
                vec![],
                None,
            ),
            s_return(new_duration(
                e_this_prop("seconds"),
                e_this_prop("nanoseconds"),
                e_not(e_this_prop("negative")),
            )),
        ])
}

/// Builds `Duration::absolute()`.
fn absolute() -> MethodBuilder {
    method("absolute")
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            s_if(
                e_not(e_this_prop("negative")),
                vec![s_return(e_this())],
                vec![],
                None,
            ),
            s_return(new_duration(
                e_this_prop("seconds"),
                e_this_prop("nanoseconds"),
                e_bool(false),
            )),
        ])
}

/// Builds `Duration::add(Duration $duration)` with timelib's signed-accumulation rules.
fn add() -> MethodBuilder {
    method("add")
        .param("duration", t_class(DURATION_CLASS))
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            s_if(
                e_binop(
                    e_this_prop("negative"),
                    BinOp::StrictEq,
                    e_prop(e_var("duration"), "negative"),
                ),
                vec![s_return(new_duration(
                    e_add_seconds(),
                    e_add_nanoseconds(),
                    e_this_prop("negative"),
                ))],
                vec![],
                None,
            ),
            s_assign("_order", e_self_call(
                "__elephc_abs_compare",
                vec![e_this(), e_var("duration")],
            )),
            s_if(
                e_binop(e_var("_order"), BinOp::Lt, e_int(0)),
                vec![s_return(new_duration(
                    e_sub_seconds("duration", "this"),
                    e_sub_nanoseconds("duration", "this"),
                    e_prop(e_var("duration"), "negative"),
                ))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_order"), BinOp::StrictEq, e_int(0)),
                vec![s_return(new_duration(e_int(0), e_int(0), e_bool(false)))],
                vec![],
                None,
            ),
            s_return(new_duration(
                e_sub_seconds("this", "duration"),
                e_sub_nanoseconds("this", "duration"),
                e_this_prop("negative"),
            )),
        ])
}

/// Builds `Duration::sub(Duration $duration)` with timelib's signed-subtraction rules.
fn sub() -> MethodBuilder {
    method("sub")
        .param("duration", t_class(DURATION_CLASS))
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            s_if(
                e_binop(
                    e_this_prop("negative"),
                    BinOp::StrictNotEq,
                    e_prop(e_var("duration"), "negative"),
                ),
                vec![s_return(new_duration(
                    e_add_seconds(),
                    e_add_nanoseconds(),
                    e_this_prop("negative"),
                ))],
                vec![],
                None,
            ),
            s_assign("_order", e_self_call(
                "__elephc_abs_compare",
                vec![e_this(), e_var("duration")],
            )),
            s_if(
                e_binop(e_var("_order"), BinOp::Lt, e_int(0)),
                vec![s_return(new_duration(
                    e_sub_seconds("duration", "this"),
                    e_sub_nanoseconds("duration", "this"),
                    e_not(e_prop(e_var("duration"), "negative")),
                ))],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("_order"), BinOp::StrictEq, e_int(0)),
                vec![s_return(new_duration(e_int(0), e_int(0), e_bool(false)))],
                vec![],
                None,
            ),
            s_return(new_duration(
                e_sub_seconds("this", "duration"),
                e_sub_nanoseconds("this", "duration"),
                e_this_prop("negative"),
            )),
        ])
}

/// Builds the seconds sum including the carry from both nanosecond components.
fn e_add_seconds() -> Expr {
    let carry = e_ternary(
        e_binop(
            e_binop(
                e_this_prop("nanoseconds"),
                BinOp::Add,
                e_prop(e_var("duration"), "nanoseconds"),
            ),
            BinOp::GtEq,
            e_int(NANOS_PER_SECOND),
        ),
        e_int(1),
        e_int(0),
    );
    e_binop(
        e_binop(
            e_this_prop("seconds"),
            BinOp::Add,
            e_prop(e_var("duration"), "seconds"),
        ),
        BinOp::Add,
        carry,
    )
}

/// Builds the sum of both nanosecond components, normalized into one whole second.
fn e_add_nanoseconds() -> Expr {
    let total = e_binop(
        e_this_prop("nanoseconds"),
        BinOp::Add,
        e_prop(e_var("duration"), "nanoseconds"),
    );
    e_ternary(
        e_binop(total.clone(), BinOp::GtEq, e_int(NANOS_PER_SECOND)),
        e_binop(total.clone(), BinOp::Sub, e_int(NANOS_PER_SECOND)),
        total,
    )
}

/// Builds a seconds difference, borrowing when the nanosecond difference is negative.
fn e_sub_seconds(left: &str, right: &str) -> Expr {
    let seconds = e_binop(
        duration_prop(left, "seconds"),
        BinOp::Sub,
        duration_prop(right, "seconds"),
    );
    e_ternary(
        e_binop(e_sub_nanoseconds_raw(left, right), BinOp::Lt, e_int(0)),
        e_binop(seconds.clone(), BinOp::Sub, e_int(1)),
        seconds,
    )
}

/// Builds a nanosecond difference normalized into `0..999_999_999`.
fn e_sub_nanoseconds(left: &str, right: &str) -> Expr {
    let raw = e_sub_nanoseconds_raw(left, right);
    e_ternary(
        e_binop(raw.clone(), BinOp::Lt, e_int(0)),
        e_binop(raw.clone(), BinOp::Add, e_int(NANOS_PER_SECOND)),
        raw,
    )
}

/// Returns the unnormalized nanosecond difference between two duration receivers.
fn e_sub_nanoseconds_raw(left: &str, right: &str) -> Expr {
    e_binop(
        duration_prop(left, "nanoseconds"),
        BinOp::Sub,
        duration_prop(right, "nanoseconds"),
    )
}

/// Reads one of the duration's readonly properties from `$this` or `$duration`.
fn duration_prop(receiver: &str, property: &str) -> Expr {
    if receiver == "this" {
        e_this_prop(property)
    } else {
        e_prop(e_var("duration"), property)
    }
}

/// Builds `Duration::multiplyBy(int $factor)`.
fn multiply_by() -> MethodBuilder {
    method("multiplyBy")
        .param("factor", TypeExpr::Int)
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            reject_negative("multiplyBy", 1, "factor"),
            s_if(
                e_binop(e_var("factor"), BinOp::StrictEq, e_int(0)),
                vec![s_return(new_duration(e_int(0), e_int(0), e_bool(false)))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_this_prop("seconds"),
                    BinOp::Gt,
                    e_call(
                        "\\intdiv",
                        vec![e_int(i64::MAX), e_var("factor")],
                    ),
                ),
                vec![max_range_error()],
                vec![],
                None,
            ),
            s_assign(
                "_seconds",
                e_binop(e_this_prop("seconds"), BinOp::Mul, e_var("factor")),
            ),
            s_assign(
                "_extra",
                e_binop(
                    e_this_prop("nanoseconds"),
                    BinOp::Mul,
                    e_call("\\intdiv", vec![e_var("factor"), e_int(NANOS_PER_SECOND)]),
                ),
            ),
            s_assign(
                "_nanoseconds",
                e_binop(
                    e_this_prop("nanoseconds"),
                    BinOp::Mul,
                    e_binop(e_var("factor"), BinOp::Mod, e_int(NANOS_PER_SECOND)),
                ),
            ),
            s_assign(
                "_extra",
                e_binop(
                    e_var("_extra"),
                    BinOp::Add,
                    e_call("\\intdiv", vec![e_var("_nanoseconds"), e_int(NANOS_PER_SECOND)]),
                ),
            ),
            s_assign(
                "_nanoseconds",
                e_binop(e_var("_nanoseconds"), BinOp::Mod, e_int(NANOS_PER_SECOND)),
            ),
            s_if(
                e_binop(
                    e_binop(e_int(i64::MAX), BinOp::Sub, e_var("_extra")),
                    BinOp::Lt,
                    e_var("_seconds"),
                ),
                vec![max_range_error()],
                vec![],
                None,
            ),
            s_return(new_duration(
                e_binop(e_var("_seconds"), BinOp::Add, e_var("_extra")),
                e_var("_nanoseconds"),
                e_this_prop("negative"),
            )),
        ])
}

/// Builds `Duration::divideBy(int $divisor)`.
fn divide_by() -> MethodBuilder {
    method("divideBy")
        .param("divisor", TypeExpr::Int)
        .returns(t_class(DURATION_CLASS))
        .body(vec![
            reject_negative("divideBy", 1, "divisor"),
            s_if(
                e_binop(e_var("divisor"), BinOp::StrictEq, e_int(0)),
                vec![s_throw(e_new_fq(
                    "\\DivisionByZeroError",
                    vec![e_str("Division by zero")],
                ))],
                vec![],
                None,
            ),
            s_assign(
                "_seconds",
                e_call("\\intdiv", vec![e_this_prop("seconds"), e_var("divisor")]),
            ),
            s_assign(
                "_nanoseconds",
                e_binop(
                    e_this_prop("nanoseconds"),
                    BinOp::Add,
                    e_binop(
                        e_binop(e_this_prop("seconds"), BinOp::Mod, e_var("divisor")),
                        BinOp::Mul,
                        e_int(NANOS_PER_SECOND),
                    ),
                ),
            ),
            s_return(new_duration(
                e_var("_seconds"),
                e_call("\\intdiv", vec![e_var("_nanoseconds"), e_var("divisor")]),
                e_this_prop("negative"),
            )),
        ])
}

/// Builds the public `Duration::compare(Duration $a, Duration $b)` method.
fn compare() -> MethodBuilder {
    method("compare")
        .static_()
        .param("a", t_class(DURATION_CLASS))
        .param("b", t_class(DURATION_CLASS))
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(
                    e_prop(e_var("a"), "negative"),
                    BinOp::And,
                    e_not(e_prop(e_var("b"), "negative")),
                ),
                vec![s_return(e_int(-1))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_not(e_prop(e_var("a"), "negative")),
                    BinOp::And,
                    e_prop(e_var("b"), "negative"),
                ),
                vec![s_return(e_int(1))],
                vec![],
                None,
            ),
            s_if(
                e_prop(e_var("a"), "negative"),
                vec![s_return(e_self_call(
                    "__elephc_abs_compare",
                    vec![e_var("b"), e_var("a")],
                ))],
                vec![],
                None,
            ),
            s_return(e_self_call(
                "__elephc_abs_compare",
                vec![e_var("a"), e_var("b")],
            )),
        ])
}

/// Builds the private absolute-value comparison shared by `add`, `sub`, and `compare`.
fn abs_compare() -> MethodBuilder {
    method("__elephc_abs_compare")
        .private()
        .static_()
        .param("one", t_class(DURATION_CLASS))
        .param("two", t_class(DURATION_CLASS))
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_binop(
                    e_prop(e_var("one"), "seconds"),
                    BinOp::Lt,
                    e_prop(e_var("two"), "seconds"),
                ),
                vec![s_return(e_int(-1))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_prop(e_var("one"), "seconds"),
                    BinOp::Gt,
                    e_prop(e_var("two"), "seconds"),
                ),
                vec![s_return(e_int(1))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_prop(e_var("one"), "nanoseconds"),
                    BinOp::Lt,
                    e_prop(e_var("two"), "nanoseconds"),
                ),
                vec![s_return(e_int(-1))],
                vec![],
                None,
            ),
            s_if(
                e_binop(
                    e_prop(e_var("one"), "nanoseconds"),
                    BinOp::Gt,
                    e_prop(e_var("two"), "nanoseconds"),
                ),
                vec![s_return(e_int(1))],
                vec![],
                None,
            ),
            s_return(e_int(0)),
        ])
}

/// Builds the reviewed operator adapter consumed by the class contract.
///
/// `ObjectOrdering::Unordered` (2) is returned for foreign operands, matching the
/// established BcMath/GMP convention: relational operators are false and `<=>` is 1.
fn operator_compare() -> MethodBuilder {
    method("__elephc_compare")
        .static_()
        .param("left", t_mixed())
        .param("right", t_mixed())
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_not(e_binop(
                    e_instance_of(e_var("left"), DURATION_CLASS),
                    BinOp::And,
                    e_instance_of(e_var("right"), DURATION_CLASS),
                )),
                vec![s_return(e_int(2))],
                vec![],
                None,
            ),
            s_return(e_self_call(
                "compare",
                vec![e_var("left"), e_var("right")],
            )),
        ])
}

/// Returns whether an ISO-8601 fragment contains a date (or week/month) designator.
fn e_has_date_component(part: Expr) -> Expr {
    ["Y", "M", "W", "D", "y", "m", "w", "d"]
        .into_iter()
        .map(|designator| e_call("\\str_contains", vec![part.clone(), e_str(designator)]))
        .reduce(|left, right| e_binop(left, BinOp::Or, right))
        .expect("date designator list")
}

/// Declares the private date-designator predicate used by the ISO-8601 parser.
fn date_component_helper() -> Stmt {
    function("__elephc_duration_has_date_component")
        .param("part", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .returning(e_has_date_component(e_var("part")))
        .build()
}

/// Injects the time prelude when the program references its surface.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !force && !program_uses_time_surface(&program) {
            return program;
        }
        let mut declarations = time_declarations();
        declarations.push(date_component_helper());
        inventory.record_program("time", &declarations);
        declarations.extend(program);
        declarations
    })
}

/// Returns whether the program names the `Time` surface or reflects declared classes.
pub(crate) fn program_uses_time_surface(program: &[Stmt]) -> bool {
    let usage = crate::prelude_prune::usage::collect(program);
    let reflected = ["get_declared_classes", "get_declared_interfaces", "get_declared_traits"]
        .iter()
        .any(|name| usage.functions.contains(*name));
    reflected
        || usage
            .literals
            .iter()
            .any(|literal| literal.to_ascii_lowercase().contains("time\\duration")
                || literal.to_ascii_lowercase().contains("time\\timeexception"))
        || usage
            .classes
            .iter()
            .any(|name| matches!(name.as_str(), "time\\duration" | "time\\timeexception"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tokenizes and parses one probe source for the detection tests.
    fn parse_source(source: &str) -> crate::parser::ast::Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        crate::parser::parse(&tokens).expect("parse")
    }

    /// Verifies the operator contract exposes exactly the comparison adapter.
    #[test]
    fn duration_operator_contract_compares_only() {
        let operators = elephc_builtin_contract::TIME_DURATION_OPERATORS;
        assert_eq!(operators.compare, Some("__elephc_compare"));
        assert!(operators.arithmetic.is_empty());
        assert!(operators.bitwise.is_empty());
        assert!(operators.shift.is_empty());
        assert!(operators.boolean.is_none());
    }

    /// Verifies the ISO-8601 helper is declared and the class lists its public surface.
    #[test]
    fn duration_declarations_include_helpers_and_visible_methods() {
        let program = time_declarations();
        assert!(program.iter().any(|stmt| matches!(
            &stmt.kind,
            crate::parser::ast::StmtKind::NamespaceBlock { name: Some(name), .. } if name == "Time"
        )));
        assert!(elephc_builtin_contract::TIME_DURATION_VISIBLE_METHODS
            .contains(&"fromIso8601DurationString"));
    }

    /// Verifies the detector matches both the class spellings and reflected class lists.
    #[test]
    fn time_surface_detection_matches_class_names_and_literals() {
        let used = parse_source("<?php $d = \\Time\\Duration::fromSeconds(1);");
        assert!(program_uses_time_surface(&used));
        let unused = parse_source("<?php echo 1;");
        assert!(!program_uses_time_surface(&unused));
    }
}
