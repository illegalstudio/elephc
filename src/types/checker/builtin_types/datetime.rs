//! Purpose:
//! Builds synthetic checker metadata for PHP builtin date/time classes (`DateTimeInterface`,
//! `DateTimeZone`, `DateTimeImmutable`). The classes are implemented as synthetic PHP method
//! bodies that delegate to the procedural date builtins (`time()`, `strtotime()`, `date()`).
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker::driver` init
//!
//! Key details:
//! - No dedicated codegen/runtime: each method is ordinary PHP lowered by the normal pipeline.
//! - Timestamps are stored as an integer property; the timezone is stored by name (defaults to UTC).

use std::collections::HashMap;

use crate::names::Name;
use crate::parser::ast::{
    BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt, StmtKind,
    TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

use super::declarations::InterfaceDeclInfo;

/// Returns a dummy source span for synthetic AST nodes.
fn dummy() -> crate::span::Span {
    crate::span::Span::dummy()
}

/// Builds a public string class constant for the synthetic date/time classes.
fn str_class_const(name: &str, value: &str) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        value: Expr::new(ExprKind::StringLiteral(value.to_string()), dummy()),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public integer class constant for the synthetic date/time classes.
fn int_class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        value: Expr::new(ExprKind::IntLiteral(value), dummy()),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the `DateTimeZone` region/group constants used by `listIdentifiers()`.
/// The per-region bits are powers of two; `ALL` is their OR, `ALL_WITH_BC` adds
/// the backward-compatibility bit (2048), and `PER_COUNTRY` (4096) switches the
/// filter to the country-code argument. Values match PHP exactly.
fn datetime_zone_group_constants() -> Vec<ClassConst> {
    vec![
        int_class_const("AFRICA", 1),
        int_class_const("AMERICA", 2),
        int_class_const("ANTARCTICA", 4),
        int_class_const("ARCTIC", 8),
        int_class_const("ASIA", 16),
        int_class_const("ATLANTIC", 32),
        int_class_const("AUSTRALIA", 64),
        int_class_const("EUROPE", 128),
        int_class_const("INDIAN", 256),
        int_class_const("PACIFIC", 512),
        int_class_const("UTC", 1024),
        int_class_const("ALL", 2047),
        int_class_const("ALL_WITH_BC", 4095),
        int_class_const("PER_COUNTRY", 4096),
    ]
}

/// Builds the shared `DateTimeInterface` format constants (`ATOM`, `COOKIE`, the
/// `RFC*` family, `RSS`, `W3C`, ...). PHP exposes them on the interface and, by
/// inheritance, on `DateTime` and `DateTimeImmutable`; the same list is attached
/// to all three synthetic declarations. Values match PHP 8.4 exactly.
fn datetime_format_constants() -> Vec<ClassConst> {
    vec![
        str_class_const("ATOM", "Y-m-d\\TH:i:sP"),
        str_class_const("COOKIE", "l, d-M-Y H:i:s T"),
        str_class_const("ISO8601", "Y-m-d\\TH:i:sO"),
        str_class_const("ISO8601_EXPANDED", "X-m-d\\TH:i:sP"),
        str_class_const("RFC822", "D, d M y H:i:s O"),
        str_class_const("RFC850", "l, d-M-y H:i:s T"),
        str_class_const("RFC1036", "D, d M y H:i:s O"),
        str_class_const("RFC1123", "D, d M Y H:i:s O"),
        str_class_const("RFC7231", "D, d M Y H:i:s \\G\\M\\T"),
        str_class_const("RFC2822", "D, d M Y H:i:s O"),
        str_class_const("RFC3339", "Y-m-d\\TH:i:sP"),
        str_class_const("RFC3339_EXTENDED", "Y-m-d\\TH:i:s.vP"),
        str_class_const("RSS", "D, d M Y H:i:s O"),
        str_class_const("W3C", "Y-m-d\\TH:i:sP"),
    ]
}

/// Builds an `$this->property` access expression.
fn this_property(property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// Builds a `$this->property = value;` statement.
fn assign_this_property(property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
            value,
        },
        dummy(),
    )
}

/// Builds a `return <expr>;` statement.
fn return_expr(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), dummy())
}

/// Builds a public instance `ClassMethod` with the given params, return type, and body.
fn method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
        variadic: None,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public instance `ClassProperty` with a default value.
fn property(name: &str, type_expr: TypeExpr, default: Expr) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: Some(type_expr),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default: Some(default),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// `DateTimeZone::__construct(string $timezone = "UTC")` — stores the identifier verbatim.
fn datetime_zone_constructor() -> ClassMethod {
    method(
        "__construct",
        vec![(
            "timezone".to_string(),
            Some(TypeExpr::Str),
            Some(Expr::new(ExprKind::StringLiteral("UTC".to_string()), dummy())),
            false,
        )],
        None,
        vec![assign_this_property(
            "name",
            Expr::new(ExprKind::Variable("timezone".to_string()), dummy()),
        )],
    )
}

/// `DateTimeZone::getName(): string` — returns the stored identifier.
fn datetime_zone_get_name() -> ClassMethod {
    method("getName", Vec::new(), Some(TypeExpr::Str), vec![return_expr(this_property("name"))])
}

/// `DateTimeZone::getOffset(DateTimeInterface $datetime): int` — UTC offset (seconds) of this zone
/// at the given instant.
///
/// Temporarily applies this zone via `date_default_timezone_set`, reads the offset with the `date()`
/// `Z` specifier for `$datetime->getTimestamp()` (so it is daylight-saving correct), then restores
/// the previous default. Returns a positive value east of UTC, negative west.
fn datetime_zone_get_offset() -> ClassMethod {
    let call = |name: &str, args: Vec<Expr>| {
        Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args }, dummy())
    };
    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    let expr_stmt = |e: Expr| Stmt::new(StmtKind::ExprStmt(e), dummy());
    // $datetime->getTimestamp()
    let dt_ts = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(var("datetime")),
            method: "getTimestamp".to_string(),
            args: Vec::new(),
        },
        dummy(),
    );
    let z_spec = Expr::new(ExprKind::StringLiteral("Z".to_string()), dummy());
    method(
        "getOffset",
        vec![(
            "datetime".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
            None,
            false,
        )],
        Some(TypeExpr::Int),
        vec![
            // $__saved = date_default_timezone_get();
            Stmt::assign("__saved", call("date_default_timezone_get", Vec::new())),
            // date_default_timezone_set($this->name);
            expr_stmt(call("date_default_timezone_set", vec![this_property("name")])),
            // $__off = intval(date("Z", $datetime->getTimestamp()));
            Stmt::assign("__off", call("intval", vec![call("date", vec![z_spec, dt_ts])])),
            // date_default_timezone_set($__saved);  (restore the previous default)
            expr_stmt(call("date_default_timezone_set", vec![var("__saved")])),
            return_expr(var("__off")),
        ],
    )
}

/// `DateTimeZone::listIdentifiers(int $timezoneGroup = DateTimeZone::ALL, ?string $countryCode = null): array`
/// — returns the embedded IANA timezone identifier list. The body is a parsed `return [ ... ];`
/// over the identifiers in `timezone_ids::TIMEZONE_IDENTIFIERS_ARRAY` (captured from PHP).
///
/// The `$timezoneGroup`/`$countryCode` filter parameters are declared for signature parity (so
/// reflection reports PHP's real signature), but the body returns the full unfiltered list: real
/// calls are desugared by the name resolver to the injected `__elephc_list_identifiers()` free
/// function (which performs the group/country filter), so this body only runs via reflection
/// invocation, where filtering is best-effort.
fn datetime_zone_list_identifiers() -> ClassMethod {
    let src = format!(
        "<?php\nreturn [{}];\n",
        super::timezone_ids::TIMEZONE_IDENTIFIERS_ARRAY
    );
    let tokens =
        crate::lexer::tokenize(&src).expect("listIdentifiers body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("listIdentifiers body source must parse");
    ClassMethod {
        name: "listIdentifiers".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "timezoneGroup".to_string(),
                Some(TypeExpr::Int),
                // `DateTimeZone::ALL` (2047) as a literal: referencing the class's own constant in a
                // default triggers a circular-inheritance error, so the literal value is used.
                Some(Expr::new(ExprKind::IntLiteral(2047), dummy())),
                false,
            ),
            (
                "countryCode".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
        ],
        variadic: None,
        variadic_type: None,
        return_type: None,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Parses a synthetic-method body from elephc-PHP source into statements. Used so
/// the introspection methods return array literals directly — the only shape
/// whose element type a synthetic method's inferred (`None`) return resolves to
/// (a call to a prelude helper would infer as a scalar). Panics on a
/// tokenize/parse failure, which is a compiler bug in the static source.
fn parse_tz_body(src: &str) -> Vec<Stmt> {
    let tokens = crate::lexer::tokenize(src).expect("tz method body must tokenize");
    crate::parser::parse(&tokens).expect("tz method body must parse")
}

/// `DateTimeZone::getLocation(): array|false` — returns the zone's country code,
/// latitude, longitude, and comments (or `false` for the few zones without a
/// location). Calls the `elephc_tz` bridge directly and marshals the tab-joined
/// result into an array literal so inference resolves the return shape. Only added
/// to `DateTimeZone` when the introspection prelude is injected.
fn datetime_zone_get_location() -> ClassMethod {
    method(
        "getLocation",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("mixed"))),
        parse_tz_body(
            r#"<?php
$raw = elephc_tz_location($this->name);
if ($raw === "") {
    return false;
}
$f = explode("\t", $raw);
return [
    "country_code" => $f[0],
    "latitude" => (float) $f[1],
    "longitude" => (float) $f[2],
    "comments" => $f[3],
];
"#,
        ),
    )
}

/// `DateTimeZone::getTransitions(int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX): array|false`
/// — returns the DST transition rows in the window. The defaults reproduce PHP's
/// full no-arg list: the synthetic first row coincides with the bridge's row 0, so
/// its precomputed `time` is reused rather than asking `gmdate` to format
/// `PHP_INT_MIN`.
fn datetime_zone_get_transitions() -> ClassMethod {
    // PHP's defaults are PHP_INT_MIN/PHP_INT_MAX. They are materialized as integer
    // literals (a `ConstRef` default is not evaluated when the method is called
    // with no args), and `i64::MIN` is exactly the bridge's row-0 timestamp, so the
    // no-arg call reproduces the full transition list.
    let int_literal = |v: i64| Expr::new(ExprKind::IntLiteral(v), dummy());
    method(
        "getTransitions",
        vec![
            (
                "timestampBegin".to_string(),
                Some(TypeExpr::Int),
                Some(int_literal(i64::MIN)),
                false,
            ),
            (
                "timestampEnd".to_string(),
                Some(TypeExpr::Int),
                Some(int_literal(i64::MAX)),
                false,
            ),
        ],
        Some(TypeExpr::Named(Name::unqualified("mixed"))),
        parse_tz_body(
            r#"<?php
$raw = elephc_tz_transitions($this->name);
if ($raw === "") {
    return false;
}
$lines = explode("\n", $raw);
$all = [];
foreach ($lines as $line) {
    $g = explode("\t", $line);
    $all[] = [
        "ts" => (int) $g[0],
        "offset" => (int) $g[1],
        "isdst" => $g[2] === "1",
        "abbr" => $g[3],
        "time" => $g[4],
    ];
}
$n = count($all);
$result = [];
$active = -1;
for ($i = 0; $i < $n; $i++) {
    if ($all[$i]["ts"] <= $timestampBegin) {
        $active = $i;
    }
}
if ($active >= 0) {
    $a = $all[$active];
    // (int) unboxes the boxed array element to a plain int so the comparison with
    // the int param is reliable (a boxed element compared directly mis-evaluates).
    // $ats <= $timestampBegin by construction; when they are equal (the
    // PHP_INT_MIN default lands on row 0, or begin hits a transition exactly),
    // reuse the bridge's ts/time rather than formatting an extreme begin with
    // gmdate — gmdate(PHP_INT_MIN) exhausts the heap.
    $ats = (int) $a["ts"];
    if ($timestampBegin <= $ats) {
        // begin coincides with this transition (the PHP_INT_MIN default lands on
        // row 0): the synthetic row IS this row, so reuse it verbatim. This also
        // avoids rebuilding an array literal carrying a PHP_INT_MIN value, which the
        // array machinery mishandles.
        $result[] = $a;
    } else {
        $result[] = [
            "ts" => $timestampBegin,
            "time" => gmdate("Y-m-d\TH:i:sP", $timestampBegin),
            "offset" => $a["offset"],
            "isdst" => $a["isdst"],
            "abbr" => $a["abbr"],
        ];
    }
}
for ($i = 0; $i < $n; $i++) {
    if ($all[$i]["ts"] > $timestampBegin && $all[$i]["ts"] <= $timestampEnd) {
        $r = $all[$i];
        $result[] = [
            "ts" => $r["ts"],
            "time" => $r["time"],
            "offset" => $r["offset"],
            "isdst" => $r["isdst"],
            "abbr" => $r["abbr"],
        ];
    }
}
return $result;
"#,
        ),
    )
}

/// `DateTimeZone::listAbbreviations(): array` — returns PHP's static
/// abbreviation→offset/DST/zone table. Static method; calls the `elephc_tz` bridge
/// directly and marshals the result into the nested array literal.
fn datetime_zone_list_abbreviations() -> ClassMethod {
    ClassMethod {
        name: "listAbbreviations".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body: parse_tz_body(
            r#"<?php
$raw = elephc_tz_abbreviations();
$lines = explode("\n", $raw);
$result = [];
foreach ($lines as $line) {
    $parts = explode("\t", $line);
    $abbr = $parts[0];
    $rows = explode(";", $parts[1]);
    $arr = [];
    foreach ($rows as $row) {
        $c = explode(":", $row);
        $id = $c[2];
        $arr[] = [
            "dst" => $c[0] === "1",
            "offset" => (int) $c[1],
            "timezone_id" => ($id === "NULL" ? null : $id),
        ];
    }
    $result[$abbr] = $arr;
}
return $result;
"#,
        ),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing the `DateTime`/`DateTimeImmutable` constructor. With no timezone, parses the
/// string in the active default zone and records that as the display zone. With a `$timezone`, the
/// wall-clock string is interpreted in that zone (the default is temporarily switched so
/// `strtotime()` resolves the local time there — an explicit zone inside the string still wins),
/// and the zone becomes the display zone. `"now"` is the current instant regardless of zone.
const CONSTRUCT_SRC: &str = r#"<?php
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
fn datetime_immutable_constructor() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(CONSTRUCT_SRC).expect("DateTime constructor source must tokenize");
    let body = crate::parser::parse(&tokens).expect("DateTime constructor source must parse");
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
fn datetime_immutable_get_timestamp() -> ClassMethod {
    method("getTimestamp", Vec::new(), Some(TypeExpr::Int), vec![return_expr(this_property("timestamp"))])
}

/// `DateTime`/`DateTimeImmutable::getMicrosecond(): int` — returns the stored sub-second component
/// (0..999999), 0 unless set by `setMicrosecond()` or parsed from a fractional second.
fn datetime_get_microsecond() -> ClassMethod {
    method("getMicrosecond", Vec::new(), Some(TypeExpr::Int), vec![return_expr(this_property("microsecond"))])
}

/// `DateTimeImmutable::getTimezone(): DateTimeZone` — re-materializes a zone from the stored name.
fn datetime_immutable_get_timezone() -> ClassMethod {
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
const FORMAT_SRC: &str = r#"<?php
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
fn datetime_immutable_format() -> ClassMethod {
    let tokens = crate::lexer::tokenize(FORMAT_SRC).expect("format() body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("format() body source must parse");
    method(
        "format",
        vec![("format".to_string(), Some(TypeExpr::Str), None, false)],
        Some(TypeExpr::Str),
        body,
    )
}

/// Builds `(int) date($fmt, $this->timestamp)` — extracts a numeric component of the stored time.
fn date_component_int(fmt: &str) -> Expr {
    Expr::new(
        ExprKind::Cast {
            target: crate::parser::ast::CastType::Int,
            expr: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("date"),
                    args: vec![
                        Expr::new(ExprKind::StringLiteral(fmt.to_string()), dummy()),
                        this_property("timestamp"),
                    ],
                },
                dummy(),
            )),
        },
        dummy(),
    )
}

/// Builds an `__elephc_mktime_raw(hour, minute, second, month, day, year)` call expression — the
/// internal fixed-arity runtime entry that the `mktime()`/`gmmktime()` procedural aliases desugar
/// to. Synthetic method bodies call it directly (they are injected after the name resolver, so
/// the alias rewrite never runs on them); using the raw name avoids an unresolved `mktime` call.
fn mktime_call(parts: [&str; 6]) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("__elephc_mktime_raw"),
            args: parts
                .iter()
                .map(|n| Expr::new(ExprKind::Variable((*n).to_string()), dummy()))
                .collect(),
        },
        dummy(),
    )
}

/// Builds the statement tail that publishes a freshly computed timestamp.
///
/// Mutable classes (`DateTime`) assign `$this->timestamp` and return `$this`. Immutable classes
/// (`DateTimeImmutable`) construct a fresh instance, copy the new timestamp and the timezone name,
/// and return it — preserving copy-on-modify semantics.
fn result_tail(result_ts: Expr, mutable: bool, class_name: &str) -> Vec<Stmt> {
    result_tail_micro(result_ts, None, mutable, class_name)
}

/// Like `result_tail`, but with an explicit sub-second value for the result. When
/// `result_micro` is `None` the existing `$this->microsecond` is carried through
/// (the common case); add()/sub() pass the recomputed microsecond instead.
fn result_tail_micro(
    result_ts: Expr,
    result_micro: Option<Expr>,
    mutable: bool,
    class_name: &str,
) -> Vec<Stmt> {
    let micro = result_micro.unwrap_or_else(|| this_property("microsecond"));
    if mutable {
        vec![
            assign_this_property("microsecond", micro),
            assign_this_property("timestamp", result_ts),
            return_expr(Expr::new(ExprKind::This, dummy())),
        ]
    } else {
        let new_var = || Expr::new(ExprKind::Variable("__new".to_string()), dummy());
        vec![
            Stmt::assign(
                "__new",
                Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified(class_name),
                        args: Vec::new(),
                    },
                    dummy(),
                ),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timestamp".to_string(),
                    value: result_ts,
                },
                dummy(),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timezone_name".to_string(),
                    value: this_property("timezone_name"),
                },
                dummy(),
            ),
            // Carry the sub-second component into the fresh immutable instance so it survives
            // setTimestamp/setTime/setDate/setTimezone/add/sub/modify.
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "microsecond".to_string(),
                    value: micro,
                },
                dummy(),
            ),
            return_expr(new_var()),
        ]
    }
}

/// `setTimestamp(int $timestamp)` — sets the stored UNIX timestamp.
fn make_set_timestamp(mutable: bool, class_name: &str) -> ClassMethod {
    method(
        "setTimestamp",
        vec![("timestamp".to_string(), Some(TypeExpr::Int), None, false)],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        result_tail(
            Expr::new(ExprKind::Variable("timestamp".to_string()), dummy()),
            mutable,
            class_name,
        ),
    )
}

/// `setMicrosecond(int $microsecond): static` — sets the sub-second component. Mutable updates
/// `$this` in place; immutable returns a fresh instance carrying the same instant/zone with the new
/// micros (the instant in seconds is unchanged).
fn make_set_microsecond(mutable: bool, class_name: &str) -> ClassMethod {
    let us = || Expr::new(ExprKind::Variable("microsecond".to_string()), dummy());
    let body = if mutable {
        vec![
            assign_this_property("microsecond", us()),
            return_expr(Expr::new(ExprKind::This, dummy())),
        ]
    } else {
        let new_var = || Expr::new(ExprKind::Variable("__new".to_string()), dummy());
        let prop_assign = |property: &str, value: Expr| {
            Stmt::new(
                StmtKind::PropertyAssign { object: Box::new(new_var()), property: property.to_string(), value },
                dummy(),
            )
        };
        vec![
            Stmt::assign(
                "__new",
                Expr::new(
                    ExprKind::NewObject { class_name: Name::unqualified(class_name), args: Vec::new() },
                    dummy(),
                ),
            ),
            prop_assign("timestamp", this_property("timestamp")),
            prop_assign("timezone_name", this_property("timezone_name")),
            prop_assign("microsecond", us()),
            return_expr(new_var()),
        ]
    };
    method(
        "setMicrosecond",
        vec![("microsecond".to_string(), Some(TypeExpr::Int), None, false)],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// `setTime(int $hour, int $minute, int $second = 0, int $microsecond = 0)` — keeps the date,
/// replaces the time-of-day and sub-second component (PHP 8.4+).
fn make_set_time(mutable: bool, class_name: &str) -> ClassMethod {
    let mut body = vec![
        Stmt::assign("__y", date_component_int("Y")),
        Stmt::assign("__mo", date_component_int("n")),
        Stmt::assign("__d", date_component_int("j")),
    ];
    body.extend(result_tail_micro(
        mktime_call(["hour", "minute", "second", "__mo", "__d", "__y"]),
        Some(Expr::new(ExprKind::Variable("microsecond".to_string()), dummy())),
        mutable,
        class_name,
    ));
    method(
        "setTime",
        vec![
            ("hour".to_string(), Some(TypeExpr::Int), None, false),
            ("minute".to_string(), Some(TypeExpr::Int), None, false),
            (
                "second".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(0), dummy())),
                false,
            ),
            (
                "microsecond".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(0), dummy())),
                false,
            ),
        ],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// `setDate(int $year, int $month, int $day)` — keeps the time-of-day, replaces the calendar date.
fn make_set_date(mutable: bool, class_name: &str) -> ClassMethod {
    let mut body = vec![
        Stmt::assign("__h", date_component_int("G")),
        Stmt::assign("__mi", date_component_int("i")),
        Stmt::assign("__s", date_component_int("s")),
    ];
    body.extend(result_tail(
        mktime_call(["__h", "__mi", "__s", "month", "day", "year"]),
        mutable,
        class_name,
    ));
    method(
        "setDate",
        vec![
            ("year".to_string(), Some(TypeExpr::Int), None, false),
            ("month".to_string(), Some(TypeExpr::Int), None, false),
            ("day".to_string(), Some(TypeExpr::Int), None, false),
        ],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// Builds a `$var->property` access expression.
fn var_property(var: &str, property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::Variable(var.to_string()), dummy())),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// `setTimezone(DateTimeZone $timezone)` — stores the zone identifier (keeps the timestamp).
///
/// Reads the public `DateTimeZone::$name`. `DateTime` mutates `$this`; `DateTimeImmutable`
/// returns a fresh instance with the same timestamp and the new timezone name.
fn make_set_timezone(mutable: bool, class_name: &str) -> ClassMethod {
    let tz_name = var_property("timezone", "name");
    let body = if mutable {
        vec![
            assign_this_property("timezone_name", tz_name),
            return_expr(Expr::new(ExprKind::This, dummy())),
        ]
    } else {
        let new_var = || Expr::new(ExprKind::Variable("__new".to_string()), dummy());
        vec![
            Stmt::assign(
                "__new",
                Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified(class_name),
                        args: Vec::new(),
                    },
                    dummy(),
                ),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timestamp".to_string(),
                    value: this_property("timestamp"),
                },
                dummy(),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timezone_name".to_string(),
                    value: tz_name,
                },
                dummy(),
            ),
            return_expr(new_var()),
        ]
    };
    method(
        "setTimezone",
        vec![(
            "timezone".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeZone"))),
            None,
            false,
        )],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// `add(DateInterval $interval)` / `sub(DateInterval $interval)` — shifts the date by the interval.
///
/// Decomposes `$this->timestamp` into calendar components via `date()`, applies each signed interval
/// component, then recomposes with `mktime()` (which normalizes overflow — e.g. day 32 rolls into the
/// next month). `$interval->invert` flips the direction (`$__sign` = `1 - 2*invert` for `add`, negated
/// for `sub`). `DateTime` mutates `$this`; `DateTimeImmutable` returns a fresh instance via
/// `result_tail`. `is_add` selects `add` (true) vs `sub` (false).
fn make_add_sub(name: &str, mutable: bool, class_name: &str, is_add: bool) -> ClassMethod {
    let bin = |l: Expr, op: BinOp, r: Expr| {
        Expr::new(ExprKind::BinaryOp { left: Box::new(l), op, right: Box::new(r) }, dummy())
    };
    let int_lit = |n: i64| Expr::new(ExprKind::IntLiteral(n), dummy());
    let sign_var = || Expr::new(ExprKind::Variable("__sign".to_string()), dummy());

    // $__sign = 1 - 2*$interval->invert  (add)  |  2*$interval->invert - 1  (sub)
    let two_invert = bin(int_lit(2), BinOp::Mul, var_property("interval", "invert"));
    let sign_expr = if is_add {
        bin(int_lit(1), BinOp::Sub, two_invert)
    } else {
        bin(two_invert, BinOp::Sub, int_lit(1))
    };

    // component(fmt, field) = (int)date(fmt, $this->timestamp) + $interval-><field> * $__sign
    let component = |fmt: &str, field: &str| {
        bin(
            date_component_int(fmt),
            BinOp::Add,
            bin(var_property("interval", field), BinOp::Mul, sign_var()),
        )
    };

    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    // $__ivu = (int) round($interval->f * 1000000) — the interval's whole microseconds.
    let interval_micros = Expr::new(
        ExprKind::Cast {
            target: crate::parser::ast::CastType::Int,
            expr: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("round"),
                    args: vec![bin(
                        var_property("interval", "f"),
                        BinOp::Mul,
                        Expr::new(ExprKind::FloatLiteral(1_000_000.0), dummy()),
                    )],
                },
                dummy(),
            )),
        },
        dummy(),
    );
    // One-second carry/borrow: $__micro stays in [0, 1000000); the carry folds into $__s
    // (which mktime() then normalizes). $__micro is bounded to a single carry by construction.
    let carry_up = Stmt::new(
        StmtKind::If {
            condition: bin(var("__micro"), BinOp::GtEq, int_lit(1_000_000)),
            then_body: vec![
                Stmt::assign("__micro", bin(var("__micro"), BinOp::Sub, int_lit(1_000_000))),
                Stmt::assign("__s", bin(var("__s"), BinOp::Add, int_lit(1))),
            ],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy(),
    );
    let borrow_down = Stmt::new(
        StmtKind::If {
            condition: bin(var("__micro"), BinOp::Lt, int_lit(0)),
            then_body: vec![
                Stmt::assign("__micro", bin(var("__micro"), BinOp::Add, int_lit(1_000_000))),
                Stmt::assign("__s", bin(var("__s"), BinOp::Sub, int_lit(1))),
            ],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy(),
    );
    let mut body = vec![
        Stmt::assign("__sign", sign_expr),
        Stmt::assign("__y", component("Y", "y")),
        Stmt::assign("__mo", component("n", "m")),
        Stmt::assign("__d", component("j", "d")),
        Stmt::assign("__h", component("G", "h")),
        Stmt::assign("__mi", component("i", "i")),
        Stmt::assign("__s", component("s", "s")),
        // Apply the interval's fractional second: $__micro = $this->microsecond ± interval µs.
        Stmt::assign("__ivu", interval_micros),
        Stmt::assign(
            "__micro",
            bin(
                this_property("microsecond"),
                BinOp::Add,
                bin(var("__ivu"), BinOp::Mul, sign_var()),
            ),
        ),
        carry_up,
        borrow_down,
    ];
    body.extend(result_tail_micro(
        mktime_call(["__h", "__mi", "__s", "__mo", "__d", "__y"]),
        Some(var("__micro")),
        mutable,
        class_name,
    ));
    method(
        name,
        vec![(
            "interval".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
            None,
            false,
        )],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// `modify(string $modifier)` — applies a relative date/time modifier (e.g. `"+1 day"`,
/// `"-2 weeks"`, `"14:30"`) by re-parsing it against the object's current timestamp via
/// `strtotime($modifier, $this->timestamp)`. Mutates in place for `DateTime` and returns a
/// new instance for `DateTimeImmutable`. Supports exactly the forms `strtotime()` accepts.
fn make_modify(mutable: bool, class_name: &str) -> ClassMethod {
    // Parsed-PHP preamble (parsing lives in static helpers to keep this frame
    // small): pull any `<±N> microsecond[s]|usec[s]` clauses out of the modifier,
    // strtotime() the remainder, then apply the microsecond delta with a carry into
    // the whole-second timestamp. result_tail_micro emits the new instant.
    let src = r#"<?php
$__md = DateTime::__elephc_extract_modify_micros($modifier);
$__rest = DateTime::__elephc_strip_modify_micros($modifier);
if ($__rest === "") {
    $__ts = $this->timestamp;
} else {
    $__ts = strtotime($__rest, $this->timestamp);
    if ($__ts === false) {
        throw new DateMalformedStringException("Failed to parse time string (" . $modifier . ")");
    }
}
$__micro = $this->microsecond + $__md;
$__carry = intdiv($__micro, 1000000);
$__micro = $__micro - $__carry * 1000000;
if ($__micro < 0) {
    $__micro = $__micro + 1000000;
    $__carry = $__carry - 1;
}
$__ts = $__ts + $__carry;
"#;
    let tokens = crate::lexer::tokenize(src).expect("modify body must tokenize");
    let mut body = crate::parser::parse(&tokens).expect("modify body must parse");
    body.extend(result_tail_micro(
        Expr::new(ExprKind::Variable("__ts".to_string()), dummy()),
        Some(Expr::new(ExprKind::Variable("__micro".to_string()), dummy())),
        mutable,
        class_name,
    ));
    method(
        "modify",
        vec![("modifier".to_string(), Some(TypeExpr::Str), None, false)],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// Builds the mutating/immutable setter set for a class.
fn datetime_setter_methods(mutable: bool, class_name: &str) -> Vec<ClassMethod> {
    vec![
        make_set_timestamp(mutable, class_name),
        make_set_microsecond(mutable, class_name),
        make_set_time(mutable, class_name),
        make_set_date(mutable, class_name),
        make_set_timezone(mutable, class_name),
        make_add_sub("add", mutable, class_name, true),
        make_add_sub("sub", mutable, class_name, false),
        make_modify(mutable, class_name),
    ]
}

/// Builds the shared instance method set used by both `DateTime` and `DateTimeImmutable`
/// (construct from `"now"`/string, `format`, `getTimestamp`, `getTimezone`).
fn datetime_shared_methods() -> Vec<ClassMethod> {
    vec![
        datetime_immutable_constructor(),
        datetime_immutable_get_timestamp(),
        datetime_get_microsecond(),
        datetime_immutable_get_timezone(),
        datetime_immutable_format(),
        datetime_get_offset(),
        datetime_diff_method(),
    ]
}

/// PHP source for the `createFromFormat` parser, used directly as the method body so the feature is
/// self-contained (no separately-injected helper function to keep in sync with the class emission).
///
/// `__CFF_CLASS__` is substituted with the concrete class so each method constructs its own type.
/// Field semantics mirror PHP: unspecified fields default to the current date/time, but once any
/// time field is parsed the unparsed time fields reset to 0; `!` resets all fields to the Unix
/// epoch, `|` resets the not-yet-parsed fields, `\` escapes the next format character, and any other
/// character must match the subject. Supported specifiers:
/// `Y y m n d j D l S F M z H G h g i s u v A a U O P Z T e X x` plus the metas `! | # ? * +`.
/// `D`/`l` parse a weekday name (full or abbreviated) and shift the result forward 0-6 days to that
/// weekday after all fields are applied (timelib's relative-weekday behavior). `z` is the 0-based
/// day of the year: it requires an already-parsed year, overrides month/day, and overflows into
/// subsequent years through `mktime` normalization. `#` matches one separator from `;:/.,-`, `?`
/// skips one subject byte, `*` skips bytes until the next digit or separator, and `+` tolerates
/// trailing subject data (without it, unconsumed trailing data is a parse failure, as in PHP).
/// Returns the constructed instance, or `false` when the subject does not match. `intval()` is used
/// instead of `(int)` casts because synthetic method bodies do not lower cast nodes. The timezone
/// specifiers (`O P Z T e`) consume the corresponding substring from the subject (validated as
/// `[-+]hhmm` / `[-+]hh:mm` / signed-or-unsigned seconds / greedy alpha chars / IANA-shape identifier)
/// and are cross-validated against the constructed instant's zone at the end of the parse — a
/// mismatch returns `false`, matching PHP.
const CREATE_FROM_FORMAT_SRC: &str = r##"<?php
__CFF_CLASS__::$lastErrorCount = 1;
$now = time();
$Y = intval(date("Y", $now));
$mo = intval(date("n", $now));
$da = intval(date("j", $now));
$H = intval(date("G", $now));
$mi = intval(date("i", $now));
$se = intval(date("s", $now));
$pY = false; $pmo = false; $pda = false; $pH = false; $pmi = false; $pse = false;
$is12 = false; $pm = -1;
$hasU = false; $U = 0;
$umicro = 0;
$parsedO = ""; $parsedP = ""; $parsedZ = ""; $parsedT = ""; $parsedE = "";
$wd = -1; $junkOk = false;
$fp = 0; $dp = 0;
$flen = strlen($format);
$dlen = strlen($datetime);
while ($fp < $flen) {
    $c = $format[$fp];
    $fp = $fp + 1;
    if ($c === "\\") {
        if ($fp < $flen) {
            $lit = $format[$fp];
            $fp = $fp + 1;
            if ($dp < $dlen && $datetime[$dp] === $lit) { $dp = $dp + 1; }
            else { return false; }
        }
        continue;
    }
    if ($c === "!") {
        $Y = 1970; $mo = 1; $da = 1; $H = 0; $mi = 0; $se = 0;
        $pY = true; $pmo = true; $pda = true; $pH = true; $pmi = true; $pse = true;
        continue;
    }
    if ($c === "|") {
        if (!$pY) { $Y = 1970; }
        if (!$pmo) { $mo = 1; }
        if (!$pda) { $da = 1; }
        if (!$pH) { $H = 0; }
        if (!$pmi) { $mi = 0; }
        if (!$pse) { $se = 0; }
        continue;
    }
    if ($c === "U") {
        $num = 0; $cnt = 0;
        while ($dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) { return false; }
        $hasU = true; $U = $num;
        continue;
    }
    if ($c === "u") {
        $num = 0; $cnt = 0;
        while ($cnt < 6 && $dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) { return false; }
        $umicro = $num;
        continue;
    }
    if ($c === "A" || $c === "a") {
        if ($dp + 1 < $dlen) {
            $two = substr($datetime, $dp, 2);
            if ($two === "AM" || $two === "am") { $pm = 0; $dp = $dp + 2; }
            else if ($two === "PM" || $two === "pm") { $pm = 1; $dp = $dp + 2; }
            else { return false; }
        } else { return false; }
        continue;
    }
    if ($c === "O") {
        // O = +hhmm or -hhmm (5 chars exactly): the sign and 4 digits.
        if ($dp + 5 > $dlen) { return false; }
        $sub = substr($datetime, $dp, 5);
        $ch0 = $sub[0];
        if (($ch0 !== "+" && $ch0 !== "-")
            || !ctype_digit($sub[1]) || !ctype_digit($sub[2])
            || !ctype_digit($sub[3]) || !ctype_digit($sub[4])) { return false; }
        $parsedO = $sub;
        $dp = $dp + 5;
        continue;
    }
    if ($c === "P") {
        // P = +hh:mm or -hh:mm (6 chars exactly): sign, 2 digits, ':', 2 digits.
        if ($dp + 6 > $dlen) { return false; }
        $sub = substr($datetime, $dp, 6);
        $ch0 = $sub[0];
        if (($ch0 !== "+" && $ch0 !== "-")
            || !ctype_digit($sub[1]) || !ctype_digit($sub[2])
            || $sub[3] !== ":"
            || !ctype_digit($sub[4]) || !ctype_digit($sub[5])) { return false; }
        $parsedP = $sub;
        $dp = $dp + 6;
        continue;
    }
    if ($c === "Z") {
        // Z = UTC offset in seconds: leading '+'/'-' followed by 1-4 digits, or up to 5
        // unsigned digits. PHP accepts 0, +7200, -14400, etc. Normalize: a leading '+'
        // is dropped (the date("Z") renderer never prefixes '+', even for positive
        // offsets), so the cross-validation below matches without special-casing.
        if ($dp >= $dlen) { return false; }
        $sub = "";
        $ch0 = $datetime[$dp];
        if ($ch0 === "+" || $ch0 === "-") {
            $sub = ($ch0 === "-") ? "-" : "";
            $dp = $dp + 1;
            $sd = 0;
            while ($sd < 4 && $dp < $dlen && ctype_digit($datetime[$dp])) {
                $sub = $sub . $datetime[$dp];
                $dp = $dp + 1; $sd = $sd + 1;
            }
            if ($sd === 0) { return false; }
        } else {
            $sd = 0;
            while ($sd < 5 && $dp < $dlen && ctype_digit($datetime[$dp])) {
                $sub = $sub . $datetime[$dp];
                $dp = $dp + 1; $sd = $sd + 1;
            }
            if ($sd === 0) { return false; }
        }
        $parsedZ = $sub;
        continue;
    }
    if ($c === "T") {
        // T = timezone abbreviation (e.g. CEST, EDT, UTC). PHP reads it greedily — all
        // consecutive alpha chars from `$datetime[$dp]`, not exactly 3 — so 3-letter
        // abbreviations match, and a 4-letter one like CEST also matches in full.
        if ($dp >= $dlen) { return false; }
        $ch0 = $datetime[$dp];
        $io0 = ord($ch0);
        $ok0 = ($io0 >= 65 && $io0 <= 90) || ($io0 >= 97 && $io0 <= 122);
        if (!$ok0) { return false; }
        $sub = "";
        while ($dp < $dlen) {
            $ch = $datetime[$dp];
            $io = ord($ch);
            $isAlpha = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
            if (!$isAlpha) { break; }
            $sub = $sub . $ch;
            $dp = $dp + 1;
        }
        if (strlen($sub) === 0) { return false; }
        $parsedT = $sub;
        continue;
    }
    if ($c === "e") {
        // e = timezone name (IANA, possibly with slashes/underscores, e.g. Europe/Paris,
        // America/Argentina/Buenos_Aires, Etc/GMT-1). Greedy read while the next char is in
        // [A-Za-z0-9_/+-] and the subject has more.
        $tzchars = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_/+-";
        $sub = "";
        while ($dp < $dlen) {
            $ch = $datetime[$dp];
            $found = 0;
            $ti = 0;
            while ($ti < 64) {
                if ($tzchars[$ti] === $ch) { $found = 1; break; }
                $ti = $ti + 1;
            }
            if ($found === 0) { break; }
            $sub = $sub . $ch;
            $dp = $dp + 1;
        }
        if (strlen($sub) === 0) { return false; }
        $parsedE = $sub;
        continue;
    }
    if ($c === "S") {
        if ($dp + 2 > $dlen) { return false; }
        $two = strtolower(substr($datetime, $dp, 2));
        if ($two !== "st" && $two !== "nd" && $two !== "rd" && $two !== "th") { return false; }
        $dp = $dp + 2;
        continue;
    }
    if ($c === "D" || $c === "l") {
        $sub = "";
        while ($dp < $dlen) {
            $io = ord($datetime[$dp]);
            $isAlpha = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
            if (!$isAlpha) { break; }
            $sub = $sub . $datetime[$dp];
            $dp = $dp + 1;
        }
        $low = strtolower($sub);
        $wdv = -1;
        if ($low === "sun" || $low === "sunday") { $wdv = 0; }
        else if ($low === "mon" || $low === "monday") { $wdv = 1; }
        else if ($low === "tue" || $low === "tues" || $low === "tuesday") { $wdv = 2; }
        else if ($low === "wed" || $low === "wednesday") { $wdv = 3; }
        else if ($low === "thu" || $low === "thur" || $low === "thurs" || $low === "thursday") { $wdv = 4; }
        else if ($low === "fri" || $low === "friday") { $wdv = 5; }
        else if ($low === "sat" || $low === "saturday") { $wdv = 6; }
        if ($wdv < 0) { return false; }
        $wd = $wdv;
        continue;
    }
    if ($c === "M" || $c === "F") {
        $sub = "";
        while ($dp < $dlen) {
            $io = ord($datetime[$dp]);
            $isAlpha = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
            if (!$isAlpha) { break; }
            $sub = $sub . $datetime[$dp];
            $dp = $dp + 1;
        }
        $low = strtolower($sub);
        $mv = 0;
        if ($low === "jan" || $low === "january") { $mv = 1; }
        else if ($low === "feb" || $low === "february") { $mv = 2; }
        else if ($low === "mar" || $low === "march") { $mv = 3; }
        else if ($low === "apr" || $low === "april") { $mv = 4; }
        else if ($low === "may") { $mv = 5; }
        else if ($low === "jun" || $low === "june") { $mv = 6; }
        else if ($low === "jul" || $low === "july") { $mv = 7; }
        else if ($low === "aug" || $low === "august") { $mv = 8; }
        else if ($low === "sep" || $low === "sept" || $low === "september") { $mv = 9; }
        else if ($low === "oct" || $low === "october") { $mv = 10; }
        else if ($low === "nov" || $low === "november") { $mv = 11; }
        else if ($low === "dec" || $low === "december") { $mv = 12; }
        if ($mv === 0) { return false; }
        $mo = $mv; $pmo = true;
        continue;
    }
    if ($c === "z") {
        if (!$pY) { return false; }
        $num = 0; $cnt = 0;
        while ($cnt < 3 && $dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) { return false; }
        $mo = 1; $da = $num + 1;
        $pmo = true; $pda = true;
        continue;
    }
    if ($c === "v") {
        $num = 0; $cnt = 0;
        while ($cnt < 3 && $dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) { return false; }
        $umicro = $num * 1000;
        continue;
    }
    if ($c === "#") {
        if ($dp >= $dlen) { return false; }
        $chs = $datetime[$dp];
        if ($chs !== ";" && $chs !== ":" && $chs !== "/" && $chs !== "." && $chs !== "," && $chs !== "-") { return false; }
        $dp = $dp + 1;
        continue;
    }
    if ($c === "?") {
        if ($dp >= $dlen) { return false; }
        $dp = $dp + 1;
        continue;
    }
    if ($c === "*") {
        while ($dp < $dlen) {
            $chs = $datetime[$dp];
            if (ctype_digit($chs)) { break; }
            if ($chs === ";" || $chs === ":" || $chs === "/" || $chs === "." || $chs === "," || $chs === "-" || $chs === " ") { break; }
            $dp = $dp + 1;
        }
        continue;
    }
    if ($c === "+") {
        $junkOk = true;
        continue;
    }
    if ($c === "X" || $c === "x") {
        $sign = 1;
        $hadSign = false;
        if ($dp < $dlen && $datetime[$dp] === "+") { $hadSign = true; $dp = $dp + 1; }
        else if ($dp < $dlen && $datetime[$dp] === "-") { $hadSign = true; $sign = -1; $dp = $dp + 1; }
        if ($c === "X" && !$hadSign) { return false; }
        $num = 0; $cnt = 0;
        while ($cnt < 6 && $dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt < 4) { return false; }
        $Y = $sign * $num; $pY = true;
        continue;
    }
    $max = 0;
    if ($c === "Y") { $max = 4; }
    else if ($c === "y") { $max = 2; }
    else if ($c === "m" || $c === "n" || $c === "d" || $c === "j" || $c === "H" || $c === "G" || $c === "h" || $c === "g" || $c === "i" || $c === "s") { $max = 2; }
    if ($max > 0) {
        $num = 0; $cnt = 0;
        while ($cnt < $max && $dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) { return false; }
        if ($c === "Y") { $Y = $num; $pY = true; }
        else if ($c === "y") { $Y = ($num < 70) ? (2000 + $num) : (1900 + $num); $pY = true; }
        else if ($c === "m" || $c === "n") { $mo = $num; $pmo = true; }
        else if ($c === "d" || $c === "j") { $da = $num; $pda = true; }
        else if ($c === "H" || $c === "G") { $H = $num; $pH = true; }
        else if ($c === "h" || $c === "g") { $H = $num; $is12 = true; $pH = true; }
        else if ($c === "i") { $mi = $num; $pmi = true; }
        else if ($c === "s") { $se = $num; $pse = true; }
        continue;
    }
    if ($dp < $dlen && $datetime[$dp] === $c) { $dp = $dp + 1; }
    else if ($c === " ") { }
    else { return false; }
}
if (!$junkOk && $dp < $dlen) { return false; }
if ($pH || $pmi || $pse) {
    if (!$pH) { $H = 0; }
    if (!$pmi) { $mi = 0; }
    if (!$pse) { $se = 0; }
}
if ($wd >= 0) {
    $zm = $mo; $zy = $Y;
    if ($zm < 3) { $zm = $zm + 12; $zy = $zy - 1; }
    $zk = $zy % 100; $zj = intdiv($zy, 100);
    $zh = ($da + intdiv(13 * ($zm + 1), 5) + $zk + intdiv($zk, 4) + intdiv($zj, 4) + 5 * $zj) % 7;
    $dow = ($zh + 6) % 7;
    $da = $da + (($wd - $dow + 7) % 7);
}
if ($is12 && $pm >= 0) {
    if ($pm === 1) { if ($H < 12) { $H = $H + 12; } }
    else { if ($H === 12) { $H = 0; } }
}
if ($hasU) {
    $ts = $U;
} else if ($timezone === null) {
    $ts = __elephc_mktime_raw($H, $mi, $se, $mo, $da, $Y);
} else {
    $saved = date_default_timezone_get();
    date_default_timezone_set($timezone->getName());
    $ts = __elephc_mktime_raw($H, $mi, $se, $mo, $da, $Y);
    date_default_timezone_set($saved);
}
// TZ cross-validation: when any of O/P/Z/T/e was parsed, re-render the same specifier
// in the same zone the wall-clock was interpreted in, and compare. A mismatch (e.g.
// "+0500" against a Europe/Paris instant) is a parse failure.
if ($parsedO !== "" || $parsedP !== "" || $parsedZ !== "" || $parsedT !== "" || $parsedE !== "") {
    $__saved = date_default_timezone_get();
    if ($timezone !== null) {
        date_default_timezone_set($timezone->getName());
    }
    $__ok = true;
    if ($__ok && $parsedO !== "" && date("O", $ts) !== $parsedO) { $__ok = false; }
    if ($__ok && $parsedP !== "" && date("P", $ts) !== $parsedP) { $__ok = false; }
    if ($__ok && $parsedZ !== "" && date("Z", $ts) !== $parsedZ) { $__ok = false; }
    if ($__ok && $parsedT !== "" && date("T", $ts) !== $parsedT) { $__ok = false; }
    if ($__ok && $parsedE !== "" && date("e", $ts) !== $parsedE) { $__ok = false; }
    date_default_timezone_set($__saved);
    if (!$__ok) { return false; }
}
$o = new __CFF_CLASS__();
$o = $o->setTimestamp($ts);
if ($timezone !== null) {
    // Set the display zone via getName() rather than setTimezone($timezone): the parameter is
    // `?DateTimeZone`, whose value reaches here boxed as Mixed, and setTimezone reads the
    // `name` property directly (which mis-reads a boxed receiver). getName() dispatches by
    // runtime class id, so it resolves correctly, mirroring the two-argument constructor.
    $o->timezone_name = $timezone->getName();
}
__CFF_CLASS__::$lastErrorCount = 0;
return $o->setMicrosecond($umicro);
"##;

