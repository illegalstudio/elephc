//! Purpose:
//! Builds the synthetic static methods backing PHP's `ext/calendar` extension. The procedural
//! calendar functions (gregoriantojd, jdtojewish, easter_days, cal_days_in_month, ...) are
//! desugared by the name resolver into these `DateTime::__elephc_*` helpers, which are pure
//! Serial-Day-Number arithmetic (a faithful port of PHP/Scott E. Lee's `sdncal` algorithms).
//!
//! Called from:
//! - `crate::types::checker::builtin_types::datetime::inject_builtin_datetime` (pushes these onto
//!   the synthetic `DateTime` class so the helpers share one host).
//!
//! Key details:
//! - Conversions go through the Julian Day Number (SDN). Multi-value cores return string-keyed
//!   integer arrays (`["y"=>, "m"=>, "d"=>]`) to keep reads on the proven assoc-array path.
//! - The Jewish calendar uses 64-bit halakim arithmetic directly (no 32-bit bit-splitting needed).
//! - Bodies are BUILT, not parsed: `bodies` holds one statement-builder function per helper, so
//!   no PHP is tokenized or parsed on the compilation path. The `*_SRC` constants survive under
//!   `cfg(test)` as the oracle those builders are checked against.

mod bodies;

use crate::parser::ast::{ClassMethod, Expr, Stmt, TypeExpr, Visibility};

/// Returns a dummy source span for synthetic AST nodes built by this module.
fn dummy() -> crate::span::Span {
    crate::span::Span::dummy()
}

/// Wraps an already-built synthetic method body in a static `ClassMethod`.
///
/// `params` is a list of `(name, type, default, by_ref)` tuples mirroring the helper's PHP
/// signature. All calendar helpers are static and return `mixed` (an int, string, or array).
///
/// `body` arrives BUILT from `bodies`. It used to arrive as PHP text that this function
/// tokenized and parsed on every compilation; the builders removed that work from the
/// compilation path, and `built_bodies_match_the_php` keeps them honest.
fn cal_method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    ret: TypeExpr,
    body: Vec<Stmt>,
) -> ClassMethod {
    ClassMethod {
        type_params: Vec::new(),
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(ret),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// The `mixed` return type used for the array/heterogeneous calendar cores.
fn mixed_ret() -> TypeExpr {
    TypeExpr::Named(crate::names::Name::unqualified("mixed"))
}

/// Builds an `(name, Int, no-default, by-value)` parameter tuple — the common shape for the
/// integer arguments every calendar helper takes.
fn int_param(name: &str) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), Some(TypeExpr::Int), None, false)
}

