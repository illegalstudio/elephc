//! Purpose:
//! Synthetic DateTime mutation and immutable-transform method construction.
//!
//! Called from:
//! - DateTime and DateTimeImmutable declaration injection.
//!
//! Key details:
//! - Both class variants share operations while retaining their return/mutation contract.

use super::*;

/// `setTimestamp(int $timestamp)` — sets the stored UNIX timestamp.
pub(super) fn make_set_timestamp(mutable: bool, class_name: &str) -> ClassMethod {
    method(
        "setTimestamp",
        vec![("timestamp".to_string(), Some(TypeExpr::Int), None, false)],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        result_tail(
            Expr::new(ExprKind::Variable("timestamp".to_string()), dummy()),
            mutable,
            class_name,
        ),
    )
}

/// `setMicrosecond(int $microsecond): static` — sets the sub-second component. Mutable updates
/// `$this` in place; immutable returns a fresh instance carrying the same instant/zone with the new
/// micros (the instant in seconds is unchanged).
pub(super) fn make_set_microsecond(mutable: bool, class_name: &str) -> ClassMethod {
    let us = || Expr::new(ExprKind::Variable("microsecond".to_string()), dummy());
    let body = if mutable {
        vec![
            assign_this_property("microsecond", us()),
            return_expr(Expr::new(ExprKind::This, dummy())),
        ]
    } else {
        let new_var = || Expr::new(ExprKind::Variable("__new".to_string()), dummy());
        let prop_assign = |property: &str, value: Expr| {
            Stmt::new(
                StmtKind::PropertyAssign { object: Box::new(new_var()), property: property.to_string(), value },
                dummy(),
            )
        };
        vec![
            Stmt::assign(
                "__new",
                Expr::new(
                    ExprKind::NewObject { class_name: Name::unqualified(class_name), args: Vec::new() },
                    dummy(),
                ),
            ),
            prop_assign("timestamp", this_property("timestamp")),
            prop_assign("timezone_name", this_property("timezone_name")),
            prop_assign("microsecond", us()),
            return_expr(new_var()),
        ]
    };
    method(
        "setMicrosecond",
        vec![("microsecond".to_string(), Some(TypeExpr::Int), None, false)],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// `setTime(int $hour, int $minute, int $second = 0, int $microsecond = 0)` — keeps the date,
/// replaces the time-of-day and sub-second component (PHP 8.4+).
pub(super) fn make_set_time(mutable: bool, class_name: &str) -> ClassMethod {
    let mut body = vec![
        Stmt::assign("__y", date_component_int("Y")),
        Stmt::assign("__mo", date_component_int("n")),
        Stmt::assign("__d", date_component_int("j")),
    ];
    body.extend(result_tail_micro(
        mktime_call(["hour", "minute", "second", "__mo", "__d", "__y"]),
        Some(Expr::new(ExprKind::Variable("microsecond".to_string()), dummy())),
        mutable,
        class_name,
    ));
    method(
        "setTime",
        vec![
            ("hour".to_string(), Some(TypeExpr::Int), None, false),
            ("minute".to_string(), Some(TypeExpr::Int), None, false),
            (
                "second".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(0), dummy())),
                false,
            ),
            (
                "microsecond".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(0), dummy())),
                false,
            ),
        ],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// `setDate(int $year, int $month, int $day)` — keeps the time-of-day, replaces the calendar date.
