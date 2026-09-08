//! Purpose:
//! Supplies php-src signatures for ext/date functions and procedural aliases.
//!
//! Called from:
//! - Reflection constructor validation and Reflection code generation.
//!
//! Key details:
//! - Direct calls, first-class callables, and Reflection share this table so nullable
//!   parameters and union returns stay coherent with php-src.
//! - Parameter names, nullable types, defaults, returns, and deprecations mirror
//!   PHP 8.5's `ext/date/php_date.stub.php` (plus standard's deprecated `strptime`).

use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, TypeExpr};
use crate::span::Span;

use super::{FunctionSig, PhpType};

type ReflectedParam = (String, PhpType, TypeExpr, Option<Expr>);

/// Returns a Reflection-compatible signature for a registry builtin or rewritten date alias.
pub(crate) fn reflection_builtin_function_sig(name: &str) -> Option<FunctionSig> {
    php_src_date_function_sig(name)
        .or_else(|| crate::builtins::registry::first_class_callable_sig(name))
}

/// Returns php-src's exact signature for one date/time function overridden by this table.
pub(crate) fn php_src_date_function_sig(name: &str) -> Option<FunctionSig> {
    reflected_date_alias_sig(name)
}

/// Returns whether php-src exposes `method_name` on one ext/date class.
///
/// `None` means the class is not owned by ext/date and ordinary class metadata
/// remains authoritative. Synthetic implementation helpers deliberately stay
/// callable internally but are excluded from PHP-visible Reflection.
pub(crate) fn php_src_date_method_visible(
    class_name: &str,
    method_name: &str,
) -> Option<bool> {
    php_src_date_method_names(class_name).map(|names| {
        names
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(method_name))
    })
}

/// Returns php-src's canonical, declaration-ordered method surface for ext/date.
pub(crate) fn php_src_date_method_names(class_name: &str) -> Option<&'static [&'static str]> {
    match crate::names::php_symbol_key(class_name.trim_start_matches('\\')).as_str() {
        "datetimeinterface" => Some(&[
            "format",
            "getTimezone",
            "getOffset",
            "getTimestamp",
            "getMicrosecond",
            "diff",
            "__wakeup",
            "__serialize",
            "__unserialize",
        ]),
        "datetime" => Some(&[
            "__construct",
            "__serialize",
            "__unserialize",
            "__wakeup",
            "__set_state",
            "createFromImmutable",
            "createFromInterface",
            "createFromFormat",
            "createFromTimestamp",
            "getLastErrors",
            "format",
            "modify",
            "add",
            "sub",
            "getTimezone",
            "setTimezone",
            "getOffset",
            "getMicrosecond",
            "setTime",
            "setDate",
            "setISODate",
            "setTimestamp",
            "setMicrosecond",
            "getTimestamp",
            "diff",
        ]),
        "datetimeimmutable" => Some(&[
            "__construct",
            "__serialize",
            "__unserialize",
            "__wakeup",
            "__set_state",
            "createFromFormat",
            "createFromTimestamp",
            "getLastErrors",
            "format",
            "getTimezone",
            "getOffset",
            "getTimestamp",
            "getMicrosecond",
            "diff",
            "modify",
            "add",
            "sub",
            "setTimezone",
            "setTime",
            "setDate",
            "setISODate",
            "setTimestamp",
            "setMicrosecond",
            "createFromMutable",
            "createFromInterface",
        ]),
        "datetimezone" => Some(&[
            "__construct",
            "getName",
            "getOffset",
            "getTransitions",
            "getLocation",
            "listAbbreviations",
            "listIdentifiers",
            "__serialize",
            "__unserialize",
            "__wakeup",
            "__set_state",
        ]),
        "dateinterval" => Some(&[
            "__construct",
            "createFromDateString",
            "format",
            "__serialize",
            "__unserialize",
            "__wakeup",
            "__set_state",
        ]),
        "dateperiod" => Some(&[
            "createFromISO8601String",
            "__construct",
            "getStartDate",
            "getEndDate",
            "getDateInterval",
            "getRecurrences",
            "__serialize",
            "__unserialize",
            "__wakeup",
            "__set_state",
            "getIterator",
        ]),
        _ => None,
    }
}