/// Returns every synthetic `DateTime::__elephc_*` calendar helper method.
///
/// Ordering is irrelevant; the methods cross-call each other by name. Pushed onto the `DateTime`
/// class during builtin injection so the whole `ext/calendar` surface compiles into the binary.
pub(super) fn calendar_methods() -> Vec<ClassMethod> {
    vec![
        // ---- Gregorian core ----
        cal_method(
            "__elephc_greg_to_sdn",
            vec![int_param("iy"), int_param("im"), int_param("id")],
            TypeExpr::Int, bodies::greg_to_sdn(),
        ),
        cal_method("__elephc_sdn_to_greg", vec![int_param("sdn")], mixed_ret(), bodies::sdn_to_greg()),
        // ---- Julian core ----
        cal_method(
            "__elephc_jul_to_sdn",
            vec![int_param("iy"), int_param("im"), int_param("id")],
            TypeExpr::Int, bodies::jul_to_sdn(),
        ),
        cal_method("__elephc_sdn_to_jul", vec![int_param("sdn")], mixed_ret(), bodies::sdn_to_jul()),
        // ---- French Republican core ----
        cal_method(
            "__elephc_fr_to_sdn",
            vec![int_param("y"), int_param("m"), int_param("d")],
            TypeExpr::Int, bodies::fr_to_sdn(),
        ),
        cal_method("__elephc_sdn_to_fr", vec![int_param("sdn")], mixed_ret(), bodies::sdn_to_fr()),
        // ---- Jewish core ----
        cal_method(
            "__elephc_jew_tishri1",
            vec![int_param("my"), int_param("moladDay"), int_param("moladHalakim")],
            TypeExpr::Int, bodies::jew_tishri1(),
        ),
        cal_method("__elephc_jew_molad_cycle", vec![int_param("mc")], mixed_ret(), bodies::jew_molad_cycle()),
        cal_method(
            "__elephc_jew_find_tishri_molad",
            vec![int_param("inputDay")],
            mixed_ret(), bodies::jew_find_tishri_molad(),
        ),
        cal_method("__elephc_jew_find_start_year", vec![int_param("year")], mixed_ret(), bodies::jew_find_start_year()),
        cal_method(
            "__elephc_jew_to_sdn",
            vec![int_param("year"), int_param("month"), int_param("day")],
            TypeExpr::Int, bodies::jew_to_sdn(),
        ),
        cal_method("__elephc_sdn_to_jew", vec![int_param("sdn")], mixed_ret(), bodies::sdn_to_jew()),
        cal_method(
            "__elephc_jew_month_name",
            vec![int_param("year"), int_param("month")],
            TypeExpr::Str, bodies::jew_month_name(),
        ),
        // ---- Easter / day-of-week ----
        cal_method(
            "__elephc_easter_calc",
            vec![int_param("year"), int_param("method"), int_param("gm")],
            TypeExpr::Int, bodies::easter_calc(),
        ),
        // ---- Public procedural targets ----
        cal_method(
            "__elephc_cal_to_jd",
            vec![int_param("calendar"), int_param("month"), int_param("day"), int_param("year")],
            TypeExpr::Int, bodies::cal_to_jd(),
        ),
        cal_method(
            "__elephc_gregoriantojd",
            vec![int_param("month"), int_param("day"), int_param("year")],
            TypeExpr::Int, bodies::gregoriantojd(),
        ),
        cal_method("__elephc_jdtogregorian", vec![int_param("jd")], TypeExpr::Str, bodies::jdtogregorian()),
        cal_method(
            "__elephc_juliantojd",
            vec![int_param("month"), int_param("day"), int_param("year")],
            TypeExpr::Int, bodies::juliantojd(),
        ),
        cal_method("__elephc_jdtojulian", vec![int_param("jd")], TypeExpr::Str, bodies::jdtojulian()),
        cal_method(
            "__elephc_frenchtojd",
            vec![int_param("month"), int_param("day"), int_param("year")],
            TypeExpr::Int, bodies::frenchtojd(),
        ),
        cal_method("__elephc_jdtofrench", vec![int_param("jd")], TypeExpr::Str, bodies::jdtofrench()),
        cal_method(
            "__elephc_jewishtojd",
            vec![int_param("month"), int_param("day"), int_param("year")],
            TypeExpr::Int, bodies::jewishtojd(),
        ),
        cal_method(
            "__elephc_jdtojewish",
            vec![
                int_param("jd"),
                ("hebrew".to_string(), Some(TypeExpr::Bool), Some(bool_false()), false),
                ("flags".to_string(), Some(TypeExpr::Int), Some(int_lit(0)), false),
            ],
            TypeExpr::Str, bodies::jdtojewish(),
        ),
        cal_method(
            "__elephc_easter_days",
            vec![
                int_param("year"),
                ("mode".to_string(), Some(TypeExpr::Int), Some(int_lit(0)), false),
            ],
            TypeExpr::Int, bodies::easter_days(),
        ),
        cal_method(
            "__elephc_easter_date",
            vec![
                int_param("year"),
                ("mode".to_string(), Some(TypeExpr::Int), Some(int_lit(0)), false),
            ],
            TypeExpr::Int, bodies::easter_date(),
        ),
        cal_method(
            "__elephc_unixtojd",
            vec![("timestamp".to_string(), Some(TypeExpr::Int), Some(int_lit(0)), false)],
            TypeExpr::Int, bodies::unixtojd(),
        ),
        cal_method("__elephc_jdtounix", vec![int_param("jd")], TypeExpr::Int, bodies::jdtounix()),
        cal_method(
            "__elephc_jddayofweek",
            vec![
                int_param("jd"),
                ("mode".to_string(), Some(TypeExpr::Int), Some(int_lit(0)), false),
            ],
            mixed_ret(), bodies::jddayofweek(),
        ),
        cal_method("__elephc_jdmonthname", vec![int_param("jd"), int_param("mode")], TypeExpr::Str, bodies::jdmonthname()),
        cal_method(
            "__elephc_cal_days_in_month",
            vec![int_param("calendar"), int_param("month"), int_param("year")],
            TypeExpr::Int, bodies::cal_days_in_month(),
        ),
        cal_method("__elephc_cal_from_jd", vec![int_param("jd"), int_param("calendar")], mixed_ret(), bodies::cal_from_jd()),
        cal_method(
            "__elephc_cal_info",
            vec![("calendar".to_string(), Some(TypeExpr::Int), Some(int_lit(-1)), false)],
            mixed_ret(), bodies::cal_info(),
        ),
    ]
}

/// `false` literal expression for a default parameter value.
fn bool_false() -> Expr {
    Expr::new(crate::parser::ast::ExprKind::BoolLiteral(false), dummy())
}

/// Integer literal expression for a default parameter value.
fn int_lit(value: i64) -> Expr {
    Expr::new(crate::parser::ast::ExprKind::IntLiteral(value), dummy())
}

// ===================== Gregorian =====================

#[cfg(test)]
const GREG_TO_SDN_SRC: &str = r#"<?php
if ($iy == 0 || $iy < -4714 || $im <= 0 || $im > 12 || $id <= 0 || $id > 31) { return 0; }
if ($iy == -4714) { if ($im < 11) { return 0; } if ($im == 11 && $id < 25) { return 0; } }
$year = ($iy < 0) ? $iy + 4801 : $iy + 4800;
if ($im > 2) { $month = $im - 3; } else { $month = $im + 9; $year = $year - 1; }
return intdiv(intdiv($year, 100) * 146097, 4) + intdiv(($year % 100) * 1461, 4) + intdiv($month * 153 + 2, 5) + $id - 32045;
"#;

