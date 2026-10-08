//! Purpose:
//! Parsed-PHP `strftime` translation and microsecond extraction/stripping helpers.
//!
//! Called from:
//! - DateTime procedural helper method assembly and constructor/modifier paths.
//!
//! Key details:
//! - Locale-independent output and sub-second preservation match the supported runtime contract.

use super::*;

/// PHP source backing `strftime()` / `gmstrftime()` (deprecated in PHP 8.1, but still in the manual).
/// Translates the strftime `%`-format into a `date()` format, then calls `date()`/`gmdate()`.
/// Common specifiers map 1:1 (or to a composite like `%T` -> `H:i:s`); `%j`/`%C` are computed and
/// inlined as literal digits (digits pass through `date()`). Literal letters are backslash-escaped so
/// `date()` keeps them literal. Locale-dependent `%c`/`%x`/`%X` reproduce PHP's default C/POSIX
/// locale byte-for-byte (elephc has no `setlocale()`, so the C locale is the only reachable behavior;
/// locale-aware output would require a separate locale system, which is out of scope here);
/// week-number `%U`/`%V`/`%W` are computed to match PHP; space-padded `%e`/`%k`/`%l` are space-padded
/// from the non-padded `date()` specifier.
#[cfg(test)]
pub(super) const STRFTIME_SRC: &str = r#"<?php
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
#[cfg(test)]
pub(super) const EXTRACT_MICROS_SRC: &str = r#"<?php
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
#[cfg(test)]
pub(super) const STRIP_MICROS_SRC: &str = r#"<?php
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
pub(super) fn datetime_extract_micros() -> ClassMethod {
    let body = super::bodies::extract_micros();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_extract_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("s".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
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
#[cfg(test)]
pub(super) const EXTRACT_MODIFY_MICROS_SRC: &str = r#"<?php
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
#[cfg(test)]
pub(super) const STRIP_MODIFY_MICROS_SRC: &str = r#"<?php
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
pub(super) fn datetime_extract_modify_micros() -> ClassMethod {
    let body = super::bodies::extract_modify_micros();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_extract_modify_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("m".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_strip_modify_micros(string $m): string`.
pub(super) fn datetime_strip_modify_micros() -> ClassMethod {
    let body = super::bodies::strip_modify_micros();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_strip_modify_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("m".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_strip_micros(string $s): string`.
pub(super) fn datetime_strip_micros() -> ClassMethod {
    let body = super::bodies::strip_micros();
    ClassMethod {
        type_params: Vec::new(),
        name: "__elephc_strip_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("s".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
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
pub(super) fn datetime_strftime() -> ClassMethod {
    let body = super::bodies::strftime();
    ClassMethod {
        type_params: Vec::new(),
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
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}
