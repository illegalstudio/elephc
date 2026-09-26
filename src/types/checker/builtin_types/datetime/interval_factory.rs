//! Purpose:
//! Parsed-PHP `DateInterval::createFromDateString` method construction.
//!
//! Called from:
//! - DateInterval declaration injection.
//!
//! Key details:
//! - Relative units keep PHP component and sign behavior without eager normalization.

use super::*;

/// PHP source backing `DateInterval::createFromDateString()`. Parses a relative date
/// string ("1 day", "2 weeks 3 days", "1 year 2 months") into a `DateInterval` by walking
/// space-separated `<count> <unit>` pairs. Counts are stored verbatim (no normalization, so
/// "90 seconds" yields `s = 90`) and signs go into the component ("-1 day" yields `d = -1`,
/// `invert = 0`), matching PHP. Weeks fold into days (×7), fortnights ×14, and the keywords
/// `tomorrow`/`yesterday` map to ±1 day. `is_numeric()` does not accept a leading `+` here,
/// so a `+`-prefixed count is detected explicitly; `(int)` then parses the signed value.
#[cfg(test)]
pub(super) const CREATE_FROM_DATE_STRING_SRC: &str = r#"<?php
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
pub(super) fn date_interval_create_from_date_string() -> ClassMethod {
    let body = super::bodies::create_from_date_string();
    ClassMethod {
        type_params: Vec::new(),
        name: "createFromDateString".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("datetime".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}
