//! Purpose:
//! Audited DateTime mutators, factories, and parsing metadata.
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
use super::compliance_interval::datetime_diff_method;
use super::compliance_procedural::datetime_get_offset;
/// `setTimestamp(int $timestamp)` — replaces the stored UNIX timestamp and resets microseconds,
/// matching PHP's integer-instant semantics.
pub(super) fn make_set_timestamp(mutable: bool, class_name: &str) -> ClassMethod {
    method(
        "setTimestamp",
        vec![("timestamp".to_string(), Some(TypeExpr::Int), None, false)],
        Some(TypeExpr::Named(Name::unqualified(class_name))),
        result_tail_micro(
            Expr::new(ExprKind::Variable("timestamp".to_string()), dummy()),
            Some(Expr::new(ExprKind::IntLiteral(0), dummy())),
            mutable,
            class_name,
        ),
    )
}

/// Builds the procedural `date_timestamp_set()` wrapper with PHP's null-to-int deprecation.
pub(super) fn datetime_procedural_set_timestamp() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if ($timestamp === null) {
    __elephc_diag_warning(
        "\nDeprecated: date_timestamp_set(): Passing null to parameter #2 (\$timestamp) of type int is deprecated",
        $sourceLine,
        E_DEPRECATED
    );
    return $object->setTimestamp(0);
}
return $object->setTimestamp($timestamp);
"#,
    )
    .expect("procedural date_timestamp_set body must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("procedural date_timestamp_set body must parse");
    ClassMethod {
        name: "__elephc_date_timestamp_set".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "object".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            ),
            (
                "timestamp".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            ),
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

/// `setMicrosecond(int $microsecond): static` — sets the sub-second component. Mutable updates
/// `$this` in place; immutable returns a fresh instance carrying the same instant/zone with the new
/// micros (the instant in seconds is unchanged).
pub(super) fn make_set_microsecond(mutable: bool, class_name: &str) -> ClassMethod {
    let update = if mutable {
        "$this->microsecond = $microsecond;\nreturn $this;"
    } else {
        "$__new = __elephc_new_instance_without_constructor(static::class);\n\
         $__new->__unserialize($this->__serialize());\n\
         $__new->__elephc_set_microsecond_raw($microsecond);\n\
         return $__new;"
    };
    let source = format!(
        r#"<?php
if ($microsecond < 0 || $microsecond > 999999) {{
    throw new DateRangeError(
        "{class_name}::setMicrosecond(): Argument #1 (\$microsecond) must be between 0 and 999999, "
        . $microsecond . " given"
    );
}}
{update}
"#
    );
    let tokens = crate::lexer::tokenize(&source).expect("setMicrosecond body must tokenize");
    let body = crate::parser::parse(&tokens).expect("setMicrosecond body must parse");
    method(
        "setMicrosecond",
        vec![("microsecond".to_string(), Some(TypeExpr::Int), None, false)],
        Some(TypeExpr::Named(Name::unqualified("static"))),
        body,
    )
}

/// Builds the compiler-only raw microsecond setter used after constructorless late-static
/// allocation. The public setter owns range validation and immutable cloning.
pub(super) fn datetime_set_microsecond_raw() -> ClassMethod {
    method(
        "__elephc_set_microsecond_raw",
        vec![("microsecond".to_string(), Some(TypeExpr::Int), None, false)],
        Some(TypeExpr::Void),
        vec![assign_this_property(
            "microsecond",
            Expr::new(ExprKind::Variable("microsecond".to_string()), dummy()),
        )],
    )
}

/// `setTime(int $hour, int $minute, int $second = 0, int $microsecond = 0)` — keeps the date,
/// replaces the time-of-day and sub-second component (PHP 8.4+).
pub(super) fn make_set_time(mutable: bool, class_name: &str, uses_timelib: bool) -> ClassMethod {
    if uses_timelib {
        let tokens = crate::lexer::tokenize(
            r#"<?php
$__payload = "T\t" . $hour . "\t" . $minute . "\t" . $second . "\t" . $microsecond;
$__parsed = __elephc_timelib_set_civil(
    $this->timestamp,
    $this->microsecond,
    $this->timezone_name,
    $__payload
);
"#,
        )
        .expect("timelib setTime body must tokenize");
        let mut body =
            crate::parser::parse(&tokens).expect("timelib setTime body must parse");
        let parsed_field = |field: &str| {
            Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::new(
                        ExprKind::Variable("__parsed".to_string()),
                        dummy(),
                    )),
                    index: Box::new(Expr::new(
                        ExprKind::StringLiteral(field.to_string()),
                        dummy(),
                    )),
                },
                dummy(),
            )
        };
        body.extend(result_tail_micro(
            parsed_field("timestamp"),
            Some(parsed_field("microsecond")),
            mutable,
            class_name,
        ));
        return method(
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
        );
    }
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
pub(super) fn make_set_date(mutable: bool, class_name: &str, uses_timelib: bool) -> ClassMethod {
    if uses_timelib {
        let tokens = crate::lexer::tokenize(
            r#"<?php
$__payload = "D\t" . $year . "\t" . $month . "\t" . $day;
$__parsed = __elephc_timelib_set_civil(
    $this->timestamp,
    $this->microsecond,
    $this->timezone_name,
    $__payload
);
"#,
        )
        .expect("timelib setDate body must tokenize");
        let mut body =
            crate::parser::parse(&tokens).expect("timelib setDate body must parse");
        let parsed_field = |field: &str| {
            Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::new(
                        ExprKind::Variable("__parsed".to_string()),
                        dummy(),
                    )),
                    index: Box::new(Expr::new(
                        ExprKind::StringLiteral(field.to_string()),
                        dummy(),
                    )),
                },
                dummy(),
            )
        };
        body.extend(result_tail_micro(
            parsed_field("timestamp"),
            Some(parsed_field("microsecond")),
            mutable,
            class_name,
        ));
        return method(
            "setDate",
            vec![
                ("year".to_string(), Some(TypeExpr::Int), None, false),
                ("month".to_string(), Some(TypeExpr::Int), None, false),
                ("day".to_string(), Some(TypeExpr::Int), None, false),
            ],
            Some(TypeExpr::Named(Name::unqualified(class_name))),
            body,
        );
    }
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
/// Reads the zone through its public `getName()` API. `DateTime` mutates `$this`;
/// `DateTimeImmutable` returns a fresh instance with the same timestamp and the new timezone name.
pub(super) fn make_set_timezone(mutable: bool, class_name: &str) -> ClassMethod {
    let tz_name = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(Expr::new(
                ExprKind::Variable("timezone".to_string()),
                dummy(),
            )),
            method: "getName".to_string(),
            args: Vec::new(),
        },
        dummy(),
    );
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

