//! Purpose:
//! Injects the built-in `DatePeriod` class as a synthetic date-range iterator.
//! Iterates from a start `DateTimeInterface` by a `DateInterval` up to an end `DateTimeInterface`.
//!
//! Called from:
//! - `crate::types::checker::driver` via `inject_builtin_date_period`, after the other
//!   date/time classes (`DateTime`, `DateInterval`) are registered.
//!
//! Key details:
//! - The interval is stored as its seven integer components; `_advance()` rebuilds a
//!   `DateInterval` and reuses `DateTime::add()` so month/day overflow normalizes like PHP.
//! - `current()` returns a fresh `DateTime` snapshot each call, so collected values are distinct.
//! - Both the `(start, interval, end)` and `(start, interval, recurrences)` constructor forms are
//!   modeled (`is_int()` on the third argument picks the form).
//! - `createFromISO8601String()` (PHP 8.3+) is supported via a synthetic PHP-source body that
//!   parses `Rn/start[/interval[/end]]` and forwards to the regular constructor; returns
//!   `false` on malformed input. The name resolver routes the deprecated
//!   `new DatePeriod(string[, options])` overload through that factory.

use std::collections::HashMap;

use crate::names::Name;
use crate::parser::ast::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, PropertyHooks, Stmt,
    StmtKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

/// Returns a dummy source span for synthetic AST nodes.
fn dummy() -> crate::span::Span {
    crate::span::Span::dummy()
}

/// Builds an integer-literal expression.
fn int_lit(value: i64) -> Expr {
    Expr::new(ExprKind::IntLiteral(value), dummy())
}

/// Builds a `$name` variable expression.
fn var(name: &str) -> Expr {
    Expr::new(ExprKind::Variable(name.to_string()), dummy())
}

/// Builds a `$this->property` access expression.
fn this_prop(property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// Builds a `$var->property` access expression.
fn var_prop(var_name: &str, property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(var(var_name)),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// Builds a `left <op> right` binary expression.
fn bin(left: Expr, op: BinOp, right: Expr) -> Expr {
    Expr::new(
        ExprKind::BinaryOp { left: Box::new(left), op, right: Box::new(right) },
        dummy(),
    )
}

/// Builds an `$object-><method>(args)` method-call expression.
fn mcall(object: Expr, method: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::MethodCall { object: Box::new(object), method: method.to_string(), args },
        dummy(),
    )
}

/// Builds a `new <class>(args)` object-construction expression.
fn new_obj(class_name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(
        ExprKind::NewObject { class_name: Name::unqualified(class_name), args },
        dummy(),
    )
}

/// Builds a `<name>(args)` free-function call expression (used for `is_int`).
fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args }, dummy())
}

/// Builds an `(int) expr` cast expression. Used to unbox a `mixed` value into an
/// integer slot without relying on flow-sensitive narrowing in the type checker.
fn cast_int(value: Expr) -> Expr {
    Expr::new(ExprKind::Cast { target: CastType::Int, expr: Box::new(value) }, dummy())
}

/// Builds a `null` literal expression.
fn null_lit() -> Expr {
    Expr::new(ExprKind::Null, dummy())
}

/// Builds a `$name = value;` local assignment statement.
fn assign(name: &str, value: Expr) -> Stmt {
    Stmt::new(StmtKind::Assign { name: name.to_string(), value }, dummy())
}

/// Builds a `$this->property = value;` statement.
fn assign_this(property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
            value,
        },
        dummy(),
    )
}

/// Builds an expression statement (a bare expression used for its side effects).
fn expr_stmt(value: Expr) -> Stmt {
    Stmt::new(StmtKind::ExprStmt(value), dummy())
}

/// Builds a `return <expr>;` statement.
fn ret(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), dummy())
}

/// Builds an `if (cond) { then } else { else_body }` statement (no elseif clauses).
fn if_else(condition: Expr, then_body: Vec<Stmt>, else_body: Option<Vec<Stmt>>) -> Stmt {
    Stmt::new(
        StmtKind::If { condition, then_body, elseif_clauses: Vec::new(), else_body },
        dummy(),
    )
}

/// Builds a public method parameter `(name, type, default, by_ref)`.
fn param(
    name: &str,
    ty: Option<TypeExpr>,
    default: Option<Expr>,
) -> (String, Option<TypeExpr>, Option<Expr>, bool) {
    (name.to_string(), ty, default, false)
}

