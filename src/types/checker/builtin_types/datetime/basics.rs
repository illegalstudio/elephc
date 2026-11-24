//! Purpose:
//! Core DateTime construction, formatting, timestamp, microsecond, and timezone methods.
//!
//! Called from:
//! - Shared DateTime and DateTimeImmutable method assembly.
//!
//! Key details:
//! - AST-built bodies preserve object-local timezone and sub-second state; the `*_SRC`
//!   strings in this file are test-only transcribed oracles.

use super::*;
use crate::parser::ast::CastType;
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_cast, e_index, e_int, e_new, e_not, e_null, e_null_coalesce,
    e_self_call, e_static_call, e_str, e_this, e_var, s_assign, s_break, s_continue, s_expr, s_for,
    s_foreach, s_if, s_return_void, s_throw, s_try,
};

/// PHP source backing the `DateTime`/`DateTimeImmutable` constructor. With no timezone, parses the
/// string in the active default zone and records that as the display zone. With a `$timezone`, the
/// wall-clock string is interpreted in that zone (the default is temporarily switched so
/// `strtotime()` resolves the local time there — an explicit zone inside the string still wins),
/// and the zone becomes the display zone. `"now"` is the current instant regardless of zone.
#[cfg(test)]
pub(super) const CONSTRUCT_SRC: &str = r#"<?php
// Capture a trailing fractional second (HH:MM:SS.ffffff) into the microsecond
// component and strip it before strtotime() (which does not accept it). The
// parsing lives in static helpers so the constructor body stays small (adding
// locals + a loop here corrupts the frame when a caller also formats the result).
$this->microsecond = DateTime::__elephc_extract_micros($datetime);
$datetime = DateTime::__elephc_strip_micros($datetime);
if ($timezone === null) {
    if ($datetime === "now") {
        $this->timestamp = time();
    } else {
        $__ts = strtotime($datetime);
        if ($__ts === false) {
            throw new DateMalformedStringException("Failed to parse time string (" . $datetime . ")");
        }
        $this->timestamp = $__ts;
    }
    $this->timezone_name = date_default_timezone_get();
    $this->__elephc_fixed_zone = false;
} else {
    $tzname = $timezone->getName();
    $__fixed = $tzname !== "UTC" && self::__elephc_abbr_offset($tzname) !== -2147483648;
    $__tzdisplay = self::__elephc_display_timezone($tzname, $__fixed);
    if ($datetime === "now") {
        $this->timestamp = time();
    } else {
        $saved = date_default_timezone_get();
        date_default_timezone_set($__tzdisplay);
        $__ts = strtotime($datetime);
        if ($__ts === false) {
            date_default_timezone_set($saved);
            throw new DateMalformedStringException("Failed to parse time string (" . $datetime . ")");
        }
        $this->timestamp = $__ts;
        date_default_timezone_set($saved);
    }
    $this->timezone_name = $__fixed ? strtoupper($tzname) : $tzname;
    $this->__elephc_fixed_zone = $__fixed;
}
"#;

/// `DateTime`/`DateTimeImmutable::__construct(string $datetime = "now", ?DateTimeZone $timezone = null)`
/// — stores a UNIX timestamp and the object's display zone.
///
/// The body is built as AST by `bodies::construct()`; `CONSTRUCT_SRC` is its test-only
/// transcribed oracle. `$timezone` is typed `?DateTimeZone` (defaulting to `null`); the
/// `=== null` discriminator selects the form and `$timezone->getName()` reads the zone on the
/// non-null arm. A later `setTimezone()` still overrides the zone. (A `mixed` default
/// of `null` here miscompiled when the constructor was called more than once per frame, so the
/// nullable-object typing is used instead — it also matches PHP's signature.)
pub(super) fn datetime_immutable_constructor() -> ClassMethod {
    let body = super::bodies::construct();
    method(
        "__construct",
        vec![
            (
                "datetime".to_string(),
                Some(TypeExpr::Str),
                Some(Expr::new(ExprKind::StringLiteral("now".to_string()), dummy())),
                false,
            ),
            (
                "timezone".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
                    "DateTimeZone",
                ))))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
        ],
        None,
        body,
    )
}

/// `DateTimeImmutable::getTimestamp(): int` — returns the stored UNIX timestamp.
pub(super) fn datetime_immutable_get_timestamp() -> ClassMethod {
    method("getTimestamp", Vec::new(), Some(TypeExpr::Int), vec![return_expr(this_property("timestamp"))])
}

