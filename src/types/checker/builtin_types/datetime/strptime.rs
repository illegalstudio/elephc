//! Purpose:
//! Parsed-PHP implementation of the procedural `strptime` adapter.
//!
//! Called from:
//! - DateTime procedural helper method assembly.
//!
//! Key details:
//! - The emitted nine-key result follows supported C `strftime` specifiers and mismatch rules.

use super::*;

/// Synthetic-PHP body of `strptime($timestamp, $format)`, the inverse of `strftime()`. Walks the
/// C `strftime` `%`-specifiers in `$format` against `$timestamp`, filling a `struct tm` array.
/// Supports `%Y %y %m %d %e %H %M %S %j %B %b %h %A %a %p %P`, the week specifiers `%u %w %U %W %V`
/// (consumed but not used to build the instant — `tm_wday`/`tm_yday` are derived from the date),
/// the timezone specifiers `%z` (offset) and `%Z` (name) (consumed only), the whitespace metas
/// `%n`/`%t`, `%%`, flexible spaces, and literal characters. Returns PHP's nine-key array
/// (`tm_sec`/`tm_min`/`tm_hour`/`tm_mday`/`tm_mon` (0-based)/`tm_year` (since 1900)/`tm_wday`/
/// `tm_yday`/`unparsed`) or `false` on mismatch. Unparsed date fields stay 0 and `tm_wday`/`tm_yday`
/// are computed (via `gmmktime`/`gmdate`) only when a full year+month+day was parsed, matching glibc.
#[cfg(test)]
pub(super) const STRPTIME_SRC: &str = r#"<?php
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
pub(super) fn datetime_strptime() -> ClassMethod {
    let body = super::bodies::strptime();
    ClassMethod {
        type_params: Vec::new(),
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