/// Returns php-src's declaration-ordered `ReflectionClass` property surface for ext/date.
///
/// Date/time objects keep their implementation storage behind native object handlers.
/// `DatePeriod` is the sole date class with class-level properties in PHP 8.5: seven
/// public virtual properties. `DateInterval` exposes state-dependent debug properties
/// through `ReflectionObject`, but none through `ReflectionClass`.
pub(crate) fn php_src_date_property_names(class_name: &str) -> Option<&'static [&'static str]> {
    match crate::names::php_symbol_key(class_name.trim_start_matches('\\')).as_str() {
        "datetime"
        | "datetimeimmutable"
        | "datetimezone"
        | "dateinterval"
        | "datetimeinterface" => Some(&[]),
        "dateperiod" => Some(&[
            "start",
            "current",
            "end",
            "interval",
            "recurrences",
            "include_start_date",
            "include_end_date",
        ]),
        _ => None,
    }
}

/// Returns php-src's canonical spelling for one visible ext/date method.
pub(crate) fn php_src_date_method_canonical_name(
    class_name: &str,
    method_name: &str,
) -> Option<&'static str> {
    php_src_date_method_names(class_name)?
        .iter()
        .copied()
        .find(|candidate| candidate.eq_ignore_ascii_case(method_name))
}

/// Returns a php-src parameter type that intentionally differs from the executable helper type.
///
/// The add/sub helpers accept `mixed` internally so they can raise php-src's runtime `TypeError`
/// with the rejected value's exact debug type. Reflection must still expose `DateInterval`.
pub(crate) fn php_src_date_method_parameter_type(
    class_name: &str,
    method_name: &str,
    parameter_index: usize,
) -> Option<TypeExpr> {
    let class_key = crate::names::php_symbol_key(class_name.trim_start_matches('\\'));
    let method_key = crate::names::php_symbol_key(method_name);
    if matches!(class_key.as_str(), "datetime" | "datetimeimmutable")
        && matches!(method_key.as_str(), "add" | "sub")
        && parameter_index == 0
    {
        return Some(TypeExpr::Named(Name::unqualified("DateInterval")));
    }
    None
}

/// Returns php-src's Reflection return contract for executable helpers whose broader internal
/// type is required by the compiler backend.
pub(crate) fn php_src_date_method_return_type(
    class_name: &str,
    method_name: &str,
) -> Option<TypeExpr> {
    let class_key = crate::names::php_symbol_key(class_name.trim_start_matches('\\'));
    let method_key = crate::names::php_symbol_key(method_name);
    match (class_key.as_str(), method_key.as_str()) {
        ("datetime", "createfromimmutable")
        | ("datetimeimmutable", "createfrommutable") => {
            Some(TypeExpr::Named(Name::unqualified("static")))
        }
        ("datetime", "getlasterrors") | ("datetimeimmutable", "getlasterrors") => {
            Some(TypeExpr::Union(vec![
                TypeExpr::Named(Name::unqualified("array")),
                TypeExpr::False,
            ]))
        }
        ("datetimezone", "gettransitions") | ("datetimezone", "getlocation") => {
            Some(TypeExpr::Union(vec![
                TypeExpr::Named(Name::unqualified("array")),
                TypeExpr::False,
            ]))
        }
        ("datetime", "gettimezone") | ("datetimeimmutable", "gettimezone") => {
            Some(TypeExpr::Union(vec![
                TypeExpr::Named(Name::unqualified("DateTimeZone")),
                TypeExpr::False,
            ]))
        }
        ("datetimezone", "listidentifiers") => {
            Some(TypeExpr::Named(Name::unqualified("array")))
        }
        _ => None,
    }
}

