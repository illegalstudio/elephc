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
    if ($this->microsecond === 0) {
        $this->microsecond = DateTime::__elephc_now_micros($datetime, "");
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
    if ($this->microsecond === 0) {
        $this->microsecond = DateTime::__elephc_now_micros($datetime, $tzname);
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
date_default_timezone_set($this->timezone_name);
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
    $fmt = $fmt . $ch;
    $k = $k + 1;
}
$r = date($fmt, $this->timestamp);
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