#[cfg(test)]
const SDN_TO_GREG_SRC: &str = r#"<?php
if ($sdn <= 0) { return ["y" => 0, "m" => 0, "d" => 0]; }
$temp = ($sdn + 32045) * 4 - 1;
$century = intdiv($temp, 146097);
$temp = intdiv($temp % 146097, 4) * 4 + 3;
$year = $century * 100 + intdiv($temp, 1461);
$doy = intdiv($temp % 1461, 4) + 1;
$temp = $doy * 5 - 3;
$month = intdiv($temp, 153);
$day = intdiv($temp % 153, 5) + 1;
if ($month < 10) { $month = $month + 3; } else { $year = $year + 1; $month = $month - 9; }
$year = $year - 4800;
if ($year <= 0) { $year = $year - 1; }
return ["y" => $year, "m" => $month, "d" => $day];
"#;

// ===================== Julian =====================

#[cfg(test)]
const JUL_TO_SDN_SRC: &str = r#"<?php
if ($iy == 0 || $iy < -4713 || $im <= 0 || $im > 12 || $id <= 0 || $id > 31) { return 0; }
if ($iy == -4713) { if ($im == 1 && $id == 1) { return 0; } }
$year = ($iy < 0) ? $iy + 4801 : $iy + 4800;
if ($im > 2) { $month = $im - 3; } else { $month = $im + 9; $year = $year - 1; }
return intdiv($year * 1461, 4) + intdiv($month * 153 + 2, 5) + $id - 32083;
"#;

#[cfg(test)]
const SDN_TO_JUL_SRC: &str = r#"<?php
if ($sdn <= 0) { return ["y" => 0, "m" => 0, "d" => 0]; }
$temp = $sdn * 4 + (32083 * 4 - 1);
$year = intdiv($temp, 1461);
$doy = intdiv($temp % 1461, 4) + 1;
$temp = $doy * 5 - 3;
$month = intdiv($temp, 153);
$day = intdiv($temp % 153, 5) + 1;
if ($month < 10) { $month = $month + 3; } else { $year = $year + 1; $month = $month - 9; }
$year = $year - 4800;
if ($year <= 0) { $year = $year - 1; }
return ["y" => $year, "m" => $month, "d" => $day];
"#;

// ===================== French Republican =====================

#[cfg(test)]
const FR_TO_SDN_SRC: &str = r#"<?php
if ($y < 1 || $y > 14 || $m < 1 || $m > 13 || $d < 1 || $d > 30) { return 0; }
return intdiv($y * 1461, 4) + ($m - 1) * 30 + $d + 2375474;
"#;

#[cfg(test)]
const SDN_TO_FR_SRC: &str = r#"<?php
if ($sdn < 2375840 || $sdn > 2380952) { return ["y" => 0, "m" => 0, "d" => 0]; }
$temp = ($sdn - 2375474) * 4 - 1;
$year = intdiv($temp, 1461);
$doy = intdiv($temp % 1461, 4);
$month = intdiv($doy, 30) + 1;
$day = $doy % 30 + 1;
return ["y" => $year, "m" => $month, "d" => $day];
"#;

// ===================== Jewish =====================

#[cfg(test)]
const JEW_TISHRI1_SRC: &str = r#"<?php
$tishri1 = $moladDay;
$dow = $tishri1 % 7;
$leap = ($my == 2 || $my == 5 || $my == 7 || $my == 10 || $my == 13 || $my == 16 || $my == 18);
$lastLeap = ($my == 3 || $my == 6 || $my == 8 || $my == 11 || $my == 14 || $my == 17 || $my == 0);
if (($moladHalakim >= 19440) || ((!$leap) && $dow == 2 && $moladHalakim >= 9924) || ($lastLeap && $dow == 1 && $moladHalakim >= 16789)) {
    $tishri1 = $tishri1 + 1;
    $dow = $dow + 1;
    if ($dow == 7) { $dow = 0; }
}
if ($dow == 3 || $dow == 5 || $dow == 0) { $tishri1 = $tishri1 + 1; }
return $tishri1;
"#;

#[cfg(test)]
const JEW_MOLAD_CYCLE_SRC: &str = r#"<?php
$total = 31524 + $mc * 179876755;
return ["md" => intdiv($total, 25920), "mh" => $total % 25920];
"#;

#[cfg(test)]
const JEW_FIND_TISHRI_MOLAD_SRC: &str = r#"<?php
$months = [12, 12, 13, 12, 12, 13, 12, 13, 12, 12, 13, 12, 12, 13, 12, 12, 13, 12, 13];
$mc = intdiv($inputDay + 310, 6940);
$mm = DateTime::__elephc_jew_molad_cycle($mc);
$md = $mm["md"];
$mh = $mm["mh"];
while ($md < $inputDay - 6940 + 310) {
    $mc = $mc + 1;
    $mh = $mh + 179876755;
    $md = $md + intdiv($mh, 25920);
    $mh = $mh % 25920;
}
$my = 0;
while ($my < 18) {
    if ($md > $inputDay - 74) { break; }
    $mh = $mh + 765433 * $months[$my];
    $md = $md + intdiv($mh, 25920);
    $mh = $mh % 25920;
    $my = $my + 1;
}
return ["mc" => $mc, "my" => $my, "md" => $md, "mh" => $mh];
"#;