/// Builds a method with the given visibility, params, return type, and body.
fn method_vis(
    name: &str,
    visibility: Visibility,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public method.
fn method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    method_vis(name, Visibility::Public, params, return_type, body)
}

/// Builds a private integer storage property defaulting to `0`.
fn int_property(name: &str) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Private,
        set_visibility: None,
        type_expr: Some(TypeExpr::Int),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(int_lit(0)),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a private boolean storage property defaulting to `false`.
fn bool_property(name: &str) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Private,
        set_visibility: None,
        type_expr: Some(TypeExpr::Bool),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(Expr::new(ExprKind::BoolLiteral(false), dummy())),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public integer class constant.
fn class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        type_expr: Some(TypeExpr::Int),
        value: int_lit(value),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// The seven `DateInterval` components stored on `DatePeriod`, paired with the
/// `DateInterval` property they mirror: (storage property, interval property).
const INTERVAL_PARTS: [(&str, &str); 7] = [
    ("iv_y", "y"),
    ("iv_m", "m"),
    ("iv_d", "d"),
    ("iv_h", "h"),
    ("iv_i", "i"),
    ("iv_s", "s"),
    ("iv_invert", "invert"),
];

/// `DatePeriod::__construct(DateTimeInterface $start, DateInterval $interval, DateTimeInterface|int $end, int $options = 0)`.
///
/// Records the start timestamp, decomposes the interval into its seven integer
/// components, and reads the `EXCLUDE_START_DATE` / `INCLUDE_END_DATE` option bits.
/// The third argument selects the period form: a `DateTimeInterface` sets an end
/// bound (`useCount = 0`, `endTs` recorded), while an `int` sets a recurrence count
/// (`useCount = 1`, `recurrences` recorded) so iteration stops by count instead of date.
fn date_period_constructor() -> ClassMethod {
    let dti = Some(TypeExpr::Named(Name::unqualified("DateTimeInterface")));
    let interval_ty = Some(TypeExpr::Named(Name::unqualified("DateInterval")));
    // `mixed` so an int recurrence count or a DateTimeInterface end both pass the checker.
    let end_ty = Some(TypeExpr::Named(Name::unqualified("mixed")));
    let validation_tokens = crate::lexer::tokenize(
        r#"<?php
if (is_int($end)) {
    if ($end < 1 || $end > 2147483639) {
        throw new DateMalformedPeriodStringException(
            "DatePeriod::__construct(): Recurrence count must be greater or equal to 1 and lower than 2147483640"
        );
    }
    $totalRecurrences = $end
        + (($options & DatePeriod::EXCLUDE_START_DATE) ? 0 : 1)
        + (($options & DatePeriod::INCLUDE_END_DATE) ? 1 : 0);
    if ($totalRecurrences > 2147483639) {
        throw new DateMalformedStringException(
            "DatePeriod::__construct(): Recurrence count must be greater or equal to 1 and lower than 2147483640 (including options)"
        );
    }
}
"#,
    )
    .expect("DatePeriod constructor validation must tokenize");
    let mut body = crate::parser::parse(&validation_tokens)
        .expect("DatePeriod constructor validation must parse");
    body.extend(vec![
        assign_this("startTs", mcall(var("start"), "getTimestamp", Vec::new())),
        assign_this(
            "startIsImmutable",
            Expr::new(
                ExprKind::InstanceOf {
                    value: Box::new(var("start")),
                    target: crate::parser::ast::InstanceOfTarget::Name(Name::unqualified("DateTimeImmutable")),
                },
                dummy(),
            ),
        ),
    ]);
    for (store, part) in INTERVAL_PARTS {
        body.push(assign_this(store, var_prop("interval", part)));
    }
    // An int third argument is a recurrence count; anything else is a DateTimeInterface
    // end bound. `(int)` unboxes the mixed value without relying on flow narrowing.
    body.push(if_else(
        call("is_int", vec![var("end")]),
        vec![
            assign_this("useCount", int_lit(1)),
            assign_this("_recurrence_count", cast_int(var("end"))),
            assign_this("endTs", int_lit(0)),
        ],
        Some(vec![
            assign_this("useCount", int_lit(0)),
            assign_this("_recurrence_count", int_lit(0)),
            assign_this("endTs", mcall(var("end"), "getTimestamp", Vec::new())),
        ]),
    ));
    // EXCLUDE_START_DATE = 1, INCLUDE_END_DATE = 2 → keep only the relevant bit.
    body.push(assign_this("excludeStart", bin(var("options"), BinOp::BitAnd, int_lit(1))));
    body.push(assign_this("includeEnd", bin(var("options"), BinOp::BitAnd, int_lit(2))));
    body.push(assign_this("curTs", this_prop("startTs")));
    body.push(assign_this("idx", int_lit(0)));
    // Populate private storage backing PHP 8.2+'s virtual public properties.
    body.push(assign_this("_start", var("start")));
    body.push(assign_this("_interval", var("interval")));
    // include_start_date = !excludeStart; include_end_date = (includeEnd != 0).
    body.push(assign_this(
        "_include_start_date",
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(this_prop("excludeStart")),
                op: BinOp::Eq,
                right: Box::new(int_lit(0)),
            },
            dummy(),
        ),
    ));
    body.push(assign_this(
        "_include_end_date",
        Expr::new(
            ExprKind::BinaryOp {
                left: Box::new(this_prop("includeEnd")),
                op: BinOp::NotEq,
                right: Box::new(int_lit(0)),
            },
            dummy(),
        ),
    ));
    // `end` depends on the overload. The public `recurrences` property is the minimum
    // number of yielded instances: explicit count + included start + included end.
    body.push(if_else(
        call("is_int", vec![var("end")]),
        vec![assign_this("_end", null_lit())],
        Some(vec![assign_this("_end", var("end"))]),
    ));
    body.push(assign_this(
        "_recurrences",
        bin(
            bin(
                this_prop("_recurrence_count"),
                BinOp::Add,
                cast_int(this_prop("_include_start_date")),
            ),
            BinOp::Add,
            cast_int(this_prop("_include_end_date")),
        ),
    ));
    method(
        "__construct",
        vec![
            param("start", dti, None),
            param("interval", interval_ty, None),
            param("end", end_ty, None),
            param("options", Some(TypeExpr::Int), Some(int_lit(0))),
        ],
        None,
        body,
    )
}

