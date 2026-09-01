//! Purpose:
//! The timezone-introspection standard-library surface
//! (`timezone_location_get`/`timezone_transitions_get`/`timezone_abbreviations_list`
//! plus the marshalling helpers the `DateTimeZone` OOP methods delegate to),
//! declared in Rust through `crate::synthetic_class`, together with the audited
//! timelib parsing helpers used by php-src-compatible DateTime methods. Declares the `elephc_tz` bridge
//! externs and decodes their serialized output into PHP arrays, so the feature compiles
//! through the normal pipeline (functions, extern C-ABI calls, arrays) with no new codegen.
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

use crate::parser::ast::{CType, Program, Stmt, StmtKind, TypeExpr};
use crate::synthetic_class::{
    e_int, e_method_call, e_static_call, e_var, extern_fn, function, internal_declarations,
};

mod detect;
mod generated_timelib;

/// The bridge these externs bind to. A program that never touches the introspection
/// surface must not declare them, or it links `libelephc_tz.a` for nothing.
const TZ_BRIDGE: &str = "elephc_tz";

/// Reachability roots needed by checker-only DateTime methods in auto-detected programs.
pub const TIMELIB_RUNTIME_REACHABILITY_GROUP: &str = "tz-timelib-runtime";

/// Returns whether the user program activates the pay-for-use DateTime/timelib surface.
pub fn program_uses_tz(program: &[Stmt]) -> bool {
    detect::program_uses_tz_introspection(program)
}
/// Test-only PHP oracle for the timelib-specific bridge ABI and marshalling helpers.
/// Production injects the checked-in direct AST from `generated_timelib`; retaining this source
/// only under `cfg(test)` lets the generator prove parity without parsing PHP during compilation.
#[cfg(test)]
const TIMELIB_PRELUDE_SRC: &str = r#"<?php

