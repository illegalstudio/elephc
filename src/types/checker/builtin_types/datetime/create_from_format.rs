//! Purpose:
//! Parsed-PHP implementation of `DateTime::createFromFormat` and its immutable peer.
//!
//! Called from:
//! - DateTime declaration injection.
//!
//! Key details:
//! - The self-contained parser preserves PHP field resets, timezone checks, and failure typing.

use super::*;

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
#[cfg(test)]
pub(super) const CREATE_FROM_FORMAT_SRC: &str = r##"<?php
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
    date_default_timezone_set(DateTimeZone::__elephc_export_name($timezone));
    $ts = __elephc_mktime_raw($H, $mi, $se, $mo, $da, $Y);
    date_default_timezone_set($saved);
}
// TZ cross-validation: when any of O/P/Z/T/e was parsed, re-render the same specifier
// in the same zone the wall-clock was interpreted in, and compare. A mismatch (e.g.
// "+0500" against a Europe/Paris instant) is a parse failure.
if ($parsedO !== "" || $parsedP !== "" || $parsedZ !== "" || $parsedT !== "" || $parsedE !== "") {
    $__saved = date_default_timezone_get();
    if ($timezone !== null) {
        date_default_timezone_set(DateTimeZone::__elephc_export_name($timezone));
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
    $o->timezone_name = DateTimeZone::__elephc_export_name($timezone);
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
pub(super) fn datetime_create_from_format(class_name: &str) -> ClassMethod {
    let body = super::bodies::create_from_format(class_name);
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
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
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