/// `DatePeriod::_advance(): void` — private helper that steps `curTs` forward by one
/// interval, reusing `DateTime::add()` so calendar overflow matches PHP exactly.
fn date_period_advance() -> ClassMethod {
    let mut body = vec![assign(
        "iv",
        mcall(
            Expr::new(ExprKind::This, dummy()),
            "getDateInterval",
            Vec::new(),
        ),
    )];
    body.push(assign("tmp", new_obj("DateTime", Vec::new())));
    body.push(expr_stmt(mcall(var("tmp"), "setTimestamp", vec![this_prop("curTs")])));
    body.push(expr_stmt(mcall(var("tmp"), "add", vec![var("iv")])));
    body.push(assign_this("curTs", mcall(var("tmp"), "getTimestamp", Vec::new())));
    method_vis("_advance", Visibility::Private, Vec::new(), Some(TypeExpr::Void), body)
}

/// `DatePeriod::rewind(): void` — resets the cursor to the start, skipping it once when
/// `EXCLUDE_START_DATE` is set.
fn date_period_rewind() -> ClassMethod {
    method_vis(
        "rewind",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Void),
        vec![
            assign_this("curTs", this_prop("startTs")),
            assign_this("idx", int_lit(0)),
            if_else(
                this_prop("excludeStart"),
                vec![expr_stmt(mcall(Expr::new(ExprKind::This, dummy()), "_advance", Vec::new()))],
                None,
            ),
        ],
    )
}

/// `DatePeriod::valid(): bool`.
///
/// Count form (`useCount`): valid while `idx <= _recurrence_count - excludeStart`, which
/// yields `recurrences + 1` dates including the start, or exactly `recurrences` dates
/// when `EXCLUDE_START_DATE` drops the start. End-date form: valid while the cursor is
/// before the end (`<=` when `INCLUDE_END_DATE` is set, `<` otherwise).
fn date_period_valid() -> ClassMethod {
    let count_branch = vec![ret(bin(
        this_prop("idx"),
        BinOp::LtEq,
        bin(this_prop("_recurrence_count"), BinOp::Sub, this_prop("excludeStart")),
    ))];
    let date_branch = vec![if_else(
        this_prop("includeEnd"),
        vec![ret(bin(this_prop("curTs"), BinOp::LtEq, this_prop("endTs")))],
        Some(vec![ret(bin(this_prop("curTs"), BinOp::Lt, this_prop("endTs")))]),
    )];
    method_vis(
        "valid",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Bool),
        vec![if_else(this_prop("useCount"), count_branch, Some(date_branch))],
    )
}

