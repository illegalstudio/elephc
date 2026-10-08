//! Purpose:
//! Parsed-PHP sources for free-form `date_parse` and `gettimeofday`.
//!
//! Called from:
//! - DateTime procedural helper method assembly.
//!
//! Key details:
//! - Common formats are attempted before the documented relative-date fallback.

use super::*;

/// PHP source backing `date_parse()`. elephc does not reimplement PHP's full free-form date
/// grammar; instead it tries a list of common formats (most specific first) via
/// `date_parse_from_format` and returns the first that consumes the whole string with no
/// errors/warnings. As a fallback for relative/English strings the list does not cover (e.g.
/// `"tomorrow"`, `"next Monday"`, `"+1 day"`), it parses with `strtotime()` and decomposes the
/// resolved instant via `date()`, filling every field (PHP leaves unparsed explicit fields as
/// `false`, but a resolved relative instant has all fields). Timezone info from the string is
/// not captured in the fallback path (documented gap).
#[cfg(test)]
pub(super) const DATE_PARSE_SRC: &str = r#"<?php
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
#[cfg(test)]
pub(super) const GETTIMEOFDAY_SRC: &str = r#"<?php
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
pub(super) fn datetime_gettimeofday() -> ClassMethod {
    let body = super::bodies::gettimeofday();
    ClassMethod {
        type_params: Vec::new(),
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
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}
