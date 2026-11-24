//! Purpose:
//! Core DateTime construction, formatting, timestamp, microsecond, and timezone methods.
//!
//! Called from:
//! - Shared DateTime and DateTimeImmutable method assembly.
//!
//! Key details:
//! - Parsed PHP bodies preserve object-local timezone and sub-second state.

use super::*;
use crate::parser::ast::CastType;
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_cast, e_index, e_int, e_new, e_not, e_null, e_null_coalesce,
    e_static_call, e_str, e_this, e_var, s_assign, s_break, s_continue, s_expr, s_for, s_foreach,
    s_if, s_return_void, s_throw,
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
} else {
    $tzname = $timezone->getName();
    if ($datetime === "now") {
        $this->timestamp = time();
    } else {
        $saved = date_default_timezone_get();
        date_default_timezone_set($tzname);
        $__ts = strtotime($datetime);
        if ($__ts === false) {
            date_default_timezone_set($saved);
            throw new DateMalformedStringException("Failed to parse time string (" . $datetime . ")");
        }
        $this->timestamp = $__ts;
        date_default_timezone_set($saved);
    }
    $this->timezone_name = $tzname;
}
"#;

/// `DateTime`/`DateTimeImmutable::__construct(string $datetime = "now", ?DateTimeZone $timezone = null)`
/// — stores a UNIX timestamp and the object's display zone.
///
/// The body is the parsed `CONSTRUCT_SRC`. `$timezone` is typed `?DateTimeZone` (defaulting to
/// `null`); the `=== null` discriminator selects the form and `$timezone->getName()` reads the
/// zone on the non-null arm. A later `setTimezone()` still overrides the zone. (A `mixed` default
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
date_default_timezone_set(self::__elephc_display_timezone($this->timezone_name));
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
$trepl = self::__elephc_zone_label($this->timezone_name, $this->timestamp);
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

/// Builds the internal static `DateTime::__elephc_display_timezone(string $name): string`.
pub(super) fn datetime_display_timezone() -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_display_timezone".to_string(),
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
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body: super::bodies::display_timezone(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_zone_label(string $name, int $timestamp): string`.
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
/// `format()`/`getTimestamp()`. Both supported shapes validate their field types before use,
/// and every other shape throws PHP's own `Invalid serialization data` Error. The PHP-shaped
/// fields are restored directly instead of through `__construct()`, so a subclass constructor
/// never runs during hydration.
pub(super) fn datetime_unserialize() -> ClassMethod {
    let mut valid_wire_shape = e_binop(
        e_binop(
            e_call("is_int", vec![e_var("timestamp")]),
            BinOp::And,
            e_call("is_string", vec![e_var("timezone")]),
        ),
        BinOp::And,
        e_call("is_int", vec![e_var("microsecond")]),
    );
    valid_wire_shape = e_binop(
        valid_wire_shape,
        BinOp::And,
        e_binop(e_call("strlen", vec![e_var("timezone")]), BinOp::Gt, e_int(0)),
    );
    for whitespace in [" ", "\t", "\n", "\r"] {
        valid_wire_shape = e_binop(
            valid_wire_shape,
            BinOp::And,
            e_not(e_call(
                "str_contains",
                vec![e_var("timezone"), e_str(whitespace)],
            )),
        );
    }
    valid_wire_shape = e_binop(
        valid_wire_shape,
        BinOp::And,
        e_binop(
            e_binop(e_var("microsecond"), BinOp::GtEq, e_int(0)),
            BinOp::And,
            e_binop(e_var("microsecond"), BinOp::LtEq, e_int(999_999)),
        ),
    );
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
            s_if(
                valid_wire_shape,
                {
                    let mut body = vec![
                        assign_this_property("timestamp", e_var("timestamp")),
                        assign_this_property("timezone_name", e_var("timezone")),
                        assign_this_property("microsecond", e_var("microsecond")),
                    ];
                    body.extend(restore_leftover_unserialize_properties());
                    body.push(s_return_void());
                    body
                },
                Vec::new(),
                None,
            ),
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
            s_if(
                e_binop(
                    e_binop(
                        e_not(e_call("is_string", vec![e_var("date")])),
                        BinOp::Or,
                        e_not(e_call("is_int", vec![e_var("type")])),
                    ),
                    BinOp::Or,
                    e_not(e_call("is_string", vec![e_var("zone")])),
                ),
                vec![s_throw(e_new(
                    "Error",
                    vec![e_str("Invalid serialization data for DateTime object")],
                ))],
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
                vec![s_throw(e_new(
                    "Error",
                    vec![e_str("Invalid serialization data for DateTime object")],
                ))],
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
            s_if(
                e_binop(e_var("type"), BinOp::StrictEq, e_int(1)),
                super::bodies::restore_offset("date", "zone"),
                Vec::new(),
                Some(vec![s_if(
                    e_binop(e_var("type"), BinOp::StrictEq, e_int(2)),
                    super::bodies::restore_abbreviation("date", "zone"),
                    Vec::new(),
                    Some(super::bodies::restore_timezone(
                        "date",
                        e_new("DateTimeZone", vec![e_var("zone")]),
                    )),
                )]),
            ),
        ]
        .into_iter()
        .chain(restore_leftover_unserialize_properties())
        .collect(),
    )
}

/// Restores every serialized property the DateTime hook does not handle itself.
///
/// `DateTime::__unserialize()` only owns its internal timestamp/timezone fields;
/// subclass properties still arrive in `$data` and must keep their mangled keys,
/// visibility and declared-type checks. The runtime helper is the decoder's own
/// property store, so hydration stays identical to the no-hook path.
fn restore_leftover_unserialize_properties() -> Vec<Stmt> {
    let handled = [
        "timestamp",
        "timezone_name",
        "microsecond",
        "date",
        "timezone_type",
        "timezone",
    ];
    let mut skip = e_binop(e_var("key"), BinOp::StrictEq, e_str(handled[0]));
    for name in &handled[1..] {
        skip = e_binop(
            skip,
            BinOp::Or,
            e_binop(e_var("key"), BinOp::StrictEq, e_str(name)),
        );
    }
    vec![s_foreach(
        e_var("data"),
        Some("key"),
        "value",
        vec![
            s_if(skip, vec![s_continue(1)], Vec::new(), None),
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
