//! Purpose:
//! Synthetic `DateTimeZone` constructors, accessors, and introspection methods.
//!
//! Called from:
//! - DateTime declaration injection.
//!
//! Key details:
//! - Introspection methods are injected only when the timezone bridge is available.

use super::*;

/// `DateTimeZone::__construct(string $timezone = "UTC")` — stores the identifier verbatim.
pub(super) fn datetime_zone_constructor() -> ClassMethod {
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
pub(super) fn datetime_zone_get_name() -> ClassMethod {
    method("getName", Vec::new(), Some(TypeExpr::Str), vec![return_expr(this_property("name"))])
}

/// `DateTimeZone::getOffset(DateTimeInterface $datetime): int` — UTC offset (seconds) of this zone
/// at the given instant.
///
/// Temporarily applies this zone via `date_default_timezone_set`, reads the offset with the `date()`
/// `Z` specifier for `$datetime->getTimestamp()` (so it is daylight-saving correct), then restores
/// the previous default. Returns a positive value east of UTC, negative west.
pub(super) fn datetime_zone_get_offset() -> ClassMethod {
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
pub(super) fn datetime_zone_list_identifiers() -> ClassMethod {
    // Built straight from the identifier slice. This body used to be assembled as PHP text and
    // handed back to the tokenizer and parser — 419 string literals formatted into a `return [];`
    // only to be read back into the same array literal this builds directly.
    let body = super::bodies::list_identifiers(super::timezone_ids::TIMEZONE_IDENTIFIERS);
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
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: None,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// The PHP the introspection bodies used to be parsed from.
///
/// They return array literals directly, which is the only shape a synthetic method's inferred
/// (`None`) return type resolves element types for — a call to a prelude helper would infer as a
/// scalar. Test-only: the compilation path builds them with `bodies::tz_*`, and the oracle checks
/// each build against the PHP below.
#[cfg(test)]
pub(super) const GET_LOCATION_SRC: &str = r#"<?php
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
"#;

/// Test-only: the compilation path builds this body; the oracle checks the two agree.
#[cfg(test)]
pub(super) const GET_TRANSITIONS_SRC: &str = r#"<?php
$raw = elephc_tz_transitions($this->name);
if ($raw === "") {
    return false;
}
$lines = explode("\n", $raw);
$lineCount = count($lines);
$result = [];
$resultIndex = 0;
$activeFound = false;
$activeTs = 0;
$activeOffset = 0;
$activeDst = false;
$activeAbbr = "";
$activeTime = "";
$i = 0;
while ($i < $lineCount) {
    $g = explode("\t", $lines[$i]);
    $ts = (int) $g[0];
    if ($ts <= $timestampBegin) {
        $activeFound = true;
        $activeTs = $ts;
        $activeOffset = (int) $g[1];
        $activeDst = $g[2] === "1";
        $activeAbbr = $g[3];
        $activeTime = $g[4];
    }
    $i = intval($i + 1);
}
if ($activeFound) {
    $result[$resultIndex] = [
        "ts" => $timestampBegin <= $activeTs ? $activeTs : $timestampBegin,
        "time" => $timestampBegin <= $activeTs ? $activeTime : gmdate("Y-m-d\TH:i:sP", $timestampBegin),
        "offset" => $activeOffset,
        "isdst" => $activeDst,
        "abbr" => $activeAbbr,
    ];
    $resultIndex = intval($resultIndex + 1);
}
$i = 0;
while ($i < $lineCount) {
    $g = explode("\t", $lines[$i]);
    $ts = (int) $g[0];
    if ($ts > $timestampBegin && $ts <= $timestampEnd) {
        $result[$resultIndex] = [
            "ts" => $ts,
            "time" => $g[4],
            "offset" => (int) $g[1],
            "isdst" => $g[2] === "1",
            "abbr" => $g[3],
        ];
        $resultIndex = intval($resultIndex + 1);
    }
    $i = intval($i + 1);
}
return array_slice($result, 0, $resultIndex);
"#;

/// Test-only PHP oracle for the direct AST abbreviation-list body.
#[cfg(test)]
pub(super) const LIST_ABBREVIATIONS_SRC: &str = r#"<?php
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
"#;


/// `DateTimeZone::getLocation(): array|false` — returns the zone's country code,
/// latitude, longitude, and comments (or `false` for the few zones without a
/// location). Calls the `elephc_tz` bridge directly and marshals the tab-joined
/// result into an array literal so inference resolves the return shape. Only added
/// to `DateTimeZone` when the introspection prelude is injected.
pub(super) fn datetime_zone_get_location() -> ClassMethod {
    method(
        "getLocation",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("mixed"))),
        super::bodies::tz_get_location(),
    )
}

/// `DateTimeZone::getTransitions(int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX): array|false`
/// — returns the DST transition rows in the window. The defaults reproduce PHP's
/// full no-arg list: the synthetic first row coincides with the bridge's row 0, so
/// its precomputed `time` is reused rather than asking `gmdate` to format
/// `PHP_INT_MIN`.
pub(super) fn datetime_zone_get_transitions() -> ClassMethod {
    // PHP's defaults are PHP_INT_MIN/PHP_INT_MAX. They are materialized as integer
    // literals (a `ConstRef` default is not evaluated when the method is called
    // with no args), and `i64::MIN` is exactly the bridge's row-0 timestamp, so the
    // no-arg call reproduces the full transition list.
    let int_literal = |v: i64| Expr::new(ExprKind::IntLiteral(v), dummy());
    let body = super::bodies::tz_get_transitions();
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
        body,
    )
}

/// `DateTimeZone::listAbbreviations(): array` — returns PHP's static
/// abbreviation→offset/DST/zone table. Static method; calls the `elephc_tz` bridge
/// directly and marshals the result into the nested array literal.
pub(super) fn datetime_zone_list_abbreviations() -> ClassMethod {
    ClassMethod {
        name: "listAbbreviations".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body: super::bodies::tz_list_abbreviations(),
        span: dummy(),
        attributes: Vec::new(),
    }
}