/// `DateTime`/`DateTimeImmutable::getMicrosecond(): int` — returns the stored sub-second component
/// (0..999999), 0 unless set by `setMicrosecond()` or parsed from a fractional second.
pub(super) fn datetime_get_microsecond() -> ClassMethod {
    method("getMicrosecond", Vec::new(), Some(TypeExpr::Int), vec![return_expr(this_property("microsecond"))])
}

/// `DateTimeImmutable::getTimezone(): DateTimeZone` — re-materializes a zone from the stored name.
pub(super) fn datetime_immutable_get_timezone() -> ClassMethod {
    method(
        "getTimezone",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateTimeZone"))),
        vec![return_expr(Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("DateTimeZone"),
                args: vec![this_property("timezone_name")],
            },
            dummy(),
        ))],
    )
}

/// PHP source backing `format()`. Applies `$this->timezone_name` via `date_default_timezone_set`
/// around the `date()` call (saving/restoring the previous default) for per-object formatting, and
/// rewrites the unescaped `u` (microseconds, 6 digits) and `v` (milliseconds, 3 digits) specifiers
/// to the stored sub-second value before calling `date()` — those decimal digits pass through
/// `date()` literally (only letters are specifiers). Backslash escapes are preserved verbatim.
#[cfg(test)]
pub(super) const FORMAT_SRC: &str = r#"<?php
$saved = date_default_timezone_get();
date_default_timezone_set(self::__elephc_display_timezone($this->timezone_name, $this->__elephc_fixed_zone));
$us = $this->microsecond;
$fmt = "";
$flen = strlen($format);
$k = 0;
while ($k < $flen) {
    $ch = $format[$k];
    if ($ch === "\\") {
        $fmt = $fmt . $ch;
        $k = $k + 1;
        if ($k < $flen) { $fmt = $fmt . $format[$k]; $k = $k + 1; }
        continue;
    }
    if ($ch === "u") {
        $s = "" . $us;
        while (strlen($s) < 6) { $s = "0" . $s; }
        $fmt = $fmt . $s;
        $k = $k + 1;
        continue;
    }
    if ($ch === "v") {
        $ms = intdiv($us, 1000);
        $s = "" . $ms;
        while (strlen($s) < 3) { $s = "0" . $s; }
        $fmt = $fmt . $s;
        $k = $k + 1;
        continue;
    }
    if ($ch === "X" || $ch === "x") {
        $year = intval(date("Y", $this->timestamp));
        if ($year < 0) {
            $year = -$year;
            $sign = "-";
        } else {
            $sign = "+";
        }
        $s = "" . $year;
        while (strlen($s) < 4) { $s = "0" . $s; }
        if ($ch === "x" && $sign === "+" && strlen($s) <= 4) {
            $fmt = $fmt . $s;
        } else {
            $fmt = $fmt . $sign . $s;
        }
        $k = $k + 1;
        continue;
    }
    if ($ch === "e") { $fmt = $fmt . "\x01"; $k = $k + 1; continue; }
    if ($ch === "T") { $fmt = $fmt . "\x02"; $k = $k + 1; continue; }
    $fmt = $fmt . $ch;
    $k = $k + 1;
}
$r = date($fmt, $this->timestamp);
$trepl = self::__elephc_zone_label($this->timezone_name, $this->timestamp, $this->__elephc_fixed_zone);
$r = str_replace("\x01", $this->timezone_name, $r);
$r = str_replace("\x02", $trepl, $r);
date_default_timezone_set($saved);
return $r;
"#;

/// `DateTime`/`DateTimeImmutable::format(string $format): string` — formats the stored timestamp in
/// the object's own timezone, with `u`/`v` reflecting the stored microseconds. Body is `FORMAT_SRC`.
pub(super) fn datetime_immutable_format() -> ClassMethod {
    let body = super::bodies::format();
    method(
        "format",
        vec![("format".to_string(), Some(TypeExpr::Str), None, false)],
        Some(TypeExpr::Str),
        body,
    )
}

