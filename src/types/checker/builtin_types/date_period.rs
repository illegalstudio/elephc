//! Purpose:
//! Injects the built-in `DatePeriod` class as a synthetic PHP `Iterator` over a date range.
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
//!   `false` on malformed input. The deprecated `new DatePeriod(string)` constructor is not
//!   registered; callers should use the static factory instead.

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

/// Builds a string-literal expression.
fn str_lit(value: &str) -> Expr {
    Expr::new(ExprKind::StringLiteral(value.to_string()), dummy())
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

/// Builds a `$var->property = value;` statement.
fn assign_var_prop(var_name: &str, property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(var(var_name)),
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
        variadic: None,
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

/// Builds a public integer property defaulting to `0`.
fn int_property(name: &str) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: Some(TypeExpr::Int),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        default: Some(int_lit(0)),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public boolean property defaulting to `false`.
fn bool_property(name: &str) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: Some(TypeExpr::Bool),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
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

/// Builds the statements that materialize a `$iv` local `DateInterval` from the
/// stored components: `$iv = new DateInterval("PT0S"); $iv->y = $this->iv_y; ...`.
fn build_interval_local() -> Vec<Stmt> {
    let mut stmts = vec![assign("iv", new_obj("DateInterval", vec![str_lit("PT0S")]))];
    for (store, part) in INTERVAL_PARTS {
        stmts.push(assign_var_prop("iv", part, this_prop(store)));
    }
    stmts
}

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
    let mut body = vec![
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
    ];
    for (store, part) in INTERVAL_PARTS {
        body.push(assign_this(store, var_prop("interval", part)));
    }
    // An int third argument is a recurrence count; anything else is a DateTimeInterface
    // end bound. `(int)` unboxes the mixed value without relying on flow narrowing.
    body.push(if_else(
        call("is_int", vec![var("end")]),
        vec![
            assign_this("useCount", int_lit(1)),
            assign_this("recurrences", cast_int(var("end"))),
            assign_this("endTs", int_lit(0)),
        ],
        Some(vec![
            assign_this("useCount", int_lit(0)),
            assign_this("recurrences", int_lit(0)),
            assign_this("endTs", mcall(var("end"), "getTimestamp", Vec::new())),
        ]),
    ));
    // EXCLUDE_START_DATE = 1, INCLUDE_END_DATE = 2 → keep only the relevant bit.
    body.push(assign_this("excludeStart", bin(var("options"), BinOp::BitAnd, int_lit(1))));
    body.push(assign_this("includeEnd", bin(var("options"), BinOp::BitAnd, int_lit(2))));
    body.push(assign_this("curTs", this_prop("startTs")));
    body.push(assign_this("idx", int_lit(0)));
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
    let mut body = build_interval_local();
    body.push(assign("tmp", new_obj("DateTime", Vec::new())));
    body.push(expr_stmt(mcall(var("tmp"), "setTimestamp", vec![this_prop("curTs")])));
    body.push(expr_stmt(mcall(var("tmp"), "add", vec![var("iv")])));
    body.push(assign_this("curTs", mcall(var("tmp"), "getTimestamp", Vec::new())));
    method_vis("_advance", Visibility::Private, Vec::new(), Some(TypeExpr::Void), body)
}

/// `DatePeriod::rewind(): void` — resets the cursor to the start, skipping it once when
/// `EXCLUDE_START_DATE` is set.
fn date_period_rewind() -> ClassMethod {
    method(
        "rewind",
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
/// Count form (`useCount`): valid while `idx <= recurrences - excludeStart`, which
/// yields `recurrences + 1` dates including the start, or exactly `recurrences` dates
/// when `EXCLUDE_START_DATE` drops the start. End-date form: valid while the cursor is
/// before the end (`<=` when `INCLUDE_END_DATE` is set, `<` otherwise).
fn date_period_valid() -> ClassMethod {
    let count_branch = vec![ret(bin(
        this_prop("idx"),
        BinOp::LtEq,
        bin(this_prop("recurrences"), BinOp::Sub, this_prop("excludeStart")),
    ))];
    let date_branch = vec![if_else(
        this_prop("includeEnd"),
        vec![ret(bin(this_prop("curTs"), BinOp::LtEq, this_prop("endTs")))],
        Some(vec![ret(bin(this_prop("curTs"), BinOp::Lt, this_prop("endTs")))]),
    )];
    method(
        "valid",
        Vec::new(),
        Some(TypeExpr::Bool),
        vec![if_else(this_prop("useCount"), count_branch, Some(date_branch))],
    )
}

const CURRENT_SRC: &str = r#"<?php
if ($this->startIsImmutable) {
    $d = new DateTimeImmutable();
    return $d->setTimestamp($this->curTs);
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
    method(
        "current",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
        body,
    )
}

/// `DatePeriod::key(): int` — returns the zero-based iteration index.
fn date_period_key() -> ClassMethod {
    method("key", Vec::new(), Some(TypeExpr::Int), vec![ret(this_prop("idx"))])
}

/// `DatePeriod::next(): void` — advances the cursor by one interval and bumps the index.
fn date_period_next() -> ClassMethod {
    method(
        "next",
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

/// `DatePeriod::getEndDate(): ?DateTime` — returns the end bound for the end-date form, or `null`
/// when the period was constructed with a recurrence count.
fn date_period_get_end_date() -> ClassMethod {
    let tokens = crate::lexer::tokenize(GET_END_DATE_SRC).expect("getEndDate body must tokenize");
    let body = crate::parser::parse(&tokens).expect("getEndDate body must parse");
    method(
        "getEndDate",
        Vec::new(),
        Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified("DateTime"))))),
        body,
    )
}

/// `DatePeriod::getDateInterval(): DateInterval` — rebuilds the interval from its components.
fn date_period_get_interval() -> ClassMethod {
    let mut body = build_interval_local();
    body.push(ret(var("iv")));
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
            vec![ret(this_prop("recurrences"))],
            Some(vec![ret(null_lit())]),
        )],
    )
}

