//! Purpose:
//! DateTime factory, cross-conversion, last-error, and ISO-week method construction.
//!
//! Called from:
//! - DateTime and DateTimeImmutable declaration injection.
//!
//! Key details:
//! - Concrete class names are substituted into self-contained parsed PHP bodies.

use super::*;

/// PHP source backing `getLastErrors()` / `date_get_last_errors()`. Returns PHP's structured result
/// array; elephc tracks only whether the last `createFromFormat()` on this class failed
/// (`error_count` 0/1, no warnings), which covers the common
/// `if (DateTime::getLastErrors()['error_count'])` check after a parse.
#[cfg(test)]
pub(super) const GET_LAST_ERRORS_SRC: &str = r#"<?php
$ec = __GLE_CLASS__::$lastErrorCount;
$errs = [];
if ($ec > 0) { $errs = [0 => "The date string failed to match the format"]; }
return ["warning_count" => 0, "warnings" => [], "error_count" => $ec, "errors" => $errs];
"#;

/// Builds the static `getLastErrors(): array` method for `class_name`, reading the per-class
/// `lastErrorCount` static that `createFromFormat()` sets (1 on entry, cleared to 0 on success).
pub(super) fn datetime_get_last_errors(class_name: &str) -> ClassMethod {
    let body = super::bodies::get_last_errors(class_name);
    ClassMethod {
        name: "getLastErrors".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![],
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

/// PHP source backing the cross-conversion factories (`createFromInterface`,
/// `createFromImmutable`, `createFromMutable`): clone the source object's timelib state into
/// a fresh target-class instance. `createFromTimestamp()` deliberately starts in GMT, so an
/// absent timezone remains absent instead of becoming an explicit UTC zone. `__TARGET__` is
/// substituted with the target class name.
#[cfg(test)]
pub(super) const CREATE_FROM_OBJECT_SRC: &str = r#"<?php
$d = __TARGET__::createFromTimestamp($object->getTimestamp());
$timezone = $object->getTimezone();
if ($timezone !== false) {
    $d = $d->setTimezone($timezone);
}
return $d;
"#;

/// Builds a cross-conversion factory (`createFromInterface` / `createFromImmutable` /
/// `createFromMutable`) returning a fresh `target_class` that carries the source object's
/// instant and timezone. Static; the body is the parsed `CREATE_FROM_OBJECT_SRC`. `$object`
/// is typed `DateTimeInterface` (the common supertype) because the body only needs interface
/// methods; the return type is declared explicitly as `target_class` since synthetic builtin
/// methods do not get body-driven return-type inference.
pub(super) fn datetime_create_from_object(method_name: &str, target_class: &str) -> ClassMethod {
    let body = super::bodies::create_from_object(target_class);
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "object".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
            None,
            false,
        )],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified(target_class))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `createFromTimestamp(int|float $timestamp): static` (PHP 8.4): build a fresh
/// instance set to the given UNIX timestamp. `__CFT_CLASS__` is substituted with the class name.
#[cfg(test)]
pub(super) const CREATE_FROM_TIMESTAMP_SRC: &str = r#"<?php
$d = new __CFT_CLASS__();
$secs = intval(floor($timestamp));
$d = $d->setTimestamp($secs);
$d->__elephc_is_localtime = true;
$d = $d->setMicrosecond(intval(round(($timestamp - $secs) * 1000000)));
return $d;
"#;

/// Builds the static `createFromTimestamp($timestamp): static` factory for `class_name`. `$timestamp`
/// is typed `mixed` (PHP accepts int or float). The whole-second part uses `floor()` (so negative
/// fractional timestamps round toward -inf like PHP) and the remaining fraction becomes microseconds
/// via `setMicrosecond()`. Self-contained parsed source; the return type is declared as `class_name`
/// since synthetic builtin methods get no body-driven return inference.
pub(super) fn datetime_create_from_timestamp(class_name: &str) -> ClassMethod {
    let body = super::bodies::create_from_timestamp(class_name);
    ClassMethod {
        name: "createFromTimestamp".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "timestamp".to_string(),
            Some(TypeExpr::Named(Name::unqualified("mixed"))),
            None,
            false,
        )],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified(class_name))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `setISODate()`. Computes the Gregorian date for an ISO 8601 week date
/// (year, week, day-of-week 1=Monday..7=Sunday), preserving the current time-of-day. ISO week 1
/// contains Jan 4, so the Monday of week 1 is `Jan 4 - (weekday(Jan 4) - 1)`; the target day is
/// that plus `(week - 1) * 7 + (dayOfWeek - 1)`, fed to `mktime()` which normalizes overflow
/// (e.g. week 53 of a 52-week year rolls into the next year). Delegates to `$this->setTimestamp()`
/// so the mutable/immutable result and timezone handling are shared with the other setters.
#[cfg(test)]
pub(super) const SET_ISODATE_SRC: &str = r#"<?php
$h = (int)date("H", $this->timestamp);
$mi = (int)date("i", $this->timestamp);
$se = (int)date("s", $this->timestamp);
$jan4 = __elephc_mktime_raw($h, $mi, $se, 1, 4, $year);
$dow = (int)date("N", $jan4);
$day = 4 - ($dow - 1) + ($week - 1) * 7 + ($dayOfWeek - 1);
return $this->setTimestamp(__elephc_mktime_raw($h, $mi, $se, 1, $day, $year));
"#;

/// `setISODate(int $year, int $week, int $dayOfWeek = 1): static` — set the date from an ISO 8601
/// week date, keeping the time-of-day. The body is the parsed `SET_ISODATE_SRC`; the return type
/// is declared as `class_name` since synthetic methods do not get body-driven return inference.
pub(super) fn datetime_set_isodate(class_name: &str) -> ClassMethod {
    let body = super::bodies::set_isodate();
    ClassMethod {
        name: "setISODate".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("year".to_string(), Some(TypeExpr::Int), None, false),
            ("week".to_string(), Some(TypeExpr::Int), None, false),
            (
                "dayOfWeek".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(1), dummy())),
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified(class_name))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}