pub(super) fn make_set_date(mutable: bool, class_name: &str) -> ClassMethod {
    let mut body = vec![
        Stmt::assign("__h", date_component_int("G")),
        Stmt::assign("__mi", date_component_int("i")),
        Stmt::assign("__s", date_component_int("s")),
    ];
    body.extend(result_tail(
        mktime_call(["__h", "__mi", "__s", "month", "day", "year"]),
        mutable,
        class_name,
    ));
    method(
        "setDate",
        vec![
            ("year".to_string(), Some(TypeExpr::Int), None, false),
            ("month".to_string(), Some(TypeExpr::Int), None, false),
            ("day".to_string(), Some(TypeExpr::Int), None, false),
        ],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// Builds a `$var->property` access expression.
pub(super) fn var_property(var: &str, property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::Variable(var.to_string()), dummy())),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// `setTimezone(DateTimeZone $timezone)` — stores the zone identifier (keeps the timestamp).
///
/// Reads the public `DateTimeZone::$name`. `DateTime` mutates `$this`; `DateTimeImmutable`
/// returns a fresh instance with the same timestamp and the new timezone name.
pub(super) fn make_set_timezone(mutable: bool, class_name: &str) -> ClassMethod {
    let tz_name = var_property("timezone", "name");
    let body = if mutable {
        vec![
            assign_this_property("timezone_name", tz_name),
            return_expr(Expr::new(ExprKind::This, dummy())),
        ]
    } else {
        let new_var = || Expr::new(ExprKind::Variable("__new".to_string()), dummy());
        vec![
            Stmt::assign(
                "__new",
                Expr::new(
                    ExprKind::NewObject {
                        class_name: Name::unqualified(class_name),
                        args: Vec::new(),
                    },
                    dummy(),
                ),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timestamp".to_string(),
                    value: this_property("timestamp"),
                },
                dummy(),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timezone_name".to_string(),
                    value: tz_name,
                },
                dummy(),
            ),
            return_expr(new_var()),
        ]
    };
    method(
        "setTimezone",
        vec![(
            "timezone".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeZone"))),
            None,
            false,
        )],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// `add(DateInterval $interval)` / `sub(DateInterval $interval)` — shifts the date by the interval.
///
/// Decomposes `$this->timestamp` into calendar components via `date()`, applies each signed interval
/// component, then recomposes with `mktime()` (which normalizes overflow — e.g. day 32 rolls into the
/// next month). `$interval->invert` flips the direction (`$__sign` = `1 - 2*invert` for `add`, negated
/// for `sub`). `DateTime` mutates `$this`; `DateTimeImmutable` returns a fresh instance via
/// `result_tail`. `is_add` selects `add` (true) vs `sub` (false).
pub(super) fn make_add_sub(name: &str, mutable: bool, class_name: &str, is_add: bool) -> ClassMethod {
    let bin = |l: Expr, op: BinOp, r: Expr| {
        Expr::new(ExprKind::BinaryOp { left: Box::new(l), op, right: Box::new(r) }, dummy())
    };
    let int_lit = |n: i64| Expr::new(ExprKind::IntLiteral(n), dummy());
    let sign_var = || Expr::new(ExprKind::Variable("__sign".to_string()), dummy());

    // $__sign = 1 - 2*$interval->invert  (add)  |  2*$interval->invert - 1  (sub)
    let two_invert = bin(int_lit(2), BinOp::Mul, var_property("interval", "invert"));
    let sign_expr = if is_add {
        bin(int_lit(1), BinOp::Sub, two_invert)
    } else {
        bin(two_invert, BinOp::Sub, int_lit(1))
    };

    // component(fmt, field) = (int)date(fmt, $this->timestamp) + $interval-><field> * $__sign
    let component = |fmt: &str, field: &str| {
        bin(
            date_component_int(fmt),
            BinOp::Add,
            bin(var_property("interval", field), BinOp::Mul, sign_var()),
        )
    };

    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    // $__ivu = (int) round($interval->f * 1000000) — the interval's whole microseconds.
    let interval_micros = Expr::new(
        ExprKind::Cast {
            target: crate::parser::ast::CastType::Int,
            expr: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("round"),
                    args: vec![bin(
                        var_property("interval", "f"),
                        BinOp::Mul,
                        Expr::new(ExprKind::FloatLiteral(1_000_000.0), dummy()),
                    )],
                },
                dummy(),
            )),
        },
        dummy(),
    );
    // One-second carry/borrow: $__micro stays in [0, 1000000); the carry folds into $__s
    // (which mktime() then normalizes). $__micro is bounded to a single carry by construction.
    let carry_up = Stmt::new(
        StmtKind::If {
            condition: bin(var("__micro"), BinOp::GtEq, int_lit(1_000_000)),
            then_body: vec![
                Stmt::assign("__micro", bin(var("__micro"), BinOp::Sub, int_lit(1_000_000))),
                Stmt::assign("__s", bin(var("__s"), BinOp::Add, int_lit(1))),
            ],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy(),
    );
    let borrow_down = Stmt::new(
        StmtKind::If {
            condition: bin(var("__micro"), BinOp::Lt, int_lit(0)),
            then_body: vec![
                Stmt::assign("__micro", bin(var("__micro"), BinOp::Add, int_lit(1_000_000))),
                Stmt::assign("__s", bin(var("__s"), BinOp::Sub, int_lit(1))),
            ],
            elseif_clauses: Vec::new(),
            else_body: None,
        },
        dummy(),
    );
    let mut body = vec![
        Stmt::assign("__sign", sign_expr),
        Stmt::assign("__y", component("Y", "y")),
        Stmt::assign("__mo", component("n", "m")),
        Stmt::assign("__d", component("j", "d")),
        Stmt::assign("__h", component("G", "h")),
        Stmt::assign("__mi", component("i", "i")),
        Stmt::assign("__s", component("s", "s")),
        // Apply the interval's fractional second: $__micro = $this->microsecond ± interval µs.
        Stmt::assign("__ivu", interval_micros),
        Stmt::assign(
            "__micro",
            bin(
                this_property("microsecond"),
                BinOp::Add,
                bin(var("__ivu"), BinOp::Mul, sign_var()),
            ),
        ),
        carry_up,
        borrow_down,
    ];
    body.extend(result_tail_micro(
        mktime_call(["__h", "__mi", "__s", "__mo", "__d", "__y"]),
        Some(var("__micro")),
        mutable,
        class_name,
    ));
    method(
        name,
        vec![(
            "interval".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
            None,
            false,
        )],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// `modify(string $modifier)` — applies a relative date/time modifier (e.g. `"+1 day"`,
/// `"-2 weeks"`, `"14:30"`) by re-parsing it against the object's current timestamp via
/// `strtotime($modifier, $this->timestamp)`. Mutates in place for `DateTime` and returns a
/// new instance for `DateTimeImmutable`. Supports exactly the forms `strtotime()` accepts.
/// The PHP the `modify()` preamble used to be parsed from. Test-only: the compilation
/// path builds it with `bodies::modify_preamble`, and the oracle checks the two agree.
#[cfg(test)]
pub(super) const MODIFY_PREAMBLE_SRC: &str = r#"<?php
$__md = DateTime::__elephc_extract_modify_micros($modifier);
$__rest = DateTime::__elephc_strip_modify_micros($modifier);
if ($__rest === "") {
    $__ts = $this->timestamp;
} else {
    $__ts = strtotime($__rest, $this->timestamp);
    if ($__ts === false) {
        throw new DateMalformedStringException("Failed to parse time string (" . $modifier . ")");
    }
}
$__micro = $this->microsecond + $__md;
$__carry = intdiv($__micro, 1000000);
$__micro = $__micro - $__carry * 1000000;
if ($__micro < 0) {
    $__micro = $__micro + 1000000;
    $__carry = $__carry - 1;
}
$__ts = $__ts + $__carry;
"#;

/// Builds DateTime modification with microsecond carry and mutable or immutable result semantics.
pub(super) fn make_modify(mutable: bool, class_name: &str) -> ClassMethod {
    // Parsed-PHP preamble (parsing lives in static helpers to keep this frame
    // small): pull any `<±N> microsecond[s]|usec[s]` clauses out of the modifier,
    // strtotime() the remainder, then apply the microsecond delta with a carry into
    // the whole-second timestamp. result_tail_micro emits the new instant.
    let mut body = super::bodies::modify_preamble();
    body.extend(result_tail_micro(
        Expr::new(ExprKind::Variable("__ts".to_string()), dummy()),
        Some(Expr::new(ExprKind::Variable("__micro".to_string()), dummy())),
        mutable,
        class_name,
    ));
    method(
        "modify",
        vec![("modifier".to_string(), Some(TypeExpr::Str), None, false)],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        body,
    )
}

/// Builds the mutating/immutable setter set for a class.
pub(super) fn datetime_setter_methods(mutable: bool, class_name: &str) -> Vec<ClassMethod> {
    vec![
        make_set_timestamp(mutable, class_name),
        make_set_microsecond(mutable, class_name),
        make_set_time(mutable, class_name),
        make_set_date(mutable, class_name),
        make_set_timezone(mutable, class_name),
        make_add_sub("add", mutable, class_name, true),
        make_add_sub("sub", mutable, class_name, false),
        make_modify(mutable, class_name),
    ]
}

/// Builds the shared instance method set used by both `DateTime` and `DateTimeImmutable`
/// (construct from `"now"`/string, `format`, `getTimestamp`, `getTimezone`).
pub(super) fn datetime_shared_methods() -> Vec<ClassMethod> {
    vec![
        datetime_immutable_constructor(),
        datetime_immutable_get_timestamp(),
        datetime_get_microsecond(),
        datetime_immutable_get_timezone(),
        datetime_immutable_format(),
        datetime_get_offset(),
        datetime_diff_method(),
    ]
}