/// Returns the php-src signature for one rewritten procedural date/time alias.
fn reflected_date_alias_sig(name: &str) -> Option<FunctionSig> {
    let signature = match name {
        "strtotime" => sig(
            vec![
                param("datetime", PhpType::Str, TypeExpr::Str, None),
                nullable_int_param("baseTimestamp", null_default()),
            ],
            union(vec![PhpType::Int, PhpType::False]),
            None,
        ),
        "date" | "gmdate" => sig(
            vec![
                param("format", PhpType::Str, TypeExpr::Str, None),
                nullable_int_param("timestamp", null_default()),
            ],
            PhpType::Str,
            None,
        ),
        "mktime" | "gmmktime" => sig(
            vec![
                param("hour", PhpType::Int, TypeExpr::Int, None),
                nullable_int_param("minute", null_default()),
                nullable_int_param("second", null_default()),
                nullable_int_param("month", null_default()),
                nullable_int_param("day", null_default()),
                nullable_int_param("year", null_default()),
            ],
            union(vec![PhpType::Int, PhpType::False]),
            None,
        ),
        "localtime" => sig(
            vec![
                nullable_int_param("timestamp", null_default()),
                param(
                    "associative",
                    PhpType::Bool,
                    TypeExpr::Bool,
                    bool_default(false),
                ),
            ],
            array_type(),
            None,
        ),
        "getdate" => sig(
            vec![nullable_int_param("timestamp", null_default())],
            array_type(),
            None,
        ),
        "idate" => sig(
            vec![
                param("format", PhpType::Str, TypeExpr::Str, None),
                nullable_int_param("timestamp", null_default()),
            ],
            union(vec![PhpType::Int, PhpType::False]),
            None,
        ),
        "date_create" => sig(
            vec![
                param("datetime", PhpType::Str, TypeExpr::Str, string_default("now")),
                nullable_object_param("timezone", "DateTimeZone", null_default()),
            ],
            union(vec![object_type("DateTime"), PhpType::False]),
            None,
        ),
        "date_create_immutable" => sig(
            vec![
                param("datetime", PhpType::Str, TypeExpr::Str, string_default("now")),
                nullable_object_param("timezone", "DateTimeZone", null_default()),
            ],
            union(vec![object_type("DateTimeImmutable"), PhpType::False]),
            None,
        ),
        "date_create_from_format" => create_from_format_sig("DateTime"),
        "date_create_immutable_from_format" => create_from_format_sig("DateTimeImmutable"),
        "date_parse_from_format" => sig(
            vec![
                param("format", PhpType::Str, TypeExpr::Str, None),
                param("datetime", PhpType::Str, TypeExpr::Str, None),
            ],
            array_type(),
            None,
        ),
        "date_parse" => sig(
            vec![param("datetime", PhpType::Str, TypeExpr::Str, None)],
            array_type(),
            None,
        ),
        "date_get_last_errors" => sig(
            Vec::new(),
            union(vec![array_type(), PhpType::False]),
            None,
        ),
        "date_format" => sig(
            vec![
                object_param("object", "DateTimeInterface"),
                param("format", PhpType::Str, TypeExpr::Str, None),
            ],
            PhpType::Str,
            None,
        ),
        "date_modify" => sig(
            vec![
                object_param("object", "DateTime"),
                param("modifier", PhpType::Str, TypeExpr::Str, None),
            ],
            union(vec![object_type("DateTime"), PhpType::False]),
            None,
        ),
        "date_add" | "date_sub" => sig(
            vec![
                object_param("object", "DateTime"),
                object_param("interval", "DateInterval"),
            ],
            object_type("DateTime"),
            None,
        ),
        "date_timezone_get" => sig(
            vec![object_param("object", "DateTimeInterface")],
            union(vec![object_type("DateTimeZone"), PhpType::False]),
            None,
        ),
        "date_timezone_set" => sig(
            vec![
                object_param("object", "DateTime"),
                object_param("timezone", "DateTimeZone"),
            ],
            object_type("DateTime"),
            None,
        ),
        "date_offset_get" => sig(
            vec![object_param("object", "DateTimeInterface")],
            PhpType::Int,
            None,
        ),
        "date_diff" => sig(
            vec![
                object_param("baseObject", "DateTimeInterface"),
                object_param("targetObject", "DateTimeInterface"),
                param("absolute", PhpType::Bool, TypeExpr::Bool, bool_default(false)),
            ],
            object_type("DateInterval"),
            None,
        ),
        "date_time_set" => sig(
            vec![
                object_param("object", "DateTime"),
                param("hour", PhpType::Int, TypeExpr::Int, None),
                param("minute", PhpType::Int, TypeExpr::Int, None),
                param("second", PhpType::Int, TypeExpr::Int, int_default(0)),
                param("microsecond", PhpType::Int, TypeExpr::Int, int_default(0)),
            ],
            object_type("DateTime"),
            None,
        ),
        "date_date_set" => sig(
            vec![
                object_param("object", "DateTime"),
                param("year", PhpType::Int, TypeExpr::Int, None),
                param("month", PhpType::Int, TypeExpr::Int, None),
                param("day", PhpType::Int, TypeExpr::Int, None),
            ],
            object_type("DateTime"),
            None,
        ),
        "date_isodate_set" => sig(
            vec![
                object_param("object", "DateTime"),
                param("year", PhpType::Int, TypeExpr::Int, None),
                param("week", PhpType::Int, TypeExpr::Int, None),
                param("dayOfWeek", PhpType::Int, TypeExpr::Int, int_default(1)),
            ],
            object_type("DateTime"),
            None,
        ),
        "date_timestamp_set" => sig(
            vec![
                object_param("object", "DateTime"),
                param("timestamp", PhpType::Int, TypeExpr::Int, None),
            ],
            object_type("DateTime"),
            None,
        ),
        "date_timestamp_get" => sig(
            vec![object_param("object", "DateTimeInterface")],
            PhpType::Int,
            None,
        ),
        "date_interval_create_from_date_string" => sig(
            vec![param("datetime", PhpType::Str, TypeExpr::Str, None)],
            union(vec![object_type("DateInterval"), PhpType::False]),
            None,
        ),
        "date_interval_format" => sig(
            vec![
                object_param("object", "DateInterval"),
                param("format", PhpType::Str, TypeExpr::Str, None),
            ],
            PhpType::Str,
            None,
        ),
        "strftime" | "gmstrftime" => sig(
            vec![
                param("format", PhpType::Str, TypeExpr::Str, None),
                nullable_int_param("timestamp", null_default()),
            ],
            union(vec![PhpType::Str, PhpType::False]),
            Some("use IntlDateFormatter::format() instead"),
        ),
        "strptime" => sig(
            vec![
                param("timestamp", PhpType::Str, TypeExpr::Str, None),
                param("format", PhpType::Str, TypeExpr::Str, None),
            ],
            union(vec![array_type(), PhpType::False]),
            Some(
                "use date_parse_from_format() (for locale-independent parsing), or \
                 IntlDateFormatter::parse() (for locale-dependent parsing) instead",
            ),
        ),
        "timezone_open" => sig(
            vec![param("timezone", PhpType::Str, TypeExpr::Str, None)],
            union(vec![object_type("DateTimeZone"), PhpType::False]),
            None,
        ),
        "timezone_name_get" => sig(
            vec![object_param("object", "DateTimeZone")],
            PhpType::Str,
            None,
        ),
        "timezone_name_from_abbr" => sig(
            vec![
                param("abbr", PhpType::Str, TypeExpr::Str, None),
                param("utcOffset", PhpType::Int, TypeExpr::Int, int_default(-1)),
                param("isDST", PhpType::Int, TypeExpr::Int, int_default(-1)),
            ],
            union(vec![PhpType::Str, PhpType::False]),
            None,
        ),
        "timezone_offset_get" => sig(
            vec![
                object_param("object", "DateTimeZone"),
                object_param("datetime", "DateTimeInterface"),
            ],
            PhpType::Int,
            None,
        ),
        "timezone_transitions_get" => sig(
            vec![
                object_param("object", "DateTimeZone"),
                param(
                    "timestampBegin",
                    PhpType::Int,
                    TypeExpr::Int,
                    global_constant_default("PHP_INT_MIN"),
                ),
                param(
                    "timestampEnd",
                    PhpType::Int,
                    TypeExpr::Int,
                    int_default(2_147_483_647),
                ),
            ],
            union(vec![array_type(), PhpType::False]),
            None,
        ),
        "timezone_location_get" => sig(
            vec![object_param("object", "DateTimeZone")],
            union(vec![array_type(), PhpType::False]),
            None,
        ),
        "timezone_identifiers_list" => sig(
            vec![
                param(
                    "timezoneGroup",
                    PhpType::Int,
                    TypeExpr::Int,
                    class_constant_default("DateTimeZone", "ALL"),
                ),
                nullable_string_param("countryCode", null_default()),
            ],
            array_type(),
            None,
        ),
        "timezone_abbreviations_list" | "timezone_version_get" => sig(
            Vec::new(),
            if name == "timezone_version_get" {
                PhpType::Str
            } else {
                array_type()
            },
            None,
        ),
        "date_sunrise" | "date_sunset" => sig(
            vec![
                param("timestamp", PhpType::Int, TypeExpr::Int, None),
                param(
                    "returnFormat",
                    PhpType::Int,
                    TypeExpr::Int,
                    global_constant_default("SUNFUNCS_RET_STRING"),
                ),
                nullable_float_param("latitude", null_default()),
                nullable_float_param("longitude", null_default()),
                nullable_float_param("zenith", null_default()),
                nullable_float_param("utcOffset", null_default()),
            ],
            union(vec![
                PhpType::Str,
                PhpType::Int,
                PhpType::Float,
                PhpType::False,
            ]),
            Some("use date_sun_info() instead"),
        ),
        "date_sun_info" => sig(
            vec![
                param("timestamp", PhpType::Int, TypeExpr::Int, None),
                param("latitude", PhpType::Float, TypeExpr::Float, None),
                param("longitude", PhpType::Float, TypeExpr::Float, None),
            ],
            array_type(),
            None,
        ),
        _ => return None,
    };
    Some(signature)
}