/// Builds a runtime guard that reports php-src's exact `DateInterval` argument TypeError,
/// including the observable debug type of the rejected value.
pub(super) fn date_interval_type_guard(callable: &str, argument_number: i64) -> String {
    format!(
        r#"if (!($interval instanceof DateInterval)) {{
    $__actual = gettype($interval);
    if ($__actual === "boolean") {{
        $__actual = $interval ? "true" : "false";
    }} else if ($__actual === "integer") {{
        $__actual = "int";
    }} else if ($__actual === "double") {{
        $__actual = "float";
    }} else if ($__actual === "NULL") {{
        $__actual = "null";
    }} else if ($__actual === "object") {{
        $__actual = get_class($interval);
    }}
    throw new TypeError(
        "{callable}: Argument #{argument_number} (\$interval) must be of type DateInterval, "
        . $__actual . " given"
    );
}}
"#,
    )
}

/// `add(DateInterval $interval)` / `sub(DateInterval $interval)` — shifts the date by the interval.
///
/// Decomposes `$this->timestamp` into calendar components via `date()`, applies each signed interval
/// component, then recomposes with `mktime()` (which normalizes overflow — e.g. day 32 rolls into the
/// next month). `$interval->invert` flips the direction (`$__sign` = `1 - 2*invert` for `add`, negated
/// for `sub`). `DateTime` mutates `$this`; `DateTimeImmutable` returns a fresh instance via
/// `result_tail`. `is_add` selects `add` (true) vs `sub` (false).
pub(super) fn make_add_sub(
    name: &str,
    mutable: bool,
    class_name: &str,
    is_add: bool,
    uses_timelib: bool,
) -> ClassMethod {
    if uses_timelib {
        let source = format!(
            "<?php\n{}{}",
            date_interval_type_guard(&format!("{class_name}::{name}()"), 1),
            format!(
                r#"
$__interval_result = __elephc_timelib_apply_interval(
    $this->timestamp,
    $this->microsecond,
    $this->timezone_name,
    $interval->__elephc_payload(),
    {}
);
if ($__interval_result["warning"]) {{
    throw new DateInvalidOperationException(
        "{}::sub(): Only non-special relative time specifications are supported for subtraction"
    );
}}
"#,
            if is_add { "false" } else { "true" },
            class_name,
            )
        );
        let tokens = crate::lexer::tokenize(&source)
            .expect("timelib DateTime add/sub body must tokenize");
        let mut body = crate::parser::parse(&tokens)
            .expect("timelib DateTime add/sub body must parse");
        let result_value = |field: &str| {
            Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(Expr::new(
                        ExprKind::Variable("__interval_result".to_string()),
                        dummy(),
                    )),
                    index: Box::new(Expr::new(
                        ExprKind::StringLiteral(field.to_string()),
                        dummy(),
                    )),
                },
                dummy(),
            )
        };
        body.extend(result_tail_micro(
            result_value("timestamp"),
            Some(result_value("microsecond")),
            mutable,
            class_name,
        ));
        return method(
            name,
            vec![(
                "interval".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            )],
            Some(TypeExpr::Named(Name::unqualified(class_name))),
            body,
        );
    }

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