#[cfg(test)]
const JEW_FIND_START_YEAR_SRC: &str = r#"<?php
$offsets = [0, 12, 24, 37, 49, 61, 74, 86, 99, 111, 123, 136, 148, 160, 173, 185, 197, 210, 222];
$mc = intdiv($year - 1, 19);
$my = ($year - 1) % 19;
$mm = DateTime::__elephc_jew_molad_cycle($mc);
$md = $mm["md"];
$mh = $mm["mh"];
$mh = $mh + 765433 * $offsets[$my];
$md = $md + intdiv($mh, 25920);
$mh = $mh % 25920;
$t1 = DateTime::__elephc_jew_tishri1($my, $md, $mh);
return ["mc" => $mc, "my" => $my, "md" => $md, "mh" => $mh, "t1" => $t1];
"#;

#[cfg(test)]
const JEW_TO_SDN_SRC: &str = r#"<?php
if ($year <= 0 || $day <= 0 || $day > 30) { return 0; }
$months = [12, 12, 13, 12, 12, 13, 12, 13, 12, 12, 13, 12, 12, 13, 12, 12, 13, 12, 13];
if ($month == 1 || $month == 2) {
    $s = DateTime::__elephc_jew_find_start_year($year);
    $t1 = $s["t1"];
    $sdn = ($month == 1) ? $t1 + $day - 1 : $t1 + $day + 29;
} else if ($month == 3) {
    $s = DateTime::__elephc_jew_find_start_year($year);
    $t1 = $s["t1"];
    $md = $s["md"];
    $mh = $s["mh"];
    $my = $s["my"];
    $mh = $mh + 765433 * $months[$my];
    $md = $md + intdiv($mh, 25920);
    $mh = $mh % 25920;
    $t1a = DateTime::__elephc_jew_tishri1(($my + 1) % 19, $md, $mh);
    $yl = $t1a - $t1;
    $sdn = ($yl == 355 || $yl == 385) ? $t1 + $day + 59 : $t1 + $day + 58;
} else if ($month == 4 || $month == 5 || $month == 6) {
    $s = DateTime::__elephc_jew_find_start_year($year + 1);
    $t1a = $s["t1"];
    $lai = ($months[($year - 1) % 19] == 12) ? 29 : 59;
    if ($month == 4) { $sdn = $t1a + $day - $lai - 237; }
    else if ($month == 5) { $sdn = $t1a + $day - $lai - 208; }
    else { $sdn = $t1a + $day - $lai - 178; }
} else {
    $s = DateTime::__elephc_jew_find_start_year($year + 1);
    $t1a = $s["t1"];
    if ($month == 7) { $sdn = $t1a + $day - 207; }
    else if ($month == 8) { $sdn = $t1a + $day - 178; }
    else if ($month == 9) { $sdn = $t1a + $day - 148; }
    else if ($month == 10) { $sdn = $t1a + $day - 119; }
    else if ($month == 11) { $sdn = $t1a + $day - 89; }
    else if ($month == 12) { $sdn = $t1a + $day - 60; }
    else if ($month == 13) { $sdn = $t1a + $day - 30; }
    else { return 0; }
}
return $sdn + 347997;
"#;