/// Builds the common `date_create*_from_format()` signature for one result class.
fn create_from_format_sig(class_name: &str) -> FunctionSig {
    sig(
        vec![
            param("format", PhpType::Str, TypeExpr::Str, None),
            param("datetime", PhpType::Str, TypeExpr::Str, None),
            nullable_object_param("timezone", "DateTimeZone", null_default()),
        ],
        union(vec![object_type(class_name), PhpType::False]),
        None,
    )
}

/// Builds a fully declared signature from Reflection parameter metadata.
fn sig(
    params: Vec<ReflectedParam>,
    return_type: PhpType,
    deprecation: Option<&str>,
) -> FunctionSig {
    let mut names_and_types = Vec::with_capacity(params.len());
    let mut type_exprs = Vec::with_capacity(params.len());
    let mut defaults = Vec::with_capacity(params.len());
    for (name, php_type, type_expr, default) in params {
        names_and_types.push((name, php_type));
        type_exprs.push(Some(type_expr));
        defaults.push(default);
    }
    let param_count = names_and_types.len();
    FunctionSig {
        params: names_and_types,
        param_type_exprs: type_exprs,
        param_attributes: vec![Vec::new(); param_count],
        defaults,
        return_type,
        declared_return: true,
        by_ref_return: false,
        ref_params: vec![false; param_count],
        declared_params: vec![true; param_count],
        variadic: None,
        deprecation: deprecation.map(str::to_string),
    }
}

