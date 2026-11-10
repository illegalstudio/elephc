//! Purpose:
//! Core DateTime construction, formatting, timestamp, microsecond, and timezone methods.
//!
//! Called from:
//! - Shared DateTime and DateTimeImmutable method assembly.
//!
//! Key details:
//! - Parsed PHP bodies preserve object-local timezone and sub-second state.

use super::*;

/// PHP source backing the `DateTime`/`DateTimeImmutable` constructor. With no timezone, parses the
/// string in the active default zone and records that as the display zone. With a `$timezone`, the
/// wall-clock string is interpreted in that zone (the default is temporarily switched so
/// `strtotime()` resolves the local time there — an explicit zone inside the string still wins),
/// and the zone becomes the display zone. `"now"` is the current instant regardless of zone.
#[cfg(test)]
pub(super) const CONSTRUCT_SRC: &str = r#"<?php
$this->microsecond = DateTime::__elephc_extract_micros($datetime);
$datetime = DateTime::__elephc_strip_micros($datetime);
$state = DateTime::__elephc_construct_state($datetime, $timezone);
$this->timestamp = $state[0];
$this->timezone_name = $state[1];
$this->timezone_offset = $state[2];
$this->timezone_fixed = $state[3];
$this->__elephc_initialized = true;
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
            ExprKind::StaticMethodCall {
                receiver: crate::parser::ast::StaticReceiver::Named(Name::unqualified("DateTimeZone")),
                method: "__elephc_from_parts".to_string(),
                args: vec![
                    this_property("timezone_name"),
                    this_property("timezone_offset"),
                    this_property("timezone_fixed"),
                ],
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
if (!$this->timezone_fixed) { date_default_timezone_set($this->timezone_name); }
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
        if ($this->timezone_fixed) { $year = intval(gmdate("Y", $this->timestamp + $this->timezone_offset)); } else { $year = intval(date("Y", $this->timestamp)); }
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
    $fmt = $fmt . $ch;
    $k = $k + 1;
}
if ($this->timezone_fixed) { $r = gmdate($fmt, $this->timestamp + $this->timezone_offset); } else { $r = date($fmt, $this->timestamp); }
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
