//! Purpose:
//! The timezone-introspection standard-library surface
//! (`timezone_location_get`/`timezone_transitions_get`/`timezone_abbreviations_list`
//! plus the marshalling helpers the `DateTimeZone` OOP methods delegate to),
//! written in elephc-PHP. Declares the `elephc_tz` bridge externs and parses
//! their serialized output into PHP arrays, so the feature compiles through the
//! normal pipeline (functions, extern C-ABI calls, arrays) with no new codegen.
//!
//! Called from:
//! - `crate::pipeline::compile()` via `inject_if_used`, after include/PDO
//!   injection and before name resolution.
//!
//! Key details:
//! - The prelude is injected only when a program references the introspection
//!   surface (see `detect`), so non-tz binaries never declare the `elephc_tz`
//!   externs and never link `libelephc_tz.a`. Its presence (the
//!   `__elephc_tz_location_get` marker) is what gates adding the three OOP methods
//!   to the synthetic `DateTimeZone` (see `inject_builtin_datetime`).
//! - `getTransitions($begin,$end)` is handled by one windowing routine whose
//!   defaults (`PHP_INT_MIN`/`PHP_INT_MAX`) reduce exactly to PHP's full no-arg
//!   list, reusing the bridge's row-0 `time` so `gmdate` is never asked to format
//!   `PHP_INT_MIN`.

use crate::parser::ast::Program;

mod detect;

/// The elephc-PHP timezone-introspection prelude: the `elephc_tz` extern block the
/// synthetic `DateTimeZone` methods call into, plus the three procedural aliases
/// that delegate to those methods. The array marshalling lives in the methods
/// (see `inject_builtin_datetime`), so it is written once; the procedural
/// functions are thin wrappers, matching PHP's procedural/OOP duality.
pub const TZ_PRELUDE_SRC: &str = r#"<?php

extern "elephc_tz" {
    function elephc_tz_location(string $zone): string;
    function elephc_tz_transitions(string $zone): string;
    function elephc_tz_abbreviations(): string;
    function elephc_tz_date_parse(string $datetime, int $datetime_length): string;
    function elephc_tz_date_parse_from_format(string $format, int $format_length, string $datetime, int $datetime_length): string;
    function elephc_tz_create_from_format(string $format, int $format_length, string $datetime, int $datetime_length, int $base_timestamp, string $timezone, int $timezone_length): string;
    function elephc_tz_interval_parse(string $input, int $input_length, int $relative): string;
    function elephc_tz_period_parse(string $input, int $input_length): string;
    function elephc_tz_apply_interval(int $timestamp, int $microsecond, string $timezone, int $timezone_length, string $payload, int $payload_length, int $subtract): string;
}

function __elephc_timelib_optional_int(int $value, int $unset): mixed {
    if ($value === $unset) {
        return false;
    }
    return $value;
}

function __elephc_timelib_optional_fraction(int $microsecond, int $unset): mixed {
    if ($microsecond === $unset) {
        return false;
    }
    return $microsecond / 1000000.0;
}