/// Builds one reflected parameter tuple.
fn param(
    name: &str,
    php_type: PhpType,
    type_expr: TypeExpr,
    default: Option<Expr>,
) -> ReflectedParam {
    (name.to_string(), php_type, type_expr, default)
}

/// Builds a required object parameter.
fn object_param(name: &str, class_name: &str) -> ReflectedParam {
    param(
        name,
        object_type(class_name),
        TypeExpr::Named(Name::unqualified(class_name)),
        None,
    )
}

/// Builds a nullable object parameter with its declared default.
fn nullable_object_param(name: &str, class_name: &str, default: Option<Expr>) -> ReflectedParam {
    param(
        name,
        union(vec![object_type(class_name), PhpType::Void]),
        TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(class_name)))),
        default,
    )
}

/// Builds a nullable integer parameter with its declared default.
fn nullable_int_param(name: &str, default: Option<Expr>) -> ReflectedParam {
    param(
        name,
        union(vec![PhpType::Int, PhpType::Void]),
        TypeExpr::Nullable(Box::new(TypeExpr::Int)),
        default,
    )
}

/// Builds a nullable float parameter with its declared default.
fn nullable_float_param(name: &str, default: Option<Expr>) -> ReflectedParam {
    param(
        name,
        union(vec![PhpType::Float, PhpType::Void]),
        TypeExpr::Nullable(Box::new(TypeExpr::Float)),
        default,
    )
}

