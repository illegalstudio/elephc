//! Purpose:
//! Parsed-PHP adapter for `date_parse_from_format` component extraction.
//!
//! Called from:
//! - DateTime procedural helper method assembly.
//!
//! Key details:
//! - Heterogeneous component results retain explicit false values for absent fields.

use super::*;

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
#[cfg(test)]
pub(super) const DATE_PARSE_FROM_FORMAT_SRC: &str = r#"<?php
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
pub(super) fn datetime_date_parse_from_format() -> ClassMethod {
    let body = super::bodies::date_parse_from_format();
    ClassMethod {
        type_params: Vec::new(),
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