#[cfg(test)]
const SDN_TO_JEW_SRC: &str = r#"<?php
if ($sdn <= 347997 || $sdn > 324542846) { return ["y" => 0, "m" => 0, "d" => 0]; }
$months = [12, 12, 13, 12, 12, 13, 12, 13, 12, 12, 13, 12, 12, 13, 12, 12, 13, 12, 13];
$inputDay = $sdn - 347997;
$f = DateTime::__elephc_jew_find_tishri_molad($inputDay);
$mc = $f["mc"];
$my = $f["my"];
$day = $f["md"];
$hal = $f["mh"];
$t1 = DateTime::__elephc_jew_tishri1($my, $day, $hal);
$t1a = 0;
$py = 0; $pm = 0; $pd = 0;
if ($inputDay >= $t1) {
    $py = $mc * 19 + $my + 1;
    if ($inputDay < $t1 + 59) {
        if ($inputDay < $t1 + 30) { return ["y" => $py, "m" => 1, "d" => $inputDay - $t1 + 1]; }
        return ["y" => $py, "m" => 2, "d" => $inputDay - $t1 - 29];
    }
    $hal = $hal + 765433 * $months[$my];
    $day = $day + intdiv($hal, 25920);
    $hal = $hal % 25920;
    $t1a = DateTime::__elephc_jew_tishri1(($my + 1) % 19, $day, $hal);
} else {
    $py = $mc * 19 + $my;
    if ($inputDay >= $t1 - 177) {
        if ($inputDay > $t1 - 30) { return ["y" => $py, "m" => 13, "d" => $inputDay - $t1 + 30]; }
        if ($inputDay > $t1 - 60) { return ["y" => $py, "m" => 12, "d" => $inputDay - $t1 + 60]; }
        if ($inputDay > $t1 - 89) { return ["y" => $py, "m" => 11, "d" => $inputDay - $t1 + 89]; }
        if ($inputDay > $t1 - 119) { return ["y" => $py, "m" => 10, "d" => $inputDay - $t1 + 119]; }
        if ($inputDay > $t1 - 148) { return ["y" => $py, "m" => 9, "d" => $inputDay - $t1 + 148]; }
        return ["y" => $py, "m" => 8, "d" => $inputDay - $t1 + 178];
    }
    if ($months[($py - 1) % 19] == 13) {
        $pm = 7; $pd = $inputDay - $t1 + 207;
        if ($pd > 0) { return ["y" => $py, "m" => $pm, "d" => $pd]; }
        $pm = $pm - 1; $pd = $pd + 30;
        if ($pd > 0) { return ["y" => $py, "m" => $pm, "d" => $pd]; }
        $pm = $pm - 1; $pd = $pd + 30;
    } else {
        $pm = 7; $pd = $inputDay - $t1 + 207;
        if ($pd > 0) { return ["y" => $py, "m" => $pm, "d" => $pd]; }
        $pm = $pm - 2; $pd = $pd + 30;
    }
    if ($pd > 0) { return ["y" => $py, "m" => $pm, "d" => $pd]; }
    $pm = $pm - 1; $pd = $pd + 29;
    if ($pd > 0) { return ["y" => $py, "m" => $pm, "d" => $pd]; }
    $t1a = $t1;
    $f2 = DateTime::__elephc_jew_find_tishri_molad($day - 365);
    $mc = $f2["mc"];
    $my = $f2["my"];
    $day = $f2["md"];
    $hal = $f2["mh"];
    $t1 = DateTime::__elephc_jew_tishri1($my, $day, $hal);
}
$yl = $t1a - $t1;
$day = $inputDay - $t1 - 29;
if ($yl == 355 || $yl == 385) {
    if ($day <= 30) { return ["y" => $py, "m" => 2, "d" => $day]; }
    $day = $day - 30;
} else {
    if ($day <= 29) { return ["y" => $py, "m" => 2, "d" => $day]; }
    $day = $day - 29;
}
return ["y" => $py, "m" => 3, "d" => $day];
"#;

#[cfg(test)]
const JEW_MONTH_NAME_SRC: &str = r#"<?php
$months = [12, 12, 13, 12, 12, 13, 12, 13, 12, 12, 13, 12, 12, 13, 12, 12, 13, 12, 13];
$leapYear = ($months[($year - 1) % 19] == 13);
$leap = ["", "Tishri", "Heshvan", "Kislev", "Tevet", "Shevat", "Adar I", "Adar II", "Nisan", "Iyyar", "Sivan", "Tammuz", "Av", "Elul"];
$reg = ["", "Tishri", "Heshvan", "Kislev", "Tevet", "Shevat", "", "Adar", "Nisan", "Iyyar", "Sivan", "Tammuz", "Av", "Elul"];
return $leapYear ? $leap[$month] : $reg[$month];
"#;

// ===================== Easter =====================

#[cfg(test)]
const EASTER_CALC_SRC: &str = r#"<?php
$golden = ($year % 19) + 1;
if (($year <= 1582 && $method != 2) || ($year >= 1583 && $year <= 1752 && $method != 1 && $method != 2) || $method == 3) {
    $dom = ($year + intdiv($year, 4) + 5) % 7;
    if ($dom < 0) { $dom = $dom + 7; }
    $pfm = (3 - (11 * $golden) - 7) % 30;
    if ($pfm < 0) { $pfm = $pfm + 30; }
} else {
    $dom = ($year + intdiv($year, 4) - intdiv($year, 100) + intdiv($year, 400)) % 7;
    if ($dom < 0) { $dom = $dom + 7; }
    $solar = intdiv($year - 1600, 100) - intdiv($year - 1600, 400);
    $lunar = intdiv(intdiv($year - 1400, 100) * 8, 25);
    $pfm = (3 - (11 * $golden) + $solar - $lunar) % 30;
    if ($pfm < 0) { $pfm = $pfm + 30; }
}
if ($pfm == 29 || ($pfm == 28 && $golden > 11)) { $pfm = $pfm - 1; }
$tmp = (4 - $pfm - $dom) % 7;
if ($tmp < 0) { $tmp = $tmp + 7; }
$easter = $pfm + $tmp + 1;
if ($gm != 0) {
    if ($easter < 11) { $mon = 3; $mday = $easter + 21; } else { $mon = 4; $mday = $easter - 10; }
    return __elephc_mktime_raw(0, 0, 0, $mon, $mday, $year);
}
return $easter;
"#;

#[cfg(test)]
const EASTER_DAYS_SRC: &str = r#"<?php
return DateTime::__elephc_easter_calc($year, $mode, 0);
"#;

#[cfg(test)]
const EASTER_DATE_SRC: &str = r#"<?php
return DateTime::__elephc_easter_calc($year, $mode, 1);
"#;

// ===================== Public targets =====================

#[cfg(test)]
const CAL_TO_JD_SRC: &str = r#"<?php
if ($calendar == 0) { return DateTime::__elephc_greg_to_sdn($year, $month, $day); }
if ($calendar == 1) { return DateTime::__elephc_jul_to_sdn($year, $month, $day); }
if ($calendar == 2) { return DateTime::__elephc_jew_to_sdn($year, $month, $day); }
if ($calendar == 3) { return DateTime::__elephc_fr_to_sdn($year, $month, $day); }
return 0;
"#;

