//! Purpose:
//! Audited DateInterval behavior, timelib diff, and final DateTime class-map injection.
//!
//! Called from:
//! - The DateTime checker metadata facade and sibling compliance modules.
//!
//! Key details:
//! - Preserves the audited php-src DateTime semantics while the checker metadata stays split.

#[allow(unused_imports)]
use super::{
    Attribute, AttributeGroup, BinOp, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind,
    FlattenedClass, HashMap, InterfaceDeclInfo, Name, NameKind, PropertyHooks, StaticReceiver, Stmt,
    StmtKind, TypeExpr, Visibility,
};
use super::compliance_core::*;
use super::compliance_methods::*;
use super::compliance_procedural::*;
/// `DateInterval::__construct(string $duration)` — parses an ISO 8601 duration into components.
///
/// Scans `P[nY][nM][nW][nD][T[nH][nM][nS]]`, accumulating each number and assigning it to the
/// matching component on the unit letter; `M` before `T` is months, after `T` is minutes; `W`
/// PHP source backing `DateInterval::__serialize()`. PHP uses a compact two-key shape for
/// relative-string intervals and a component shape without `date_string` for ISO intervals.
pub(super) const DATEINTERVAL_SERIALIZE_SRC: &str = r#"<?php
if ($this->_from_string) {
    return [
        "from_string" => true,
        "date_string" => $this->_date_string,
    ];
}
return [
    "y" => $this->y, "m" => $this->m, "d" => $this->d,
    "h" => $this->h, "i" => $this->i, "s" => $this->s,
    "f" => $this->f, "invert" => $this->invert,
    "days" => $this->days, "from_string" => false,
];
"#;

/// PHP source backing `DateInterval::__unserialize()`. Restores either of PHP's two serialized
/// shapes. A string `date_string` takes precedence regardless of `from_string`, matching
/// php-src's hash initializer, and accepts absolute fields while retaining only timelib's
/// relative sub-structure.
pub(super) const DATEINTERVAL_UNSERIALIZE_SRC: &str = r#"<?php
if (array_key_exists("date_string", $data) && is_string($data["date_string"])) {
    $parsed = __elephc_timelib_interval_restore_parse($data["date_string"]);
    if ($parsed["status"] === "E") {
        throw new Error(
            "Unknown or bad format (" . $data["date_string"] . ") at position "
            . $parsed["position"] . " (" . $parsed["character"]
            . ") while unserializing: " . $parsed["message"]
        );
    }
    $this->y = $parsed["y"];
    $this->m = $parsed["m"];
    $this->d = $parsed["d"];
    $this->h = $parsed["h"];
    $this->i = $parsed["i"];
    $this->s = $parsed["s"];
    $this->f = $parsed["us"] / 1000000.0;
    $this->invert = $parsed["invert"];
    $this->days = $parsed["days"] === -9999999 ? false : $parsed["days"];
    $this->_from_string = true;
    $this->_date_string = $data["date_string"];
    $this->_period_from_string = false;
    $this->_period_date_string = "";
    $this->_wall = false;
    $this->__elephc_initialized = true;
    return;
}
$this->y = intval(array_key_exists("y", $data) ? $data["y"] : -1);
$this->m = intval(array_key_exists("m", $data) ? $data["m"] : -1);
$this->d = intval(array_key_exists("d", $data) ? $data["d"] : -1);
$this->h = intval(array_key_exists("h", $data) ? $data["h"] : -1);
$this->i = intval(array_key_exists("i", $data) ? $data["i"] : -1);
$this->s = intval(array_key_exists("s", $data) ? $data["s"] : -1);
$fValue = floatval(array_key_exists("f", $data) ? $data["f"] : 0.0);
if ($fValue > 9223372036854.775 || $fValue < -9223372036854.775) {
    __elephc_diag_warning(
        "Warning: The float " . ($fValue * 1000000.0)
        . " is not representable as an int, cast occurred",
        1
    );
}
$this->f = $fValue;
$this->invert = intval(array_key_exists("invert", $data) ? $data["invert"] : 0);
$daysValue = array_key_exists("days", $data) ? $data["days"] : -1;
if ($daysValue === false) {
    $this->days = false;
} elseif (is_array($daysValue) || is_object($daysValue)) {
    $this->days = -1;
} else {
    $this->days = intval($daysValue);
}
$this->_from_string = false;
$this->_date_string = "";
$this->_period_from_string = false;
$this->_period_date_string = "";
$this->_wall = true;
$this->__elephc_initialized = true;
"#;