function __elephc_timelib_decode_parse_result(string $serialized) {
    $lines = explode("\n", $serialized);
    $header = explode("\t", $lines[0]);
    $unset = -9999999;
    $year = intval($header[1]);
    $month = intval($header[2]);
    $day = intval($header[3]);
    $hour = intval($header[4]);
    $minute = intval($header[5]);
    $second = intval($header[6]);
    $microsecond = intval($header[7]);
    $warnings = ["" => ""];
    unset($warnings[""]);
    $errors = ["" => ""];
    unset($errors[""]);
    $relative = [];
    $hasRelative = false;
    $lineCount = count($lines);
    $lineIndex = 1;
    while ($lineIndex < $lineCount) {
        $parts = explode("\t", $lines[$lineIndex]);
        if ($parts[0] === "W") {
            $warnings[intval($parts[1])] = $parts[2];
        } else if ($parts[0] === "E") {
            $errors[intval($parts[1])] = $parts[2];
        } else if ($parts[0] === "R") {
            $hasRelative = true;
            $relative = [
                "year" => intval($parts[1]),
                "month" => intval($parts[2]),
                "day" => intval($parts[3]),
                "hour" => intval($parts[4]),
                "minute" => intval($parts[5]),
                "second" => intval($parts[6]),
            ];
            if (intval($parts[7]) !== $unset) {
                $relative["weekday"] = intval($parts[7]);
            }
            if (intval($parts[8]) !== $unset) {
                $relative["weekdays"] = intval($parts[8]);
            }
            if (intval($parts[9]) === 1) {
                $relative["first_day_of_month"] = true;
            } else if (intval($parts[9]) === 2) {
                $relative["last_day_of_month"] = true;
            }
        }
        $lineIndex = $lineIndex + 1;
    }
    $result = [
        "year" => __elephc_timelib_optional_int($year, $unset),
        "month" => __elephc_timelib_optional_int($month, $unset),
        "day" => __elephc_timelib_optional_int($day, $unset),
        "hour" => __elephc_timelib_optional_int($hour, $unset),
        "minute" => __elephc_timelib_optional_int($minute, $unset),
        "second" => __elephc_timelib_optional_int($second, $unset),
        "fraction" => __elephc_timelib_optional_fraction($microsecond, $unset),
        "warning_count" => intval($header[14]),
        "warnings" => $warnings,
        "error_count" => intval($header[15]),
        "errors" => $errors,
        "is_localtime" => intval($header[8]) !== 0,
    ];
    if ($result["is_localtime"]) {
        $zoneType = intval($header[9]);
        $result["zone_type"] = $zoneType;
        if ($zoneType === 1 || $zoneType === 2) {
            $result["zone"] = intval($header[10]);
            $result["is_dst"] = intval($header[11]) !== 0;
        }
        if (($zoneType === 2 || $zoneType === 3) && $header[12] !== "") {
            $result["tz_abbr"] = $header[12];
        }
        if ($zoneType === 3 && $header[13] !== "") {
            $result["tz_id"] = $header[13];
        }
    }
    if ($hasRelative) {
        $result["relative"] = $relative;
    }
    return $result;
}

function __elephc_timelib_date_parse(string $datetime) {
    return __elephc_timelib_decode_parse_result(
        elephc_tz_date_parse($datetime, strlen($datetime))
    );
}

function __elephc_timelib_date_parse_from_format(string $format, string $datetime) {
    return __elephc_timelib_decode_parse_result(
        elephc_tz_date_parse_from_format($format, strlen($format), $datetime, strlen($datetime))
    );
}

function __elephc_timelib_create_from_format(string $format, string $datetime, string $timezone) {
    $serialized = elephc_tz_create_from_format(
        $format,
        strlen($format),
        $datetime,
        strlen($datetime),
        time(),
        $timezone,
        strlen($timezone)
    );
    $result = __elephc_timelib_decode_parse_result($serialized);
    $header = explode("\t", explode("\n", $serialized)[0]);
    $result["__elephc_timestamp"] = intval($header[17]);
    $result["__elephc_serialized"] = $serialized;
    return $result;
}