#[cfg(test)]
const GREGORIANTOJD_SRC: &str = r#"<?php
return DateTime::__elephc_greg_to_sdn($year, $month, $day);
"#;

#[cfg(test)]
const JDTOGREGORIAN_SRC: &str = r#"<?php
$r = DateTime::__elephc_sdn_to_greg($jd);
return $r["m"] . "/" . $r["d"] . "/" . $r["y"];
"#;

#[cfg(test)]
const JULIANTOJD_SRC: &str = r#"<?php
return DateTime::__elephc_jul_to_sdn($year, $month, $day);
"#;

#[cfg(test)]
const JDTOJULIAN_SRC: &str = r#"<?php
$r = DateTime::__elephc_sdn_to_jul($jd);
return $r["m"] . "/" . $r["d"] . "/" . $r["y"];
"#;

#[cfg(test)]
const FRENCHTOJD_SRC: &str = r#"<?php
return DateTime::__elephc_fr_to_sdn($year, $month, $day);
"#;

#[cfg(test)]
const JDTOFRENCH_SRC: &str = r#"<?php
$r = DateTime::__elephc_sdn_to_fr($jd);
return $r["m"] . "/" . $r["d"] . "/" . $r["y"];
"#;

#[cfg(test)]
const JEWISHTOJD_SRC: &str = r#"<?php
return DateTime::__elephc_jew_to_sdn($year, $month, $day);
"#;

#[cfg(test)]
const JDTOJEWISH_SRC: &str = r#"<?php
$r = DateTime::__elephc_sdn_to_jew($jd);
return $r["m"] . "/" . $r["d"] . "/" . $r["y"];
"#;

#[cfg(test)]
const UNIXTOJD_SRC: &str = r#"<?php
$y = intval(gmdate("Y", $timestamp));
$m = intval(gmdate("n", $timestamp));
$d = intval(gmdate("j", $timestamp));
return DateTime::__elephc_greg_to_sdn($y, $m, $d);
"#;

#[cfg(test)]
const JDTOUNIX_SRC: &str = r#"<?php
return ($jd - 2440588) * 86400;
"#;

#[cfg(test)]
const JDDAYOFWEEK_SRC: &str = r#"<?php
$d = ($jd % 7 + 8) % 7;
$long = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
$short = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
if ($mode == 1) { return $long[$d]; }
if ($mode == 2) { return $short[$d]; }
return $d;
"#;