/// Component-only fallback used when no source operation can reach serialized date-string state.
///
/// Any direct `__unserialize`, `unserialize`, `serialize`, or `var_export` use enables the timelib
/// prelude and selects the full body above. Keeping this body bridge-free prevents unrelated
/// `DateTime` mutators on union receivers from demand-lowering an unavailable timelib helper.
pub(super) const DATEINTERVAL_UNSERIALIZE_COMPONENT_SRC: &str = r#"<?php
$this->y = intval(array_key_exists("y", $data) ? $data["y"] : -1);
$this->m = intval(array_key_exists("m", $data) ? $data["m"] : -1);
$this->d = intval(array_key_exists("d", $data) ? $data["d"] : -1);
$this->h = intval(array_key_exists("h", $data) ? $data["h"] : -1);
$this->i = intval(array_key_exists("i", $data) ? $data["i"] : -1);
$this->s = intval(array_key_exists("s", $data) ? $data["s"] : -1);
$fValue = floatval(array_key_exists("f", $data) ? $data["f"] : 0.0);
if ($fValue > 9223372036854.775 || $fValue < -9223372036854.775) {
    __elephc_diag_warning(
        "Warning: The float " . ($fValue * 1000000.0)
        . " is not representable as an int, cast occurred",
        1
    );
}
$this->f = $fValue;
$this->invert = intval(array_key_exists("invert", $data) ? $data["invert"] : 0);
$daysValue = array_key_exists("days", $data) ? $data["days"] : -1;
if ($daysValue === false) {
    $this->days = false;
} elseif (is_array($daysValue) || is_object($daysValue)) {
    $this->days = -1;
} else {
    $this->days = intval($daysValue);
}
$this->_from_string = false;
$this->_date_string = "";
$this->_period_from_string = false;
$this->_period_date_string = "";
$this->_wall = true;
$this->__elephc_initialized = true;
"#;

/// PHP source backing `DateInterval::__set_state()`. Rebuilds the relative-string form directly
/// or creates a zero interval before restoring the component form.
pub(super) const DATEINTERVAL_SET_STATE_SRC: &str = r#"<?php
$iv = new DateInterval("PT0S");
$iv->__unserialize($array);
return $iv;
"#;

/// PHP source backing `DateInterval::__get()` for php-src's debug-only fields.
pub(super) const DATEINTERVAL_MAGIC_GET_SRC: &str = r#"<?php
__elephc_diag_warning("\nWarning: Undefined property: DateInterval::$" . $name, 1);
return null;
"#;

/// Builds `DateInterval::__get(string $name): mixed`.
///
/// `from_string` and `date_string` exist only in debug/serialization state in
/// php-src. Ordinary reads therefore emit an undefined-property warning and
/// return `null`; the generic magic method preserves that behavior for either
/// spelling without declaring reflection-visible properties.
pub(super) fn dateinterval_magic_get() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEINTERVAL_MAGIC_GET_SRC)
        .expect("DateInterval::__get body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval::__get body source must parse");
    ClassMethod {
        name: "__get".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("name".to_string(), Some(TypeExpr::Str), None, false)],
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

/// Builds `DateInterval::__wakeup(): void` (no-op).
pub(super) fn dateinterval_wakeup() -> ClassMethod {
    datetime_wakeup("DateInterval")
}

/// Builds `DateInterval::__serialize(): array`.
pub(super) fn dateinterval_serialize() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEINTERVAL_SERIALIZE_SRC)
        .expect("DateInterval::__serialize body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval::__serialize body source must parse");
    ClassMethod {
        name: "__serialize".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: Vec::new(),
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("array"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds `DateInterval::__unserialize(array $data): void`.
pub(super) fn dateinterval_unserialize(uses_timelib: bool) -> ClassMethod {
    let source = if uses_timelib {
        DATEINTERVAL_UNSERIALIZE_SRC
    } else {
        DATEINTERVAL_UNSERIALIZE_COMPONENT_SRC
    };
    let tokens = crate::lexer::tokenize(source)
        .expect("DateInterval::__unserialize body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval::__unserialize body source must parse");
    ClassMethod {
        name: "__unserialize".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "data".to_string(),
            Some(TypeExpr::Named(Name::unqualified("array"))),
            None,
            false,
        )],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds `static DateInterval::__set_state(array $array): static`.
pub(super) fn dateinterval_set_state() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEINTERVAL_SET_STATE_SRC)
        .expect("DateInterval::__set_state body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval::__set_state body source must parse");
    ClassMethod {
        name: "__set_state".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "array".to_string(),
            Some(TypeExpr::Named(Name::unqualified("array"))),
            None,
            false,
        )],
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

