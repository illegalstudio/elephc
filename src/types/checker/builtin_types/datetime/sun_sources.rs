//! Purpose:
//! Parsed-PHP solar calculations and timezone-abbreviation lookup sources.
//!
//! Called from:
//! - DateTime procedural helper method assembly.
//!
//! Key details:
//! - Solar calculations retain timelib-compatible degree and return-format semantics.

use super::*;

/// Synthetic-PHP body of the shared solar "rise/set" core, a faithful port of timelib's
/// `astro.c` (Paul Schlyter's algorithm). Given the UTC-midnight timestamp of a day, an observer
/// longitude/latitude, a target altitude (degrees), and an upper-limb flag, it returns the
/// diurnal-arc result as an associative array `["rc"=>int, "hr"=>float, "hs"=>float, "ts"=>float]`:
/// `rc` is 0 (sun crosses the altitude), +1 (always above), or -1 (always below); `hr`/`hs` are the
/// rise/set hours UT (valid only when `rc==0`); `ts` is the south-transit hour UT. All angles are in
/// degrees, matching the original; `M_PI` provides the exact conversion factor PHP's C code uses.
#[cfg(test)]
pub(super) const SUN_RS_SRC: &str = r#"<?php
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
#[cfg(test)]
pub(super) const SUN_VAL_SRC: &str = r#"<?php
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
#[cfg(test)]
pub(super) const SUN_INFO_SRC: &str = r#"<?php
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
#[cfg(test)]
pub(super) const SUNFUNC_SRC: &str = r#"<?php
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
#[cfg(test)]
pub(super) const TZ_NAME_FROM_ABBR_SRC: &str = r#"<?php
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
pub(super) fn datetime_tz_name_from_abbr() -> ClassMethod {
    let body = super::bodies::tz_name_from_abbr();
    ClassMethod {
        type_params: Vec::new(),
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