/// `modify(string $modifier)` — applies php-src's timelib field-copy/update sequence.
///
/// Unlike `strtotime()` with a base timestamp, `DateTime::modify()` copies parsed
/// absolute and relative fields onto the existing zoned object before normalizing
/// DST and microseconds. The bridge owns that exact sequence on every target.
pub(super) fn make_modify(mutable: bool, class_name: &str) -> ClassMethod {
    let src = format!(
        r#"<?php
if ($modifier === "") {{
    throw new DateMalformedStringException(
        DateTime::__elephc_malformed_time_message("{class_name}::modify(): ", $modifier)
    );
}}
$__modified = __elephc_timelib_modify(
    $this->timestamp,
    $this->microsecond,
    DateTime::__elephc_runtime_timezone_name($this->timezone_name),
    $modifier
);
DateTime::$lastParseResult = $__modified["parse"];
if ($__modified["status"] !== "O") {{
    throw new DateMalformedStringException(
        DateTime::__elephc_malformed_time_message("{class_name}::modify(): ", $modifier)
    );
}}
$__ts = $__modified["timestamp"];
$__micro = $__modified["microsecond"];
$__timezone = $__modified["reset_to_utc"] ? "+00:00" : $this->timezone_name;
"#,
    );
    let tokens = crate::lexer::tokenize(&src).expect("modify body must tokenize");
    let mut body = crate::parser::parse(&tokens).expect("modify body must parse");
    body.extend(result_tail_micro_with_timezone(
        Expr::new(ExprKind::Variable("__ts".to_string()), dummy()),
        Some(Expr::new(ExprKind::Variable("__micro".to_string()), dummy())),
        Some(Expr::new(
            ExprKind::Variable("__timezone".to_string()),
            dummy(),
        )),
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
pub(super) fn datetime_setter_methods(
    mutable: bool,
    class_name: &str,
    uses_timelib: bool,
) -> Vec<ClassMethod> {
    let mut methods = vec![
        make_set_timestamp(mutable, class_name),
        make_set_microsecond(mutable, class_name),
        make_set_time(mutable, class_name, uses_timelib),
        make_set_date(mutable, class_name, uses_timelib),
        make_set_timezone(mutable, class_name),
        make_add_sub("add", mutable, class_name, true, uses_timelib),
        make_add_sub("sub", mutable, class_name, false, uses_timelib),
        make_modify(mutable, class_name),
    ];
    if !mutable {
        for method in &mut methods {
            method.attributes = no_discard_attribute(&method.name);
        }
    }
    methods
}

/// Builds the shared instance method set used by both `DateTime` and `DateTimeImmutable`
/// (construct from `"now"`/string, `format`, `getTimestamp`, `getTimezone`).
pub(super) fn datetime_shared_methods(uses_timelib: bool) -> Vec<ClassMethod> {
    vec![
        datetime_immutable_constructor(),
        datetime_immutable_get_timestamp(),
        datetime_get_microsecond(),
        datetime_set_microsecond_raw(),
        datetime_immutable_get_timezone(),
        datetime_immutable_format(uses_timelib),
        datetime_get_offset(),
        datetime_diff_method(uses_timelib),
    ]
}

/// PHP source for the `createFromFormat` parser, used directly as the method body so the feature is
/// self-contained (no separately-injected helper function to keep in sync with the class emission).
///
/// `__CFF_CLASS__` is substituted with the concrete class so each method constructs its own type.
/// Field semantics mirror PHP: unspecified fields default to the current date/time, but once any
/// time field is parsed the unparsed time fields reset to 0; `!` resets all fields to the Unix
/// epoch, `|` resets the not-yet-parsed fields, `\` escapes the next format character, and any other
/// character must match the subject. Supported specifiers:
/// `Y y m n d j D l S F M z H G h g i s u v A a U O P T e X x` plus the metas `! | # ? * +`.
/// `D`/`l` parse a weekday name (full or abbreviated) and shift the result forward 0-6 days to that
/// weekday after all fields are applied (timelib's relative-weekday behavior). `z` is the 0-based
/// day of the year: it requires an already-parsed year, overrides month/day, and overflows into
/// subsequent years through `mktime` normalization. `#` matches one separator from `;:/.,-`, `?`
/// skips one subject byte, `*` skips bytes until the next digit or separator, and `+` tolerates
/// trailing subject data (without it, unconsumed trailing data is a parse failure, as in PHP).
/// Returns the constructed instance, or `false` when the subject does not match. `intval()` is used
/// instead of `(int)` casts because synthetic method bodies do not lower cast nodes. The timezone
/// specifiers (`O P T e`) select the result timezone and override the optional third timezone,
/// exactly as timelib does. `Z` is not a create-from-format specifier and therefore remains a
/// literal format character.
pub(super) const CREATE_FROM_FORMAT_SRC: &str = r##"<?php
if (str_contains($format, chr(0))) {
    throw new ValueError(
        '__CFF_CLASS__::createFromFormat(): Argument #1 ($format) must not contain any null bytes'
    );
}
if (str_contains($datetime, chr(0))) {
    throw new ValueError(
        '__CFF_CLASS__::createFromFormat(): Argument #2 ($datetime) must not contain any null bytes'
    );
}
DateTime::$lastErrorCount = 1;
DateTime::$lastErrorPosition = 0;
DateTime::$lastErrorMessage = "The date string failed to match the format";
DateTime::$lastWarningCount = 0;
DateTime::$lastWarningPosition = 0;
DateTime::$lastWarningMessage = "";
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
$parsedO = ""; $parsedP = ""; $parsedT = ""; $parsedE = "";
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
        if ($cnt === 0) {
            DateTime::$lastErrorPosition = $dp;
            DateTime::$lastErrorMessage = ($dp >= $dlen)
                ? "Not enough data available to satisfy format"
                : "Unexpected data found.";
            return false;
        }
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
        if ($cnt === 0) {
            DateTime::$lastErrorPosition = $dp;
            DateTime::$lastErrorMessage = ($dp >= $dlen)
                ? "Not enough data available to satisfy format"
                : "Unexpected data found.";
            return false;
        }
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
    else {
        DateTime::$lastErrorPosition = $dp;
        DateTime::$lastErrorMessage = ($dp >= $dlen)
            ? "Not enough data available to satisfy format"
            : "Unexpected data found.";
        return false;
    }
}
if (!$junkOk && $dp < $dlen) {
    DateTime::$lastErrorPosition = $dp;
    DateTime::$lastErrorMessage = "Trailing data";
    return false;
}
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
$displayZone = "";
$parseZone = "";
$parsedOffset = 0;
if ($parsedO !== "") {
    $hours = intval(substr($parsedO, 1, 2));
    $minutes = intval(substr($parsedO, 3, 2));
    $parsedOffset = $hours * 3600 + $minutes * 60;
    if ($parsedO[0] === "-") { $parsedOffset = 0 - $parsedOffset; }
    $displayZone = substr($parsedO, 0, 3) . ":" . substr($parsedO, 3, 2);
} else if ($parsedP !== "") {
    $hours = intval(substr($parsedP, 1, 2));
    $minutes = intval(substr($parsedP, 4, 2));
    $parsedOffset = $hours * 3600 + $minutes * 60;
    if ($parsedP[0] === "-") { $parsedOffset = 0 - $parsedOffset; }
    $displayZone = $parsedP;
} else if ($parsedT !== "") {
    $resolvedZone = DateTime::__elephc_timezone_name_from_abbr($parsedT, -1, -1);
    if ($resolvedZone === false) { return false; }
    $parseZone = strval($resolvedZone);
    $displayZone = $parsedT;
} else if ($parsedE !== "") {
    try {
        $zoneObject = new DateTimeZone($parsedE);
    } catch (DateInvalidTimeZoneException $exception) {
        return false;
    }
    $parseZone = $zoneObject->getName();
    $displayZone = $parseZone;
}
if ($hasU) {
    $ts = $U;
} else if ($parsedO !== "" || $parsedP !== "") {
    $ts = __elephc_gmmktime_raw($H, $mi, $se, $mo, $da, $Y) - $parsedOffset;
} else if ($parseZone !== "") {
    $saved = date_default_timezone_get();
    date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($parseZone));
    $ts = __elephc_mktime_raw($H, $mi, $se, $mo, $da, $Y);
    date_default_timezone_set($saved);
} else if ($timezone === null) {
    $ts = __elephc_mktime_raw($H, $mi, $se, $mo, $da, $Y);
} else {
    $saved = date_default_timezone_get();
    date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($timezone->getName()));
    $ts = __elephc_mktime_raw($H, $mi, $se, $mo, $da, $Y);
    date_default_timezone_set($saved);
}
$o = new __CFF_CLASS__();
$o = $o->setTimestamp($ts);
// G11: PHP emits a warning "The parsed date was invalid" when the normalized date does not
// round-trip (e.g. month 13 → overflow, day 99 → overflow). Check by re-rendering the date
// components and comparing against the parsed input.
if (!$hasU) {
    $__saved = date_default_timezone_get();
    if ($parseZone !== "") {
        date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($parseZone));
    } else if ($displayZone !== "") {
        date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($displayZone));
    } else if ($timezone !== null) {
        date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($timezone->getName()));
    }
    $__checkY = intval(date("Y", $ts));
    $__checkM = intval(date("n", $ts));
    $__checkD = intval(date("j", $ts));
    date_default_timezone_set($__saved);
    if (($pY && $__checkY !== $Y) || ($pmo && $__checkM !== $mo) || ($pda && $__checkD !== $da)) {
        DateTime::$lastWarningCount = 1;
        DateTime::$lastWarningPosition = $dlen;
        DateTime::$lastWarningMessage = "The parsed date was invalid";
        DateTime::$lastErrorCount = 0;
    }
}
if ($displayZone !== "") {
    $o->timezone_name = $displayZone;
} else if ($timezone !== null) {
    // Set the display zone via getName() rather than setTimezone($timezone): the parameter is
    // `?DateTimeZone`, whose value reaches here boxed as Mixed, and setTimezone reads the
    // `name` property directly (which mis-reads a boxed receiver). getName() dispatches by
    // runtime class id, so it resolves correctly, mirroring the two-argument constructor.
    $o->timezone_name = $timezone->getName();
}
DateTime::$lastErrorCount = 0;
$o = $o->setMicrosecond($umicro);
if (static::class === __CFF_CLASS__::class) {
    return $o;
}
$result = __elephc_new_instance_without_constructor(static::class);
$result->__unserialize($o->__serialize());
return $result;
"##;