const CURRENT_SRC: &str = r#"<?php
if ($this->startIsImmutable) {
    $d = new DateTimeImmutable();
    $d = $d->setTimestamp($this->curTs);
    return $d;
}
$d = new DateTime();
$d->setTimestamp($this->curTs);
return $d;
"#;

/// `DatePeriod::current(): DateTimeInterface` — returns a fresh snapshot at the cursor,
/// preserving the concrete class of the start object (`DateTime` or `DateTimeImmutable`).
fn date_period_current() -> ClassMethod {
    let tokens = crate::lexer::tokenize(CURRENT_SRC).expect("current body must tokenize");
    let body = crate::parser::parse(&tokens).expect("current body must parse");
    method_vis(
        "current",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
        body,
    )
}

/// `DatePeriod::key(): int` — returns the zero-based iteration index.
fn date_period_key() -> ClassMethod {
    method_vis(
        "key",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Int),
        vec![ret(this_prop("idx"))],
    )
}

/// `DatePeriod::next(): void` — advances the cursor by one interval and bumps the index.
fn date_period_next() -> ClassMethod {
    method_vis(
        "next",
        Visibility::Private,
        Vec::new(),
        Some(TypeExpr::Void),
        vec![
            expr_stmt(mcall(Expr::new(ExprKind::This, dummy()), "_advance", Vec::new())),
            assign_this("idx", bin(this_prop("idx"), BinOp::Add, int_lit(1))),
        ],
    )
}

const GET_START_DATE_SRC: &str = r#"<?php
if ($this->startIsImmutable) {
    $d = new DateTimeImmutable();
    return $d->setTimestamp($this->startTs);
}
$d = new DateTime();
$d->setTimestamp($this->startTs);
return $d;
"#;

const GET_END_DATE_SRC: &str = r#"<?php
if ($this->useCount) { return null; }
$d = new DateTime();
$d->setTimestamp($this->endTs);
return $d;
"#;

/// `DatePeriod::getStartDate(): DateTimeInterface` — returns the start instant as the same
/// concrete class that was passed to the constructor.
fn date_period_get_start_date() -> ClassMethod {
    let tokens = crate::lexer::tokenize(GET_START_DATE_SRC).expect("getStartDate body must tokenize");
    let body = crate::parser::parse(&tokens).expect("getStartDate body must parse");
    method(
        "getStartDate",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
        body,
    )
}

/// `DatePeriod::getEndDate(): ?DateTimeInterface` — returns the end bound for the end-date form, or
/// `null` when the period was constructed with a recurrence count (matching PHP's nullable interface
/// return type).
fn date_period_get_end_date() -> ClassMethod {
    let tokens = crate::lexer::tokenize(GET_END_DATE_SRC).expect("getEndDate body must tokenize");
    let body = crate::parser::parse(&tokens).expect("getEndDate body must parse");
    method(
        "getEndDate",
        Vec::new(),
        Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
            "DateTimeInterface",
        ))))),
        body,
    )
}

/// `DatePeriod::getDateInterval(): DateInterval` — rebuilds the interval from its components.
fn date_period_get_interval() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
$interval = $this->_interval;
if (!($interval instanceof DateInterval)) {
    throw new DateObjectError("Object of type DatePeriod has not been correctly initialized by calling parent::__construct() in its constructor");
}
return $interval->__elephc_clone();
"#,
    )
    .expect("DatePeriod getDateInterval body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod getDateInterval body must parse");
    method(
        "getDateInterval",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateInterval"))),
        body,
    )
}

/// `DatePeriod::getRecurrences(): ?int` — returns the recurrence count for the
/// count form, or `null` for the end-date form (matching PHP).
fn date_period_get_recurrences() -> ClassMethod {
    method(
        "getRecurrences",
        Vec::new(),
        Some(TypeExpr::Nullable(Box::new(TypeExpr::Int))),
        vec![if_else(
            this_prop("useCount"),
            vec![ret(this_prop("_recurrence_count"))],
            Some(vec![ret(null_lit())]),
        )],
    )
}