function __elephc_timelib_interval_parse(string $input, bool $relative) {
    $serialized = elephc_tz_interval_parse($input, strlen($input), $relative ? 1 : 0);
    $parts = explode("\t", $serialized);
    if ($parts[0] === "O") {
        return [
            "status" => "O",
            "y" => intval($parts[1]),
            "m" => intval($parts[2]),
            "d" => intval($parts[3]),
            "h" => intval($parts[4]),
            "i" => intval($parts[5]),
            "s" => intval($parts[6]),
            "us" => intval($parts[7]),
            "invert" => intval($parts[8]),
            "days" => intval($parts[9]),
            "position" => 0,
            "character" => "",
            "message" => "",
        ];
    }
    if ($parts[0] === "E" && count($parts) >= 4) {
        return [
            "status" => "E",
            "y" => 0, "m" => 0, "d" => 0,
            "h" => 0, "i" => 0, "s" => 0,
            "us" => 0, "invert" => 0, "days" => -9999999,
            "position" => intval($parts[1]),
            "character" => chr(intval($parts[2])),
            "message" => $parts[3],
        ];
    }
    return [
        "status" => $parts[0],
        "y" => 0, "m" => 0, "d" => 0,
        "h" => 0, "i" => 0, "s" => 0,
        "us" => 0, "invert" => 0, "days" => -9999999,
        "position" => 0, "character" => "", "message" => "",
    ];
}

function __elephc_timelib_period_parse(string $input) {
    $parts = explode("\t", elephc_tz_period_parse($input, strlen($input)));
    if ($parts[0] === "P") {
        return [
            "status" => "P",
            "has_start" => intval($parts[1]) !== 0,
            "start" => intval($parts[2]),
            "has_end" => intval($parts[3]) !== 0,
            "end" => intval($parts[4]),
            "has_interval" => intval($parts[5]) !== 0,
            "recurrences" => intval($parts[6]),
            "y" => intval($parts[7]),
            "m" => intval($parts[8]),
            "d" => intval($parts[9]),
            "h" => intval($parts[10]),
            "i" => intval($parts[11]),
            "s" => intval($parts[12]),
            "us" => intval($parts[13]),
        ];
    }
    return [
        "status" => $parts[0],
        "has_start" => false, "start" => 0,
        "has_end" => false, "end" => 0,
        "has_interval" => false, "recurrences" => 0,
        "y" => 0, "m" => 0, "d" => 0,
        "h" => 0, "i" => 0, "s" => 0, "us" => 0,
    ];
}

function __elephc_timelib_apply_interval(
    int $timestamp,
    int $microsecond,
    string $timezone,
    string $payload,
    bool $subtract
) {
    $parts = explode(
        "\t",
        elephc_tz_apply_interval(
            $timestamp,
            $microsecond,
            $timezone,
            strlen($timezone),
            $payload,
            strlen($payload),
            $subtract ? 1 : 0
        )
    );
    return [
        "timestamp" => intval($parts[0]),
        "microsecond" => intval($parts[1]),
        "warning" => intval($parts[2]) !== 0,
    ];
}

function __elephc_timelib_offset_name(int $offset) {
    $sign = "+";
    if ($offset < 0) {
        $sign = "-";
        $offset = 0 - $offset;
    }
    $hours = intdiv($offset, 3600);
    $minutes = intdiv($offset % 3600, 60);
    return sprintf("%s%02d:%02d", $sign, $hours, $minutes);
}

function timezone_location_get(DateTimeZone $object) {
    return $object->getLocation();
}

function timezone_transitions_get(DateTimeZone $object, int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX) {
    return $object->getTransitions($timestampBegin, $timestampEnd);
}

function timezone_abbreviations_list() {
    return DateTimeZone::listAbbreviations();
}
"#;

/// Prepends the timezone-introspection prelude to `program` when it references the
/// introspection surface, so the `elephc_tz` externs and helper functions compile
/// through the normal pipeline only for programs that use them. The prelude is
/// declarations only (extern block + functions), which are hoisted, so prepending
/// does not change top-level execution order. It is static and tested, so a
/// tokenize/parse failure is a compiler bug and panics rather than degrading.
///
/// `force` (set by `--with-tz`) bypasses the usage scan so the timezone surface
/// is always injected, making it available even when auto-detection would not see
/// the usage.
pub fn inject_if_used(program: Program, force: bool) -> Program {
    if !force && !detect::program_uses_tz_introspection(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(TZ_PRELUDE_SRC).expect("tz prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("tz prelude must parse");
    combined.extend(program);
    combined
}