/// Builds the internal static
/// `DateTime::__elephc_display_timezone(string $name, bool $fixed): string`.
pub(super) fn datetime_display_timezone() -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_display_timezone".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("name".to_string(), Some(TypeExpr::Str), None, false),
            ("fixed".to_string(), Some(TypeExpr::Bool), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body: super::bodies::display_timezone(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static
/// `DateTime::__elephc_zone_label(string $name, int $timestamp, bool $fixed): string`.
pub(super) fn datetime_zone_label() -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_zone_label".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("name".to_string(), Some(TypeExpr::Str), None, false),
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            ("fixed".to_string(), Some(TypeExpr::Bool), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body: super::bodies::zone_label(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static
/// `DateTime::__elephc_tz_identifier(string $name): string|false`.
pub(super) fn datetime_tz_identifier() -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_tz_identifier".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Union(vec![TypeExpr::Str, TypeExpr::False])),
        by_ref_return: false,
        body: super::bodies::tz_identifier(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_abbr_offset(string $abbr): int`.
pub(super) fn datetime_abbr_offset() -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_abbr_offset".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("abbr".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body: super::bodies::abbr_offset(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_offset_zone(int $offset): string`.
pub(super) fn datetime_offset_zone() -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_offset_zone".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("offset".to_string(), Some(TypeExpr::Int), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body: super::bodies::offset_zone(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// `DateTime`/`DateTimeImmutable::__unserialize(array $data): void` — accepts elephc's
/// timestamp/timezone_name/microsecond wire shape and PHP's date/timezone_type/timezone one.
///
/// The backing slots are initialized by the constructor, not by PHP property defaults, so a
/// hostile payload using PHP's field names used to leave them unset and crash the next
/// `format()`/`getTimestamp()`. Both supported shapes validate their field types and values
/// before use, and every other shape throws PHP's own `Invalid serialization data` Error. The
/// PHP-shaped fields are restored directly instead of through `__construct()`, so a subclass
/// constructor never runs during hydration. The backing properties are private, so a subclass
/// property named `timestamp`, `timezone_name`, or `microsecond` stays independent.
pub(super) fn datetime_unserialize(class_name: &str) -> ClassMethod {
    let mangled = |property: &str| format!("\0{class_name}\0{property}");
    let invalid = || {
        s_throw(e_new(
            "Error",
            vec![e_str(&format!(
                "Invalid serialization data for {class_name} object"
            ))],
        ))
    };
    let plain_shape = valid_wire_values("timestamp", "timezone", "microsecond");
    let mangled_shape = valid_wire_values("wire_timestamp", "wire_timezone", "wire_microsecond");
    method(
        "__unserialize",
        vec![(
            "data".to_string(),
            Some(TypeExpr::Named(Name::unqualified("array"))),
            None,
            false,
        )],
        Some(TypeExpr::Void),
        vec![
            // Read PHP's fields first: a payload carrying them must never be mistaken
            // for elephc's timestamp/timezone_name/microsecond shape.
            s_assign(
                "date",
                e_null_coalesce(e_index(e_var("data"), e_str("date")), e_null()),
            ),
            s_assign(
                "type",
                e_null_coalesce(e_index(e_var("data"), e_str("timezone_type")), e_null()),
            ),
            s_assign(
                "zone",
                e_null_coalesce(e_index(e_var("data"), e_str("timezone")), e_null()),
            ),
            s_assign(
                "php_shape",
                e_binop(
                    e_binop(
                        e_call("is_string", vec![e_var("date")]),
                        BinOp::And,
                        e_call("is_int", vec![e_var("type")]),
                    ),
                    BinOp::And,
                    e_call("is_string", vec![e_var("zone")]),
                ),
            ),
            // Old public-property wire shape and the current private-property shape.
            s_assign(
                "timestamp",
                e_null_coalesce(e_index(e_var("data"), e_str("timestamp")), e_null()),
            ),
            s_assign(
                "timezone",
                e_null_coalesce(e_index(e_var("data"), e_str("timezone_name")), e_null()),
            ),
            s_assign(
                "microsecond",
                e_null_coalesce(e_index(e_var("data"), e_str("microsecond")), e_null()),
            ),
            s_assign(
                "wire_timestamp",
                e_null_coalesce(e_index(e_var("data"), e_str(&mangled("timestamp"))), e_null()),
            ),
            s_assign(
                "wire_timezone",
                e_null_coalesce(
                    e_index(e_var("data"), e_str(&mangled("timezone_name"))),
                    e_null(),
                ),
            ),
            s_assign(
                "wire_microsecond",
                e_null_coalesce(e_index(e_var("data"), e_str(&mangled("microsecond"))), e_null()),
            ),
            s_if(
                e_binop(e_not(e_var("php_shape")), BinOp::And, mangled_shape),
                {
                    let mut body = vec![
                        assign_this_property("timestamp", e_var("wire_timestamp")),
                        assign_this_property("timezone_name", e_var("wire_timezone")),
                        assign_this_property("microsecond", e_var("wire_microsecond")),
                        assign_this_property(
                            "__elephc_fixed_zone",
                            e_cast(
                                CastType::Bool,
                                e_null_coalesce(
                                    e_index(
                                        e_var("data"),
                                        e_str(&mangled("__elephc_fixed_zone")),
                                    ),
                                    // Older elephc wire shapes carried no zone kind and
                                    // always displayed abbreviation-table hits as fixed.
                                    super::bodies::zone_is_fixed(e_var("wire_timezone")),
                                ),
                            ),
                        ),
                    ];
                    body.extend(restore_leftover_unserialize_properties(&[
                        &mangled("timestamp"),
                        &mangled("timezone_name"),
                        &mangled("microsecond"),
                        &mangled("__elephc_fixed_zone"),
                    ]));
                    body.push(s_return_void());
                    body
                },
                Vec::new(),
                None,
            ),
            s_if(
                e_binop(e_not(e_var("php_shape")), BinOp::And, plain_shape),
                {
                    let mut body = vec![
                        assign_this_property("timestamp", e_var("timestamp")),
                        assign_this_property("timezone_name", e_var("timezone")),
                        assign_this_property("microsecond", e_var("microsecond")),
                        assign_this_property(
                            "__elephc_fixed_zone",
                            super::bodies::zone_is_fixed(e_var("timezone")),
                        ),
                    ];
                    body.extend(restore_leftover_unserialize_properties(&[
                        "timestamp",
                        "timezone_name",
                        "microsecond",
                        "__elephc_fixed_zone",
                    ]));
                    body.push(s_return_void());
                    body
                },
                Vec::new(),
                None,
            ),
            s_if(e_not(e_var("php_shape")), vec![invalid()], Vec::new(), None),
            // php_date_initialize_from_hash accepts only the three timelib zone types.
            s_if(
                e_binop(
                    e_binop(e_var("type"), BinOp::Lt, e_int(1)),
                    BinOp::Or,
                    e_binop(e_var("type"), BinOp::Gt, e_int(3)),
                ),
                vec![invalid()],
                Vec::new(),
                None,
            ),
            // Type 3 names a tz database identifier; an unknown one fails initialization.
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_int(3)),
                vec![s_if(
                    e_binop(
                        e_self_call("__elephc_tz_identifier", vec![e_var("zone")]),
                        BinOp::StrictEq,
                        e_bool(false),
                    ),
                    vec![invalid()],
                    Vec::new(),
                    None,
                )],
                Vec::new(),
                None,
            ),
            // Relative modifiers only reach DateTimeZone as text when this field contains
            // whitespace ("+2 days"). PHP's timezone identifiers, offsets and abbreviations
            // never do, so refuse those strings instead of applying a date shift.
            s_assign("len", e_call("strlen", vec![e_var("zone")])),
            s_assign("bad", e_bool(false)),
            s_for(
                Some(s_assign("i", e_int(0))),
                Some(e_binop(e_var("i"), BinOp::Lt, e_var("len"))),
                Some(s_assign(
                    "i",
                    e_binop(e_var("i"), BinOp::Add, e_int(1)),
                )),
                vec![
                    s_assign("ch", e_index(e_var("zone"), e_var("i"))),
                    s_if(
                        e_binop(
                            e_binop(
                                e_binop(e_var("ch"), BinOp::StrictEq, e_str(" ")),
                                BinOp::Or,
                                e_binop(e_var("ch"), BinOp::StrictEq, e_str("\t")),
                            ),
                            BinOp::Or,
                            e_binop(
                                e_binop(e_var("ch"), BinOp::StrictEq, e_str("\n")),
                                BinOp::Or,
                                e_binop(e_var("ch"), BinOp::StrictEq, e_str("\r")),
                            ),
                        ),
                        vec![s_assign("bad", e_bool(true)), s_break(1)],
                        Vec::new(),
                        None,
                    ),
                ],
            ),
            s_if(
                e_binop(
                    e_var("bad"),
                    BinOp::Or,
                    e_binop(e_var("len"), BinOp::StrictEq, e_int(0)),
                ),
                vec![invalid()],
                Vec::new(),
                None,
            ),
            assign_this_property(
                "microsecond",
                e_static_call("DateTime", "__elephc_extract_micros", vec![e_var("date")]),
            ),
            s_assign(
                "date",
                e_static_call("DateTime", "__elephc_strip_micros", vec![e_var("date")]),
            ),
            // Numeric offsets and abbreviations must reach strtotime() with their
            // fixed offset: date_default_timezone_set() makes libc read `+02:00`
            // as UTC-2 and does not resolve abbreviations such as CEST at all.
            s_try(
                vec![s_if(
                    e_binop(e_var("type"), BinOp::StrictEq, e_int(1)),
                    super::bodies::restore_offset("date", "zone"),
                    Vec::new(),
                    Some(vec![s_if(
                        e_binop(e_var("type"), BinOp::StrictEq, e_int(2)),
                        super::bodies::restore_abbreviation("date", "zone"),
                        Vec::new(),
                        // A type 3 payload names a tz-database identifier, even when
                        // the identifier also exists in the abbreviation table.
                        Some(super::bodies::restore_timezone(
                            "date",
                            e_new("DateTimeZone", vec![e_var("zone")]),
                            false,
                        )),
                    )]),
                )],
                vec![(
                    vec!["Throwable"],
                    Some("__elephc_date_error"),
                    vec![invalid()],
                )],
                None,
            ),
        ]
        .into_iter()
        .chain(restore_leftover_unserialize_properties(&[
            "date",
            "timezone_type",
            "timezone",
            "__elephc_fixed_zone",
        ]))
        .collect(),
    )
}

/// Builds the shared type-and-value validation for one elephc wire shape.
///
/// Both shapes reject a non-int timestamp/microsecond, an empty or whitespace-bearing
/// timezone name (a relative modifier such as `+2 days` must never reach
/// `date_default_timezone_set()`), and a microsecond outside `0..=999999`.
fn valid_wire_values(timestamp: &str, timezone: &str, microsecond: &str) -> Expr {
    let mut valid = e_binop(
        e_binop(
            e_call("is_int", vec![e_var(timestamp)]),
            BinOp::And,
            e_call("is_string", vec![e_var(timezone)]),
        ),
        BinOp::And,
        e_call("is_int", vec![e_var(microsecond)]),
    );
    valid = e_binop(
        valid,
        BinOp::And,
        e_binop(e_call("strlen", vec![e_var(timezone)]), BinOp::Gt, e_int(0)),
    );
    for whitespace in [" ", "\t", "\n", "\r"] {
        valid = e_binop(
            valid,
            BinOp::And,
            e_not(e_call(
                "str_contains",
                vec![e_var(timezone), e_str(whitespace)],
            )),
        );
    }
    e_binop(
        valid,
        BinOp::And,
        e_binop(
            e_binop(e_var(microsecond), BinOp::GtEq, e_int(0)),
            BinOp::And,
            e_binop(e_var(microsecond), BinOp::LtEq, e_int(999_999)),
        ),
    )
}

/// Restores every serialized property the active DateTime wire shape does not own.
///
/// PHP's `restore_custom_datetime_properties()` ignores non-string keys and only the
/// three internal fields. Every other string key reaches the decoder's own property
/// store, keeping mangled keys, visibility, and declared-type checks identical to the
/// no-hook path. `skip` names the fields the selected wire shape consumed.
fn restore_leftover_unserialize_properties(skip: &[&str]) -> Vec<Stmt> {
    let mut skipped = e_not(e_call("is_string", vec![e_var("key")]));
    for name in skip {
        skipped = e_binop(
            skipped,
            BinOp::Or,
            e_binop(e_var("key"), BinOp::StrictEq, e_str(name)),
        );
    }
    vec![s_foreach(
        e_var("data"),
        Some("key"),
        "value",
        vec![
            s_if(skipped, vec![s_continue(1)], Vec::new(), None),
            s_expr(e_call(
                "__elephc_unserialize_store_property",
                vec![
                    e_this(),
                    e_cast(CastType::String, e_var("key")),
                    e_var("value"),
                ],
            )),
        ],
    )]
}