#[cfg(test)]
const JDMONTHNAME_SRC: &str = r#"<?php
$gregShort = ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
$gregLong = ["", "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
$french = ["", "Vendemiaire", "Brumaire", "Frimaire", "Nivose", "Pluviose", "Ventose", "Germinal", "Floreal", "Prairial", "Messidor", "Thermidor", "Fructidor", "Extra"];
if ($mode == 1) { $r = DateTime::__elephc_sdn_to_greg($jd); return $gregLong[$r["m"]]; }
if ($mode == 2) { $r = DateTime::__elephc_sdn_to_jul($jd); return $gregShort[$r["m"]]; }
if ($mode == 3) { $r = DateTime::__elephc_sdn_to_jul($jd); return $gregLong[$r["m"]]; }
if ($mode == 4) { $r = DateTime::__elephc_sdn_to_jew($jd); return ($r["y"] > 0) ? DateTime::__elephc_jew_month_name($r["y"], $r["m"]) : ""; }
if ($mode == 5) { $r = DateTime::__elephc_sdn_to_fr($jd); return $french[$r["m"]]; }
$r = DateTime::__elephc_sdn_to_greg($jd);
return $gregShort[$r["m"]];
"#;

#[cfg(test)]
const CAL_DAYS_IN_MONTH_SRC: &str = r#"<?php
$start = DateTime::__elephc_cal_to_jd($calendar, $month, 1, $year);
$next = DateTime::__elephc_cal_to_jd($calendar, $month + 1, 1, $year);
if ($next == 0) {
    if ($year == -1) {
        $next = DateTime::__elephc_cal_to_jd($calendar, 1, 1, 1);
    } else {
        $next = DateTime::__elephc_cal_to_jd($calendar, 1, 1, $year + 1);
        if ($calendar == 3 && $next == 0) { $next = 2380953; }
    }
}
return $next - $start;
"#;

#[cfg(test)]
const CAL_FROM_JD_SRC: &str = r#"<?php
$gregShort = ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
$gregLong = ["", "January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
$french = ["", "Vendemiaire", "Brumaire", "Frimaire", "Nivose", "Pluviose", "Ventose", "Germinal", "Floreal", "Prairial", "Messidor", "Thermidor", "Fructidor", "Extra"];
$dayLong = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
$dayShort = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
if ($calendar == 1) { $r = DateTime::__elephc_sdn_to_jul($jd); }
else if ($calendar == 2) { $r = DateTime::__elephc_sdn_to_jew($jd); }
else if ($calendar == 3) { $r = DateTime::__elephc_sdn_to_fr($jd); }
else { $r = DateTime::__elephc_sdn_to_greg($jd); }
$y = $r["y"]; $m = $r["m"]; $d = $r["d"];
$dow = ($jd % 7 + 8) % 7;
if ($calendar == 2 && $y <= 0) {
    $abMonth = ""; $monthName = "";
} else if ($calendar == 2) {
    $abMonth = DateTime::__elephc_jew_month_name($y, $m);
    $monthName = $abMonth;
} else if ($calendar == 1) {
    $abMonth = $gregShort[$m]; $monthName = $gregLong[$m];
} else if ($calendar == 3) {
    $abMonth = $french[$m]; $monthName = $french[$m];
} else {
    $abMonth = $gregShort[$m]; $monthName = $gregLong[$m];
}
return [
    "date" => $m . "/" . $d . "/" . $y,
    "month" => $m,
    "day" => $d,
    "year" => $y,
    "dow" => $dow,
    "abbrevdayname" => $dayShort[$dow],
    "dayname" => $dayLong[$dow],
    "abbrevmonth" => $abMonth,
    "monthname" => $monthName,
];
"#;

#[cfg(test)]
const CAL_INFO_SRC: &str = r#"<?php
$greg = [
    "months" => [1 => "January", 2 => "February", 3 => "March", 4 => "April", 5 => "May", 6 => "June", 7 => "July", 8 => "August", 9 => "September", 10 => "October", 11 => "November", 12 => "December"],
    "abbrevmonths" => [1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr", 5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug", 9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec"],
    "maxdaysinmonth" => 31,
    "calname" => "Gregorian",
    "calsymbol" => "CAL_GREGORIAN",
];
$jul = [
    "months" => [1 => "January", 2 => "February", 3 => "March", 4 => "April", 5 => "May", 6 => "June", 7 => "July", 8 => "August", 9 => "September", 10 => "October", 11 => "November", 12 => "December"],
    "abbrevmonths" => [1 => "Jan", 2 => "Feb", 3 => "Mar", 4 => "Apr", 5 => "May", 6 => "Jun", 7 => "Jul", 8 => "Aug", 9 => "Sep", 10 => "Oct", 11 => "Nov", 12 => "Dec"],
    "maxdaysinmonth" => 31,
    "calname" => "Julian",
    "calsymbol" => "CAL_JULIAN",
];
$jew = [
    "months" => [1 => "Tishri", 2 => "Heshvan", 3 => "Kislev", 4 => "Tevet", 5 => "Shevat", 6 => "Adar I", 7 => "Adar II", 8 => "Nisan", 9 => "Iyyar", 10 => "Sivan", 11 => "Tammuz", 12 => "Av", 13 => "Elul"],
    "abbrevmonths" => [1 => "Tishri", 2 => "Heshvan", 3 => "Kislev", 4 => "Tevet", 5 => "Shevat", 6 => "Adar I", 7 => "Adar II", 8 => "Nisan", 9 => "Iyyar", 10 => "Sivan", 11 => "Tammuz", 12 => "Av", 13 => "Elul"],
    "maxdaysinmonth" => 30,
    "calname" => "Jewish",
    "calsymbol" => "CAL_JEWISH",
];
$fr = [
    "months" => [1 => "Vendemiaire", 2 => "Brumaire", 3 => "Frimaire", 4 => "Nivose", 5 => "Pluviose", 6 => "Ventose", 7 => "Germinal", 8 => "Floreal", 9 => "Prairial", 10 => "Messidor", 11 => "Thermidor", 12 => "Fructidor", 13 => "Extra"],
    "abbrevmonths" => [1 => "Vendemiaire", 2 => "Brumaire", 3 => "Frimaire", 4 => "Nivose", 5 => "Pluviose", 6 => "Ventose", 7 => "Germinal", 8 => "Floreal", 9 => "Prairial", 10 => "Messidor", 11 => "Thermidor", 12 => "Fructidor", 13 => "Extra"],
    "maxdaysinmonth" => 30,
    "calname" => "French",
    "calsymbol" => "CAL_FRENCH",
];
if ($calendar == 0) { return $greg; }
if ($calendar == 1) { return $jul; }
if ($calendar == 2) { return $jew; }
if ($calendar == 3) { return $fr; }
return [0 => $greg, 1 => $jul, 2 => $jew, 3 => $fr];
"#;

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `(label, php, built)` triple the oracle sweeps.
    fn cases() -> Vec<(&'static str, &'static str, Vec<Stmt>)> {
        vec![
            ("GREG_TO_SDN_SRC", GREG_TO_SDN_SRC, bodies::greg_to_sdn()),
            ("SDN_TO_GREG_SRC", SDN_TO_GREG_SRC, bodies::sdn_to_greg()),
            ("JUL_TO_SDN_SRC", JUL_TO_SDN_SRC, bodies::jul_to_sdn()),
            ("SDN_TO_JUL_SRC", SDN_TO_JUL_SRC, bodies::sdn_to_jul()),
            ("FR_TO_SDN_SRC", FR_TO_SDN_SRC, bodies::fr_to_sdn()),
            ("SDN_TO_FR_SRC", SDN_TO_FR_SRC, bodies::sdn_to_fr()),
            ("JEW_TISHRI1_SRC", JEW_TISHRI1_SRC, bodies::jew_tishri1()),
            ("JEW_MOLAD_CYCLE_SRC", JEW_MOLAD_CYCLE_SRC, bodies::jew_molad_cycle()),
            ("JEW_FIND_TISHRI_MOLAD_SRC", JEW_FIND_TISHRI_MOLAD_SRC, bodies::jew_find_tishri_molad()),
            ("JEW_FIND_START_YEAR_SRC", JEW_FIND_START_YEAR_SRC, bodies::jew_find_start_year()),
            ("JEW_TO_SDN_SRC", JEW_TO_SDN_SRC, bodies::jew_to_sdn()),
            ("SDN_TO_JEW_SRC", SDN_TO_JEW_SRC, bodies::sdn_to_jew()),
            ("JEW_MONTH_NAME_SRC", JEW_MONTH_NAME_SRC, bodies::jew_month_name()),
            ("EASTER_CALC_SRC", EASTER_CALC_SRC, bodies::easter_calc()),
            ("EASTER_DAYS_SRC", EASTER_DAYS_SRC, bodies::easter_days()),
            ("EASTER_DATE_SRC", EASTER_DATE_SRC, bodies::easter_date()),
            ("CAL_TO_JD_SRC", CAL_TO_JD_SRC, bodies::cal_to_jd()),
            ("GREGORIANTOJD_SRC", GREGORIANTOJD_SRC, bodies::gregoriantojd()),
            ("JDTOGREGORIAN_SRC", JDTOGREGORIAN_SRC, bodies::jdtogregorian()),
            ("JULIANTOJD_SRC", JULIANTOJD_SRC, bodies::juliantojd()),
            ("JDTOJULIAN_SRC", JDTOJULIAN_SRC, bodies::jdtojulian()),
            ("FRENCHTOJD_SRC", FRENCHTOJD_SRC, bodies::frenchtojd()),
            ("JDTOFRENCH_SRC", JDTOFRENCH_SRC, bodies::jdtofrench()),
            ("JEWISHTOJD_SRC", JEWISHTOJD_SRC, bodies::jewishtojd()),
            ("JDTOJEWISH_SRC", JDTOJEWISH_SRC, bodies::jdtojewish()),
            ("UNIXTOJD_SRC", UNIXTOJD_SRC, bodies::unixtojd()),
            ("JDTOUNIX_SRC", JDTOUNIX_SRC, bodies::jdtounix()),
            ("JDDAYOFWEEK_SRC", JDDAYOFWEEK_SRC, bodies::jddayofweek()),
            ("JDMONTHNAME_SRC", JDMONTHNAME_SRC, bodies::jdmonthname()),
            ("CAL_DAYS_IN_MONTH_SRC", CAL_DAYS_IN_MONTH_SRC, bodies::cal_days_in_month()),
            ("CAL_FROM_JD_SRC", CAL_FROM_JD_SRC, bodies::cal_from_jd()),
            ("CAL_INFO_SRC", CAL_INFO_SRC, bodies::cal_info()),
        ]
    }

    /// THE ORACLE FOR THE TRANSCRIPTION: each built body must equal the parse of the PHP it
    /// replaced, statement by statement.
    ///
    /// The builders were generated by `synthetic_class::transcribe` and then reviewed by hand,
    /// and neither step proves anything on its own — a transcription that drops a qualifier or
    /// an argument still compiles and quietly means something else. This is what makes them
    /// safe to rely on, and it is why the PHP stays in the file under `cfg(test)`.
    ///
    /// Spans are stripped because a built node has none and a parsed one does; everything else
    /// — order, operators, nesting, name qualification — has to match exactly.
    #[test]
    fn built_bodies_match_the_php() {
        for (label, php, built) in cases() {
            let tokens = crate::lexer::tokenize(php)
                .unwrap_or_else(|e| panic!("{label} must tokenize: {e:?}"));
            let parsed = crate::parser::parse_internal(&tokens)
                .unwrap_or_else(|e| panic!("{label} must parse: {e:?}"));

            assert_eq!(
                built.len(),
                parsed.len(),
                "{label}: statement COUNT differs — built {} vs parsed {}",
                built.len(),
                parsed.len()
            );
            for (index, (built_stmt, parsed_stmt)) in built.iter().zip(parsed.iter()).enumerate() {
                assert_eq!(
                    strip_spans(&format!("{built_stmt:?}")),
                    strip_spans(&format!("{parsed_stmt:?}")),
                    "{label}: statement {index} differs from its PHP"
                );
            }
        }
    }

    /// Removes span payloads so a built node and a parsed node compare on structure alone.
    fn strip_spans(rendered: &str) -> String {
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered;
        while let Some(at) = rest.find("Span {") {
            cleaned.push_str(&rest[..at]);
            cleaned.push_str("Span");
            let after = &rest[at..];
            let close = after.find('}').map(|end| end + 1).unwrap_or(after.len());
            rest = &after[close..];
        }
        cleaned.push_str(rest);
        cleaned
    }
}