/// PHP source backing `DatePeriod::getIterator()`.
///
/// It snapshots fresh date values into an `InternalIterator`. The callback mirrors
/// the iterator's live cursor onto `DatePeriod::$current`, matching php-src while
/// keeping separate `getIterator()` calls independent.
const DATEPERIOD_GET_ITERATOR_SRC: &str = r#"<?php
$items = [];
$this->rewind();
while ($this->valid()) {
    $items[] = $this->current();
    $this->next();
}
$onCurrent = function($value): void {
    if ($value === null) {
        $this->_current = $this->current();
    } else {
        $this->_current = $value;
    }
};
return new InternalIterator($items, $onCurrent);
"#;

/// `DatePeriod::getIterator(): Iterator` — returns an independent internal iterator
/// over the period's dates.
fn date_period_get_iterator() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEPERIOD_GET_ITERATOR_SRC)
        .expect("DatePeriod::getIterator body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod::getIterator body must parse");
    method(
        "getIterator",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("Iterator"))),
        body,
    )
}

/// PHP source backing `DatePeriod::createFromISO8601String()` (PHP 8.3+).
///
/// Delegates the complete grammar to the vendored php-src timelib parser, then
/// performs php-src's start/interval/end-or-recurrence validation in the same order.
const CREATE_FROM_ISO8601_SRC: &str = r#"<?php
$parsed = __elephc_timelib_period_parse($specification);
if ($parsed["status"] !== "P") {
    throw new DateMalformedPeriodStringException(
        "Unknown or bad format (" . $specification . ")"
    );
}
if (!$parsed["has_start"]) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::createFromISO8601String(): ISO interval must contain a start date, \""
        . $specification . "\" given"
    );
}
if (!$parsed["has_interval"]) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::createFromISO8601String(): ISO interval must contain an interval, \""
        . $specification . "\" given"
    );
}
if (!$parsed["has_end"] && $parsed["recurrences"] === 0) {
    throw new DateMalformedPeriodStringException(
        "DatePeriod::createFromISO8601String(): ISO interval must contain an end date or a recurrence count, \""
        . $specification . "\" given"
    );
}
$start = DateTimeImmutable::createFromTimestamp($parsed["start"]);
$interval = new DateInterval("PT0S");
$interval->y = $parsed["y"];
$interval->m = $parsed["m"];
$interval->d = $parsed["d"];
$interval->h = $parsed["h"];
$interval->i = $parsed["i"];
$interval->s = $parsed["s"];
$interval->f = $parsed["us"] / 1000000.0;
if ($parsed["has_end"]) {
    $end = DateTimeImmutable::createFromTimestamp($parsed["end"]);
    return new DatePeriod($start, $interval, $end, $options);
}
return new DatePeriod($start, $interval, $parsed["recurrences"], $options);
"#;