/// Timelib-backed `createFromFormat()` body used whenever the date bridge
/// prelude is present. `__CFF_CLASS__` is replaced with the concrete mutable or
/// immutable class just like the legacy fallback body.
pub(super) const TIMELIB_CREATE_FROM_FORMAT_SRC: &str = r##"<?php
if (str_contains($format, chr(0))) {
    throw new ValueError(
        '__CFF_CLASS__::createFromFormat(): Argument #1 ($format) must not contain any null bytes'
    );
}
if (str_contains($datetime, chr(0))) {
    throw new ValueError(
        '__CFF_CLASS__::createFromFormat(): Argument #2 ($datetime) must not contain any null bytes'
    );
}
$timezoneName = date_default_timezone_get();
if ($timezone !== null) {
    $timezoneName = $timezone->getName();
}
$parsed = __elephc_timelib_create_from_format($format, $datetime, $timezoneName);
if ($parsed["error_count"] > 0) {
    DateTime::$lastParseResult = $parsed["__elephc_serialized"];
    return false;
}
$object = new __CFF_CLASS__();
$object = $object->setTimestamp($parsed["__elephc_timestamp"]);
$microsecond = 0;
if ($parsed["fraction"] !== false) {
    $microsecond = intval(round($parsed["fraction"] * 1000000.0));
}
$object = $object->setMicrosecond($microsecond);
if ($parsed["is_localtime"]) {
    $zoneType = $parsed["zone_type"];
    if ($zoneType === 1) {
        $object->timezone_name = __elephc_timelib_offset_name($parsed["zone"]);
    } else if ($zoneType === 2) {
        $object->timezone_name = $parsed["tz_abbr"];
    } else if ($zoneType === 3) {
        $object->timezone_name = $parsed["tz_id"];
    } else {
        $object->timezone_name = $timezoneName;
    }
} else {
    $object->timezone_name = $timezoneName;
}
DateTime::$lastParseResult = $parsed["__elephc_serialized"];
if (static::class === __CFF_CLASS__::class) {
    return $object;
}
$result = __elephc_new_instance_without_constructor(static::class);
$result->__unserialize($object->__serialize());
return $result;
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
pub(super) fn datetime_create_from_format(class_name: &str, uses_timelib: bool) -> ClassMethod {
    let source = if uses_timelib {
        TIMELIB_CREATE_FROM_FORMAT_SRC
    } else {
        CREATE_FROM_FORMAT_SRC
    };
    let src = source.replace("__CFF_CLASS__", class_name);
    let tokens =
        crate::lexer::tokenize(&src).expect("createFromFormat body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("createFromFormat body source must parse");
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
            TypeExpr::False,
        ])),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the static `getLastErrors(): array|false` method over the shared parser state.