/// Builds a nullable string parameter with its declared default.
fn nullable_string_param(name: &str, default: Option<Expr>) -> ReflectedParam {
    param(
        name,
        union(vec![PhpType::Str, PhpType::Void]),
        TypeExpr::Nullable(Box::new(TypeExpr::Str)),
        default,
    )
}

/// Returns an object type for a reflected class name.
fn object_type(class_name: &str) -> PhpType {
    PhpType::Object(class_name.to_string())
}

/// Returns PHP's generic `array` type.
fn array_type() -> PhpType {
    PhpType::Array(Box::new(PhpType::Mixed))
}

/// Returns a union type in php-src declaration order.
fn union(members: Vec<PhpType>) -> PhpType {
    PhpType::Union(members)
}

/// Builds a `null` default expression.
fn null_default() -> Option<Expr> {
    Some(Expr::new(ExprKind::Null, Span::dummy()))
}

/// Builds an integer default expression.
fn int_default(value: i64) -> Option<Expr> {
    Some(Expr::new(ExprKind::IntLiteral(value), Span::dummy()))
}

/// Builds a class-constant default expression retained by Reflection metadata.
fn class_constant_default(class_name: &str, constant_name: &str) -> Option<Expr> {
    Some(Expr::new(
        ExprKind::ScopedConstantAccess {
            receiver: StaticReceiver::Named(Name::unqualified(class_name)),
            name: constant_name.to_string(),
        },
        Span::dummy(),
    ))
}

/// Builds an internal marker for a global-constant parameter default.
///
/// The ordinary AST folds `PHP_INT_MIN` into a literal, so Reflection would lose
/// its source-visible constant name. Codegen recognizes this private receiver and
/// materializes the php-src value/name pair without exposing the marker to PHP.
fn global_constant_default(constant_name: &str) -> Option<Expr> {
    class_constant_default("__ElephcReflectionGlobalConstant", constant_name)
}

/// Builds a boolean default expression.
fn bool_default(value: bool) -> Option<Expr> {
    Some(Expr::new(ExprKind::BoolLiteral(value), Span::dummy()))
}

/// Builds a string default expression.
fn string_default(value: &str) -> Option<Expr> {
    Some(Expr::new(
        ExprKind::StringLiteral(value.to_string()),
        Span::dummy(),
    ))
}