/// Builds the static `createFromISO8601String(string $specification, int $options = 0): DatePeriod`
/// method.
///
/// The body is the parsed `CREATE_FROM_ISO8601_SRC` PHP source. It forwards to the regular
/// `(start, interval, end|recurrences, options)` constructor on success and throws
/// `DateMalformedPeriodStringException` on malformed input (PHP 8.3+ never returns `false`).
fn date_period_create_from_iso8601_string(uses_timelib: bool) -> ClassMethod {
    let source = if uses_timelib {
        CREATE_FROM_ISO8601_SRC
    } else {
        r#"<?php
throw new DateMalformedPeriodStringException(
    "Unknown or bad format (" . $specification . ")"
);
"#
    };
    let tokens = crate::lexer::tokenize(source)
        .expect("createFromISO8601String body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("createFromISO8601String body source must parse");
    ClassMethod {
        name: "createFromISO8601String".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            param("specification", Some(TypeExpr::Str), None),
            param("options", Some(TypeExpr::Int), Some(int_lit(0))),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        // PHP 8.3+: returns a `DatePeriod` or throws (never `false`).
        return_type: Some(TypeExpr::Named(Name::unqualified("static"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing the deprecated string constructor overload.
const DEPRECATED_STRING_CONSTRUCTOR_SRC: &str = r#"<?php
__elephc_diag_warning("Deprecated: Calling DatePeriod::__construct(string $isostr, int $options = 0) is deprecated, use DatePeriod::createFromISO8601String() instead\n");
return DatePeriod::createFromISO8601String($specification, $options);
"#;

/// Builds the internal wrapper used for `new DatePeriod(string[, int])`.
fn date_period_deprecated_string_constructor() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DEPRECATED_STRING_CONSTRUCTOR_SRC)
        .expect("DatePeriod deprecated string constructor body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod deprecated string constructor body must parse");
    ClassMethod {
        name: "__elephc_deprecated_string_constructor".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            param("specification", Some(TypeExpr::Str), None),
            param("options", Some(TypeExpr::Int), Some(int_lit(0))),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("DatePeriod"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal renderer for php-src's seven-field `DatePeriod` debug shape.
fn date_period_debug_dump() -> ClassMethod {
    let src = r#"<?php
echo "object(DatePeriod)#" . spl_object_id($this) . " (7) {\n";
echo "  [\"start\"]=>\n"; var_dump($this->start);
echo "  [\"current\"]=>\n"; var_dump($this->current);
echo "  [\"end\"]=>\n"; var_dump($this->end);
echo "  [\"interval\"]=>\n"; var_dump($this->interval);
echo "  [\"recurrences\"]=>\n"; var_dump($this->recurrences);
echo "  [\"include_start_date\"]=>\n"; var_dump($this->include_start_date);
echo "  [\"include_end_date\"]=>\n"; var_dump($this->include_end_date);
echo "}\n";
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DatePeriod debug dump source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod debug dump source must parse");
    method(
        "__elephc_debug_dump",
        Vec::new(),
        Some(TypeExpr::Void),
        body,
    )
}

/// Builds the full `DatePeriod` method list.
fn date_period_methods(uses_timelib: bool) -> Vec<ClassMethod> {
    let mut methods = vec![
        date_period_constructor(),
        date_period_advance(),
        date_period_rewind(),
        date_period_valid(),
        date_period_current(),
        date_period_key(),
        date_period_next(),
        date_period_get_start_date(),
        date_period_get_end_date(),
        date_period_get_interval(),
        date_period_get_recurrences(),
        date_period_get_iterator(),
        date_period_create_from_iso8601_string(uses_timelib),
        date_period_deprecated_string_constructor(),
        date_period_debug_dump(),
    ];
    methods.extend(date_period_property_getters());
    methods.extend(date_period_serialize_methods());
    methods
}

/// PHP source backing `DatePeriod::__serialize()`. Returns the period's state as an array with
/// `start`, `current`, `end`, `interval`, `recurrences`, `include_start_date`, `include_end_date`.
const DATEPERIOD_SERIALIZE_SRC: &str = r#"<?php
return [
    "start" => $this->start,
    "current" => $this->current,
    "end" => $this->end,
    "interval" => $this->interval,
    "recurrences" => $this->recurrences,
    "include_start_date" => $this->include_start_date,
    "include_end_date" => $this->include_end_date,
];
"#;

/// PHP source backing `DatePeriod::__set_state()`. Reconstructs from the array by forwarding to the
/// constructor with the start/interval/end or start/interval/recurrences form.
const DATEPERIOD_SET_STATE_SRC: &str = r#"<?php
if (!array_key_exists("start", $array)
    || !array_key_exists("current", $array)
    || !array_key_exists("end", $array)
    || !array_key_exists("interval", $array)
    || !array_key_exists("recurrences", $array)
    || !array_key_exists("include_start_date", $array)
    || !array_key_exists("include_end_date", $array)
    || !($array["start"] instanceof DateTimeInterface)
    || !($array["current"] === null || $array["current"] instanceof DateTimeInterface)
    || !($array["end"] === null || $array["end"] instanceof DateTimeInterface)
    || !($array["interval"] instanceof DateInterval)
    || !is_int($array["recurrences"])
    || !is_bool($array["include_start_date"])
    || !is_bool($array["include_end_date"])) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$options = ($array["include_start_date"] ? 0 : DatePeriod::EXCLUDE_START_DATE)
    | ($array["include_end_date"] ? DatePeriod::INCLUDE_END_DATE : 0);
if ($array["end"] === null) {
    $count = $array["recurrences"]
        - ($array["include_start_date"] ? 1 : 0)
        - ($array["include_end_date"] ? 1 : 0);
    return new DatePeriod($array["start"], $array["interval"], $count, $options);
}
return new DatePeriod($array["start"], $array["interval"], $array["end"], $options);
"#;

/// Builds `DatePeriod::__wakeup(): void` (no-op, reusing the datetime wakeup builder).
fn date_period_wakeup() -> ClassMethod {
    let tokens = crate::lexer::tokenize(r#"<?php
__elephc_diag_warning("Deprecated: Method DatePeriod::__wakeup() is deprecated since 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()\n");
throw new Error("Invalid serialization data for DatePeriod object");
"#)
        .expect("DatePeriod::__wakeup body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod::__wakeup body source must parse");
    ClassMethod {
        name: "__wakeup".to_string(),
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
        return_type: Some(TypeExpr::Void),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: super::datetime::deprecated_attribute(
            "8.5",
            "this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()",
        ),
    }
}

/// Builds `DatePeriod::__serialize(): array`.
fn date_period_serialize() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEPERIOD_SERIALIZE_SRC)
        .expect("DatePeriod::__serialize body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod::__serialize body source must parse");
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

/// Builds `DatePeriod::__unserialize(array $data): void`. Restores the mirror properties.
fn date_period_unserialize() -> ClassMethod {
    let src = r#"<?php
if (!array_key_exists("start", $data)
    || !array_key_exists("current", $data)
    || !array_key_exists("end", $data)
    || !array_key_exists("interval", $data)
    || !array_key_exists("recurrences", $data)
    || !array_key_exists("include_start_date", $data)
    || !array_key_exists("include_end_date", $data)
    || !($data["start"] instanceof DateTimeInterface)
    || !($data["current"] === null || $data["current"] instanceof DateTimeInterface)
    || !($data["end"] === null || $data["end"] instanceof DateTimeInterface)
    || !($data["interval"] instanceof DateInterval)
    || !is_int($data["recurrences"])
    || !is_bool($data["include_start_date"])
    || !is_bool($data["include_end_date"])) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$options = ($data["include_start_date"] ? 0 : DatePeriod::EXCLUDE_START_DATE)
    | ($data["include_end_date"] ? DatePeriod::INCLUDE_END_DATE : 0);
if ($data["end"] === null) {
    $count = $data["recurrences"]
        - ($data["include_start_date"] ? 1 : 0)
        - ($data["include_end_date"] ? 1 : 0);
    $this->__construct($data["start"], $data["interval"], $count, $options);
} else {
    $this->__construct($data["start"], $data["interval"], $data["end"], $options);
}
$this->_current = $data["current"];
"#;
    let tokens = crate::lexer::tokenize(src).expect("DatePeriod::__unserialize body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("DatePeriod::__unserialize body source must parse");
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

/// Builds `static DatePeriod::__set_state(array $array): static`.
fn date_period_set_state() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATEPERIOD_SET_STATE_SRC)
        .expect("DatePeriod::__set_state body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod::__set_state body source must parse");
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
        return_type: Some(TypeExpr::Named(Name::unqualified("DatePeriod"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Returns the serialization methods for `DatePeriod`.
fn date_period_serialize_methods() -> Vec<ClassMethod> {
    vec![
        date_period_wakeup(),
        date_period_serialize(),
        date_period_unserialize(),
        date_period_set_state(),
    ]
}

/// Builds a public object property defaulting to `null` (for the `DateTimeInterface`/`DateInterval`
/// mirror properties exposed by PHP's `DatePeriod`).
fn nullable_object_property(name: &str, class_name: &str, visibility: Visibility) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility,
        set_visibility: None,
        type_expr: Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
            class_name,
        ))))),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(null_lit()),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds one public virtual get-only property backed by a synthetic getter method.
fn virtual_property(name: &str, type_expr: TypeExpr) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: Some(type_expr),
        hooks: PropertyHooks { get: true, set: false, get_by_ref: false },
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: None,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds one synthetic getter for a public `DatePeriod` virtual property.
fn virtual_property_getter(name: &str, backing: &str, return_type: TypeExpr) -> ClassMethod {
    method(
        &crate::names::property_hook_get_method(name),
        Vec::new(),
        Some(return_type),
        vec![ret(this_prop(backing))],
    )
}

/// Builds the seven php-src virtual property getters.
fn date_period_property_getters() -> Vec<ClassMethod> {
    let dti = TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
        "DateTimeInterface",
    ))));
    let interval = TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
        "DateInterval",
    ))));
    vec![
        virtual_property_getter("start", "_start", dti.clone()),
        virtual_property_getter("current", "_current", dti.clone()),
        virtual_property_getter("end", "_end", dti),
        virtual_property_getter("interval", "_interval", interval),
        virtual_property_getter("recurrences", "_recurrences", TypeExpr::Int),
        virtual_property_getter(
            "include_start_date",
            "_include_start_date",
            TypeExpr::Bool,
        ),
        virtual_property_getter(
            "include_end_date",
            "_include_end_date",
            TypeExpr::Bool,
        ),
    ]
}