/// Builds the static `createFromFormat(string $format, string $datetime, ?DateTimeZone $timezone = null)`
/// factory for `class_name` (`"DateTime"` or `"DateTimeImmutable"`). When `$timezone` is given, the
/// parsed wall-clock is interpreted in that zone (default zone switched around `mktime`, then
/// restored) and it becomes the result's display zone, mirroring the constructor's zone handling.
///
/// The body is the parsed `CREATE_FROM_FORMAT_SRC` parser with the class name substituted, so the
/// method is self-contained and emitted together with the class (no externally-injected helper to
/// gate). The return type is declared explicitly as `class|false` because synthetic builtin methods
/// do not get body-driven return-type inference, and the union lets the method-dispatch path resolve
/// `->format()` etc. on the success arm.
fn datetime_create_from_format(class_name: &str) -> ClassMethod {
    let src = CREATE_FROM_FORMAT_SRC.replace("__CFF_CLASS__", class_name);
    let tokens =
        crate::lexer::tokenize(&src).expect("createFromFormat body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("createFromFormat body source must parse");
    ClassMethod {
        name: "createFromFormat".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("format".to_string(), Some(TypeExpr::Str), None, false),
            ("datetime".to_string(), Some(TypeExpr::Str), None, false),
            (
                "timezone".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
                    "DateTimeZone",
                ))))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Union(vec![
            TypeExpr::Named(Name::unqualified(class_name)),
            TypeExpr::Bool,
        ])),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `getLastErrors()` / `date_get_last_errors()`. Returns PHP's structured result