/// contributes 7 days each. The leading `P` is required (a missing/lowercase `P` throws); the
/// `T` time separator is consumed as a no-op and unknown letters throw.
pub(super) fn date_interval_legacy_constructor() -> ClassMethod {
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
        assign_this_property(
            "__elephc_initialized",
            Expr::new(ExprKind::BoolLiteral(true), dummy()),
        ),
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

/// Timelib-backed DateInterval constructor body. This covers php-src's period,
/// combined representation, and start/end interval forms without a parallel parser.
pub(super) const DATEINTERVAL_TIMELIB_CONSTRUCTOR_SRC: &str = r#"<?php
$this->__elephc_initialized = true;
$parsed = __elephc_timelib_interval_parse($duration, false);
if ($parsed["status"] === "E") {
    throw new DateMalformedIntervalStringException("Unknown or bad format (" . $duration . ")");
}
if ($parsed["status"] !== "O") {
    throw new DateMalformedIntervalStringException("Failed to parse interval (" . $duration . ")");
}
$this->y = $parsed["y"];
$this->m = $parsed["m"];
$this->d = $parsed["d"];
$this->h = $parsed["h"];
$this->i = $parsed["i"];
$this->s = $parsed["s"];
$this->f = $parsed["us"] / 1000000.0;
$this->invert = $parsed["invert"];
if ($parsed["days"] !== -9999999) {
    $this->days = $parsed["days"];
}
$this->_from_string = false;
$this->_date_string = "";
$this->_period_from_string = false;
$this->_period_date_string = "";
$this->_wall = true;
"#;

/// Builds DateInterval::__construct(), selecting the exact timelib body whenever
/// the timezone bridge prelude is present.
pub(super) fn date_interval_constructor(uses_timelib: bool) -> ClassMethod {
    if !uses_timelib {
        return date_interval_legacy_constructor();
    }
    let tokens = crate::lexer::tokenize(DATEINTERVAL_TIMELIB_CONSTRUCTOR_SRC)
        .expect("timelib DateInterval constructor body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("timelib DateInterval constructor body must parse");
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
pub(super) fn interval_property(name: &str) -> ClassProperty {
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
pub(super) const CREATE_FROM_DATE_STRING_SRC: &str = r#"<?php
$iv = new DateInterval("PT0S");
$iv->_from_string = true;
$iv->_date_string = $datetime;
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
pub(super) fn date_interval_legacy_create_from_date_string() -> ClassMethod {
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

/// Timelib-backed DateInterval::createFromDateString() body. The parser result
/// preserves php-src's special weekday/first-last rules through `_date_string`.
pub(super) const DATEINTERVAL_TIMELIB_FROM_STRING_SRC: &str = r#"<?php
$parsed = __elephc_timelib_interval_parse($datetime, true);
if ($parsed["status"] === "E") {
    throw new DateMalformedIntervalStringException(
        "Unknown or bad format (" . $datetime . ") at position "
        . $parsed["position"] . " (" . $parsed["character"] . "): " . $parsed["message"]
    );
}
if ($parsed["status"] === "N") {
    throw new DateMalformedIntervalStringException(
        "String '" . $datetime . "' contains non-relative elements"
    );
}
$iv = new DateInterval("PT0S");
$iv->y = $parsed["y"];
$iv->m = $parsed["m"];
$iv->d = $parsed["d"];
$iv->h = $parsed["h"];
$iv->i = $parsed["i"];
$iv->s = $parsed["s"];
$iv->f = $parsed["us"] / 1000000.0;
$iv->invert = $parsed["invert"];
$iv->days = false;
$iv->_from_string = true;
$iv->_date_string = $datetime;
$iv->_wall = false;
return $iv;
"#;

/// Builds the exact timelib-backed createFromDateString() when available.
pub(super) fn date_interval_create_from_date_string(uses_timelib: bool) -> ClassMethod {
    if !uses_timelib {
        return date_interval_legacy_create_from_date_string();
    }
    let tokens = crate::lexer::tokenize(DATEINTERVAL_TIMELIB_FROM_STRING_SRC)
        .expect("timelib createFromDateString body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("timelib createFromDateString body must parse");
    ClassMethod {
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

/// PHP source for the procedural alias, which warns and returns false instead
/// of propagating DateMalformedIntervalStringException.
pub(super) const DATEINTERVAL_PROCEDURAL_FROM_STRING_SRC: &str = r#"<?php
try {
    return DateInterval::createFromDateString($datetime);
} catch (DateMalformedIntervalStringException $exception) {
    __elephc_diag_warning(
        "\nWarning: date_interval_create_from_date_string(): "
        . $exception->getMessage(),
        $sourceLine
    );
    return false;
}
"#;

/// Builds the hidden procedural DateInterval parser wrapper.
pub(super) fn date_interval_procedural_from_date_string() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEINTERVAL_PROCEDURAL_FROM_STRING_SRC)
        .expect("procedural DateInterval parser body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("procedural DateInterval parser body must parse");
    ClassMethod {
        name: "__elephc_create_from_date_string".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("datetime".to_string(), Some(TypeExpr::Str), None, false),
            ("sourceLine".to_string(), Some(TypeExpr::Int), None, false),
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

/// PHP source that serializes the current, user-mutable DateInterval components
/// plus any retained free-form specification for exact timelib arithmetic.
pub(super) const DATEINTERVAL_PAYLOAD_SRC: &str = r#"<?php
$microseconds = (int)round($this->f * 1000000.0);
$days = $this->days === false ? -9999999 : $this->days;
$fields = $this->y . "\t" . $this->m . "\t" . $this->d . "\t"
    . $this->h . "\t" . $this->i . "\t" . $this->s . "\t"
    . $microseconds . "\t" . $this->invert . "\t" . $days;
if ($this->_from_string || $this->_period_from_string) {
    $dateString = $this->_from_string ? $this->_date_string : $this->_period_date_string;
    return "R" . strlen($dateString) . "\t" . $dateString . "\t" . $fields;
}
return ($this->_wall ? "W\t" : "C\t") . $fields;
"#;

/// Builds the hidden interval-payload method consumed by DateTime add/sub.
pub(super) fn date_interval_payload() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEINTERVAL_PAYLOAD_SRC)
        .expect("DateInterval payload body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval payload body must parse");
    method(
        "__elephc_payload",
        Vec::new(),
        Some(TypeExpr::Str),
        body,
    )
}

/// Builds the hidden marker used by DateTime::diff() to select timelib's civil
/// arithmetic rather than the wall arithmetic used by ISO constructor intervals.
pub(super) fn date_interval_mark_civil() -> ClassMethod {
    method(
        "__elephc_mark_civil",
        Vec::new(),
        Some(TypeExpr::Void),
        vec![assign_this_property(
            "_wall",
            Expr::new(ExprKind::BoolLiteral(false), dummy()),
        )],
    )
}

/// Builds the hidden strongly-typed clone hook used by DatePeriod, avoiding a
/// nullable backing-property clone at the caller.
pub(super) fn date_interval_clone() -> ClassMethod {
    method(
        "__elephc_clone",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        vec![return_expr(Expr::new(
            ExprKind::Clone(Box::new(Expr::new(ExprKind::This, dummy()))),
            dummy(),
        ))],
    )
}

/// Builds the handleless clone used exclusively for DatePeriod's private interval backing state.
pub(super) fn date_interval_clone_storage() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        "<?php return __elephc_object_clone_internal($this);",
    )
    .expect("DateInterval storage clone body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval storage clone body must parse");
    method(
        "__elephc_clone_storage",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        body,
    )
}

/// Builds DatePeriod's interval snapshot: php-src hides the relative-string
/// debug/serialization shape while retaining that string for calendar stepping.
pub(super) fn date_interval_clone_for_period() -> ClassMethod {
    let source = r#"<?php
$interval = clone $this;
if ($interval->_from_string) {
    $interval->_period_from_string = true;
    $interval->_period_date_string = $interval->_date_string;
    $interval->_from_string = false;
    $interval->_date_string = "";
}
return $interval;
"#;
    let tokens = crate::lexer::tokenize(source)
        .expect("DateInterval DatePeriod snapshot body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval DatePeriod snapshot body must parse");
    method(
        "__elephc_clone_interval_for_period",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        body,
    )
}

/// Builds DatePeriod's handleless interval snapshot while preserving relative-step metadata.
pub(super) fn date_interval_clone_for_period_storage() -> ClassMethod {
    let source = r#"<?php
$interval = __elephc_object_clone_internal($this);
if ($interval->_from_string) {
    $interval->_period_from_string = true;
    $interval->_period_date_string = $interval->_date_string;
    $interval->_from_string = false;
    $interval->_date_string = "";
}
return $interval;
"#;
    let tokens = crate::lexer::tokenize(source)
        .expect("DateInterval handleless DatePeriod snapshot body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval handleless DatePeriod snapshot body must parse");
    method(
        "__elephc_clone_interval_for_period_storage",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        body,
    )
}

/// Builds the hidden exact-type clone boundary used by DatePeriod construction.
///
/// Keeping the clone in the concrete class avoids the temporary objects created
/// by `createFromInterface()` and preserves php-src's observable object-handle
/// allocation order.
pub(super) fn datetime_clone_for_period(class_name: &str) -> ClassMethod {
    method(
        "__elephc_clone_for_period",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        vec![return_expr(Expr::new(
            ExprKind::Clone(Box::new(Expr::new(
                ExprKind::This,
                dummy(),
            ))),
            dummy(),
        ))],
    )
}

/// Builds the handleless exact-type clone used for DatePeriod's private date backing state.
pub(super) fn datetime_clone_for_period_storage(class_name: &str) -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        "<?php return __elephc_object_clone_internal($this);",
    )
    .expect("DateTime storage clone body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime storage clone body must parse");
    method(
        "__elephc_clone_for_period_storage",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// Builds the hidden static wrapper used by the procedural `date_add()` alias.
pub(super) fn datetime_procedural_add() -> ClassMethod {
    let source = format!(
        "<?php\n{}return $object->add($interval);\n",
        date_interval_type_guard("date_add()", 2),
    );
    let tokens = crate::lexer::tokenize(&source)
        .expect("procedural date_add body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("procedural date_add body must parse");
    datetime_procedural_interval_method("__elephc_date_add", body)
}

/// Builds the hidden static wrapper used by the procedural date_sub() alias.
pub(super) fn datetime_procedural_sub() -> ClassMethod {
    let source = format!(
        r#"<?php
{}
try {{
    return $object->sub($interval);
}} catch (DateInvalidOperationException $exception) {{
    __elephc_diag_warning(
        "\nWarning: date_sub(): Only non-special relative time specifications are supported for subtraction",
        $sourceLine,
        E_WARNING
    );
    return $object;
}}
"#,
        date_interval_type_guard("date_sub()", 2),
    );
    let tokens = crate::lexer::tokenize(&source)
        .expect("procedural date_sub body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("procedural date_sub body must parse");
    datetime_procedural_interval_method("__elephc_date_sub", body)
}

/// Builds one hidden procedural `date_add()`/`date_sub()` wrapper with a runtime-checked interval.
pub(super) fn datetime_procedural_interval_method(name: &str, body: Vec<Stmt>) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "object".to_string(),
                Some(TypeExpr::Named(Name::unqualified("DateTime"))),
                None,
                false,
            ),
            (
                "interval".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            ),
            (
                "sourceLine".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(0), dummy())),
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("DateTime"))),
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
pub(super) fn date_interval_format() -> ClassMethod {
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
pub(super) fn datetime_diff_method(uses_timelib: bool) -> ClassMethod {
    if uses_timelib {
        return datetime_timelib_diff_method();
    }
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
            Stmt::new(
                StmtKind::ExprStmt(Expr::new(
                    ExprKind::MethodCall {
                        object: Box::new(iv_var()),
                        method: "__elephc_mark_civil".to_string(),
                        args: Vec::new(),
                    },
                    dummy(),
                )),
                dummy(),
            ),
            return_expr(iv_var()),
        ],
    )
}

/// Builds `DateTimeInterface::diff()` directly on the vendored php-src timelib implementation.
pub(super) fn datetime_timelib_diff_method() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
$leftTimestamp = $this->timestamp;
$leftMicrosecond = $this->microsecond;
$leftTimezone = $this->timezone_name;
$rightTimestamp = $targetObject->getTimestamp();
$rightMicrosecond = $targetObject->getMicrosecond();
$rightTimezone = $targetObject->format("e");
$parsed = __elephc_timelib_diff(
    $leftTimestamp,
    $leftMicrosecond,
    $leftTimezone,
    $rightTimestamp,
    $rightMicrosecond,
    $rightTimezone
);
$interval = new DateInterval("PT0S");
$interval->y = $parsed["y"];
$interval->m = $parsed["m"];
$interval->d = $parsed["d"];
$interval->h = $parsed["h"];
$interval->i = $parsed["i"];
$interval->s = $parsed["s"];
$interval->f = $parsed["us"] / 1000000.0;
$interval->invert = $parsed["invert"];
if ($absolute) {
    $interval->invert = 0;
}
$interval->days = $parsed["days"];
$interval->__elephc_mark_civil();
return $interval;
"#,
    )
    .expect("timelib DateTime diff body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("timelib DateTime diff body must parse");
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
        body,
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
        let mut properties = vec![
            interval_property("y"),
            interval_property("m"),
            interval_property("d"),
            interval_property("h"),
            interval_property("i"),
            interval_property("s"),
            // `f` stores the interval fraction as PHP's 0.0..1.0 public value.
            property("f", TypeExpr::Float, Expr::new(ExprKind::FloatLiteral(0.0), dummy())),
            interval_property("invert"),
            interval_property("days"),
            // PHP exposes these values only in debug/serialization state; they are not
            // declared readable properties in php-src's class stub.
            private_property(
                "_from_string",
                TypeExpr::Bool,
                Expr::new(ExprKind::BoolLiteral(false), dummy()),
            ),
            private_property(
                "_date_string",
                TypeExpr::Str,
                Expr::new(ExprKind::StringLiteral(String::new()), dummy()),
            ),
            // DatePeriod snapshots hide the relative-string public shape while
            // retaining the specification needed for special weekday arithmetic.
            private_property(
                "_period_from_string",
                TypeExpr::Bool,
                Expr::new(ExprKind::BoolLiteral(false), dummy()),
            ),
            private_property(
                "_period_date_string",
                TypeExpr::Str,
                Expr::new(ExprKind::StringLiteral(String::new()), dummy()),
            ),
            // ISO constructor intervals use timelib wall arithmetic; relative-string and
            // diff intervals use civil arithmetic.
            private_property(
                "_wall",
                TypeExpr::Bool,
                Expr::new(ExprKind::BoolLiteral(true), dummy()),
            ),
            date_object_initialized_property(),
        ];
        properties.extend(date_constructor_unpack_properties());
        let mut methods = vec![
            date_interval_constructor(uses_tz_introspection),
            date_interval_format(),
            date_interval_create_from_date_string(uses_tz_introspection),
            date_interval_procedural_from_date_string(),
            date_interval_payload(),
            date_interval_mark_civil(),
            date_interval_clone(),
            date_interval_clone_storage(),
            date_interval_clone_for_period(),
            date_interval_clone_for_period_storage(),
            dateinterval_magic_get(),
            dateinterval_wakeup(),
            dateinterval_serialize(),
            dateinterval_unserialize(uses_tz_introspection),
            dateinterval_set_state(),
            dateinterval_debug_dump(),
            dateinterval_print_r_dump(),
        ];
        methods.extend(date_constructor_unpack_methods(
            "DateInterval",
            &["duration"],
        ));
        methods.push(date_object_is_initialized());
        methods.push(date_object_assert_initialized("DateInterval"));
        guard_date_object_instance_methods(&mut methods);
        class_map.insert(
            "DateInterval".to_string(),
            FlattenedClass {
                name: "DateInterval".to_string(),
                span: dummy(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties,
                methods,
                attributes: Vec::new(),
                constants: Vec::new(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
            },
        );
    }

    if !class_map.contains_key("DateTimeZone") {
        let mut properties = vec![
            private_property(
                "name",
                TypeExpr::Str,
                Expr::new(ExprKind::StringLiteral("UTC".to_string()), dummy()),
            ),
            date_object_initialized_property(),
        ];
        properties.extend(date_constructor_unpack_properties());
        class_map.insert(
            "DateTimeZone".to_string(),
            FlattenedClass {
                name: "DateTimeZone".to_string(),
                span: dummy(),
                extends: None,
                implements: Vec::new(),
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties,
                methods: {
                    let mut methods = vec![
                        datetime_zone_normalize_timezone(),
                        datetime_zone_constructor(),
                        datetime_zone_procedural_open(),
                        datetime_zone_get_name(),
                        datetime_zone_get_offset(),
                        datetime_zone_list_identifiers(),
                        datetime_zone_compare(),
                    ];
                    methods.extend(date_constructor_unpack_methods(
                        "DateTimeZone",
                        &["timezone"],
                    ));
                    methods.extend(datetimezone_serialize_methods());
                    methods.push(datetimezone_debug_dump());
                    methods.push(datetimezone_print_r_dump());
                    // getLocation/getTransitions/listAbbreviations call the
                    // tz_prelude marshalling helpers, which are only declared when
                    // the introspection prelude is injected. Adding them
                    // unconditionally would make every DateTimeZone program
                    // reference (and link) the elephc_tz bridge, since method
                    // bodies are type-checked eagerly. So they are gated on the
                    // prelude's presence.
                    if uses_tz_introspection {
                        methods.push(datetime_zone_get_location());
                        methods.push(datetime_zone_get_transitions_flat());
                        methods.push(datetime_zone_list_abbreviations());
                    }
                    methods.push(date_object_assert_initialized("DateTimeZone"));
                    guard_date_object_instance_methods(&mut methods);
                    methods
                },
                attributes: Vec::new(),
                constants: datetime_zone_group_constants(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
            },
        );
    }

    if !class_map.contains_key("DateTimeImmutable") {
        class_map.insert(
            "DateTimeImmutable".to_string(),
            FlattenedClass {
                name: "DateTimeImmutable".to_string(),
                span: dummy(),
                extends: None,
                implements: vec!["DateTimeInterface".to_string()],
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties: datetime_backing_properties(),
                methods: {
                    let mut m = datetime_shared_methods(uses_tz_introspection);
                    m.extend(datetime_setter_methods(
                        false,
                        "DateTimeImmutable",
                        uses_tz_introspection,
                    ));
                    m.push(datetime_create_from_format(
                        "DateTimeImmutable",
                        uses_tz_introspection,
                    ));
                    m.push(datetime_get_last_errors(uses_tz_introspection));
                    m.push(datetime_create_from_timestamp("DateTimeImmutable"));
                    m.push(datetime_create_from_object(
                        "createFromInterface",
                        "DateTimeInterface",
                        "DateTimeImmutable",
                    ));
                    m.push(datetime_create_from_object(
                        "createFromMutable",
                        "DateTime",
                        "DateTimeImmutable",
                    ));
                    let mut set_iso_date = datetime_set_isodate("DateTimeImmutable");
                    set_iso_date.attributes = no_discard_attribute("setISODate");
                    m.push(set_iso_date);
                    m.push(datetime_date_create("DateTimeImmutable"));
                    m.extend(datetime_serialize_methods("DateTimeImmutable"));
                    m.push(datetime_debug_dump("DateTimeImmutable"));
                    m.push(datetime_print_r_dump());
                    m.push(datetime_clone_for_period("DateTimeImmutable"));
                    m.push(datetime_clone_for_period_storage("DateTimeImmutable"));
                    m.extend(date_constructor_unpack_methods(
                        "DateTimeImmutable",
                        &["datetime", "timezone"],
                    ));
                    m.push(date_object_is_initialized());
                    m.push(date_object_assert_initialized("DateTimeImmutable"));
                    m.push(datetime_assert_comparable());
                    m.push(datetime_compare());
                    guard_date_object_instance_methods(&mut m);
                    m
                },
                attributes: Vec::new(),
                constants: Vec::new(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
            },
        );
    }

    if !class_map.contains_key("DateTime") {
        let mut methods = datetime_shared_methods(uses_tz_introspection);
        methods.extend(datetime_setter_methods(
            true,
            "DateTime",
            uses_tz_introspection,
        ));
        methods.push(datetime_create_from_format("DateTime", uses_tz_introspection));
        methods.push(datetime_get_last_errors(uses_tz_introspection));
        methods.push(datetime_create_from_timestamp("DateTime"));
        methods.push(datetime_create_from_object(
            "createFromInterface",
            "DateTimeInterface",
            "DateTime",
        ));
        methods.push(datetime_create_from_object(
            "createFromImmutable",
            "DateTimeImmutable",
            "DateTime",
        ));
        methods.push(datetime_set_isodate("DateTime"));
        methods.push(datetime_date_parse_from_format(uses_tz_introspection));
        methods.push(datetime_date_parse(uses_tz_introspection));
        methods.push(datetime_gettimeofday());
        methods.push(datetime_idate());
        methods.push(datetime_timezone_type());
        methods.push(datetime_runtime_timezone_name());
        methods.push(datetime_date_create("DateTime"));
        methods.extend(datetime_serialize_methods("DateTime"));
        methods.push(datetime_debug_dump("DateTime"));
        methods.push(datetime_print_r_dump());
        methods.push(datetime_clone_for_period("DateTime"));
        methods.push(datetime_clone_for_period_storage("DateTime"));
        methods.extend(date_constructor_unpack_methods(
            "DateTime",
            &["datetime", "timezone"],
        ));
        methods.push(datetime_date_modify());
        methods.push(datetime_procedural_set_timestamp());
        methods.push(datetime_procedural_add());
        methods.push(datetime_procedural_sub());
        methods.push(datetime_strftime());
        methods.push(datetime_extract_micros());
        methods.push(datetime_strip_micros());
        methods.push(datetime_extract_constructor_zone());
        methods.push(datetime_extract_modify_micros());
        methods.push(datetime_strip_modify_micros());
        methods.push(datetime_malformed_time_message());
        methods.push(datetime_sun_rs());
        methods.push(datetime_sun_val());
        methods.push(datetime_sun_info());
        methods.push(datetime_sunfunc());
        methods.push(datetime_strptime());
        methods.push(datetime_tz_name_from_abbr());
        methods.push(datetime_argument_type_error());
        methods.push(datetime_weak_string_argument());
        methods.push(deprecated_constant_passthrough(
            "__elephc_deprecated_string_constant",
            TypeExpr::Str,
        ));
        methods.push(deprecated_constant_passthrough(
            "__elephc_deprecated_int_constant",
            TypeExpr::Int,
        ));
        methods.extend(super::calendar::calendar_methods());
        methods.push(date_object_is_initialized());
        methods.push(date_object_assert_initialized("DateTime"));
        methods.push(datetime_assert_comparable());
        methods.push(datetime_compare());
        guard_date_object_instance_methods(&mut methods);
        class_map.insert(
            "DateTime".to_string(),
            FlattenedClass {
                name: "DateTime".to_string(),
                span: dummy(),
                extends: None,
                implements: vec!["DateTimeInterface".to_string()],
                is_abstract: false,
                is_final: false,
                is_readonly_class: false,
                properties: datetime_backing_properties(),
                methods,
                attributes: Vec::new(),
                constants: Vec::new(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
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
pub(super) fn date_exception_subclass(name: &str, parent: &str) -> FlattenedClass {
    FlattenedClass {
        name: name.to_string(),
        span: dummy(),
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
        trait_aliases: Vec::new(),
    }
}

/// Injects the PHP 8.3 date/time exception hierarchy.
///
/// `DateError` and its subclasses (`DateObjectError`, `DateRangeError`) extend `Error`; the
/// `DateException` family (`DateInvalidTimeZoneException`, `DateInvalidOperationException`, and the
/// `DateMalformed*` string/interval/period exceptions) extend `Exception`. `Error`/`Exception` are
/// already registered by `inject_builtin_throwables`, which runs before this. User declarations of
/// the same names are left untouched.
pub(super) fn inject_builtin_date_exceptions(class_map: &mut HashMap<String, FlattenedClass>) {
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
    ] {
        if !class_map.contains_key(name) {
            class_map.insert(name.to_string(), date_exception_subclass(name, parent));
        }
    }
}