/// Builds the `DatePeriod` integer state properties.
fn date_period_properties() -> Vec<ClassProperty> {
    let mut props = vec![int_property("startTs"), int_property("endTs"), bool_property("startIsImmutable")];
    for (store, _) in INTERVAL_PARTS {
        props.push(int_property(store));
    }
    props.push(int_property("excludeStart"));
    props.push(int_property("includeEnd"));
    props.push(int_property("curTs"));
    props.push(int_property("idx"));
    // useCount selects the count form; _recurrence_count holds its explicit repeat count.
    props.push(int_property("useCount"));
    props.push(int_property("_recurrence_count"));
    // Private materialized storage and public virtual get-only properties reproduce
    // php-src's special handlers: Reflection reports virtual properties while direct
    // user writes are rejected even though `isReadOnly()` itself is false.
    props.push(nullable_object_property(
        "_start",
        "DateTimeInterface",
        Visibility::Private,
    ));
    props.push(nullable_object_property(
        "_current",
        "DateTimeInterface",
        Visibility::Private,
    ));
    props.push(nullable_object_property(
        "_end",
        "DateTimeInterface",
        Visibility::Private,
    ));
    props.push(nullable_object_property(
        "_interval",
        "DateInterval",
        Visibility::Private,
    ));
    let mut recurrence_store = int_property("_recurrences");
    recurrence_store.visibility = Visibility::Private;
    props.push(recurrence_store);
    let mut include_start_store = bool_property("_include_start_date");
    include_start_store.visibility = Visibility::Private;
    props.push(include_start_store);
    let mut include_end_store = bool_property("_include_end_date");
    include_end_store.visibility = Visibility::Private;
    props.push(include_end_store);
    props.push(virtual_property(
        "start",
        TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
            "DateTimeInterface",
        )))),
    ));
    props.push(virtual_property(
        "current",
        TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
            "DateTimeInterface",
        )))),
    ));
    props.push(virtual_property(
        "end",
        TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
            "DateTimeInterface",
        )))),
    ));
    props.push(virtual_property(
        "interval",
        TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
            "DateInterval",
        )))),
    ));
    props.push(virtual_property("recurrences", TypeExpr::Int));
    props.push(virtual_property("include_start_date", TypeExpr::Bool));
    props.push(virtual_property("include_end_date", TypeExpr::Bool));
    props
}

/// Injects the built-in `DatePeriod` class into the checker's class map.
///
/// `DatePeriod` implements only `IteratorAggregate`, like php-src, and returns an
/// independent `InternalIterator`. It is registered after `DateTime`/`DateInterval`
/// (which its method bodies reference). The constructor models the
/// `(start, interval, end)` and `(start, interval, recurrences)` forms.
pub(crate) fn inject_builtin_date_period(
    class_map: &mut HashMap<String, FlattenedClass>,
    uses_timelib: bool,
) {
    if class_map.contains_key("DatePeriod") {
        return;
    }
    class_map.insert(
        "DatePeriod".to_string(),
        FlattenedClass {
            name: "DatePeriod".to_string(),
            span: dummy(),
            extends: None,
            implements: vec!["IteratorAggregate".to_string(), "Traversable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: date_period_properties(),
            methods: date_period_methods(uses_timelib),
            attributes: Vec::new(),
            constants: vec![
                class_const("EXCLUDE_START_DATE", 1),
                class_const("INCLUDE_END_DATE", 2),
            ],
            used_traits: Vec::new(),
            trait_aliases: Vec::new(),
        },
    );
}