extern "elephc_tz" {
    function elephc_tz_mktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int;
    function elephc_tz_gmmktime(int $hour, int $minute, int $second, int $month, int $day, int $year): int;
    function elephc_tz_format_civil(int $timestamp, int $microsecond, string $format, int $format_length, string $payload, int $payload_length): string;
    function elephc_tz_date_parse(string $datetime, int $datetime_length): string;
    function elephc_tz_date_parse_from_format(string $format, int $format_length, string $datetime, int $datetime_length): string;
    function elephc_tz_create_from_format(string $format, int $format_length, string $datetime, int $datetime_length, int $base_timestamp, string $timezone, int $timezone_length): string;
    function elephc_tz_interval_parse(string $input, int $input_length, int $relative): string;
    function elephc_tz_period_parse(string $input, int $input_length): string;
    function elephc_tz_apply_interval(int $timestamp, int $microsecond, string $timezone, int $timezone_length, string $payload, int $payload_length, int $subtract): string;
    function elephc_tz_modify(int $timestamp, int $microsecond, string $timezone, int $timezone_length, string $modifier, int $modifier_length): string;
    function elephc_tz_set_civil(int $timestamp, int $microsecond, string $timezone, int $timezone_length, string $payload, int $payload_length): string;
    function elephc_tz_set_iso_date(int $timestamp, int $microsecond, string $timezone, int $timezone_length, int $year, int $week, int $day): string;
    function elephc_tz_diff(int $left_timestamp, int $left_microsecond, string $left_timezone, int $left_timezone_length, int $right_timestamp, int $right_microsecond, string $right_timezone, int $right_timezone_length): string;
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

function __elephc_timelib_decode_interval_result(string $serialized) {
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

function __elephc_timelib_interval_parse(string $input, bool $relative) {
    return __elephc_timelib_decode_interval_result(
        elephc_tz_interval_parse($input, strlen($input), $relative ? 1 : 0)
    );
}

function __elephc_timelib_interval_restore_parse(string $input) {
    return __elephc_timelib_decode_interval_result(
        elephc_tz_interval_parse($input, strlen($input), 2)
    );
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

function __elephc_timelib_modify(
    int $timestamp,
    int $microsecond,
    string $timezone,
    string $modifier
) {
    $serialized = elephc_tz_modify(
        $timestamp,
        $microsecond,
        $timezone,
        strlen($timezone),
        $modifier,
        strlen($modifier)
    );
    $lineBreak = strpos($serialized, "\n");
    $metadataLine = $lineBreak === false ? $serialized : substr($serialized, 0, $lineBreak);
    $parse = $lineBreak === false ? "" : substr($serialized, $lineBreak + 1);
    $parts = explode("\t", $metadataLine);
    if (count($parts) >= 4 && $parts[0] === "O") {
        return [
            "status" => "O",
            "timestamp" => intval($parts[1]),
            "microsecond" => intval($parts[2]),
            "reset_to_utc" => intval($parts[3]) !== 0,
            "parse" => $parse,
        ];
    }
    return [
        "status" => "E",
        "timestamp" => 0,
        "microsecond" => 0,
        "reset_to_utc" => false,
        "parse" => $parse,
    ];
}

function __elephc_timelib_set_civil(
    int $timestamp,
    int $microsecond,
    string $timezone,
    string $payload
) {
    $parts = explode(
        "\t",
        elephc_tz_set_civil(
            $timestamp,
            $microsecond,
            $timezone,
            strlen($timezone),
            $payload,
            strlen($payload)
        )
    );
    return [
        "timestamp" => intval($parts[0]),
        "microsecond" => intval($parts[1]),
    ];
}

function __elephc_timelib_set_iso_date(
    int $timestamp,
    int $microsecond,
    string $timezone,
    int $year,
    int $week,
    int $day
) {
    $parts = explode(
        "\t",
        elephc_tz_set_iso_date(
            $timestamp,
            $microsecond,
            $timezone,
            strlen($timezone),
            $year,
            $week,
            $day
        )
    );
    return [
        "timestamp" => intval($parts[0]),
        "microsecond" => intval($parts[1]),
        "year" => intval($parts[2]),
        "month" => intval($parts[3]),
        "day" => intval($parts[4]),
    ];
}

function __elephc_timelib_diff(
    int $leftTimestamp,
    int $leftMicrosecond,
    string $leftTimezone,
    int $rightTimestamp,
    int $rightMicrosecond,
    string $rightTimezone
) {
    return __elephc_timelib_decode_interval_result(
        elephc_tz_diff(
            $leftTimestamp,
            $leftMicrosecond,
            $leftTimezone,
            strlen($leftTimezone),
            $rightTimestamp,
            $rightMicrosecond,
            $rightTimezone,
            strlen($rightTimezone)
        )
    );
}

function __elephc_timelib_offset_name(int $offset) {
    $sign = "+";
    if ($offset < 0) {
        $sign = "-";
        $offset = 0 - $offset;
    }
    $hours = intdiv($offset, 3600);
    $minutes = intdiv($offset % 3600, 60);
    $seconds = $offset % 60;
    if ($seconds !== 0) {
        return sprintf("%s%02d:%02d:%02d", $sign, $hours, $minutes, $seconds);
    }
    return sprintf("%s%02d:%02d", $sign, $hours, $minutes);
}
function __elephc_timezone_argument_type(mixed $value): string {
    if (is_object($value)) {
        return get_class($value);
    }
    $type = gettype($value);
    if ($type === "integer") {
        return "int";
    }
    if ($type === "double") {
        return "float";
    }
    if ($type === "boolean") {
        return "bool";
    }
    if ($type === "NULL") {
        return "null";
    }
    return $type;
}

function timezone_offset_get(mixed $object, mixed $datetime) {
    if (!($object instanceof DateTimeZone)) {
        throw new TypeError(
            'timezone_offset_get(): Argument #1 ($object) must be of type DateTimeZone, '
            . __elephc_timezone_argument_type($object)
            . ' given'
        );
    }
    if (!($datetime instanceof DateTimeInterface)) {
        throw new TypeError(
            'timezone_offset_get(): Argument #2 ($datetime) must be of type DateTimeInterface, '
            . __elephc_timezone_argument_type($datetime)
            . ' given'
        );
    }
    return $object->getOffset($datetime);
}

"#;


/// Builds the timezone-introspection prelude: the `elephc_tz` extern block the synthetic
/// `DateTimeZone` methods call into, plus the three procedural aliases that delegate to
/// those methods. The array marshalling lives in the methods (see `inject_builtin_datetime`),
/// so it is written once; the procedural functions are thin wrappers, matching PHP's
/// procedural/OOP duality.
///
/// `getTransitions`'s window defaults are integer LITERALS, not constant references:
/// `PHP_INT_MIN`/`PHP_INT_MAX` are dedicated lexer tokens that never reach the parser as
/// names, so the PHP form produced `IntLiteral` here too. They reduce exactly to PHP's
/// full no-arg list.
pub(crate) fn tz_declarations() -> Program {
    internal_declarations(|| {
        vec![
            extern_fn("elephc_tz_location", TZ_BRIDGE)
                .param("zone", CType::Str)
                .returns(CType::Str)
                .build(),
            extern_fn("elephc_tz_transitions", TZ_BRIDGE)
                .param("zone", CType::Str)
                .returns(CType::Str)
                .build(),
            extern_fn("elephc_tz_abbreviations", TZ_BRIDGE)
                .returns(CType::Str)
                .build(),
            function("timezone_location_get")
                .param("object", t_datetimezone())
                .returning(e_method_call(e_var("object"), "getLocation", vec![]))
                .build(),
            function("timezone_transitions_get")
                .param_untyped("object")
                .param_default("timestampBegin", TypeExpr::Int, e_int(i64::MIN))
                .param_default("timestampEnd", TypeExpr::Int, e_int(i64::MAX))
                .returning(e_method_call(
                    e_var("object"),
                    "getTransitions",
                    vec![e_var("timestampBegin"), e_var("timestampEnd")],
                ))
                .build(),
            function("timezone_abbreviations_list")
                .returning(e_static_call("DateTimeZone", "listAbbreviations", vec![]))
                .build(),
        ]
    })
}

/// The class the three procedural wrappers delegate to.
fn t_datetimezone() -> TypeExpr {
    crate::synthetic_class::t_class("DateTimeZone")
}

/// Prepends the timezone-introspection prelude to `program` when it references the
/// introspection surface, so the `elephc_tz` externs and helper functions compile
/// through the normal pipeline only for programs that use them. The prelude is
/// declarations only (extern block + functions), which are hoisted, so prepending
/// does not change top-level execution order.
///
/// `force` (set by `--with-tz`) bypasses the usage scan so the timezone surface
/// is always injected, making it available even when auto-detection would not see
/// the usage.
pub fn inject_if_used(
    program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !program_uses_tz(&program) {
        return program;
    }
    let mut combined = tz_declarations();
    combined.extend(generated_timelib::timelib_declarations());
    inventory.record_program("tz", &combined);
    record_timelib_runtime_roots(inventory, &combined);
    combined.extend(program);
    combined
}

/// Records only hidden timelib helpers and bridge externs as synthetic-method roots.
///
/// Rooting the complete public timezone group also made `timezone_offset_get()` reachable. Its
/// mixed-value diagnostics use dynamic class lookup, which conservatively retained every class
/// and method in otherwise unrelated preludes such as PDO. User-visible wrappers remain in the
/// ordinary `tz` group and are reached from source calls; checker-only DateTime bodies need just
/// the hidden helpers plus their extern targets.
fn record_timelib_runtime_roots(
    inventory: &mut crate::optimize::reachability::PreludeInventory,
    declarations: &[Stmt],
) {
    let group = inventory.group_mut(TIMELIB_RUNTIME_REACHABILITY_GROUP);
    for declaration in declarations {
        match &declaration.kind {
            StmtKind::ExternFunctionDecl { name, .. } => {
                group.externs.insert(crate::names::php_symbol_key(name));
            }
            StmtKind::FunctionDecl { name, .. } if name.starts_with("__elephc_timelib_") => {
                group.functions.insert(crate::names::php_symbol_key(name));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// The surface is fixed: three externs, then the three procedural wrappers.
    #[test]
    fn declares_three_externs_and_three_wrappers() {
        let declared: Vec<String> = tz_declarations()
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ExternFunctionDecl { name, .. } | StmtKind::FunctionDecl { name, .. } => {
                    Some(name.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            declared,
            vec![
                "elephc_tz_location",
                "elephc_tz_transitions",
                "elephc_tz_abbreviations",
                "timezone_location_get",
                "timezone_transitions_get",
                "timezone_abbreviations_list",
            ]
        );
    }

    /// Every extern must name the `elephc_tz` library, or the bridge is not linked and the
    /// symbol resolves to nothing at link time.
    #[test]
    fn every_extern_names_the_bridge() {
        for stmt in tz_declarations() {
            let StmtKind::ExternFunctionDecl { name, library, .. } = &stmt.kind else {
                continue;
            };
            assert_eq!(
                library.as_deref(),
                Some(TZ_BRIDGE),
                "{} must bind to {}",
                name,
                TZ_BRIDGE
            );
        }
    }

    /// The window defaults reduce to PHP's full no-arg list. They are integer literals
    /// because `PHP_INT_MIN`/`PHP_INT_MAX` are lexer tokens, not parsed constant names.
    #[test]
    fn the_transition_window_defaults_span_the_whole_range() {
        let transitions = tz_declarations()
            .into_iter()
            .find(|stmt| {
                matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == "timezone_transitions_get")
            })
            .expect("timezone_transitions_get must be declared");
        let StmtKind::FunctionDecl { params, .. } = &transitions.kind else {
            unreachable!("filtered above");
        };
        let defaults: Vec<&crate::parser::ast::ExprKind> = params
            .iter()
            .filter_map(|(_, _, default, _)| default.as_ref().map(|expr| &expr.kind))
            .collect();
        assert_eq!(
            defaults,
            vec![
                &crate::parser::ast::ExprKind::IntLiteral(i64::MIN),
                &crate::parser::ast::ExprKind::IntLiteral(i64::MAX),
            ]
        );
    }

    /// DatePeriod activation records hidden timelib functions and externs for reachability roots.
    #[test]
    fn date_period_records_hidden_timelib_dependencies() {
        let tokens = crate::lexer::tokenize(
            r#"<?php $period = DatePeriod::createFromISO8601String("R2/2000-01-01T00:00:00Z/P1D");"#,
        )
        .expect("DatePeriod detection fixture must tokenize");
        let program = crate::parser::parse(&tokens).expect("DatePeriod detection fixture must parse");
        assert!(program_uses_tz(&program));
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let _ = inject_if_used(program, false, &mut inventory);
        let group = inventory.groups.get("tz").expect("tz group must be recorded");
        assert!(group.functions.contains("__elephc_timelib_period_parse"));
        assert!(group.externs.contains("elephc_tz_format_civil"));
        let runtime_group = inventory
            .groups
            .get(TIMELIB_RUNTIME_REACHABILITY_GROUP)
            .expect("timelib runtime roots must be recorded");
        assert!(
            runtime_group
                .functions
                .contains("__elephc_timelib_period_parse")
        );
        assert!(runtime_group.externs.contains("elephc_tz_format_civil"));
        assert!(!runtime_group.functions.contains("timezone_offset_get"));
        assert!(
            !runtime_group
                .functions
                .contains("__elephc_timezone_argument_type")
        );
    }

    /// Parses the retained test-only timelib source as the generator oracle.
    fn audited_timelib_declarations() -> Program {
        let tokens = crate::lexer::tokenize(TIMELIB_PRELUDE_SRC)
            .expect("timelib oracle source must tokenize");
        crate::parser::parse(&tokens).expect("timelib oracle source must parse")
    }

    /// Generates the production direct-AST timelib declarations on explicit request.
    #[test]
    fn generate_direct_timelib_declarations_on_request() {
        let Ok(output_path) = std::env::var("ELEPHC_GENERATED_TIMELIB_OUT") else {
            return;
        };
        let generated = crate::synthetic_class::transcribe::transcribe_split_plain(
            &audited_timelib_declarations(),
            "timelib_declarations",
        );
        let source = format!(
            "//! Purpose:\n//! Generated direct AST for timelib bridge declarations and decoding helpers.\n//!\n//! Called from:\n//! - `crate::tz_prelude::inject_if_used()`.\n//!\n//! Key details:\n//! - Generated from a test-only PHP oracle; production performs no PHP parsing.\n\nuse crate::parser::ast::*;\nuse crate::synthetic_class::*;\n\n{generated}"
        );
        std::fs::write(output_path, source).expect("generated timelib declarations must write");
    }

    /// Proves the checked-in timelib AST renders identically to its test-only PHP oracle.
    #[test]
    fn generated_timelib_declarations_match_audited_source() {
        assert_eq!(
            crate::synthetic_class::print::print_program(
                &generated_timelib::timelib_declarations(),
            ),
            crate::synthetic_class::print::print_program(&audited_timelib_declarations()),
        );
    }
}