/// array; elephc tracks only whether the last `createFromFormat()` on this class failed
/// (`error_count` 0/1, no warnings), which covers the common
/// `if (DateTime::getLastErrors()['error_count'])` check after a parse.
const GET_LAST_ERRORS_SRC: &str = r#"<?php
$ec = __GLE_CLASS__::$lastErrorCount;
$errs = [];
if ($ec > 0) { $errs = [0 => "The date string failed to match the format"]; }
return ["warning_count" => 0, "warnings" => [], "error_count" => $ec, "errors" => $errs];
"#;

/// Builds the static `getLastErrors(): array` method for `class_name`, reading the per-class
/// `lastErrorCount` static that `createFromFormat()` sets (1 on entry, cleared to 0 on success).
fn datetime_get_last_errors(class_name: &str) -> ClassMethod {
    let src = GET_LAST_ERRORS_SRC.replace("__GLE_CLASS__", class_name);
    let tokens = crate::lexer::tokenize(&src).expect("getLastErrors body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("getLastErrors body source must parse");
    ClassMethod {
        name: "getLastErrors".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing the cross-conversion factories (`createFromInterface`,
/// `createFromImmutable`, `createFromMutable`): copy the source object's instant and display
/// timezone into a fresh instance of the target class. `__TARGET__` is substituted with the
/// target class name.
const CREATE_FROM_OBJECT_SRC: &str = r#"<?php
$d = new __TARGET__();
$d = $d->setTimestamp($object->getTimestamp());
$d = $d->setTimezone($object->getTimezone());
return $d;
"#;

/// Builds a cross-conversion factory (`createFromInterface` / `createFromImmutable` /
/// `createFromMutable`) returning a fresh `target_class` that carries the source object's
/// instant and timezone. Static; the body is the parsed `CREATE_FROM_OBJECT_SRC`. `$object`
/// is typed `DateTimeInterface` (the common supertype) because the body only needs interface
/// methods; the return type is declared explicitly as `target_class` since synthetic builtin
/// methods do not get body-driven return-type inference.
fn datetime_create_from_object(method_name: &str, target_class: &str) -> ClassMethod {
    let src = CREATE_FROM_OBJECT_SRC.replace("__TARGET__", target_class);
    let tokens =
        crate::lexer::tokenize(&src).expect("createFrom* body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("createFrom* body source must parse");
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "object".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
            None,
            false,
        )],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified(target_class))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `createFromTimestamp(int|float $timestamp): static` (PHP 8.4): build a fresh
/// instance set to the given UNIX timestamp. `__CFT_CLASS__` is substituted with the class name.
const CREATE_FROM_TIMESTAMP_SRC: &str = r#"<?php
$d = new __CFT_CLASS__();
$secs = intval(floor($timestamp));
$d = $d->setTimestamp($secs);
$d = $d->setMicrosecond(intval(round(($timestamp - $secs) * 1000000)));
return $d;
"#;

/// Builds the static `createFromTimestamp($timestamp): static` factory for `class_name`. `$timestamp`
/// is typed `mixed` (PHP accepts int or float). The whole-second part uses `floor()` (so negative
/// fractional timestamps round toward -inf like PHP) and the remaining fraction becomes microseconds
/// via `setMicrosecond()`. Self-contained parsed source; the return type is declared as `class_name`
/// since synthetic builtin methods get no body-driven return inference.
fn datetime_create_from_timestamp(class_name: &str) -> ClassMethod {
    let src = CREATE_FROM_TIMESTAMP_SRC.replace("__CFT_CLASS__", class_name);
    let tokens =
        crate::lexer::tokenize(&src).expect("createFromTimestamp body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("createFromTimestamp body source must parse");
    ClassMethod {
        name: "createFromTimestamp".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "timestamp".to_string(),
            Some(TypeExpr::Named(Name::unqualified("mixed"))),
            None,
            false,
        )],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified(class_name))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `setISODate()`. Computes the Gregorian date for an ISO 8601 week date
/// (year, week, day-of-week 1=Monday..7=Sunday), preserving the current time-of-day. ISO week 1
/// contains Jan 4, so the Monday of week 1 is `Jan 4 - (weekday(Jan 4) - 1)`; the target day is
/// that plus `(week - 1) * 7 + (dayOfWeek - 1)`, fed to `mktime()` which normalizes overflow
/// (e.g. week 53 of a 52-week year rolls into the next year). Delegates to `$this->setTimestamp()`
/// so the mutable/immutable result and timezone handling are shared with the other setters.
const SET_ISODATE_SRC: &str = r#"<?php
$h = (int)date("H", $this->timestamp);
$mi = (int)date("i", $this->timestamp);
$se = (int)date("s", $this->timestamp);
$jan4 = __elephc_mktime_raw($h, $mi, $se, 1, 4, $year);
$dow = (int)date("N", $jan4);
$day = 4 - ($dow - 1) + ($week - 1) * 7 + ($dayOfWeek - 1);
return $this->setTimestamp(__elephc_mktime_raw($h, $mi, $se, 1, $day, $year));
"#;

/// `setISODate(int $year, int $week, int $dayOfWeek = 1): static` — set the date from an ISO 8601
/// week date, keeping the time-of-day. The body is the parsed `SET_ISODATE_SRC`; the return type
/// is declared as `class_name` since synthetic methods do not get body-driven return inference.
fn datetime_set_isodate(class_name: &str) -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(SET_ISODATE_SRC).expect("setISODate body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("setISODate body source must parse");
    ClassMethod {
        name: "setISODate".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("year".to_string(), Some(TypeExpr::Int), None, false),
            ("week".to_string(), Some(TypeExpr::Int), None, false),
            (
                "dayOfWeek".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(1), dummy())),
                false,
            ),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified(class_name))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `date_parse_from_format()` (and `date_parse()` via format detection): the
/// same format parser as `CREATE_FROM_FORMAT_SRC`, but instead of building an object it returns
/// PHP's component array — each field set to its parsed integer or left `false` when not present,
/// plus `warning_count`/`error_count` (trailing/unmatched input) and the empty `warnings`/`errors`
/// slots. Supports the numeric specifiers (`Y y m n d j H G h g i s`), AM/PM (`A a`), textual month
/// names (`F M`), textual weekday names (`D l`, consumed only), Unix timestamp (`U`),
/// microseconds/milliseconds (`u v` → `fraction`), the timezone specifiers (`O P Z T e`, consumed
/// with `is_localtime` set), and the reset metas (`! |`). Built as `false` literals then
/// conditionally overwritten, because an int|false union flowing through a single variable would
/// coerce to `0`.
const DATE_PARSE_FROM_FORMAT_SRC: &str = r#"<?php
$Y = 0; $mo = 0; $da = 0; $H = 0; $mi = 0; $se = 0;
$pY = false; $pmo = false; $pda = false; $pH = false; $pmi = false; $pse = false;
$is12 = false; $pm = -1;
$us = 0; $pus = false;
$hasU = false; $U = 0;
$isLocal = false;
$errors = 0; $warnings = 0;
$fp = 0; $dp = 0;
$flen = strlen($format);
$dlen = strlen($datetime);
while ($fp < $flen) {
    $c = $format[$fp];
    $fp = $fp + 1;
    if ($c === "\\") {
        if ($fp < $flen) {
            $lit = $format[$fp];
            $fp = $fp + 1;
            if ($dp < $dlen && $datetime[$dp] === $lit) { $dp = $dp + 1; }
            else { $errors = $errors + 1; }
        }
        continue;
    }
    if ($c === "!") {
        $Y = 1970; $mo = 1; $da = 1; $H = 0; $mi = 0; $se = 0;
        $pY = true; $pmo = true; $pda = true; $pH = true; $pmi = true; $pse = true;
        continue;
    }
    if ($c === "|") {
        if (!$pY) { $Y = 1970; }
        if (!$pmo) { $mo = 1; }
        if (!$pda) { $da = 1; }
        if (!$pH) { $H = 0; }
        if (!$pmi) { $mi = 0; }
        if (!$pse) { $se = 0; }
        continue;
    }
    if ($c === "A" || $c === "a") {
        if ($dp + 1 < $dlen) {
            $two = substr($datetime, $dp, 2);
            if ($two === "AM" || $two === "am") { $pm = 0; $dp = $dp + 2; }
            else if ($two === "PM" || $two === "pm") { $pm = 1; $dp = $dp + 2; }
            else { $errors = $errors + 1; }
        } else { $errors = $errors + 1; }
        continue;
    }
    if ($c === "F" || $c === "M") {
        $sub = "";
        while ($dp < $dlen) {
            $io = ord($datetime[$dp]);
            $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
            if (!$a) { break; }
            $sub = $sub . $datetime[$dp]; $dp = $dp + 1;
        }
        $low = strtolower($sub); $mv = 0;
        if ($low === "jan" || $low === "january") { $mv = 1; }
        else if ($low === "feb" || $low === "february") { $mv = 2; }
        else if ($low === "mar" || $low === "march") { $mv = 3; }
        else if ($low === "apr" || $low === "april") { $mv = 4; }
        else if ($low === "may") { $mv = 5; }
        else if ($low === "jun" || $low === "june") { $mv = 6; }
        else if ($low === "jul" || $low === "july") { $mv = 7; }
        else if ($low === "aug" || $low === "august") { $mv = 8; }
        else if ($low === "sep" || $low === "sept" || $low === "september") { $mv = 9; }
        else if ($low === "oct" || $low === "october") { $mv = 10; }
        else if ($low === "nov" || $low === "november") { $mv = 11; }
        else if ($low === "dec" || $low === "december") { $mv = 12; }
        if ($mv === 0) { $errors = $errors + 1; }
        else { $mo = $mv; $pmo = true; }
        continue;
    }
    if ($c === "D" || $c === "l") {
        while ($dp < $dlen) {
            $io = ord($datetime[$dp]);
            $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
            if (!$a) { break; }
            $dp = $dp + 1;
        }
        continue;
    }
    if ($c === "U") {
        $num = 0; $cnt = 0;
        while ($dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) { $errors = $errors + 1; }
        else { $U = $num; $hasU = true; }
        continue;
    }
    if ($c === "u" || $c === "v") {
        $num = 0; $cnt = 0; $maxu = ($c === "u") ? 6 : 3;
        while ($cnt < $maxu && $dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) { $errors = $errors + 1; }
        else { $us = $num; $pus = true; }
        continue;
    }
    if ($c === "O" || $c === "P" || $c === "Z" || $c === "T" || $c === "e") {
        if ($c === "O" || $c === "P") {
            if ($dp < $dlen && ($datetime[$dp] === "+" || $datetime[$dp] === "-")) { $dp = $dp + 1; }
            $cnt = 0;
            while ($cnt < 5 && $dp < $dlen && (ctype_digit($datetime[$dp]) || $datetime[$dp] === ":")) {
                $dp = $dp + 1; $cnt = $cnt + 1;
            }
        } else if ($c === "Z") {
            if ($dp < $dlen && ($datetime[$dp] === "+" || $datetime[$dp] === "-")) { $dp = $dp + 1; }
            while ($dp < $dlen && ctype_digit($datetime[$dp])) { $dp = $dp + 1; }
        } else {
            while ($dp < $dlen) {
                $io = ord($datetime[$dp]);
                $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122) || $io === 95 || $io === 47 || ($io >= 48 && $io <= 57);
                if (!$a) { break; }
                $dp = $dp + 1;
            }
        }
        $isLocal = true;
        continue;
    }
    $max = 0;
    if ($c === "Y") { $max = 4; }
    else if ($c === "y") { $max = 2; }
    else if ($c === "m" || $c === "n" || $c === "d" || $c === "j" || $c === "H" || $c === "G" || $c === "h" || $c === "g" || $c === "i" || $c === "s") { $max = 2; }
    if ($max > 0) {
        $num = 0; $cnt = 0;
        while ($cnt < $max && $dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) { $errors = $errors + 1; }
        else if ($c === "Y") { $Y = $num; $pY = true; }
        else if ($c === "y") { $Y = ($num < 70) ? (2000 + $num) : (1900 + $num); $pY = true; }
        else if ($c === "m" || $c === "n") { $mo = $num; $pmo = true; }
        else if ($c === "d" || $c === "j") { $da = $num; $pda = true; }
        else if ($c === "H" || $c === "G") { $H = $num; $pH = true; }
        else if ($c === "h" || $c === "g") { $H = $num; $is12 = true; $pH = true; }
        else if ($c === "i") { $mi = $num; $pmi = true; }
        else if ($c === "s") { $se = $num; $pse = true; }
        continue;
    }
    if ($dp < $dlen && $datetime[$dp] === $c) { $dp = $dp + 1; }
    else if ($c === " ") { }
    else { $errors = $errors + 1; }
}
if ($is12 && $pm >= 0) {
    if ($pm === 1) { if ($H < 12) { $H = $H + 12; } }
    else { if ($H === 12) { $H = 0; } }
}
if ($pH || $pmi || $pse) {
    if (!$pH) { $H = 0; $pH = true; }
    if (!$pmi) { $mi = 0; $pmi = true; }
    if (!$pse) { $se = 0; $pse = true; }
}
if ($dp < $dlen) { $warnings = $warnings + 1; }
$r = ["year" => false, "month" => false, "day" => false, "hour" => false, "minute" => false, "second" => false, "fraction" => false, "warning_count" => $warnings, "warnings" => [], "error_count" => $errors, "errors" => [], "is_localtime" => $isLocal];
if ($pY) { $r["year"] = $Y; }
if ($pmo) { $r["month"] = $mo; }
if ($pda) { $r["day"] = $da; }
if ($pH) { $r["hour"] = $H; }
if ($pmi) { $r["minute"] = $mi; }
if ($pse) { $r["second"] = $se; }
if ($pus) { $r["fraction"] = $us; }
else if ($pH || $pmi || $pse) { $r["fraction"] = 0; }
if ($hasU) { $r["timestamp"] = $U; }
return $r;
"#;

/// Builds the internal static `__elephc_date_parse_from_format(string $format, string $datetime)`
/// method on `DateTime` that backs the `date_parse_from_format()` procedural function (the
/// name resolver desugars the call to this static method). Returns PHP's component array (`mixed`,
/// since values are heterogeneous int|false). Self-contained parsed-source body, like
/// `createFromFormat`.
fn datetime_date_parse_from_format() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATE_PARSE_FROM_FORMAT_SRC)
        .expect("date_parse_from_format body source must tokenize");
    let body =
        crate::parser::parse(&tokens).expect("date_parse_from_format body source must parse");
    ClassMethod {
        name: "__elephc_date_parse_from_format".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("format".to_string(), Some(TypeExpr::Str), None, false),
            ("datetime".to_string(), Some(TypeExpr::Str), None, false),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `date_parse()`. elephc does not reimplement PHP's full free-form date
/// grammar; instead it tries a list of common formats (most specific first) via
/// `date_parse_from_format` and returns the first that consumes the whole string with no
/// errors/warnings. As a fallback for relative/English strings the list does not cover (e.g.
/// `"tomorrow"`, `"next Monday"`, `"+1 day"`), it parses with `strtotime()` and decomposes the
/// resolved instant via `date()`, filling every field (PHP leaves unparsed explicit fields as
/// `false`, but a resolved relative instant has all fields). Timezone info from the string is
/// not captured in the fallback path (documented gap).
const DATE_PARSE_SRC: &str = r#"<?php
$fmts = ["Y-m-d\\TH:i:sP", "Y-m-d\\TH:i:s", "Y-m-d H:i:s.u", "Y-m-d H:i:s", "Y-m-d H:i", "Y-m-d", "Y/m/d H:i:s", "Y/m/d", "d.m.Y H:i:s", "d.m.Y", "m/d/Y H:i:s", "m/d/Y", "d-m-Y H:i:s", "d-m-Y", "d/m/Y H:i:s", "d/m/Y", "H:i:s", "H:i", "j F Y H:i:s", "j F Y", "Y M j", "M j Y"];
$n = count($fmts);
$i = 0;
while ($i < $n) {
    $r = DateTime::__elephc_date_parse_from_format($fmts[$i], $datetime);
    if ($r["error_count"] === 0 && $r["warning_count"] === 0) { return $r; }
    $i = $i + 1;
}
$ts = strtotime($datetime);
if ($ts === false) {
    return ["year" => false, "month" => false, "day" => false, "hour" => false, "minute" => false, "second" => false, "fraction" => false, "warning_count" => 0, "warnings" => [], "error_count" => 1, "errors" => [], "is_localtime" => false];
}
return [
    "year" => intval(date("Y", $ts)),
    "month" => intval(date("n", $ts)),
    "day" => intval(date("j", $ts)),
    "hour" => intval(date("G", $ts)),
    "minute" => intval(date("i", $ts)),
    "second" => intval(date("s", $ts)),
    "fraction" => false,
    "warning_count" => 0,
    "warnings" => [],
    "error_count" => 0,
    "errors" => [],
    "is_localtime" => true,
];
"#;

/// PHP source backing `gettimeofday()`. Returns PHP's `[sec, usec, minuteswest, dsttime]` array, or
/// a float (seconds + fractional) when `$as_float` is true. `usec` is derived from `microtime(true)`
/// (so sub-microsecond precision may vary); `minuteswest`/`dsttime` come from the default zone's
/// current UTC offset (`date("Z")`) and DST flag (`date("I")`). Uses `(int)` casts on the
/// `microtime()` float and `intval()` on the `date()` strings.
const GETTIMEOFDAY_SRC: &str = r#"<?php
$mt = microtime(true);
if ($as_float) {
    return $mt;
}
$sec = (int)$mt;
$usec = (int)(($mt - $sec) * 1000000.0);
$z = intval(date("Z"));
$mw = intdiv(-$z, 60);
$dst = intval(date("I"));
return ["sec" => $sec, "usec" => $usec, "minuteswest" => $mw, "dsttime" => $dst];
"#;

/// Builds the internal static `__elephc_gettimeofday($as_float = false)` method on `DateTime` backing
/// the `gettimeofday()` procedural function (the name resolver desugars the call to it). Returns the
/// component array, or a float when `$as_float` is true. Self-contained parsed source.
fn datetime_gettimeofday() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(GETTIMEOFDAY_SRC).expect("gettimeofday body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("gettimeofday body source must parse");
    ClassMethod {
        name: "__elephc_gettimeofday".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "as_float".to_string(),
            Some(TypeExpr::Bool),
            Some(Expr::new(ExprKind::BoolLiteral(false), dummy())),
            false,
        )],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `strftime()` / `gmstrftime()` (deprecated in PHP 8.1, but still in the manual).
/// Translates the strftime `%`-format into a `date()` format, then calls `date()`/`gmdate()`.
/// Common specifiers map 1:1 (or to a composite like `%T` -> `H:i:s`); `%j`/`%C` are computed and
/// inlined as literal digits (digits pass through `date()`). Literal letters are backslash-escaped so
/// `date()` keeps them literal. Locale-dependent `%c`/`%x`/`%X` reproduce PHP's default C/POSIX
/// locale byte-for-byte (elephc has no `setlocale()`, so the C locale is the only reachable behavior;
/// locale-aware output would require a separate locale system, which is out of scope here);
/// week-number `%U`/`%V`/`%W` are computed to match PHP; space-padded `%e`/`%k`/`%l` are space-padded
/// from the non-padded `date()` specifier.
const STRFTIME_SRC: &str = r#"<?php
$out = "";
$flen = strlen($format);
$k = 0;
while ($k < $flen) {
    $ch = $format[$k];
    if ($ch !== "%") {
        $cc = ord($ch);
        if (($cc >= 65 && $cc <= 90) || ($cc >= 97 && $cc <= 122)) {
            $out = $out . "\\" . $ch;
        } else {
            $out = $out . $ch;
        }
        $k = $k + 1;
        continue;
    }
    $k = $k + 1;
    if ($k >= $flen) { break; }
    $spec = $format[$k];
    $k = $k + 1;
    if ($spec === "a") { $out = $out . "D"; }
    else if ($spec === "A") { $out = $out . "l"; }
    else if ($spec === "d") { $out = $out . "d"; }
    else if ($spec === "e") {
        if ($utc) { $dd = intval(gmdate("j", $timestamp)); } else { $dd = intval(date("j", $timestamp)); }
        $ds = "" . $dd;
        if (strlen($ds) < 2) { $ds = " " . $ds; }
        $out = $out . $ds;
    }
    else if ($spec === "j") {
        if ($utc) { $z = intval(gmdate("z", $timestamp)); } else { $z = intval(date("z", $timestamp)); }
        $z = $z + 1;
        $zs = "" . $z;
        while (strlen($zs) < 3) { $zs = "0" . $zs; }
        $out = $out . $zs;
    }
    else if ($spec === "u") { $out = $out . "N"; }
    else if ($spec === "w") { $out = $out . "w"; }
    else if ($spec === "V") { $out = $out . "W"; }
    else if ($spec === "U" || $spec === "W") {
        if ($utc) { $wd = intval(gmdate("w", $timestamp)); $yd = intval(gmdate("z", $timestamp)); }
        else { $wd = intval(date("w", $timestamp)); $yd = intval(date("z", $timestamp)); }
        // %U counts weeks from the first Sunday; %W from the first Monday.
        if ($spec === "W") { if ($wd === 0) { $wd = 6; } else { $wd = $wd - 1; } }
        $wk = intdiv($yd + 7 - $wd, 7);
        $ws = "" . $wk;
        while (strlen($ws) < 2) { $ws = "0" . $ws; }
        $out = $out . $ws;
    }
    else if ($spec === "G") { $out = $out . "o"; }
    else if ($spec === "g") {
        if ($utc) { $iy = intval(gmdate("o", $timestamp)); } else { $iy = intval(date("o", $timestamp)); }
        $g2 = $iy % 100;
        $gs = "" . $g2;
        while (strlen($gs) < 2) { $gs = "0" . $gs; }
        $out = $out . $gs;
    }
    else if ($spec === "b" || $spec === "h") { $out = $out . "M"; }
    else if ($spec === "B") { $out = $out . "F"; }
    else if ($spec === "m") { $out = $out . "m"; }
    else if ($spec === "y") { $out = $out . "y"; }
    else if ($spec === "Y") { $out = $out . "Y"; }
    else if ($spec === "C") {
        if ($utc) { $yy = intval(gmdate("Y", $timestamp)); } else { $yy = intval(date("Y", $timestamp)); }
        $cen = intdiv($yy, 100);
        $cs = "" . $cen;
        while (strlen($cs) < 2) { $cs = "0" . $cs; }
        $out = $out . $cs;
    }
    else if ($spec === "H") { $out = $out . "H"; }
    else if ($spec === "k") {
        if ($utc) { $kh = intval(gmdate("G", $timestamp)); } else { $kh = intval(date("G", $timestamp)); }
        $ks = "" . $kh;
        if (strlen($ks) < 2) { $ks = " " . $ks; }
        $out = $out . $ks;
    }
    else if ($spec === "I") { $out = $out . "h"; }
    else if ($spec === "l") {
        if ($utc) { $hh = intval(gmdate("g", $timestamp)); } else { $hh = intval(date("g", $timestamp)); }
        $hs = "" . $hh;
        if (strlen($hs) < 2) { $hs = " " . $hs; }
        $out = $out . $hs;
    }
    else if ($spec === "M") { $out = $out . "i"; }
    else if ($spec === "p") { $out = $out . "A"; }
    else if ($spec === "P") { $out = $out . "a"; }
    else if ($spec === "r") { $out = $out . "h:i:s A"; }
    else if ($spec === "R") { $out = $out . "H:i"; }
    else if ($spec === "S") { $out = $out . "s"; }
    else if ($spec === "T" || $spec === "X") { $out = $out . "H:i:s"; }
    else if ($spec === "D" || $spec === "x") { $out = $out . "m/d/y"; }
    else if ($spec === "F") { $out = $out . "Y-m-d"; }
    else if ($spec === "s") { $out = $out . "U"; }
    else if ($spec === "z") { $out = $out . "O"; }
    else if ($spec === "Z") { $out = $out . "T"; }
    else if ($spec === "c") {
        if ($utc) { $cd = intval(gmdate("j", $timestamp)); } else { $cd = intval(date("j", $timestamp)); }
        $cs = "" . $cd;
        if (strlen($cs) < 2) { $cs = " " . $cs; }
        $out = $out . "D M " . $cs . " H:i:s Y";
    }
    else if ($spec === "n") { $out = $out . "\n"; }
    else if ($spec === "t") { $out = $out . "\t"; }
    else if ($spec === "%") { $out = $out . "%"; }
    else {
        $sc = ord($spec);
        if (($sc >= 65 && $sc <= 90) || ($sc >= 97 && $sc <= 122)) {
            $out = $out . "\\" . $spec;
        } else {
            $out = $out . $spec;
        }
    }
}
if ($utc) { return gmdate($out, $timestamp); }
return date($out, $timestamp);
"#;

/// PHP source for `DateTime::__elephc_extract_micros($s)` — returns the
/// microseconds (0..999999) of a trailing fractional second `HH:MM:SS.ffffff`, or
/// 0 when absent. The dot must follow `:SS` so a `DD.MM.YYYY` separator is never
/// mistaken for a fraction. `substr` (not `$s[$i]`) reads single chars to avoid a
/// computed string-index miscompile.
const EXTRACT_MICROS_SRC: &str = r#"<?php
$__dot = strrpos($s, ".");
if ($__dot !== false && $__dot >= 3 && substr($s, $__dot - 3, 1) === ":") {
    $__fd = "";
    $__k = $__dot + 1;
    $__len = strlen($s);
    while ($__k < $__len) {
        $__c = substr($s, $__k, 1);
        if ($__c >= "0" && $__c <= "9") { $__fd = $__fd . $__c; $__k = $__k + 1; }
        else { break; }
    }
    if ($__fd !== "") {
        while (strlen($__fd) < 6) { $__fd = $__fd . "0"; }
        return intval(substr($__fd, 0, 6));
    }
}
return 0;
"#;

/// PHP source for `DateTime::__elephc_strip_micros($s)` — returns the string with a
/// trailing fractional second removed, so `strtotime()` can parse the remainder. Always
/// returns a freshly allocated string (never the borrowed argument) so the constructor's
/// `$datetime = __elephc_strip_micros($datetime)` self-reassignment cannot free-then-reuse
/// an owned source string.
const STRIP_MICROS_SRC: &str = r#"<?php
$__dot = strrpos($s, ".");
if ($__dot !== false && $__dot >= 3 && substr($s, $__dot - 3, 1) === ":") {
    $__k = $__dot + 1;
    $__len = strlen($s);
    while ($__k < $__len) {
        $__c = substr($s, $__k, 1);
        if ($__c >= "0" && $__c <= "9") { $__k = $__k + 1; }
        else { break; }
    }
    return substr($s, 0, $__dot) . substr($s, $__k);
}
// Return a fresh copy (concat with "") rather than `$s` itself: the constructor
// self-reassigns `$datetime = __elephc_strip_micros($datetime)`, and returning the
// borrowed argument would make that assignment release the owned source string and
// then store the same freed pointer (use-after-free) when the source is an owned
// temporary, e.g. a Mixed datetime string materialized from an untyped argument.
return $s . "";
"#;

/// Builds the internal static `DateTime::__elephc_extract_micros(string $s): int`.
fn datetime_extract_micros() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(EXTRACT_MICROS_SRC).expect("extract_micros body must tokenize");
    let body = crate::parser::parse(&tokens).expect("extract_micros body must parse");
    ClassMethod {
        name: "__elephc_extract_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("s".to_string(), Some(TypeExpr::Str), None, false)],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source for `DateTime::__elephc_extract_modify_micros($m)` — sums the
/// microsecond deltas in a modify() string (each `<±N> microsecond[s]|usec[s]`
/// clause), returning the total (which may exceed one second or be negative).
const EXTRACT_MODIFY_MICROS_SRC: &str = r#"<?php
$__toks = explode(" ", $m);
$__n = count($__toks);
$__sum = 0;
$__i = 0;
while ($__i < $__n) {
    $__t = strtolower($__toks[$__i]);
    if ($__t === "microsecond" || $__t === "microseconds" || $__t === "usec" || $__t === "usecs") {
        if ($__i > 0) { $__sum = $__sum + intval($__toks[$__i - 1]); }
    }
    $__i = $__i + 1;
}
return $__sum;
"#;

/// PHP source for `DateTime::__elephc_strip_modify_micros($m)` — returns the
/// modify() string with every `<±N> microsecond[s]|usec[s]` clause removed, so the
/// remainder can be parsed by strtotime().
const STRIP_MODIFY_MICROS_SRC: &str = r#"<?php
$__toks = explode(" ", $m);
$__n = count($__toks);
$__out = "";
$__i = 0;
while ($__i < $__n) {
    $__unit = 0;
    if ($__i + 1 < $__n) {
        $__nt = strtolower($__toks[$__i + 1]);
        if ($__nt === "microsecond" || $__nt === "microseconds" || $__nt === "usec" || $__nt === "usecs") {
            $__unit = 1;
        }
    }
    if ($__unit === 1) {
        $__i = $__i + 2;
    } else {
        if ($__out !== "") { $__out = $__out . " "; }
        $__out = $__out . $__toks[$__i];
        $__i = $__i + 1;
    }
}
return $__out;
"#;

/// Builds the internal static `DateTime::__elephc_extract_modify_micros(string $m): int`.
fn datetime_extract_modify_micros() -> ClassMethod {
    let tokens = crate::lexer::tokenize(EXTRACT_MODIFY_MICROS_SRC)
        .expect("extract_modify_micros body must tokenize");
    let body = crate::parser::parse(&tokens).expect("extract_modify_micros body must parse");
    ClassMethod {
        name: "__elephc_extract_modify_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("m".to_string(), Some(TypeExpr::Str), None, false)],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_strip_modify_micros(string $m): string`.
fn datetime_strip_modify_micros() -> ClassMethod {
    let tokens = crate::lexer::tokenize(STRIP_MODIFY_MICROS_SRC)
        .expect("strip_modify_micros body must tokenize");
    let body = crate::parser::parse(&tokens).expect("strip_modify_micros body must parse");
    ClassMethod {
        name: "__elephc_strip_modify_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("m".to_string(), Some(TypeExpr::Str), None, false)],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_strip_micros(string $s): string`.
fn datetime_strip_micros() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(STRIP_MICROS_SRC).expect("strip_micros body must tokenize");
    let body = crate::parser::parse(&tokens).expect("strip_micros body must parse");
    ClassMethod {
        name: "__elephc_strip_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("s".to_string(), Some(TypeExpr::Str), None, false)],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_strftime($format, $timestamp, $utc)` method
/// backing the `strftime()`/`gmstrftime()` procedural functions (the name resolver desugars the
/// calls to it, injecting `time()` for the default timestamp and the local/UTC flag). Self-contained
/// parsed source.
fn datetime_strftime() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(STRFTIME_SRC).expect("strftime body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("strftime body source must parse");
    ClassMethod {
        name: "__elephc_strftime".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("format".to_string(), Some(TypeExpr::Str), None, false),
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            ("utc".to_string(), Some(TypeExpr::Bool), None, false),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Synthetic-PHP body of the shared solar "rise/set" core, a faithful port of timelib's
/// `astro.c` (Paul Schlyter's algorithm). Given the UTC-midnight timestamp of a day, an observer
/// longitude/latitude, a target altitude (degrees), and an upper-limb flag, it returns the
/// diurnal-arc result as an associative array `["rc"=>int, "hr"=>float, "hs"=>float, "ts"=>float]`:
/// `rc` is 0 (sun crosses the altitude), +1 (always above), or -1 (always below); `hr`/`hs` are the
/// rise/set hours UT (valid only when `rc==0`); `ts` is the south-transit hour UT. All angles are in
/// degrees, matching the original; `M_PI` provides the exact conversion factor PHP's C code uses.
const SUN_RS_SRC: &str = r#"<?php
$j2000 = $t_utc_sse / 86400.0 + 2440587.5 - 2451545.0;
$d = $j2000 + 2 - $lon / 360.0;
$gmst0 = (180.0 + 356.0470 + 282.9404) + (0.9856002585 + 4.70935e-5) * $d;
$gmst0 = $gmst0 - 360.0 * floor($gmst0 / 360.0);
$M = 356.0470 + 0.9856002585 * $d;
$M = $M - 360.0 * floor($M / 360.0);
$w = 282.9404 + 4.70935e-5 * $d;
$e = 0.016709 - 1.151e-9 * $d;
$E = $M + $e * (180.0 / M_PI) * sin($M * M_PI / 180.0) * (1.0 + $e * cos($M * M_PI / 180.0));
$x = cos($E * M_PI / 180.0) - $e;
$y = sqrt(1.0 - $e * $e) * sin($E * M_PI / 180.0);
$sr = sqrt($x * $x + $y * $y);
$v = (180.0 / M_PI) * atan2($y, $x);
$slon = $v + $w;
if ($slon >= 360.0) { $slon = $slon - 360.0; }
$xx = $sr * cos($slon * M_PI / 180.0);
$yy = $sr * sin($slon * M_PI / 180.0);
$obl = 23.4393 - 3.563e-7 * $d;
$z = $yy * sin($obl * M_PI / 180.0);
$yy = $yy * cos($obl * M_PI / 180.0);
$sRA = (180.0 / M_PI) * atan2($yy, $xx);
$sdec = (180.0 / M_PI) * atan2($z, sqrt($xx * $xx + $yy * $yy));
$sidtime = $gmst0 + 180.0 + $lon;
$sidtime = $sidtime - 360.0 * floor($sidtime / 360.0);
$diff = $sidtime - $sRA;
$diff = $diff - 360.0 * floor($diff / 360.0 + 0.5);
$tsouth = 12.0 - $diff / 15.0;
$sradius = 0.2666 / $sr;
if ($limb != 0) { $altit = $altit - $sradius; }
$cost = (sin($altit * M_PI / 180.0) - sin($lat * M_PI / 180.0) * sin($sdec * M_PI / 180.0)) / (cos($lat * M_PI / 180.0) * cos($sdec * M_PI / 180.0));
$rc = 0;
$hr = 0.0;
$hs = 0.0;
if ($cost >= 1.0) {
    $rc = -1;
} else if ($cost <= -1.0) {
    $rc = 1;
} else {
    $t = ((180.0 / M_PI) * acos($cost)) / 15.0;
    $hr = $tsouth - $t;
    $hs = $tsouth + $t;
}
return ["rc" => $rc, "hr" => $hr, "hs" => $hs, "ts" => $tsouth];
"#;

/// Synthetic-PHP body of the `__elephc_sun_val($rc, $tsval)` selector shared by `date_sun_info()`.
/// Maps a diurnal-arc return code to PHP's per-key value: `true` when the sun stays above the
/// altitude all day (`$rc == 1`), `false` when it stays below (`$rc == -1`), otherwise the
/// precomputed Unix timestamp `$tsval`. The `: mixed` return keeps each branch's runtime type tag
/// (`bool` vs `int`) intact when the result is boxed into the result array; computing the selection
/// inline as a ternary would unify the branches to `int` and coerce `true`/`false` to `1`/`0`.
const SUN_VAL_SRC: &str = r#"<?php
if ($rc == 1) {
    return true;
}
if ($rc == -1) {
    return false;
}
return $tsval;
"#;

/// Synthetic-PHP body of `date_sun_info($timestamp, $latitude, $longitude)`. Breaks the timestamp
/// into its UTC calendar day, runs the shared solar core at the four standard altitudes (official
/// rise/set at -35/60 deg with the upper-limb correction, then -6/-12/-18 deg for civil/nautical/
/// astronomical twilight), and assembles PHP's nine-key array. Each rise/set key is an `int` Unix
/// timestamp when the sun crosses that altitude, `true` when the sun stays above it all day, or
/// `false` when it stays below; `transit` is always the south-transit timestamp.
const SUN_INFO_SRC: &str = r#"<?php
$y = intval(gmdate("Y", $timestamp));
$mo = intval(gmdate("n", $timestamp));
$dy = intval(gmdate("j", $timestamp));
$u = __elephc_gmmktime_raw(0, 0, 0, $mo, $dy, $y);
$off = DateTime::__elephc_sun_rs($u, $longitude, $latitude, -35.0 / 60.0, 1);
$civ = DateTime::__elephc_sun_rs($u, $longitude, $latitude, -6.0, 0);
$nau = DateTime::__elephc_sun_rs($u, $longitude, $latitude, -12.0, 0);
$ast = DateTime::__elephc_sun_rs($u, $longitude, $latitude, -18.0, 0);
// Select each rise/set value through the `: mixed` helper so the true/false edge cases keep
// their bool type tag in the result array; a bare ternary here would unify to int and store
// 1/0. The timestamp argument is computed inline (arithmetic context preserves the fractional
// hour) and ignored by the helper when the sun never crosses the altitude.
$sunrise = DateTime::__elephc_sun_val($off["rc"], intval($off["hr"] * 3600 + $u));
$sunset = DateTime::__elephc_sun_val($off["rc"], intval($off["hs"] * 3600 + $u));
$transit = intval($off["ts"] * 3600 + $u);
$cb = DateTime::__elephc_sun_val($civ["rc"], intval($civ["hr"] * 3600 + $u));
$ce = DateTime::__elephc_sun_val($civ["rc"], intval($civ["hs"] * 3600 + $u));
$nb = DateTime::__elephc_sun_val($nau["rc"], intval($nau["hr"] * 3600 + $u));
$ne = DateTime::__elephc_sun_val($nau["rc"], intval($nau["hs"] * 3600 + $u));
$ab = DateTime::__elephc_sun_val($ast["rc"], intval($ast["hr"] * 3600 + $u));
$ae = DateTime::__elephc_sun_val($ast["rc"], intval($ast["hs"] * 3600 + $u));
return [
    "sunrise" => $sunrise,
    "sunset" => $sunset,
    "transit" => $transit,
    "civil_twilight_begin" => $cb,
    "civil_twilight_end" => $ce,
    "nautical_twilight_begin" => $nb,
    "nautical_twilight_end" => $ne,
    "astronomical_twilight_begin" => $ab,
    "astronomical_twilight_end" => $ae,
];
"#;

/// Synthetic-PHP body of the shared `date_sunrise()` / `date_sunset()` implementation. `$which` is 0
/// for sunrise and 1 for sunset; the return format is `SUNFUNCS_RET_TIMESTAMP` (0), `_STRING` (1),
/// or `_DOUBLE` (2). The zenith parameter (default 90°50′) becomes the altitude `90 - zenith` with
/// the upper-limb correction applied by the core. Returns `false` when the sun never reaches the
/// altitude; otherwise the Unix timestamp, an `"HH:MM"` string (with `$utcOffset` hours applied), or
/// the hour-of-day float. Negative `$latitude`/`$longitude`/`$zenith` sentinels select PHP's ini
/// defaults (latitude 31.7667, longitude 35.2333, zenith 90+50/60).
const SUNFUNC_SRC: &str = r#"<?php
$lat = ($latitude <= -999.0) ? 31.7667 : $latitude;
$lon = ($longitude <= -999.0) ? 35.2333 : $longitude;
$zen = ($zenith <= -999.0) ? (90.0 + 50.0 / 60.0) : $zenith;
$y = intval(gmdate("Y", $timestamp));
$mo = intval(gmdate("n", $timestamp));
$dy = intval(gmdate("j", $timestamp));
$u = __elephc_gmmktime_raw(0, 0, 0, $mo, $dy, $y);
$r = DateTime::__elephc_sun_rs($u, $lon, $lat, 90.0 - $zen, 1);
if ($r["rc"] != 0) {
    return false;
}
// Keep the selected rise/set hour in arithmetic context: assigning a Mixed associative-array
// element to a bare local coerces it to the array's inferred element type (int) and drops the
// fractional hour, so the timestamp/offset math reads `$r["hr"]`/`$r["hs"]` inline instead.
if ($returnFormat == 0) {
    if ($which == 0) {
        return intval($r["hr"] * 3600 + $u);
    }
    return intval($r["hs"] * 3600 + $u);
}
if ($which == 0) {
    $N = $r["hr"] + $utcOffset;
} else {
    $N = $r["hs"] + $utcOffset;
}
if ($returnFormat == 2) {
    return $N;
}
$NN = $N;
while ($NN >= 24.0) { $NN = $NN - 24.0; }
while ($NN < 0.0) { $NN = $NN + 24.0; }
$hh = intval($NN);
$mm = intval(60.0 * ($NN - $hh));
return sprintf("%02d:%02d", $hh, $mm);
"#;

/// Synthetic-PHP body of `timezone_name_from_abbr($abbr, $utcOffset, $isDST)`. Maps a common
/// timezone abbreviation to the IANA zone name PHP returns for it (the first match in PHP's internal
/// table), or `false` when the abbreviation is not recognized. The `$utcOffset`/`$isDST` arguments
/// are accepted for signature compatibility; offset/DST disambiguation is a documented gap because
/// the full abbreviation table (built on demand via `timezone_abbreviations_list()`) is not
/// released between calls and exhausts the runtime heap when built repeatedly. The abbreviation's
/// default zone is returned. The lookup is case-insensitive.
const TZ_NAME_FROM_ABBR_SRC: &str = r#"<?php
$key = strtoupper($abbr);
$map = [
    "UTC" => "UTC", "GMT" => "UTC",
    "EST" => "America/New_York", "EDT" => "America/New_York",
    "CST" => "America/Chicago", "CDT" => "America/Chicago",
    "MST" => "America/Denver", "MDT" => "America/Denver",
    "PST" => "America/Los_Angeles", "PDT" => "America/Los_Angeles",
    "AKST" => "America/Anchorage", "AKDT" => "America/Anchorage",
    "HST" => "Pacific/Honolulu", "ADT" => "America/Halifax",
    "AST" => "America/Anguilla", "NST" => "America/St_Johns", "NDT" => "America/St_Johns",
    "BDT" => "America/Adak", "NPT" => "America/St_Johns",
    "CET" => "Europe/Berlin", "CEST" => "Europe/Berlin",
    "BST" => "Europe/London", "WET" => "Europe/Paris", "WEST" => "Europe/Paris",
    "EET" => "Europe/Helsinki", "EEST" => "Europe/Helsinki",
    "MSK" => "Europe/Moscow", "MMT" => "Europe/Moscow",
    "JST" => "Asia/Tokyo", "IST" => "Asia/Jerusalem", "HKT" => "Asia/Hong_Kong",
    "KST" => "Asia/Seoul", "PKT" => "Asia/Karachi",
    "WIB" => "Asia/Jakarta", "WITA" => "Asia/Makassar", "WIT" => "Asia/Jayapura",
    "CAT" => "Africa/Khartoum", "EAT" => "Africa/Addis_Ababa",
    "WAT" => "Africa/Brazzaville", "SAST" => "Africa/Johannesburg",
    "AEST" => "Australia/Melbourne", "AEDT" => "Australia/Melbourne",
    "ACST" => "Australia/Adelaide", "ACDT" => "Australia/Adelaide",
    "AWST" => "Australia/Perth",
    "NZST" => "Pacific/Auckland", "NZDT" => "Pacific/Auckland",
    "GST" => "Pacific/Guam", "CHST" => "Pacific/Guam", "SST" => "Pacific/Samoa",
];
if (isset($map[$key])) {
    return $map[$key];
}
return false;
"#;

/// Builds the internal static `__elephc_timezone_name_from_abbr(...)` method on `DateTime` backing
/// the `timezone_name_from_abbr()` procedural function. See `TZ_NAME_FROM_ABBR_SRC`.
fn datetime_tz_name_from_abbr() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(TZ_NAME_FROM_ABBR_SRC).expect("tz_name_from_abbr must tokenize");
    let body = crate::parser::parse(&tokens).expect("tz_name_from_abbr must parse");
    ClassMethod {
        name: "__elephc_timezone_name_from_abbr".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("abbr".to_string(), Some(TypeExpr::Str), None, false),
            (
                "utcOffset".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(-1), dummy())),
                false,
            ),
            (
                "isDST".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(-1), dummy())),
                false,
            ),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Synthetic-PHP body of `strptime($timestamp, $format)`, the inverse of `strftime()`. Walks the
/// C `strftime` `%`-specifiers in `$format` against `$timestamp`, filling a `struct tm` array.
/// Supports `%Y %y %m %d %e %H %M %S %j %B %b %h %A %a %p %P`, the week specifiers `%u %w %U %W %V`
/// (consumed but not used to build the instant — `tm_wday`/`tm_yday` are derived from the date),
/// the timezone specifiers `%z` (offset) and `%Z` (name) (consumed only), the whitespace metas
/// `%n`/`%t`, `%%`, flexible spaces, and literal characters. Returns PHP's nine-key array
/// (`tm_sec`/`tm_min`/`tm_hour`/`tm_mday`/`tm_mon` (0-based)/`tm_year` (since 1900)/`tm_wday`/
/// `tm_yday`/`unparsed`) or `false` on mismatch. Unparsed date fields stay 0 and `tm_wday`/`tm_yday`
/// are computed (via `gmmktime`/`gmdate`) only when a full year+month+day was parsed, matching glibc.
const STRPTIME_SRC: &str = r#"<?php
$slen = strlen($timestamp);
$flen = strlen($format);
$sec = 0; $min = 0; $hour = 0; $mday = 0; $mon = 0; $year = 0;
$gotY = false; $gotMon = false; $gotMday = false;
$sp = 0; $fp = 0; $ok = true;
while ($fp < $flen) {
    $fc = $format[$fp];
    if ($fc === "%") {
        $fp = $fp + 1;
        if ($fp >= $flen) { $ok = false; break; }
        $spec = $format[$fp];
        $fp = $fp + 1;
        if ($spec === "%") {
            if ($sp >= $slen || $timestamp[$sp] !== "%") { $ok = false; break; }
            $sp = $sp + 1;
        } else if ($spec === "n" || $spec === "t") {
            while ($sp < $slen && ($timestamp[$sp] === " " || $timestamp[$sp] === "\t" || $timestamp[$sp] === "\n")) { $sp = $sp + 1; }
        } else if ($spec === "Y" || $spec === "y" || $spec === "m" || $spec === "d" || $spec === "e" || $spec === "H" || $spec === "M" || $spec === "S" || $spec === "j") {
            if ($spec === "e") { while ($sp < $slen && $timestamp[$sp] === " ") { $sp = $sp + 1; } }
            $num = 0; $cnt = 0;
            $maxd = ($spec === "Y") ? 4 : (($spec === "j") ? 3 : 2);
            while ($cnt < $maxd && $sp < $slen && ctype_digit($timestamp[$sp])) {
                $num = $num * 10 + (ord($timestamp[$sp]) - 48);
                $sp = $sp + 1; $cnt = $cnt + 1;
            }
            if ($cnt === 0) { $ok = false; break; }
            if ($spec === "Y") { $year = $num; $gotY = true; }
            else if ($spec === "y") { $year = ($num < 69) ? (2000 + $num) : (1900 + $num); $gotY = true; }
            else if ($spec === "m") { $mon = $num; $gotMon = true; }
            else if ($spec === "d" || $spec === "e") { $mday = $num; $gotMday = true; }
            else if ($spec === "H") { $hour = $num; }
            else if ($spec === "M") { $min = $num; }
            else if ($spec === "S") { $sec = $num; }
        } else if ($spec === "B" || $spec === "b" || $spec === "h") {
            $sub = "";
            while ($sp < $slen) {
                $io = ord($timestamp[$sp]);
                $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
                if (!$a) { break; }
                $sub = $sub . $timestamp[$sp];
                $sp = $sp + 1;
            }
            $low = strtolower($sub); $mv = 0;
            if ($low === "jan" || $low === "january") { $mv = 1; }
            else if ($low === "feb" || $low === "february") { $mv = 2; }
            else if ($low === "mar" || $low === "march") { $mv = 3; }
            else if ($low === "apr" || $low === "april") { $mv = 4; }
            else if ($low === "may") { $mv = 5; }
            else if ($low === "jun" || $low === "june") { $mv = 6; }
            else if ($low === "jul" || $low === "july") { $mv = 7; }
            else if ($low === "aug" || $low === "august") { $mv = 8; }
            else if ($low === "sep" || $low === "sept" || $low === "september") { $mv = 9; }
            else if ($low === "oct" || $low === "october") { $mv = 10; }
            else if ($low === "nov" || $low === "november") { $mv = 11; }
            else if ($low === "dec" || $low === "december") { $mv = 12; }
            if ($mv === 0) { $ok = false; break; }
            $mon = $mv; $gotMon = true;
        } else if ($spec === "A" || $spec === "a") {
            while ($sp < $slen) {
                $io = ord($timestamp[$sp]);
                $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
                if (!$a) { break; }
                $sp = $sp + 1;
            }
        } else if ($spec === "p" || $spec === "P") {
            $two = strtoupper(substr($timestamp, $sp, 2));
            if ($two === "PM") { if ($hour < 12) { $hour = $hour + 12; } $sp = $sp + 2; }
            else if ($two === "AM") { if ($hour === 12) { $hour = 0; } $sp = $sp + 2; }
            else { $ok = false; break; }
        } else if ($spec === "u" || $spec === "w" || $spec === "U" || $spec === "W" || $spec === "V") {
            $num = 0; $cnt = 0;
            $maxd = ($spec === "u" || $spec === "w") ? 1 : 2;
            while ($cnt < $maxd && $sp < $slen && ctype_digit($timestamp[$sp])) {
                $num = $num * 10 + (ord($timestamp[$sp]) - 48);
                $sp = $sp + 1; $cnt = $cnt + 1;
            }
            if ($cnt === 0) { $ok = false; break; }
        } else if ($spec === "z" || $spec === "Z") {
            if ($spec === "z") {
                if ($sp < $slen && ($timestamp[$sp] === "+" || $timestamp[$sp] === "-")) { $sp = $sp + 1; }
                $cnt = 0;
                while ($cnt < 4 && $sp < $slen && (ctype_digit($timestamp[$sp]) || $timestamp[$sp] === ":")) {
                    $sp = $sp + 1; $cnt = $cnt + 1;
                }
            } else {
                while ($sp < $slen) {
                    $io = ord($timestamp[$sp]);
                    $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
                    if (!$a) { break; }
                    $sp = $sp + 1;
                }
            }
        } else {
            $ok = false; break;
        }
    } else if ($fc === " ") {
        while ($sp < $slen && $timestamp[$sp] === " ") { $sp = $sp + 1; }
        $fp = $fp + 1;
    } else {
        if ($sp >= $slen || $timestamp[$sp] !== $fc) { $ok = false; break; }
        $sp = $sp + 1; $fp = $fp + 1;
    }
}
if (!$ok) { return false; }
$wday = 0; $yday = 0; $tmMon = 0; $tmYear = 0;
if ($gotMon) { $tmMon = $mon - 1; }
if ($gotY) { $tmYear = $year - 1900; }
if ($gotY && $gotMon && $gotMday) {
    $ts = __elephc_gmmktime_raw($hour, $min, $sec, $mon, $mday, $year);
    $wday = intval(gmdate("w", $ts));
    $yday = intval(gmdate("z", $ts));
}
return [
    "tm_sec" => $sec,
    "tm_min" => $min,
    "tm_hour" => $hour,
    "tm_mday" => $mday,
    "tm_mon" => $tmMon,
    "tm_year" => $tmYear,
    "tm_wday" => $wday,
    "tm_yday" => $yday,
    "unparsed" => substr($timestamp, $sp),
];
"#;

/// Builds the internal static `__elephc_strptime($timestamp, $format)` method on `DateTime` backing
/// the `strptime()` procedural function (the name resolver desugars the call to it). See
/// `STRPTIME_SRC` for the supported specifiers and return shape.
fn datetime_strptime() -> ClassMethod {
    let tokens = crate::lexer::tokenize(STRPTIME_SRC).expect("strptime body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("strptime body source must parse");
    ClassMethod {
        name: "__elephc_strptime".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("timestamp".to_string(), Some(TypeExpr::Str), None, false),
            ("format".to_string(), Some(TypeExpr::Str), None, false),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_sun_rs(...)` core shared by `date_sun_info()`,
/// `date_sunrise()`, and `date_sunset()`. See `SUN_RS_SRC` for the algorithm and return shape.
fn datetime_sun_rs() -> ClassMethod {
    let tokens = crate::lexer::tokenize(SUN_RS_SRC).expect("sun_rs body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("sun_rs body source must parse");
    ClassMethod {
        name: "__elephc_sun_rs".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("t_utc_sse".to_string(), Some(TypeExpr::Int), None, false),
            ("lon".to_string(), Some(TypeExpr::Float), None, false),
            ("lat".to_string(), Some(TypeExpr::Float), None, false),
            ("altit".to_string(), Some(TypeExpr::Float), None, false),
            ("limb".to_string(), Some(TypeExpr::Int), None, false),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_sun_val($rc, $tsval)` selector shared by `date_sun_info()`.
/// Returns `bool` for the polar all-day/all-night edge cases and the precomputed `int` timestamp
/// otherwise; the `mixed` return type preserves each branch's runtime tag. See `SUN_VAL_SRC`.
fn datetime_sun_val() -> ClassMethod {
    let tokens = crate::lexer::tokenize(SUN_VAL_SRC).expect("sun_val body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("sun_val body source must parse");
    ClassMethod {
        name: "__elephc_sun_val".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("rc".to_string(), Some(TypeExpr::Int), None, false),
            ("tsval".to_string(), Some(TypeExpr::Int), None, false),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_date_sun_info($timestamp, $latitude, $longitude)` method on
/// `DateTime` backing the `date_sun_info()` procedural function. See `SUN_INFO_SRC`.
fn datetime_sun_info() -> ClassMethod {
    let tokens = crate::lexer::tokenize(SUN_INFO_SRC).expect("sun_info body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("sun_info body source must parse");
    ClassMethod {
        name: "__elephc_date_sun_info".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            ("latitude".to_string(), Some(TypeExpr::Float), None, false),
            ("longitude".to_string(), Some(TypeExpr::Float), None, false),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_date_sunfunc(...)` method on `DateTime` backing both
/// `date_sunrise()` (`$which == 0`) and `date_sunset()` (`$which == 1`). See `SUNFUNC_SRC`. The
/// optional latitude/longitude/zenith parameters default to a `-999` sentinel so the body can
/// substitute PHP's ini defaults; `$returnFormat` defaults to `SUNFUNCS_RET_STRING` (1).
fn datetime_sunfunc() -> ClassMethod {
    let tokens = crate::lexer::tokenize(SUNFUNC_SRC).expect("sunfunc body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("sunfunc body source must parse");
    ClassMethod {
        name: "__elephc_date_sunfunc".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("which".to_string(), Some(TypeExpr::Int), None, false),
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            (
                "returnFormat".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(1), dummy())),
                false,
            ),
            (
                "latitude".to_string(),
                Some(TypeExpr::Float),
                Some(Expr::new(ExprKind::FloatLiteral(-1000.0), dummy())),
                false,
            ),
            (
                "longitude".to_string(),
                Some(TypeExpr::Float),
                Some(Expr::new(ExprKind::FloatLiteral(-1000.0), dummy())),
                false,
            ),
            (
                "zenith".to_string(),
                Some(TypeExpr::Float),
                Some(Expr::new(ExprKind::FloatLiteral(-1000.0), dummy())),
                false,
            ),
            (
                "utcOffset".to_string(),
                Some(TypeExpr::Float),
                Some(Expr::new(ExprKind::FloatLiteral(0.0), dummy())),
                false,
            ),
        ],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `__elephc_date_parse(string $datetime)` method on `DateTime` backing
/// the `date_parse()` procedural function (the name resolver desugars the call to it). Returns the
/// same component array as `date_parse_from_format`. Self-contained parsed-source body.
fn datetime_date_parse() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(DATE_PARSE_SRC).expect("date_parse body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("date_parse body source must parse");
    ClassMethod {
        name: "__elephc_date_parse".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("datetime".to_string(), Some(TypeExpr::Str), None, false)],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the `timestamp` (int) and `timezone_name` (str, default "UTC") backing properties.
fn datetime_backing_properties() -> Vec<ClassProperty> {
    vec![
        property("timestamp", TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy())),
        property(
            "timezone_name",
            TypeExpr::Str,
            Expr::new(ExprKind::StringLiteral("UTC".to_string()), dummy()),
        ),
        // Sub-second component (0..999999) preserved across operations; surfaced by getMicrosecond()
        // and the `u`/`v` format specifiers. elephc otherwise works at libc second resolution.
        property("microsecond", TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy())),
        // Per-class static (0 = last createFromFormat succeeded, 1 = it failed) backing
        // getLastErrors()/date_get_last_errors(). Its storage now emits correctly for the used
        // synthetic class (see emit_static_property_initializers' emitted-class filter).
        {
            let mut p =
                property("lastErrorCount", TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy()));
            p.is_static = true;
            p
        },
    ]
}

/// Builds an abstract (bodyless) interface method declaration.
fn abstract_method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params,
        variadic: None,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body: Vec::new(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// The `DateTimeInterface` method contract (`format`, `getTimestamp`, `getTimezone`).
fn datetime_interface_methods() -> Vec<ClassMethod> {
    vec![
        abstract_method(
            "format",
            vec![("format".to_string(), Some(TypeExpr::Str), None, false)],
            Some(TypeExpr::Str),
        ),
        abstract_method("getTimestamp", Vec::new(), Some(TypeExpr::Int)),
        // PHP 8.4 promoted getMicrosecond() onto the interface; both concrete
        // classes implement it, and diff() reads it through the interface.
        abstract_method("getMicrosecond", Vec::new(), Some(TypeExpr::Int)),
        abstract_method(
            "getTimezone",
            Vec::new(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeZone"))),
        ),
        abstract_method("getOffset", Vec::new(), Some(TypeExpr::Int)),
    ]
}

/// `DateTime`/`DateTimeImmutable::getOffset(): int` — UTC offset (seconds) of the object's own zone
/// at its stored instant, daylight-saving aware.
///
/// Like `DateTimeZone::getOffset` but reads `$this->timezone_name`/`$this->timestamp`: temporarily
/// applies the object's zone, reads the `date()` `Z` specifier, then restores the previous default.
fn datetime_get_offset() -> ClassMethod {
    let call = |name: &str, args: Vec<Expr>| {
        Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args }, dummy())
    };
    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    let expr_stmt = |e: Expr| Stmt::new(StmtKind::ExprStmt(e), dummy());
    let z_spec = Expr::new(ExprKind::StringLiteral("Z".to_string()), dummy());
    method(
        "getOffset",
        Vec::new(),
        Some(TypeExpr::Int),
        vec![
            // $__saved = date_default_timezone_get();
            Stmt::assign("__saved", call("date_default_timezone_get", Vec::new())),
            // date_default_timezone_set($this->timezone_name);
            expr_stmt(call("date_default_timezone_set", vec![this_property("timezone_name")])),
            // $__off = intval(date("Z", $this->timestamp));
            Stmt::assign(
                "__off",
                call("intval", vec![call("date", vec![z_spec, this_property("timestamp")])]),
            ),
            // date_default_timezone_set($__saved);  (restore the previous default)
            expr_stmt(call("date_default_timezone_set", vec![var("__saved")])),
            return_expr(var("__off")),
        ],
    )
}

/// `DateInterval::__construct(string $duration)` — parses an ISO 8601 duration into components.
///
/// Scans `P[nY][nM][nW][nD][T[nH][nM][nS]]`, accumulating each number and assigning it to the
/// matching component on the unit letter; `M` before `T` is months, after `T` is minutes; `W`
/// contributes 7 days each. The leading `P` is required (a missing/lowercase `P` throws); the
/// `T` time separator is consumed as a no-op and unknown letters throw.
fn date_interval_constructor() -> ClassMethod {
    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    let int = |n: i64| Expr::new(ExprKind::IntLiteral(n), dummy());
    let strlit = |s: &str| Expr::new(ExprKind::StringLiteral(s.to_string()), dummy());
    let binop = |l: Expr, op: BinOp, r: Expr| {
        Expr::new(ExprKind::BinaryOp { left: Box::new(l), op, right: Box::new(r) }, dummy())
    };
    let call = |name: &str, args: Vec<Expr>| {
        Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args }, dummy())
    };
    // $p = $p + 1;
    let p_inc = || Stmt::assign("p", binop(var("p"), BinOp::Add, int(1)));
    // $num = 0;
    let reset_num = || Stmt::assign("num", int(0));
    // $c === "<letter>"
    let is_c = |ch: &str| binop(var("c"), BinOp::StrictEq, strlit(ch));

    // if ($o >= 48 && $o <= 57) { $num = $num * 10 + ($o - 48); $p = $p + 1; continue; }
    let digit_if = Stmt::new(
        StmtKind::If {
            condition: binop(
                binop(var("o"), BinOp::GtEq, int(48)),
                BinOp::And,
                binop(var("o"), BinOp::LtEq, int(57)),
            ),
            then_body: vec![
                Stmt::assign(
                    "num",
                    binop(
                        binop(var("num"), BinOp::Mul, int(10)),
                        BinOp::Add,
                        binop(var("o"), BinOp::Sub, int(48)),
                    ),
                ),
                p_inc(),
                Stmt::new(StmtKind::Continue(1), dummy()),
            ],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy(),
    );


    let inc_units = || Stmt::assign("units", binop(var("units"), BinOp::Add, int(1)));
    let throw_malformed_interval = || {
        Stmt::new(
            StmtKind::Throw(Expr::new(
                ExprKind::NewObject {
                    class_name: Name::unqualified("DateMalformedIntervalStringException"),
                    args: vec![strlit("Unknown or bad format")],
                },
                dummy(),
            )),
            dummy(),
        )
    };

    // M dispatch: minutes after T, months before; counts as a recognized unit.
    let m_branch = vec![
        Stmt::new(
            StmtKind::If {
                condition: binop(var("inTime"), BinOp::StrictEq, int(1)),
                then_body: vec![assign_this_property("i", var("num")), inc_units()],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![assign_this_property("m", var("num")), inc_units()]),
            },
            dummy(),
        ),
        reset_num(),
    ];

    // if ($c === "T") {...} elseif ... unit letters ... elseif "P" (leading, no-op) else throw
    let unit_if = Stmt::new(
        StmtKind::If {
            condition: is_c("T"),
            then_body: vec![Stmt::assign("inTime", int(1))],
            elseif_clauses: vec![
                (is_c("Y"), vec![assign_this_property("y", var("num")), inc_units(), reset_num()]),
                (
                    is_c("W"),
                    vec![
                        assign_this_property(
                            "d",
                            binop(this_property("d"), BinOp::Add, binop(var("num"), BinOp::Mul, int(7))),
                        ),
                        inc_units(),
                        reset_num(),
                    ],
                ),
                (
                    is_c("D"),
                    vec![
                        assign_this_property("d", binop(this_property("d"), BinOp::Add, var("num"))),
                        inc_units(),
                        reset_num(),
                    ],
                ),
                (is_c("H"), vec![assign_this_property("h", var("num")), inc_units(), reset_num()]),
                (is_c("S"), vec![assign_this_property("s", var("num")), inc_units(), reset_num()]),
                (is_c("M"), m_branch),
                (is_c("P"), vec![]),
            ],
            else_body: Some(vec![throw_malformed_interval()]),
        },
        dummy(),
    );

    let while_body = vec![
        Stmt::assign(
            "c",
            Expr::new(
                ExprKind::ArrayAccess { array: Box::new(var("duration")), index: Box::new(var("p")) },
                dummy(),
            ),
        ),
        Stmt::assign("o", call("ord", vec![var("c")])),
        digit_if,
        unit_if,
        p_inc(),
    ];

    let body = vec![
        Stmt::assign("len", call("strlen", vec![var("duration")])),
        // PHP requires the duration to start with a literal `P`; anything else
        // (e.g. "1Y", "p1y", "") is a DateMalformedIntervalStringException.
        Stmt::new(
            StmtKind::If {
                condition: binop(
                    call("substr", vec![var("duration"), int(0), int(1)]),
                    BinOp::StrictNotEq,
                    strlit("P"),
                ),
                then_body: vec![throw_malformed_interval()],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            dummy(),
        ),
        Stmt::assign("num", int(0)),
        Stmt::assign("inTime", int(0)),
        Stmt::assign("units", int(0)),
        Stmt::assign("p", int(0)),
        Stmt::new(
            StmtKind::While { condition: binop(var("p"), BinOp::Lt, var("len")), body: while_body },
            dummy(),
        ),
        Stmt::new(
            StmtKind::If {
                condition: binop(var("units"), BinOp::StrictEq, int(0)),
                then_body: vec![throw_malformed_interval()],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            dummy(),
        ),
    ];

    method(
        "__construct",
        vec![("duration".to_string(), Some(TypeExpr::Str), None, false)],
        None,
        body,
    )
}

/// Builds a `DateInterval` component property. The numeric components
/// (`y`/`m`/`d`/`h`/`i`/`s`/`invert`) are `int` defaulting to `0`. `days` is special: PHP exposes it
/// as `int|false`, holding an absolute whole-day count only for intervals produced by
/// `DateTime::diff()` and the boolean `false` for intervals constructed directly (which
/// `format("%a")` renders as `(unknown)`). The boxed `false` default relies on the EIR object_new
/// scalar-into-Mixed default support.
fn interval_property(name: &str) -> ClassProperty {
    if name == "days" {
        return property(
            "days",
            TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::Bool]),
            Expr::new(ExprKind::BoolLiteral(false), dummy()),
        );
    }
    property(name, TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy()))
}

/// PHP source backing `DateInterval::createFromDateString()`. Parses a relative date
/// string ("1 day", "2 weeks 3 days", "1 year 2 months") into a `DateInterval` by walking
/// space-separated `<count> <unit>` pairs. Counts are stored verbatim (no normalization, so
/// "90 seconds" yields `s = 90`) and signs go into the component ("-1 day" yields `d = -1`,
/// `invert = 0`), matching PHP. Weeks fold into days (×7), fortnights ×14, and the keywords
/// `tomorrow`/`yesterday` map to ±1 day. `is_numeric()` does not accept a leading `+` here,
/// so a `+`-prefixed count is detected explicitly; `(int)` then parses the signed value.
const CREATE_FROM_DATE_STRING_SRC: &str = r#"<?php
$iv = new DateInterval("PT0S");
$s = strtolower(trim($datetime));
if ($s === "tomorrow") { $iv->d = 1; return $iv; }
if ($s === "yesterday") { $iv->d = -1; return $iv; }
if ($s === "today" || $s === "midnight" || $s === "now") { return $iv; }
$parts = explode(" ", $s);
$num = 0;
$haveNum = false;
foreach ($parts as $p) {
    if ($p === "") { continue; }
    if (is_numeric($p) || $p[0] === "+") { $num = (int)$p; $haveNum = true; continue; }
    $n = $haveNum ? $num : 1;
    $ok = false;
    if ($p === "sec" || $p === "secs" || $p === "second" || $p === "seconds") { $iv->s = $iv->s + $n; $ok = true; }
    elseif ($p === "min" || $p === "mins" || $p === "minute" || $p === "minutes") { $iv->i = $iv->i + $n; $ok = true; }
    elseif ($p === "hour" || $p === "hours") { $iv->h = $iv->h + $n; $ok = true; }
    elseif ($p === "day" || $p === "days") { $iv->d = $iv->d + $n; $ok = true; }
    elseif ($p === "week" || $p === "weeks") { $iv->d = $iv->d + $n * 7; $ok = true; }
    elseif ($p === "fortnight" || $p === "fortnights") { $iv->d = $iv->d + $n * 14; $ok = true; }
    elseif ($p === "month" || $p === "months") { $iv->m = $iv->m + $n; $ok = true; }
    elseif ($p === "year" || $p === "years") { $iv->y = $iv->y + $n; $ok = true; }
    if (!$ok) {
        throw new DateMalformedIntervalStringException("Unknown or bad format (" . $datetime . ")");
    }
    $haveNum = false;
    $num = 0;
}
return $iv;
"#;

/// `DateInterval::createFromDateString(string $datetime): DateInterval` — builds an interval
/// from a relative date string. Static method; the body is the parsed
/// `CREATE_FROM_DATE_STRING_SRC` parser, so it is self-contained and emitted with the class.
/// Unknown words are ignored (PHP throws on malformed input); the ISO 8601 duration form is
/// handled by the constructor instead.
fn date_interval_create_from_date_string() -> ClassMethod {
    let tokens = crate::lexer::tokenize(CREATE_FROM_DATE_STRING_SRC)
        .expect("createFromDateString body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("createFromDateString body source must parse");
    ClassMethod {
        name: "createFromDateString".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("datetime".to_string(), Some(TypeExpr::Str), None, false)],
        variadic: None,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// `DateInterval::format(string $format): string` — render the interval using PHP's `%` specifiers.
///
/// Scans `$format`; `%` introduces a specifier and every other character is copied literally.
/// Supports `%y/%Y %m/%M %d/%D %h/%H %i/%I %s/%S` (lowercase = no padding, uppercase = at least two
/// digits, zero-padded), `%a` (total days, or `(unknown)` for intervals not produced by `diff()`),
/// `%R` (`-`/`+`), `%r` (`-`/empty), and `%%`. An unrecognized specifier is copied verbatim.
fn date_interval_format() -> ClassMethod {
    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    let int = |n: i64| Expr::new(ExprKind::IntLiteral(n), dummy());
    let strlit = |s: &str| Expr::new(ExprKind::StringLiteral(s.to_string()), dummy());
    let binop = |l: Expr, op: BinOp, r: Expr| {
        Expr::new(ExprKind::BinaryOp { left: Box::new(l), op, right: Box::new(r) }, dummy())
    };
    // $r = $r . <e>;
    let cat = |e: Expr| Stmt::assign("r", binop(var("r"), BinOp::Concat, e));
    // $p = $p + 1;
    let p_inc = || Stmt::assign("p", binop(var("p"), BinOp::Add, int(1)));
    // $spec === "<ch>"
    let spec_is = |ch: &str| binop(var("spec"), BinOp::StrictEq, strlit(ch));
    // append $this-><prop> with no padding.
    let nopad = |prop: &str| vec![cat(this_property(prop))];
    // append $this-><prop> zero-padded to at least two digits.
    let padded = |prop: &str| {
        vec![
            Stmt::new(
                StmtKind::If {
                    condition: binop(this_property(prop), BinOp::Lt, int(10)),
                    then_body: vec![cat(strlit("0"))],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy(),
            ),
            cat(this_property(prop)),
        ]
    };
    // $format[$p]
    let fmt_at = |idx: Expr| {
        Expr::new(
            ExprKind::ArrayAccess { array: Box::new(var("format")), index: Box::new(idx) },
            dummy(),
        )
    };
    // intval($this->f * 1000000) — whole microseconds from the fractional-second float.
    let micros = || {
        Expr::new(
            ExprKind::FunctionCall {
                name: Name::unqualified("intval"),
                args: vec![binop(this_property("f"), BinOp::Mul, int(1_000_000))],
            },
            dummy(),
        )
    };

    // The %-specifier dispatch executed once $spec has been read.
    let dispatch = Stmt::new(
        StmtKind::If {
            condition: spec_is("%"),
            then_body: vec![cat(strlit("%"))],
            elseif_clauses: vec![
                (spec_is("y"), nopad("y")),
                (spec_is("Y"), padded("y")),
                (spec_is("m"), nopad("m")),
                (spec_is("M"), padded("m")),
                (spec_is("d"), nopad("d")),
                (spec_is("D"), padded("d")),
                (spec_is("h"), nopad("h")),
                (spec_is("H"), padded("h")),
                (spec_is("i"), nopad("i")),
                (spec_is("I"), padded("i")),
                (spec_is("s"), nopad("s")),
                (spec_is("S"), padded("s")),
                // %f: whole microseconds from $this->f, no padding.
                (spec_is("f"), vec![Stmt::assign("us", micros()), cat(var("us"))]),
                // %F: whole microseconds zero-padded to six digits.
                (
                    spec_is("F"),
                    {
                        let mut stmts = vec![Stmt::assign("us", micros())];
                        // One leading zero per power of ten the value falls short of 6 digits.
                        for threshold in [100_000, 10_000, 1_000, 100, 10] {
                            stmts.push(Stmt::new(
                                StmtKind::If {
                                    condition: binop(var("us"), BinOp::Lt, int(threshold)),
                                    then_body: vec![cat(strlit("0"))],
                                    elseif_clauses: Vec::new(),
                                    else_body: None,
                                },
                                dummy(),
                            ));
                        }
                        stmts.push(cat(var("us")));
                        stmts
                    },
                ),
                // %a: total days, or "(unknown)" when `days === false` (interval not from diff()).
                (
                    spec_is("a"),
                    vec![Stmt::new(
                        StmtKind::If {
                            condition: binop(
                                this_property("days"),
                                BinOp::StrictEq,
                                Expr::new(ExprKind::BoolLiteral(false), dummy()),
                            ),
                            then_body: vec![cat(strlit("(unknown)"))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![cat(this_property("days"))]),
                        },
                        dummy(),
                    )],
                ),
                // %R: "-" when inverted, otherwise "+".
                (
                    spec_is("R"),
                    vec![Stmt::new(
                        StmtKind::If {
                            condition: binop(this_property("invert"), BinOp::StrictEq, int(1)),
                            then_body: vec![cat(strlit("-"))],
                            elseif_clauses: Vec::new(),
                            else_body: Some(vec![cat(strlit("+"))]),
                        },
                        dummy(),
                    )],
                ),
                // %r: "-" when inverted, otherwise nothing.
                (
                    spec_is("r"),
                    vec![Stmt::new(
                        StmtKind::If {
                            condition: binop(this_property("invert"), BinOp::StrictEq, int(1)),
                            then_body: vec![cat(strlit("-"))],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy(),
                    )],
                ),
            ],
            // Unknown specifier: copy the "%" and the following character verbatim.
            else_body: Some(vec![cat(strlit("%")), cat(var("spec"))]),
        },
        dummy(),
    );

    let while_body = vec![
        Stmt::assign("c", fmt_at(var("p"))),
        Stmt::new(
            StmtKind::If {
                condition: binop(var("c"), BinOp::StrictEq, strlit("%")),
                then_body: vec![
                    p_inc(),
                    Stmt::new(
                        StmtKind::If {
                            condition: binop(var("p"), BinOp::Lt, var("len")),
                            then_body: vec![Stmt::assign("spec", fmt_at(var("p"))), dispatch, p_inc()],
                            elseif_clauses: Vec::new(),
                            else_body: None,
                        },
                        dummy(),
                    ),
                ],
                elseif_clauses: Vec::new(),
                else_body: Some(vec![cat(var("c")), p_inc()]),
            },
            dummy(),
        ),
    ];

    method(
        "format",
        vec![("format".to_string(), Some(TypeExpr::Str), None, false)],
        Some(TypeExpr::Str),
        vec![
            Stmt::assign("len", Expr::new(
                ExprKind::FunctionCall { name: Name::unqualified("strlen"), args: vec![var("format")] },
                dummy(),
            )),
            Stmt::assign("p", int(0)),
            Stmt::assign("r", strlit("")),
            Stmt::new(
                StmtKind::While {
                    condition: binop(var("p"), BinOp::Lt, var("len")),
                    body: while_body,
                },
                dummy(),
            ),
            return_expr(var("r")),
        ],
    )
}

/// `DateTimeInterface::diff(DateTimeInterface $target): DateInterval` — exact elapsed difference.
///
/// Populates a fresh `DateInterval` with the total `days` and the `h`/`i`/`s` remainder computed
/// from the timestamp difference, plus `invert` (1 when `$target` precedes `$this`), and the
/// calendar `y`/`m`/`d` breakdown counted by advancing whole years/months/days through `mktime()`.
/// `days` is the exact whole-day count.
fn datetime_diff_method() -> ClassMethod {
    let target_ts = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(Expr::new(ExprKind::Variable("targetObject".to_string()), dummy())),
            method: "getTimestamp".to_string(),
            args: Vec::new(),
        },
        dummy(),
    );
    // $target->getMicrosecond() — read the target's sub-second component (PHP 8.4
    // promoted it onto DateTimeInterface).
    let target_micro = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(Expr::new(ExprKind::Variable("targetObject".to_string()), dummy())),
            method: "getMicrosecond".to_string(),
            args: Vec::new(),
        },
        dummy(),
    );
    let secs_var = || Expr::new(ExprKind::Variable("secs".to_string()), dummy());
    let rem_var = || Expr::new(ExprKind::Variable("rem".to_string()), dummy());
    let iv_var = || Expr::new(ExprKind::Variable("iv".to_string()), dummy());
    let int_lit = |n: i64| Expr::new(ExprKind::IntLiteral(n), dummy());
    let binop = |l: Expr, op: BinOp, r: Expr| {
        Expr::new(ExprKind::BinaryOp { left: Box::new(l), op, right: Box::new(r) }, dummy())
    };
    // Integer division via the PHP intdiv() builtin. (It now unboxes Mixed/Union operands, so it is
    // safe here even though $secs/$rem are Mixed locals derived from an interface method call.)
    let intdiv = |a: Expr, b: Expr| {
        Expr::new(
            ExprKind::FunctionCall { name: Name::unqualified("intdiv"), args: vec![a, b] },
            dummy(),
        )
    };
    let set_iv = |prop: &str, value: Expr| {
        Stmt::new(
            StmtKind::PropertyAssign {
                object: Box::new(iv_var()),
                property: prop.to_string(),
                value,
            },
            dummy(),
        )
    };
    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    // (int)date(fmt, $ts_var): decompose a timestamp local into one calendar component.
    let date_of = |fmt: &str, ts: &str| {
        Expr::new(
            ExprKind::Cast {
                target: crate::parser::ast::CastType::Int,
                expr: Box::new(Expr::new(
                    ExprKind::FunctionCall {
                        name: Name::unqualified("date"),
                        args: vec![
                            Expr::new(ExprKind::StringLiteral(fmt.to_string()), dummy()),
                            Expr::new(ExprKind::Variable(ts.to_string()), dummy()),
                        ],
                    },
                    dummy(),
                )),
            },
            dummy(),
        )
    };
    let mktime6 = |h: Expr, mi: Expr, s: Expr, mo: Expr, d: Expr, y: Expr| {
        Expr::new(
            ExprKind::FunctionCall { name: Name::unqualified("__elephc_mktime_raw"), args: vec![h, mi, s, mo, d, y] },
            dummy(),
        )
    };
    // while (<candidate> <= $later) { $ctr = $ctr + 1; }: count whole calendar units.
    let advance_while = |ctr: &str, candidate: Expr| {
        Stmt::new(
            StmtKind::While {
                condition: binop(candidate, BinOp::LtEq, var("later")),
                body: vec![Stmt::assign(ctr, binop(var(ctr), BinOp::Add, int_lit(1)))],
            },
            dummy(),
        )
    };
    method(
        "diff",
        vec![
            (
                "targetObject".to_string(),
                Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
                None,
                false,
            ),
            (
                "absolute".to_string(),
                Some(TypeExpr::Bool),
                Some(Expr::new(ExprKind::BoolLiteral(false), dummy())),
                false,
            ),
        ],
        Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        vec![
            // Cache $this->timestamp BEFORE the method call: evaluating $target->getTimestamp()
            // first would otherwise clobber the $this receiver before the property read.
            Stmt::assign("base", this_property("timestamp")),
            // Read $this->microsecond before the target method calls clobber the receiver.
            Stmt::assign("mus", this_property("microsecond")),
            // $tts = $target->getTimestamp();
            Stmt::assign("tts", target_ts),
            // $mut = $target->getMicrosecond();
            Stmt::assign("mut", target_micro),
            // $secs = $tts - $base;
            Stmt::assign("secs", binop(var("tts"), BinOp::Sub, var("base"))),
            // $iv = new DateInterval("P0D");
            Stmt::assign(
                "iv",
                Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified("DateInterval"),
                        args: vec![Expr::new(ExprKind::StringLiteral("P0D".to_string()), dummy())],
                    },
                    dummy(),
                ),
            ),
            // Order by the full instant (seconds, then microseconds): invert when $target is
            // earlier — including the same-second case where its microseconds are smaller.
            // earlier/later carry the second component; mearlier/mlater the microseconds.
            Stmt::new(
                StmtKind::If {
                    condition: binop(
                        binop(secs_var(), BinOp::Lt, int_lit(0)),
                        BinOp::Or,
                        binop(
                            binop(secs_var(), BinOp::Eq, int_lit(0)),
                            BinOp::And,
                            binop(var("mut"), BinOp::Lt, var("mus")),
                        ),
                    ),
                    then_body: vec![
                        set_iv("invert", int_lit(1)),
                        Stmt::assign("secs", binop(int_lit(0), BinOp::Sub, secs_var())),
                        Stmt::assign("earlier", var("tts")),
                        Stmt::assign("mearlier", var("mut")),
                        Stmt::assign("later", var("base")),
                        Stmt::assign("mlater", var("mus")),
                    ],
                    elseif_clauses: Vec::new(),
                    else_body: Some(vec![
                        Stmt::assign("earlier", var("base")),
                        Stmt::assign("mearlier", var("mus")),
                        Stmt::assign("later", var("tts")),
                        Stmt::assign("mlater", var("mut")),
                    ]),
                },
                dummy(),
            ),
            // Fractional-second difference with a one-second borrow: when the later
            // microseconds are smaller, borrow a whole second into the fraction. This keeps
            // $secs and $later consistent for the breakdown and calendar walk below.
            Stmt::assign("frac", binop(var("mlater"), BinOp::Sub, var("mearlier"))),
            Stmt::new(
                StmtKind::If {
                    condition: binop(var("frac"), BinOp::Lt, int_lit(0)),
                    then_body: vec![
                        Stmt::assign("frac", binop(var("frac"), BinOp::Add, int_lit(1_000_000))),
                        Stmt::assign("later", binop(var("later"), BinOp::Sub, int_lit(1))),
                        Stmt::assign("secs", binop(secs_var(), BinOp::Sub, int_lit(1))),
                    ],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy(),
            ),
            // $iv->f = $frac / 1000000.0;
            set_iv(
                "f",
                binop(
                    var("frac"),
                    BinOp::Div,
                    Expr::new(ExprKind::FloatLiteral(1_000_000.0), dummy()),
                ),
            ),
            // $iv->days = intdiv($secs, 86400);
            set_iv("days", intdiv(secs_var(), int_lit(86400))),
            // $rem = $secs % 86400;
            Stmt::assign("rem", binop(secs_var(), BinOp::Mod, int_lit(86400))),
            // $iv->h = intdiv($rem, 3600);
            set_iv("h", intdiv(rem_var(), int_lit(3600))),
            // $iv->i = intdiv($rem % 3600, 60);
            set_iv("i", intdiv(binop(rem_var(), BinOp::Mod, int_lit(3600)), int_lit(60))),
            // $iv->s = $rem % 60;
            set_iv("s", binop(rem_var(), BinOp::Mod, int_lit(60))),
            // -- calendar components: decompose the earlier date, then count whole years, months,
            //    and days by advancing through mktime() (which normalizes month/day overflow)
            //    until the next unit would pass $later. Matches PHP's calendar y/m/d breakdown.
            Stmt::assign("ey", date_of("Y", "earlier")),
            Stmt::assign("emo", date_of("n", "earlier")),
            Stmt::assign("ed", date_of("j", "earlier")),
            Stmt::assign("eh", date_of("G", "earlier")),
            Stmt::assign("ei", date_of("i", "earlier")),
            Stmt::assign("es", date_of("s", "earlier")),
            // years: while mktime(eh,ei,es, emo, ed, ey + y + 1) <= later { y++ }
            Stmt::assign("y", int_lit(0)),
            advance_while(
                "y",
                mktime6(
                    var("eh"),
                    var("ei"),
                    var("es"),
                    var("emo"),
                    var("ed"),
                    binop(binop(var("ey"), BinOp::Add, var("y")), BinOp::Add, int_lit(1)),
                ),
            ),
            // months: while mktime(eh,ei,es, emo + m + 1, ed, ey + y) <= later { m++ }
            Stmt::assign("m", int_lit(0)),
            advance_while(
                "m",
                mktime6(
                    var("eh"),
                    var("ei"),
                    var("es"),
                    binop(binop(var("emo"), BinOp::Add, var("m")), BinOp::Add, int_lit(1)),
                    var("ed"),
                    binop(var("ey"), BinOp::Add, var("y")),
                ),
            ),
            // days: while mktime(eh,ei,es, emo + m, ed + d + 1, ey + y) <= later { d++ }
            Stmt::assign("d", int_lit(0)),
            advance_while(
                "d",
                mktime6(
                    var("eh"),
                    var("ei"),
                    var("es"),
                    binop(var("emo"), BinOp::Add, var("m")),
                    binop(binop(var("ed"), BinOp::Add, var("d")), BinOp::Add, int_lit(1)),
                    binop(var("ey"), BinOp::Add, var("y")),
                ),
            ),
            set_iv("y", var("y")),
            set_iv("m", var("m")),
            set_iv("d", var("d")),
            // PHP's `$absolute` flag forces a positive interval: drop the invert flag set above so
            // the returned DateInterval never reads as negative regardless of argument order.
            Stmt::new(
                StmtKind::If {
                    condition: var("absolute"),
                    then_body: vec![set_iv("invert", int_lit(0))],
                    elseif_clauses: Vec::new(),
                    else_body: None,
                },
                dummy(),
            ),
            return_expr(iv_var()),
        ],
    )
}

/// Injects the builtin `DateTimeInterface`, `DateTimeZone`, `DateTimeImmutable`, `DateTime`, and `DateInterval` declarations.
///
/// Registers synthetic class/interface metadata so user code can construct, type-hint, and call
/// methods on these classes. Existing user declarations of the same names are left untouched.
///
/// `uses_tz_introspection` gates the three `DateTimeZone` introspection methods
/// (`getLocation`/`getTransitions`/`listAbbreviations`): they delegate to the
/// `tz_prelude` helpers, which only exist when that prelude is injected, so they
/// are added only when the program uses the introspection surface — otherwise
/// every `DateTimeZone` program would reference and link the `elephc_tz` bridge.
pub(crate) fn inject_builtin_datetime(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    uses_tz_introspection: bool,
) {
    if !interface_map.contains_key("DateTimeInterface") {
        interface_map.insert(
            "DateTimeInterface".to_string(),
            InterfaceDeclInfo {
                name: "DateTimeInterface".to_string(),
                extends: Vec::new(),
                properties: Vec::new(),
                methods: datetime_interface_methods(),
                span: dummy(),
                constants: datetime_format_constants(),
            },
        );
    }

    if !class_map.contains_key("DateInterval") {
        class_map.insert(
            "DateInterval".to_string(),
            FlattenedClass {
                name: "DateInterval".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties: vec![
                    interval_property("y"),
                    interval_property("m"),
                    interval_property("d"),
                    interval_property("h"),
                    interval_property("i"),
                    interval_property("s"),
                    // `f` (fraction of a second, 0.0..1.0) exists for API completeness; elephc works
                    // at second resolution so it stays 0.0 (sub-second durations are not parsed).
                    property("f", TypeExpr::Float, Expr::new(ExprKind::FloatLiteral(0.0), dummy())),
                    interval_property("invert"),
                    interval_property("days"),
                ],
                methods: vec![
                    date_interval_constructor(),
                    date_interval_format(),
                    date_interval_create_from_date_string(),
                ],
                attributes: Vec::new(),
                constants: Vec::new(),
                used_traits: Vec::new(),
            },
        );
    }

    if !class_map.contains_key("DateTimeZone") {
        class_map.insert(
            "DateTimeZone".to_string(),
            FlattenedClass {
                name: "DateTimeZone".to_string(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties: vec![property(
                    "name",
                    TypeExpr::Str,
                    Expr::new(ExprKind::StringLiteral("UTC".to_string()), dummy()),
                )],
                methods: {
                    let mut methods = vec![
                        datetime_zone_constructor(),
                        datetime_zone_get_name(),
                        datetime_zone_get_offset(),
                        datetime_zone_list_identifiers(),
                    ];
                    // getLocation/getTransitions/listAbbreviations call the
                    // tz_prelude marshalling helpers, which are only declared when
                    // the introspection prelude is injected. Adding them
                    // unconditionally would make every DateTimeZone program
                    // reference (and link) the elephc_tz bridge, since method
                    // bodies are type-checked eagerly. So they are gated on the
                    // prelude's presence.
                    if uses_tz_introspection {
                        methods.push(datetime_zone_get_location());
                        methods.push(datetime_zone_get_transitions());
                        methods.push(datetime_zone_list_abbreviations());
                    }
                    methods
                },
                attributes: Vec::new(),
                constants: datetime_zone_group_constants(),
                used_traits: Vec::new(),
            },
        );
    }

    if !class_map.contains_key("DateTimeImmutable") {
        class_map.insert(
            "DateTimeImmutable".to_string(),
            FlattenedClass {
                name: "DateTimeImmutable".to_string(),
                extends: None,
                implements: vec!["DateTimeInterface".to_string()],
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties: datetime_backing_properties(),
                methods: {
                    let mut m = datetime_shared_methods();
                    m.extend(datetime_setter_methods(false, "DateTimeImmutable"));
                    m.push(datetime_create_from_format("DateTimeImmutable"));
                    m.push(datetime_get_last_errors("DateTimeImmutable"));
                    m.push(datetime_create_from_timestamp("DateTimeImmutable"));
                    m.push(datetime_create_from_object("createFromInterface", "DateTimeImmutable"));
                    m.push(datetime_create_from_object("createFromMutable", "DateTimeImmutable"));
                    m.push(datetime_set_isodate("DateTimeImmutable"));
                    m
                },
                attributes: Vec::new(),
                constants: datetime_format_constants(),
                used_traits: Vec::new(),
            },
        );
    }

    if !class_map.contains_key("DateTime") {
        let mut methods = datetime_shared_methods();
        methods.extend(datetime_setter_methods(true, "DateTime"));
        methods.push(datetime_create_from_format("DateTime"));
        methods.push(datetime_get_last_errors("DateTime"));
        methods.push(datetime_create_from_timestamp("DateTime"));
        methods.push(datetime_create_from_object("createFromInterface", "DateTime"));
        methods.push(datetime_create_from_object("createFromImmutable", "DateTime"));
        methods.push(datetime_set_isodate("DateTime"));
        methods.push(datetime_date_parse_from_format());
        methods.push(datetime_date_parse());
        methods.push(datetime_gettimeofday());
        methods.push(datetime_strftime());
        methods.push(datetime_extract_micros());
        methods.push(datetime_strip_micros());
        methods.push(datetime_extract_modify_micros());
        methods.push(datetime_strip_modify_micros());
        methods.push(datetime_sun_rs());
        methods.push(datetime_sun_val());
        methods.push(datetime_sun_info());
        methods.push(datetime_sunfunc());
        methods.push(datetime_strptime());
        methods.push(datetime_tz_name_from_abbr());
        methods.extend(super::calendar::calendar_methods());
        class_map.insert(
            "DateTime".to_string(),
            FlattenedClass {
                name: "DateTime".to_string(),
                extends: None,
                implements: vec!["DateTimeInterface".to_string()],
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties: datetime_backing_properties(),
                methods,
                attributes: Vec::new(),
                constants: datetime_format_constants(),
                used_traits: Vec::new(),
            },
        );
    }

    inject_builtin_date_exceptions(class_map);
}

/// Builds an empty synthetic exception/error subclass named `name` extending `parent`.
///
/// Mirrors the `RuntimeException`/`JsonException` pattern in `declarations.rs`: the Throwable
/// API (message/code properties, `getMessage()`, etc.) is inherited from the parent through the
/// standard inheritance machinery, so no members are redeclared locally.
fn date_exception_subclass(name: &str, parent: &str) -> FlattenedClass {
    FlattenedClass {
        name: name.to_string(),
        extends: Some(parent.to_string()),
        implements: Vec::new(),
        is_abstract: false,
        is_final: false,
        is_readonly_class: false,
        properties: Vec::new(),
        methods: Vec::new(),
        attributes: Vec::new(),
        constants: Vec::new(),
        used_traits: Vec::new(),
    }
}

/// Injects the PHP 8.3 date/time exception hierarchy.
///
/// `DateError` and its subclasses (`DateObjectError`, `DateRangeError`) extend `Error`; the
/// `DateException` family (`DateInvalidTimeZoneException`, `DateInvalidOperationException`, and the
/// `DateMalformed*` string/interval/period exceptions) extend `Exception`. `Error`/`Exception` are
/// already registered by `inject_builtin_throwables`, which runs before this. User declarations of
/// the same names are left untouched.
fn inject_builtin_date_exceptions(class_map: &mut HashMap<String, FlattenedClass>) {
    for (name, parent) in [
        ("DateError", "Error"),
        ("DateObjectError", "DateError"),
        ("DateRangeError", "DateError"),
        ("DateException", "Exception"),
        ("DateInvalidTimeZoneException", "DateException"),
        ("DateInvalidOperationException", "DateException"),
        ("DateMalformedStringException", "DateException"),
        ("DateMalformedIntervalStringException", "DateException"),
        ("DateMalformedPeriodStringException", "DateException"),
        ("DateUnknownException", "DateException"),
    ] {
        if !class_map.contains_key(name) {
            class_map.insert(name.to_string(), date_exception_subclass(name, parent));
        }
    }
}