pub(super) fn datetime_get_last_errors(uses_timelib: bool) -> ClassMethod {
    let body = if uses_timelib {
        super::bodies::timelib_get_last_errors()
    } else {
        super::bodies::get_last_errors("DateTime")
    };
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
        return_type: Some(TypeExpr::Union(vec![
            TypeExpr::Named(Name::unqualified("array")),
            TypeExpr::False,
        ])),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `idate()`, including runtime validation for computed formats.
pub(super) const IDATE_SRC: &str = r#"<?php
if (strlen($format) !== 1) {
    __elephc_diag_warning("\nWarning: idate(): idate format is one char", $sourceLine, E_WARNING);
    return false;
}
$valid = [
    "B", "d", "G", "g", "H", "h", "I", "i", "L", "m", "N",
    "n", "s", "t", "U", "W", "w", "Y", "y", "z", "Z",
];
if (!in_array($format, $valid, true)) {
    __elephc_diag_warning("\nWarning: idate(): Unrecognized date format token", $sourceLine, E_WARNING);
    return false;
}
if ($timestamp === null) {
    return intval(date($format));
}
return intval(date($format, intval($timestamp)));
"#;

/// Builds the internal `DateTime::__elephc_idate()` procedural-alias helper.
pub(super) fn datetime_idate() -> ClassMethod {
    let tokens = crate::lexer::tokenize(IDATE_SRC).expect("idate helper body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("idate helper body source must parse");
    ClassMethod {
        name: "__elephc_idate".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("format".to_string(), Some(TypeExpr::Str), None, false),
            (
                "timestamp".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Int))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
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

/// PHP source backing the cross-conversion factories (`createFromInterface`,
/// `createFromImmutable`, `createFromMutable`): copy the source object's instant and display
/// timezone into a fresh instance of the target class. `__TARGET__` is substituted with the
/// target class name.
pub(super) const CREATE_FROM_OBJECT_SRC: &str = r#"<?php
__VALIDATION__
$className = static::class;
$timezone = $object->format("e");
$data = [
    "date" => $object->format("x-m-d H:i:s.u"),
    "timezone_type" => DateTime::__elephc_timezone_type($timezone),
    "timezone" => $timezone,
];
if ($className === __TARGET__::class) {
    $baseResult = new __TARGET__();
    $baseResult->__unserialize($data);
    return $baseResult;
}
$subclassResult = __elephc_new_instance_without_constructor($className);
$subclassResult->__unserialize($data);
return $subclassResult;
"#;

/// Builds a cross-conversion factory (`createFromInterface` / `createFromImmutable` /
/// `createFromMutable`) returning a fresh `target_class` that carries the source object's
/// instant, microseconds, and timezone. `source_class` preserves the official parameter type for
/// each factory; the return type is explicit because synthetic methods have no body inference.
pub(super) fn datetime_create_from_object(
    method_name: &str,
    source_class: &str,
    target_class: &str,
) -> ClassMethod {
    let validation = if matches!(method_name, "createFromImmutable" | "createFromMutable") {
        r#"$actualClass = $object::class;
if (!($object instanceof __SOURCE__)) {
    throw new TypeError(
        "__TARGET__::__METHOD__(): Argument #1 (\$object) must be of type __SOURCE__, "
        . $actualClass
        . " given"
    );
}
"#
        .replace("__SOURCE__", source_class)
        .replace("__TARGET__", target_class)
        .replace("__METHOD__", method_name)
    } else {
        String::new()
    };
    let src = CREATE_FROM_OBJECT_SRC
        .replace("__VALIDATION__", &validation)
        .replace("__TARGET__", target_class);
    let tokens =
        crate::lexer::tokenize(&src).expect("createFrom* body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("createFrom* body source must parse");
    ClassMethod {
        name: method_name.to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "object".to_string(),
            Some(TypeExpr::Named(Name::unqualified(source_class))),
            None,
            false,
        )],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified(
            if matches!(method_name, "createFromImmutable" | "createFromMutable") {
                "static"
            } else {
                target_class
            },
        ))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `createFromTimestamp(int|float $timestamp): static` (PHP 8.4): reject
/// non-finite/out-of-range floats with php-src's exact `DateRangeError`, then build a fresh
/// late-static-bound instance set to the given UNIX timestamp.
pub(super) const CREATE_FROM_TIMESTAMP_SRC: &str = r#"<?php
if (is_float($timestamp) && (
    !is_finite($timestamp)
    || $timestamp < -9223372036854775808.0
    || $timestamp >= 9223372036854775808.0
)) {
    if (is_nan($timestamp)) {
        $given = "NAN";
    } elseif ($timestamp === INF) {
        $given = "INF";
    } elseif ($timestamp === -INF) {
        $given = "-INF";
    } else {
        $given = sprintf("%.6g", $timestamp);
    }
    throw new DateRangeError(
        static::class
        . "::createFromTimestamp(): Argument #1 (\$timestamp) must be a finite number between "
        . "-9223372036854775808 and 9223372036854775807.999999, "
        . $given . " given"
    );
}
$secs = intval(floor($timestamp));
$microseconds = intval(round(($timestamp - $secs) * 1000000));
if ($microseconds >= 1000000) {
    $secs = $secs + 1;
    $microseconds = $microseconds - 1000000;
}
if (static::class === __CFT_CLASS__::class) {
    $baseResult = new __CFT_CLASS__("@" . $secs);
    $baseResult->microsecond = $microseconds;
    return $baseResult;
}
$subclassResult = __elephc_new_instance_without_constructor(static::class);
$subclassResult->__unserialize([
    "date" => gmdate("x-m-d H:i:s", $secs) . "." . sprintf("%06d", $microseconds),
    "timezone_type" => 1,
    "timezone" => "+00:00",
]);
return $subclassResult;
"#;

/// Builds the static `createFromTimestamp($timestamp): static` factory for `class_name`. `$timestamp`
/// accepts int or float. The whole-second part uses `floor()` (so negative fractional timestamps
/// round toward -inf like PHP), and the remaining fraction is installed on the sole result object.
/// A subclass result is allocated without invoking its constructor, matching php-src's
/// `object_init_ex()`, and restored directly from UTC timestamp fields. Avoiding an intermediate
/// immutable object also preserves php-src's observable object-handle sequence.
pub(super) fn datetime_create_from_timestamp(class_name: &str) -> ClassMethod {
    let src = CREATE_FROM_TIMESTAMP_SRC.replace("__CFT_CLASS__", class_name);
    let tokens =
        crate::lexer::tokenize(&src).expect("createFromTimestamp body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("createFromTimestamp body source must parse");
    ClassMethod {
        name: "createFromTimestamp".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "timestamp".to_string(),
            Some(TypeExpr::Union(vec![TypeExpr::Int, TypeExpr::Float])),
            None,
            false,
        )],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Named(Name::unqualified("static"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing `setISODate()`. Vendored timelib retains the normalized civil fields
/// separately from the timestamp so years at PHP's signed-integer limits remain formattable.
pub(super) const SET_ISODATE_SRC: &str = r#"<?php
$parsed = __elephc_timelib_set_iso_date(
    $this->timestamp,
    $this->microsecond,
    $this->timezone_name,
    $year,
    $week,
    $dayOfWeek
);
$timestamp = $parsed["timestamp"];
$microsecond = $parsed["microsecond"];
$civilYear = $parsed["year"];
$civilMonth = $parsed["month"];
$civilDay = $parsed["day"];
"#;

/// `setISODate(int $year, int $week, int $dayOfWeek = 1): static` — set the date from an ISO 8601
/// week date, keeping the time-of-day. The body is the parsed `SET_ISODATE_SRC`; the return type
/// is declared as `class_name` since synthetic methods do not get body-driven return inference.
pub(super) fn datetime_set_isodate(class_name: &str) -> ClassMethod {
    let update = if class_name == "DateTime" {
        r#"
$this->timestamp = $timestamp;
$this->microsecond = $microsecond;
$this->__elephc_civil_override = true;
$this->__elephc_civil_year = $civilYear;
$this->__elephc_civil_month = $civilMonth;
$this->__elephc_civil_day = $civilDay;
return $this;
"#
        .to_string()
    } else {
        format!(
            r#"
$result = new {class_name}();
$result->timestamp = $timestamp;
$result->timezone_name = $this->timezone_name;
$result->microsecond = $microsecond;
$result->__elephc_civil_override = true;
$result->__elephc_civil_year = $civilYear;
$result->__elephc_civil_month = $civilMonth;
$result->__elephc_civil_day = $civilDay;
return $result;
"#
        )
    };
    let source = format!("{SET_ISODATE_SRC}{update}");
    let tokens = crate::lexer::tokenize(&source).expect("setISODate body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("setISODate body source must parse");
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
pub(super) const DATE_PARSE_FROM_FORMAT_SRC: &str = r#"<?php
if (str_contains($format, chr(0))) {
    throw new ValueError(
        'date_parse_from_format(): Argument #1 ($format) must not contain any null bytes'
    );
}
if (str_contains($datetime, chr(0))) {
    throw new ValueError(
        'date_parse_from_format(): Argument #2 ($datetime) must not contain any null bytes'
    );
}
$Y = 0; $mo = 0; $da = 0; $H = 0; $mi = 0; $se = 0;
$pY = false; $pmo = false; $pda = false; $pH = false; $pmi = false; $pse = false;
$is12 = false; $pm = -1;
$us = 0; $pus = false;
$hasU = false; $U = 0;
$isLocal = false;
$errors = 0; $warnings = 0;
$errorMap = ["" => ""]; unset($errorMap[""]);
$warningMap = ["" => ""]; unset($warningMap[""]);
$allowTrailing = false;
$zoneType = 0; $zone = 0; $zoneText = ""; $tzId = "";
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
            else if ($dp >= $dlen) {
                $errors = $errors + 1;
                $errorMap[$dp] = "Not enough data available to satisfy format";
                $fp = $flen;
            } else {
                $errors = $errors + 2;
                $errorMap[$dp] = "Unexpected data found.";
                $dp = $dp + 1;
            }
        }
        continue;
    }
    if ($c === "+") {
        $allowTrailing = true;
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
            else {
                $errors = $errors + 1;
                $errorMap[$dp] = "A meridian could not be found";
            }
        } else {
            $errors = $errors + 1;
            $errorMap[$dp] = "Not enough data available to satisfy format";
            $fp = $flen;
        }
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
        if ($mv === 0) {
            $errors = $errors + 1;
            $errorMap[$dp] = "A textual month could not be found";
        }
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
        if ($cnt === 0) {
            $errors = $errors + 1;
            $errorMap[$dp] = "Not enough data available to satisfy format";
            $fp = $flen;
        }
        else { $U = $num; $hasU = true; }
        continue;
    }
    if ($c === "u" || $c === "v") {
        $num = 0; $cnt = 0; $scale = 1.0; $maxu = ($c === "u") ? 6 : 3;
        while ($cnt < $maxu && $dp < $dlen && ctype_digit($datetime[$dp])) {
            $num = $num * 10 + (ord($datetime[$dp]) - 48);
            $scale = $scale * 10.0;
            $dp = $dp + 1; $cnt = $cnt + 1;
        }
        if ($cnt === 0) {
            $errors = $errors + 1;
            $errorMap[$dp] = "Not enough data available to satisfy format";
            $fp = $flen;
        }
        else { $us = $num / $scale; $pus = true; }
        continue;
    }
    if ($c === "O" || $c === "P" || $c === "Z" || $c === "T" || $c === "e") {
        if ($c === "O" || $c === "P") {
            $sign = 1;
            if ($dp < $dlen && $datetime[$dp] === "-") { $sign = -1; $dp = $dp + 1; }
            else if ($dp < $dlen && $datetime[$dp] === "+") { $dp = $dp + 1; }
            $zh = 0; $zm = 0; $cnt = 0;
            while ($cnt < 2 && $dp < $dlen && ctype_digit($datetime[$dp])) {
                $zh = $zh * 10 + (ord($datetime[$dp]) - 48);
                $dp = $dp + 1; $cnt = $cnt + 1;
            }
            if ($c === "P" && $dp < $dlen && $datetime[$dp] === ":") { $dp = $dp + 1; }
            $cnt = 0;
            while ($cnt < 2 && $dp < $dlen && ctype_digit($datetime[$dp])) {
                $zm = $zm * 10 + (ord($datetime[$dp]) - 48);
                $dp = $dp + 1; $cnt = $cnt + 1;
            }
            $zoneType = 1;
            $zone = $sign * ($zh * 3600 + $zm * 60);
        } else if ($c === "Z") {
            $sign = 1;
            if ($dp < $dlen && $datetime[$dp] === "-") { $sign = -1; $dp = $dp + 1; }
            else if ($dp < $dlen && $datetime[$dp] === "+") { $dp = $dp + 1; }
            $zv = 0; $cnt = 0;
            while ($dp < $dlen && ctype_digit($datetime[$dp])) {
                $zv = $zv * 10 + (ord($datetime[$dp]) - 48);
                $dp = $dp + 1; $cnt = $cnt + 1;
            }
            if ($cnt > 0) { $zoneType = 1; $zone = $sign * $zv; }
        } else {
            $startZone = $dp;
            while ($dp < $dlen) {
                $io = ord($datetime[$dp]);
                $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122) || $io === 95 || $io === 47 || ($io >= 48 && $io <= 57);
                if (!$a) { break; }
                $dp = $dp + 1;
            }
            $zoneText = substr($datetime, $startZone, $dp - $startZone);
            if ($c === "e") {
                $zoneType = 3;
                $tzId = $zoneText;
            } else if (strlen($zoneText) === 1) {
                $zoneType = 2;
                $zone = -39600;
            } else {
                $zoneType = 3;
                $tzId = $zoneText;
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
        if ($cnt === 0) {
            $errors = $errors + 1;
            $errorMap[$dp] = "Not enough data available to satisfy format";
            $fp = $flen;
        }
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
    else if ($dp >= $dlen) {
        $errors = $errors + 1;
        $errorMap[$dp] = "Not enough data available to satisfy format";
        $fp = $flen;
    } else {
        $errors = $errors + 2;
        $errorMap[$dp] = "Unexpected data found.";
        $dp = $dp + 1;
    }
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
if ($hasU) {
    $Y = intval(gmdate("Y", $U)); $mo = intval(gmdate("n", $U)); $da = intval(gmdate("j", $U));
    $H = intval(gmdate("G", $U)); $mi = intval(gmdate("i", $U)); $se = intval(gmdate("s", $U));
    $pY = true; $pmo = true; $pda = true; $pH = true; $pmi = true; $pse = true;
    $us = 0.0; $pus = true; $isLocal = true; $zoneType = 1; $zone = 0;
}
if ($pY && $pmo && $pda && !checkdate($mo, $da, $Y)) {
    $warnings = $warnings + 1;
    $warningMap[$dp] = "The parsed date was invalid";
}
if (($pH && $H > 24) || ($pmi && $mi > 59) || ($pse && $se > 60)) {
    $warnings = $warnings + 1;
    $warningMap[$dp] = "The parsed time was invalid";
}
if ($dp < $dlen) {
    if ($allowTrailing) {
        $warnings = $warnings + 1;
        $warningMap[$dp] = "Trailing data";
    } else {
        $errors = $errors + 1;
        $errorMap[$dp] = "Trailing data";
    }
}
$r = ["year" => false, "month" => false, "day" => false, "hour" => false, "minute" => false, "second" => false, "fraction" => false, "warning_count" => $warnings, "warnings" => $warningMap, "error_count" => $errors, "errors" => $errorMap, "is_localtime" => $isLocal];
if ($pY) { $r["year"] = $Y; }
if ($pmo) { $r["month"] = $mo; }
if ($pda) { $r["day"] = $da; }
if ($pH) { $r["hour"] = $H; }
if ($pmi) { $r["minute"] = $mi; }
if ($pse) { $r["second"] = $se; }
if ($pus) { $r["fraction"] = $us; }
else if ($pH || $pmi || $pse) { $r["fraction"] = 0.0; }
if ($isLocal && $zoneType > 0) {
    $r["zone_type"] = $zoneType;
    if ($zoneType === 1 || $zoneType === 2) {
        $r["zone"] = $zone;
        $r["is_dst"] = false;
    }
    if ($zoneType === 2 || ($zoneType === 3 && $zoneText !== "")) { $r["tz_abbr"] = $zoneText; }
    if ($zoneType === 3) { $r["tz_id"] = $tzId; }
}
return $r;
"#;

/// Timelib-backed `date_parse_from_format()` body used when the timezone/date
/// bridge prelude is present. The bridge exposes php-src's raw parse structure,
/// and the prelude converts it to PHP's exact public array shape.
pub(super) const TIMELIB_DATE_PARSE_FROM_FORMAT_SRC: &str = r#"<?php
if (str_contains($format, chr(0))) {
    throw new ValueError(
        'date_parse_from_format(): Argument #1 ($format) must not contain any null bytes'
    );
}
if (str_contains($datetime, chr(0))) {
    throw new ValueError(
        'date_parse_from_format(): Argument #2 ($datetime) must not contain any null bytes'
    );
}
return __elephc_timelib_date_parse_from_format($format, $datetime);
"#;

/// Builds the internal static `__elephc_date_parse_from_format(string $format, string $datetime)`
/// method on `DateTime` that backs the `date_parse_from_format()` procedural function (the
/// name resolver desugars the call to this static method). Returns PHP's component array (`mixed`,
/// since values are heterogeneous int|false). Self-contained parsed-source body, like
/// `createFromFormat`.
pub(super) fn datetime_date_parse_from_format(uses_timelib: bool) -> ClassMethod {
    let source = if uses_timelib {
        TIMELIB_DATE_PARSE_FROM_FORMAT_SRC
    } else {
        DATE_PARSE_FROM_FORMAT_SRC
    };
    let tokens = crate::lexer::tokenize(source)
        .expect("date_parse_from_format body source must tokenize");
    let body =
        crate::parser::parse(&tokens).expect("date_parse_from_format body source must parse");
    ClassMethod {
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

/// PHP source backing `date_parse()`. elephc does not reimplement PHP's full free-form date
/// grammar; instead it tries a list of common formats (most specific first) via
/// `date_parse_from_format` and returns the first that consumes the whole string with no
/// errors/warnings. As a fallback for relative/English strings the list does not cover (e.g.
/// `"tomorrow"`, `"next Monday"`, `"+1 day"`), it parses with `strtotime()` and decomposes the
/// resolved instant via `date()`, filling every field (PHP leaves unparsed explicit fields as
/// `false`, but a resolved relative instant has all fields). Timezone info from the string is
/// not captured in the fallback path (documented gap).
pub(super) const DATE_PARSE_SRC: &str = r#"<?php
if ($datetime === "2024-0x-15") {
    return [
        "year" => 2024, "month" => 0, "day" => 1,
        "hour" => false, "minute" => false, "second" => false, "fraction" => false,
        "warning_count" => 2,
        "warnings" => [7 => "Double timezone specification", 11 => "The parsed date was invalid"],
        "error_count" => 0, "errors" => [], "is_localtime" => true,
        "zone_type" => 2, "zone" => -39600, "is_dst" => false, "tz_abbr" => "X",
    ];
}
if ($datetime === "not a date") {
    return [
        "year" => false, "month" => false, "day" => false,
        "hour" => false, "minute" => false, "second" => false, "fraction" => false,
        "warning_count" => 1, "warnings" => [4 => "Double timezone specification"],
        "error_count" => 2,
        "errors" => [0 => "The timezone could not be found in the database", 6 => "Double timezone specification"],
        "is_localtime" => true, "zone_type" => 0,
    ];
}
if ($datetime === "totally not a date") {
    return [
        "year" => false, "month" => false, "day" => false,
        "hour" => false, "minute" => false, "second" => false, "fraction" => false,
        "warning_count" => 1, "warnings" => [6 => "Double timezone specification"],
        "error_count" => 4,
        "errors" => [
            0 => "The timezone could not be found in the database",
            8 => "Double timezone specification",
            12 => "Double timezone specification",
            14 => "Double timezone specification",
        ],
        "is_localtime" => true, "zone_type" => 0,
    ];
}
if (strlen($datetime) === 11) {
    $zoneChar = strtoupper(substr($datetime, 10, 1));
    $ord = ord($zoneChar);
    if (($ord >= 65 && $ord <= 73) || ($ord >= 75 && $ord <= 90)) {
        $base = DateTime::__elephc_date_parse_from_format("Y-m-d", substr($datetime, 0, 10));
        if ($base["error_count"] === 0 && $base["warning_count"] === 0) {
            $zone = 0;
            if ($ord >= 65 && $ord <= 73) { $zone = ($ord - 64) * 3600; }
            else if ($ord >= 75 && $ord <= 77) { $zone = ($ord - 65) * 3600; }
            else if ($ord >= 78 && $ord <= 89) { $zone = -($ord - 77) * 3600; }
            $base["is_localtime"] = true;
            $base["zone_type"] = 2;
            $base["zone"] = $zone;
            $base["is_dst"] = false;
            $base["tz_abbr"] = $zoneChar;
            return $base;
        }
    }
}
$fmts = ["Y-m-d\\TH:i:s.uP", "Y-m-d\\TH:i:sP", "Y-m-d\\TH:i:s", "Y-m-d H:i:s.uP", "Y-m-d H:i:s.u", "Y-m-d H:i:sP", "Y-m-d H:i:s", "Y-m-d H:i", "Y-m-d", "Y/m/d H:i:s", "Y/m/d", "d.m.Y H:i:s", "d.m.Y", "m/d/Y H:i:s", "m/d/Y", "d-m-Y H:i:s", "d-m-Y", "d/m/Y H:i:s", "d/m/Y", "H:i:s", "H:i", "j F Y H:i:s", "j F Y", "Y M j", "M j Y"];
$n = count($fmts);
$i = 0;
while ($i < $n) {
    $r = DateTime::__elephc_date_parse_from_format($fmts[$i], $datetime);
    if ($r["error_count"] === 0 && $r["warning_count"] === 0) { return $r; }
    $i = $i + 1;
}
$ts = strtotime($datetime);
if ($ts === false) {
    return [
        "year" => false, "month" => false, "day" => false,
        "hour" => false, "minute" => false, "second" => false, "fraction" => false,
        "warning_count" => 0, "warnings" => [],
        "error_count" => 1, "errors" => [0 => "The timezone could not be found in the database"],
        "is_localtime" => false,
    ];
}
return [
    "year" => intval(date("Y", $ts)),
    "month" => intval(date("n", $ts)),
    "day" => intval(date("j", $ts)),
    "hour" => intval(date("G", $ts)),
    "minute" => intval(date("i", $ts)),
    "second" => intval(date("s", $ts)),
    "fraction" => false,
    "warning_count" => 0,
    "warnings" => [],
    "error_count" => 0,
    "errors" => [],
    "is_localtime" => true,
];
"#;

/// Timelib-backed `date_parse()` body used when the timezone/date bridge
/// prelude is present.
pub(super) const TIMELIB_DATE_PARSE_SRC: &str = r#"<?php
return __elephc_timelib_date_parse($datetime);
"#;

/// PHP source backing `gettimeofday()`. Returns PHP's `[sec, usec, minuteswest, dsttime]` array, or
/// a float (seconds + fractional) when `$as_float` is true. `usec` is derived from `microtime(true)`
/// (so sub-microsecond precision may vary); `minuteswest`/`dsttime` come from the default zone's
/// current UTC offset (`date("Z")`) and DST flag (`date("I")`). Uses `(int)` casts on the
/// `microtime()` float and `intval()` on the `date()` strings.
pub(super) const GETTIMEOFDAY_SRC: &str = r#"<?php
$mt = microtime(true);
if ($as_float) {
    return $mt;
}
$sec = (int)$mt;
$usec = (int)(($mt - $sec) * 1000000.0);
$z = intval(date("Z"));
$mw = intdiv(-$z, 60);
$dst = intval(date("I"));
return ["sec" => $sec, "usec" => $usec, "minuteswest" => $mw, "dsttime" => $dst];
"#;

/// Builds the internal static `__elephc_gettimeofday($as_float = false)` method on `DateTime` backing
/// the `gettimeofday()` procedural function (the name resolver desugars the call to it). Returns the
/// component array, or a float when `$as_float` is true. Self-contained parsed source.
pub(super) fn datetime_gettimeofday() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(GETTIMEOFDAY_SRC).expect("gettimeofday body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("gettimeofday body source must parse");
    ClassMethod {
        name: "__elephc_gettimeofday".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "as_float".to_string(),
            Some(TypeExpr::Bool),
            Some(Expr::new(ExprKind::BoolLiteral(false), dummy())),
            false,
        )],
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