/// `DatePeriod::getIterator(): Iterator` — returns an iterator over the period's
/// dates. PHP's `DatePeriod` is an `IteratorAggregate` whose `getIterator()`
/// returns a separate iterator; elephc's `DatePeriod` is itself an `Iterator`, so
/// this rewinds and returns `$this`, which supports the common
/// `foreach ($p->getIterator() ...)` / `iterator_to_array($p->getIterator())`
/// uses (a single live iterator rather than independent ones).
fn date_period_get_iterator() -> ClassMethod {
    method(
        "getIterator",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("Iterator"))),
        vec![
            expr_stmt(mcall(
                Expr::new(ExprKind::This, dummy()),
                "rewind",
                Vec::new(),
            )),
            ret(Expr::new(ExprKind::This, dummy())),
        ],
    )
}

/// PHP source backing `DatePeriod::createFromISO8601String()` (PHP 8.3+).
///
/// Parses a subset of the RFC 5545 repeating-interval specification and forwards to the regular
/// `(start, interval, end|recurrences)` constructor. Two forms are accepted:
/// - `Rn/start[/interval[/end]]` with `n` ≥ 1 (recurrence form; the interval defaults to `P1D`).
/// - `start/interval[/end]` (no `R` prefix; a finite end bound is required — the endless form
///   `start/interval` is rejected, since elephc cannot model an unbounded iteration).
/// On malformed input it throws `DateMalformedPeriodStringException`, matching PHP 8.3+: a
/// `Recurrence count must be greater or equal to 1 ...` message for an out-of-range recurrence
/// (`R0`), and `Unknown or bad format (<input>)` for every other parse failure.
const CREATE_FROM_ISO8601_SRC: &str = r#"<?php
$bad = "Unknown or bad format (" . $specification . ")";
$badRecur = "DatePeriod::createFromISO8601String(): Recurrence count must be greater or equal to 1 and lower than 2147483640";
$len = strlen($specification);
if ($len < 3) { throw new DateMalformedPeriodStringException($bad); }
$recurrences = 0;
if ($specification[0] === "R") {
    // The recurrence prefix is Rn where n is a non-empty digit run; R<digits>/ is required
    // (endless "R/" is a bad-format error, and "R0" is a recurrence-count error, per PHP 8.3+).
    $slash = strpos($specification, "/");
    if ($slash === false || $slash < 2) { throw new DateMalformedPeriodStringException($bad); }
    $prefix = substr($specification, 1, $slash - 1);
    if ($prefix === "") { throw new DateMalformedPeriodStringException($bad); }
    if ($prefix[0] === "0") { throw new DateMalformedPeriodStringException($badRecur); }
    $digits = "0123456789";
    $all_digits = 1;
    $pi = 0;
    while ($pi < strlen($prefix)) {
        $ch = $prefix[$pi];
        $found = 0;
        $di = 0;
        while ($di < 10) {
            if ($digits[$di] === $ch) { $found = 1; break; }
            $di = $di + 1;
        }
        if ($found === 0) { $all_digits = 0; break; }
        $pi = $pi + 1;
    }
    if ($all_digits === 0) { throw new DateMalformedPeriodStringException($bad); }
    $recurrences = (int)$prefix;
    if ($recurrences < 1) { throw new DateMalformedPeriodStringException($badRecur); }
    $rest = substr($specification, $slash + 1);
} else {
    // No R prefix: start/interval[/end] form.
    $rest = $specification;
}
$parts = explode("/", $rest);
// count()-guarded reads avoid an undefined-key notice on the 2-part (no-end) form.
$nparts = count($parts);
$start_str = $parts[0];
$interval_str = ($nparts >= 2) ? $parts[1] : "P1D";
$end_str = ($nparts >= 3) ? $parts[2] : "";
if ($start_str === "" || $interval_str === "") { throw new DateMalformedPeriodStringException($bad); }
$has_end = ($end_str !== "") ? 1 : 0;
if ($has_end === 0 && $recurrences < 1) { throw new DateMalformedPeriodStringException($bad); }
try {
    $start_dt = new DateTime($start_str);
    $iv = new DateInterval($interval_str);
} catch (Exception $e) {
    throw new DateMalformedPeriodStringException($bad);
}
if ($has_end === 1) {
    try { $end_dt = new DateTime($end_str); }
    catch (Exception $e) { throw new DateMalformedPeriodStringException($bad); }
    return new DatePeriod($start_dt, $iv, $end_dt, $options);
}
return new DatePeriod($start_dt, $iv, $recurrences, $options);
"#;

/// Builds the static `createFromISO8601String(string $specification, int $options = 0): DatePeriod`
/// method.
///
/// The body is the parsed `CREATE_FROM_ISO8601_SRC` PHP source. It forwards to the regular
/// `(start, interval, end|recurrences, options)` constructor on success and throws
/// `DateMalformedPeriodStringException` on malformed input (PHP 8.3+ never returns `false`).
fn date_period_create_from_iso8601_string() -> ClassMethod {
    let tokens = crate::lexer::tokenize(CREATE_FROM_ISO8601_SRC)
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
        variadic: None,
        variadic_type: None,
        // PHP 8.3+: returns a `DatePeriod` or throws (never `false`).
        return_type: Some(TypeExpr::Named(Name::unqualified("DatePeriod"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the full `DatePeriod` method list.
fn date_period_methods() -> Vec<ClassMethod> {
    vec![
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
        date_period_create_from_iso8601_string(),
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
    // useCount selects the count form; recurrences holds its repeat count.
    props.push(int_property("useCount"));
    props.push(int_property("recurrences"));
    props
}

/// Injects the built-in `DatePeriod` class into the checker's class map.
///
/// `DatePeriod` implements `Iterator` so it can be used directly in `foreach`. It is
/// registered after `DateTime`/`DateInterval` (which its method bodies reference). The
/// constructor models the `(start, interval, end)` and `(start, interval, recurrences)` forms.
pub(crate) fn inject_builtin_date_period(class_map: &mut HashMap<String, FlattenedClass>) {
    if class_map.contains_key("DatePeriod") {
        return;
    }
    class_map.insert(
        "DatePeriod".to_string(),
        FlattenedClass {
            name: "DatePeriod".to_string(),
            extends: None,
            implements: vec!["Iterator".to_string(), "Traversable".to_string()],
            is_abstract: false,
            is_final: false,
            is_readonly_class: false,
            properties: date_period_properties(),
            methods: date_period_methods(),
            attributes: Vec::new(),
            constants: vec![
                class_const("EXCLUDE_START_DATE", 1),
                class_const("INCLUDE_END_DATE", 2),
            ],
            used_traits: Vec::new(),
        },
    );
}
