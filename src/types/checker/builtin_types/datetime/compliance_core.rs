//! Purpose:
//! Core audited DateTime metadata, initialization guards, timezone declarations, and shared AST helpers.
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

pub(super) fn dummy() -> crate::span::Span {
    crate::span::Span::dummy()
}

/// Builds one fully-qualified builtin attribute group with named string arguments.
pub(super) fn builtin_string_attribute(name: &str, args: &[(&str, &str)]) -> Vec<AttributeGroup> {
    let arguments = args
        .iter()
        .map(|(argument, value)| {
            Expr::new(
                ExprKind::NamedArg {
                    name: (*argument).to_string(),
                    value: Box::new(Expr::new(
                        ExprKind::StringLiteral((*value).to_string()),
                        dummy(),
                    )),
                },
                dummy(),
            )
        })
        .collect();
    vec![AttributeGroup {
        attributes: vec![Attribute {
            name: Name::from_parts(NameKind::FullyQualified, vec![name.to_string()]),
            args: arguments,
            span: dummy(),
        }],
        span: dummy(),
    }]
}

/// Builds php-src's PHP 8.5 date/time `#[\Deprecated]` metadata.
pub(in crate::types::checker::builtin_types) fn deprecated_attribute(
    since: &str,
    message: &str,
) -> Vec<AttributeGroup> {
    builtin_string_attribute("Deprecated", &[("since", since), ("message", message)])
}

/// Builds php-src's `#[\NoDiscard]` metadata for an immutable mutator.
pub(super) fn no_discard_attribute(method: &str) -> Vec<AttributeGroup> {
    let message = format!(
        "as DateTimeImmutable::{method}() does not modify the object itself"
    );
    builtin_string_attribute("NoDiscard", &[("message", message.as_str())])
}

/// Builds a public string class constant for the synthetic date/time classes.
pub(super) fn str_class_const(name: &str, value: &str) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        type_expr: Some(TypeExpr::Str),
        value: Expr::new(ExprKind::StringLiteral(value.to_string()), dummy()),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a public integer class constant for the synthetic date/time classes.
pub(super) fn int_class_const(name: &str, value: i64) -> ClassConst {
    ClassConst {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_final: false,
        type_expr: Some(TypeExpr::Int),
        value: Expr::new(ExprKind::IntLiteral(value), dummy()),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds an internal typed passthrough used for deprecated predefined constants.
///
/// Name resolution wraps each deprecated constant read in one of these methods so
/// the suppression-aware diagnostic is emitted at the PHP-observable access point
/// while the original constant type and value are preserved.
pub(super) fn deprecated_constant_passthrough(name: &str, value_type: TypeExpr) -> ClassMethod {
    let src = r#"<?php
__elephc_diag_warning($message, $line, E_DEPRECATED);
return $value;
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("deprecated constant passthrough body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("deprecated constant passthrough body source must parse");
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("value".to_string(), Some(value_type.clone()), None, false),
            ("message".to_string(), Some(TypeExpr::Str), None, false),
            ("line".to_string(), Some(TypeExpr::Int), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(value_type),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the hidden runtime `TypeError` helper used by dynamic ext/date arguments.
pub(super) fn datetime_argument_type_error() -> ClassMethod {
    let src = r#"<?php
if (is_object($value)) {
    $actual = get_class($value);
} elseif (is_array($value)) {
    $actual = "array";
} elseif (is_int($value)) {
    $actual = "int";
} elseif (is_float($value)) {
    $actual = "float";
} elseif (is_bool($value)) {
    $actual = "bool";
} elseif (is_string($value)) {
    $actual = "string";
} elseif (is_null($value)) {
    $actual = "null";
} elseif (is_resource($value)) {
    $actual = "resource";
} else {
    $actual = "unknown";
}
throw new TypeError($prefix . $actual . " given");
"#;
    let tokens =
        crate::lexer::tokenize(src).expect("date argument type-error helper must tokenize");
    let body =
        crate::parser::parse(&tokens).expect("date argument type-error helper must parse");
    ClassMethod {
        name: "__elephc_argument_type_error".to_string(),
        visibility: Visibility::Private,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "value".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            ),
            ("prefix".to_string(), Some(TypeExpr::Str), None, false),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Never),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds php-src's weak string-parameter coercion for dynamic ext/date values.
pub(super) fn datetime_weak_string_argument() -> ClassMethod {
    let src = r#"<?php
if (is_array($value) || (is_object($value) && !($value instanceof Stringable))) {
    if ($fixedError !== "") {
        throw new TypeError($fixedError);
    }
    DateTime::__elephc_argument_type_error($value, $prefix);
}
return (string) $value;
"#;
    let tokens =
        crate::lexer::tokenize(src).expect("date weak string argument helper must tokenize");
    let body =
        crate::parser::parse(&tokens).expect("date weak string argument helper must parse");
    ClassMethod {
        name: "__elephc_weak_string_argument".to_string(),
        visibility: Visibility::Private,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "value".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            ),
            ("prefix".to_string(), Some(TypeExpr::Str), None, false),
            (
                "fixedError".to_string(),
                Some(TypeExpr::Str),
                Some(Expr::new(ExprKind::StringLiteral(String::new()), dummy())),
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the `DateTimeZone` region/group constants used by `listIdentifiers()`.
/// The per-region bits are powers of two; `ALL` is their OR, `ALL_WITH_BC` adds
/// the backward-compatibility bit (2048), and `PER_COUNTRY` (4096) switches the
/// filter to the country-code argument. Values match PHP exactly.
pub(super) fn datetime_zone_group_constants() -> Vec<ClassConst> {
    vec![
        int_class_const("AFRICA", 1),
        int_class_const("AMERICA", 2),
        int_class_const("ANTARCTICA", 4),
        int_class_const("ARCTIC", 8),
        int_class_const("ASIA", 16),
        int_class_const("ATLANTIC", 32),
        int_class_const("AUSTRALIA", 64),
        int_class_const("EUROPE", 128),
        int_class_const("INDIAN", 256),
        int_class_const("PACIFIC", 512),
        int_class_const("UTC", 1024),
        int_class_const("ALL", 2047),
        int_class_const("ALL_WITH_BC", 4095),
        int_class_const("PER_COUNTRY", 4096),
    ]
}

/// Builds the shared `DateTimeInterface` format constants (`ATOM`, `COOKIE`, the
/// `RFC*` family, `RSS`, `W3C`, ...). PHP exposes them on the interface and, by
/// inheritance, on `DateTime` and `DateTimeImmutable`; the same list is attached
/// to all three synthetic declarations. Values match PHP 8.4 exactly.
pub(super) fn datetime_format_constants() -> Vec<ClassConst> {
    let mut constants = vec![
        str_class_const("ATOM", "Y-m-d\\TH:i:sP"),
        str_class_const("COOKIE", "l, d-M-Y H:i:s T"),
        str_class_const("ISO8601", "Y-m-d\\TH:i:sO"),
        str_class_const("ISO8601_EXPANDED", "X-m-d\\TH:i:sP"),
        str_class_const("RFC822", "D, d M y H:i:s O"),
        str_class_const("RFC850", "l, d-M-y H:i:s T"),
        str_class_const("RFC1036", "D, d M y H:i:s O"),
        str_class_const("RFC1123", "D, d M Y H:i:s O"),
        str_class_const("RFC7231", "D, d M Y H:i:s \\G\\M\\T"),
        str_class_const("RFC2822", "D, d M Y H:i:s O"),
        str_class_const("RFC3339", "Y-m-d\\TH:i:sP"),
        str_class_const("RFC3339_EXTENDED", "Y-m-d\\TH:i:s.vP"),
        str_class_const("RSS", "D, d M Y H:i:s O"),
        str_class_const("W3C", "Y-m-d\\TH:i:sP"),
    ];
    if let Some(constant) = constants.iter_mut().find(|constant| constant.name == "RFC7231") {
        constant.attributes = deprecated_attribute(
            "8.5",
            "as this format ignores the associated timezone and always uses GMT",
        );
    }
    constants
}

/// Builds an `$this->property` access expression.
pub(super) fn this_property(property: &str) -> Expr {
    Expr::new(
        ExprKind::PropertyAccess {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
        },
        dummy(),
    )
}

/// Builds a `$this->property = value;` statement.
pub(super) fn assign_this_property(property: &str, value: Expr) -> Stmt {
    Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, dummy())),
            property: property.to_string(),
            value,
        },
        dummy(),
    )
}

/// Builds a `return <expr>;` statement.
pub(super) fn return_expr(value: Expr) -> Stmt {
    Stmt::new(StmtKind::Return(Some(value)), dummy())
}

/// Builds a public instance `ClassMethod` with the given params, return type, and body.
pub(super) fn method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
    body: Vec<Stmt>,
) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
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

/// Builds a public instance `ClassProperty` with a default value.
pub(super) fn property(name: &str, type_expr: TypeExpr, default: Expr) -> ClassProperty {
    ClassProperty {
        name: name.to_string(),
        visibility: Visibility::Public,
        set_visibility: None,
        type_expr: Some(type_expr),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(default),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds a private instance property used only by the synthetic ext/date implementation.
pub(super) fn private_property(name: &str, type_expr: TypeExpr, default: Expr) -> ClassProperty {
    let mut property = property(name, type_expr, default);
    property.visibility = Visibility::Private;
    property
}

/// Builds the hidden initialization marker used by php-src date objects.
pub(super) fn date_object_initialized_property() -> ClassProperty {
    private_property(
        "__elephc_initialized",
        TypeExpr::Bool,
        Expr::new(ExprKind::BoolLiteral(false), dummy()),
    )
}

/// Builds the non-throwing internal initialization probe used by composite date objects.
pub(super) fn date_object_is_initialized() -> ClassMethod {
    let tokens = crate::lexer::tokenize("<?php return $this->__elephc_initialized;")
        .expect("date object initialization probe must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("date object initialization probe must parse");
    let mut result = method(
        "__elephc_is_initialized",
        Vec::new(),
        Some(TypeExpr::Bool),
        body,
    );
    result.is_final = true;
    result
}

/// Builds the shared instance guard that reports php-src's exact uninitialized-object error.
pub(super) fn date_object_assert_initialized(class_name: &str) -> ClassMethod {
    let source = format!(
        r#"<?php
if (!$this->__elephc_initialized) {{
    $objectClass = get_class($this);
    $inheritance = $objectClass === "{class_name}" ? "" : " (inheriting {class_name})";
    throw new DateObjectError(
        "Object of type " . $objectClass . $inheritance
        . " has not been correctly initialized by calling parent::__construct() in its constructor"
    );
}}
"#
    );
    let tokens = crate::lexer::tokenize(&source)
        .expect("date object initialization guard must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("date object initialization guard must parse");
    let mut result = method(
        "__elephc_assert_initialized",
        Vec::new(),
        Some(TypeExpr::Void),
        body,
    );
    result.is_final = true;
    result
}

/// Builds php-src's specialized incomplete-object guard for date comparisons.
pub(super) fn datetime_assert_comparable() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
if (!$this->__elephc_initialized) {
    throw new DateObjectError(
        "Trying to compare an incomplete DateTime or DateTimeImmutable object"
    );
}
"#,
    )
    .expect("DateTime comparison guard must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime comparison guard must parse");
    let mut result = method(
        "__elephc_assert_comparable",
        Vec::new(),
        Some(TypeExpr::Void),
        body,
    );
    result.is_final = true;
    result
}

/// Builds the overflow-safe php-src comparator shared by mutable and immutable dates.
pub(super) fn datetime_compare() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
$this->__elephc_assert_comparable();
$other->__elephc_assert_comparable();
$leftTimestamp = $this->getTimestamp();
$rightTimestamp = $other->getTimestamp();
if ($leftTimestamp < $rightTimestamp) { return -1; }
if ($leftTimestamp > $rightTimestamp) { return 1; }
$leftMicrosecond = $this->getMicrosecond();
$rightMicrosecond = $other->getMicrosecond();
if ($leftMicrosecond < $rightMicrosecond) { return -1; }
if ($leftMicrosecond > $rightMicrosecond) { return 1; }
return 0;
"#,
    )
    .expect("DateTime comparison source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime comparison source must parse");
    let mut result = method(
        "__elephc_compare",
        vec![(
            "other".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
            None,
            false,
        )],
        Some(TypeExpr::Int),
        body,
    );
    result.is_final = true;
    result
}

/// Prepends the initialization guard to instance methods that require a live date payload.
///
/// Constructors, unserializers, wakeup hooks, and constructor-unpack helpers intentionally operate
/// before initialization and are therefore excluded.
pub(super) fn guard_date_object_instance_methods(methods: &mut [ClassMethod]) {
    let tokens = crate::lexer::tokenize("<?php $this->__elephc_assert_initialized();")
        .expect("date object guard call must tokenize");
    let guard = crate::parser::parse(&tokens)
        .expect("date object guard call must parse")
        .into_iter()
        .next()
        .expect("date object guard call must contain one statement");
    for method in methods {
        if method.is_static
            || matches!(
                method.name.as_str(),
                "__construct"
                    | "__unserialize"
                    | "__wakeup"
                    | "__elephc_is_initialized"
                    | "__elephc_assert_initialized"
                    | "__elephc_assert_comparable"
                    | "__elephc_compare"
                    | "__elephc_begin_argument_array"
                    | "__elephc_append_one_argument"
                    | "__elephc_append_argument_chunk"
                    | "__elephc_finish_argument_array"
            )
        {
            continue;
        }
        method.body.insert(0, guard.clone());
    }
}

/// Builds the two hidden fields used while normalizing constructor unpack chunks.
pub(super) fn date_constructor_unpack_properties() -> Vec<ClassProperty> {
    vec![
        private_property(
            "__elephc_arguments",
            TypeExpr::Named(Name::unqualified("mixed")),
            Expr::new(ExprKind::Null, dummy()),
        ),
        private_property(
            "__elephc_seen_named_argument",
            TypeExpr::Bool,
            Expr::new(ExprKind::BoolLiteral(false), dummy()),
        ),
    ]
}

/// Parses one hidden private constructor-unpack helper method.
pub(super) fn date_constructor_unpack_method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    source: &str,
) -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(source).expect("date constructor unpack helper must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("date constructor unpack helper must parse");
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params,
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

/// Builds the fixed-arity finalizer source for one ext/date constructor.
pub(super) fn date_constructor_unpack_finish_source(class_name: &str) -> String {
    match class_name {
        "DateTime" | "DateTimeImmutable" => format!(
            r#"<?php
$arguments = $this->__elephc_arguments;
$datetime = "now";
$timezone = null;
$hasDatetime = false;
$hasTimezone = false;
$nextPosition = 0;
foreach ($arguments as $key => $value) {{
    if (is_int($key)) {{
        if ($nextPosition === 0) {{
            $datetime = $value;
            $hasDatetime = true;
        }} elseif ($nextPosition === 1) {{
            $timezone = $value;
            $hasTimezone = true;
        }} else {{
            throw new ArgumentCountError(
                "{class_name}::__construct() expects at most 2 arguments, " . count($arguments) . " given"
            );
        }}
        $nextPosition++;
    }} elseif ($key === "datetime") {{
        if ($hasDatetime) {{
            throw new Error("Named parameter \$datetime overwrites previous argument");
        }}
        $datetime = $value;
        $hasDatetime = true;
    }} else {{
        if ($hasTimezone) {{
            throw new Error("Named parameter \$timezone overwrites previous argument");
        }}
        $timezone = $value;
        $hasTimezone = true;
    }}
}}
$datetime = DateTime::__elephc_weak_string_argument(
    $datetime,
    "{class_name}::__construct(): Argument #1 (\$datetime) must be of type string, ",
    ""
);
if (!is_null($timezone) && !($timezone instanceof DateTimeZone)) {{
    DateTime::__elephc_argument_type_error(
        $timezone,
        "{class_name}::__construct(): Argument #2 (\$timezone) must be of type ?DateTimeZone, "
    );
}}
$this->__construct($datetime, $timezone);
$this->__elephc_arguments = null;
$this->__elephc_seen_named_argument = false;
"#
        ),
        "DateTimeZone" => r#"<?php
$arguments = $this->__elephc_arguments;
$hasTimezone = false;
$nextPosition = 0;
foreach ($arguments as $key => $value) {
    if (is_int($key)) {
        if ($nextPosition > 0) {
            throw new ArgumentCountError(
                "DateTimeZone::__construct() expects exactly 1 argument, " . count($arguments) . " given"
            );
        }
        $timezone = $value;
        $hasTimezone = true;
        $nextPosition++;
    } else {
        if ($hasTimezone) {
            throw new Error("Named parameter \$timezone overwrites previous argument");
        }
        $timezone = $value;
        $hasTimezone = true;
    }
}
if (!$hasTimezone) {
    throw new ArgumentCountError(
        "DateTimeZone::__construct() expects exactly 1 argument, 0 given"
    );
}
$timezone = DateTime::__elephc_weak_string_argument(
    $timezone,
    "DateTimeZone::__construct(): Argument #1 (\$timezone) must be of type string, ",
    ""
);
$this->__construct($timezone);
$this->__elephc_arguments = null;
$this->__elephc_seen_named_argument = false;
"#
        .to_string(),
        "DateInterval" => r#"<?php
$arguments = $this->__elephc_arguments;
$hasDuration = false;
$nextPosition = 0;
foreach ($arguments as $key => $value) {
    if (is_int($key)) {
        if ($nextPosition > 0) {
            throw new ArgumentCountError(
                "DateInterval::__construct() expects exactly 1 argument, " . count($arguments) . " given"
            );
        }
        $duration = $value;
        $hasDuration = true;
        $nextPosition++;
    } else {
        if ($hasDuration) {
            throw new Error("Named parameter \$duration overwrites previous argument");
        }
        $duration = $value;
        $hasDuration = true;
    }
}
if (!$hasDuration) {
    throw new ArgumentCountError(
        "DateInterval::__construct() expects exactly 1 argument, 0 given"
    );
}
$duration = DateTime::__elephc_weak_string_argument(
    $duration,
    "DateInterval::__construct(): Argument #1 (\$duration) must be of type string, ",
    ""
);
$this->__construct($duration);
$this->__elephc_arguments = null;
$this->__elephc_seen_named_argument = false;
"#
        .to_string(),
        _ => panic!("unsupported ext/date constructor unpack class {class_name}"),
    }
}

/// Builds source-order unpack helpers for one fixed ext/date constructor signature.
pub(super) fn date_constructor_unpack_methods(
    class_name: &str,
    parameter_names: &[&str],
) -> Vec<ClassMethod> {
    let known_name_condition = parameter_names
        .iter()
        .map(|name| format!("$key === \"{name}\""))
        .collect::<Vec<_>>()
        .join("\n    || ");
    let parameter_index_assignments = parameter_names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let prefix = if index == 0 { "if" } else { "elseif" };
            format!(
                "{prefix} ($key === \"{name}\") {{\n    $parameterIndex = {index};\n}}"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    let append_one_source = format!(
        r#"<?php
$arguments = $this->__elephc_arguments;
if (is_int($key)) {{
    if ($this->__elephc_seen_named_argument) {{
        throw new Error(
            "Cannot use positional argument after named argument during unpacking"
        );
    }}
    $arguments[] = $value;
    $this->__elephc_arguments = $arguments;
    return;
}}
if (!is_string($key)) {{
    throw new Error(
        "Keys must be of type int|string during argument unpacking"
    );
}}
$this->__elephc_seen_named_argument = true;
if (!({known_name_condition})) {{
    throw new Error("Unknown named parameter \$" . $key);
}}
$parameterIndex = -1;
{parameter_index_assignments}
$positionalCount = 0;
foreach ($arguments as $existingKey => $existingValue) {{
    if (is_int($existingKey)) {{
        $positionalCount++;
    }}
}}
if ($parameterIndex < $positionalCount) {{
    throw new Error("Named parameter \$" . $key . " overwrites previous argument");
}}
if (array_key_exists($key, $arguments)) {{
    throw new Error("Named parameter \$" . $key . " overwrites previous argument");
}}
$arguments[$key] = $value;
$this->__elephc_arguments = $arguments;
"#
    );
    let begin = date_constructor_unpack_method(
        "__elephc_begin_argument_array",
        Vec::new(),
        r#"<?php
$this->__elephc_arguments = [];
$this->__elephc_seen_named_argument = false;
"#,
    );
    let append_one = date_constructor_unpack_method(
        "__elephc_append_one_argument",
        vec![
            (
                "key".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            ),
            (
                "value".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            ),
        ],
        &append_one_source,
    );
    let append_chunk = date_constructor_unpack_method(
        "__elephc_append_argument_chunk",
        vec![
            ("kind".to_string(), Some(TypeExpr::Int), None, false),
            ("name".to_string(), Some(TypeExpr::Str), None, false),
            (
                "value".to_string(),
                Some(TypeExpr::Named(Name::unqualified("mixed"))),
                None,
                false,
            ),
        ],
        r#"<?php
if ($kind === 1) {
    if (!(is_array($value) || $value instanceof Traversable)) {
        DateTime::__elephc_argument_type_error(
            $value,
            "Only arrays and Traversables can be unpacked, "
        );
    }
    foreach ($value as $key => $unpackedValue) {
        $this->__elephc_append_one_argument($key, $unpackedValue);
    }
    return;
}
if ($kind === 2) {
    $this->__elephc_append_one_argument($name, $value);
    return;
}
$this->__elephc_append_one_argument(0, $value);
"#,
    );
    let finish_source = date_constructor_unpack_finish_source(class_name);
    let finish = date_constructor_unpack_method(
        "__elephc_finish_argument_array",
        Vec::new(),
        &finish_source,
    );
    vec![begin, append_one, append_chunk, finish]
}

/// PHP source backing the allocation-free timezone canonicalizer shared by `DateTimeZone` and
/// free-form `DateTime` parsing. Returns an empty string for an invalid timezone.
pub(super) const DATETIME_ZONE_NORMALIZE_SRC: &str = r#"<?php
if (strtoupper($timezone) === "UTC" || strtoupper($timezone) === "GMT") {
    return strtoupper($timezone);
}
if (strlen($timezone) >= 5 && substr($timezone, 0, 3) === "GMT"
    && ($timezone[3] === "+" || $timezone[3] === "-")) {
    $timezone = substr($timezone, 3);
}
if (strlen($timezone) >= 2 && ($timezone[0] === "+" || $timezone[0] === "-")) {
    $len = strlen($timezone);
    $hours = 0;
    $minutes = 0;
    $seconds = 0;
    $ok = false;
    $digits = substr($timezone, 1);
    if (($len === 2 || $len === 3) && ctype_digit($digits)) {
        $hours = intval($digits);
        $ok = true;
    } elseif ($len === 4 && ctype_digit($digits)) {
        $hours = intval(substr($timezone, 1, 1));
        $minutes = intval(substr($timezone, 2, 2));
        $ok = true;
    } elseif ($len === 4 && $timezone[2] === ":"
        && ctype_digit($timezone[1]) && ctype_digit($timezone[3])) {
        $hours = intval($timezone[1]);
        $minutes = intval($timezone[3]);
        $ok = true;
    } elseif ($len === 5 && ctype_digit($digits)) {
        $hours = intval(substr($timezone, 1, 2));
        $minutes = intval(substr($timezone, 3, 2));
        $ok = true;
    } elseif ($len === 5 && $timezone[2] === ":"
        && ctype_digit($timezone[1])
        && ctype_digit($timezone[3]) && ctype_digit($timezone[4])) {
        $hours = intval($timezone[1]);
        $minutes = intval(substr($timezone, 3, 2));
        $ok = true;
    } elseif ($len === 5 && $timezone[3] === ":"
        && ctype_digit($timezone[1]) && ctype_digit($timezone[2])
        && ctype_digit($timezone[4])) {
        $hours = intval(substr($timezone, 1, 2));
        $minutes = intval($timezone[4]);
        $ok = true;
    } elseif ($len === 6 && $timezone[3] === ":"
        && ctype_digit($timezone[1]) && ctype_digit($timezone[2])
        && ctype_digit($timezone[4]) && ctype_digit($timezone[5])) {
        $hours = intval(substr($timezone, 1, 2));
        $minutes = intval(substr($timezone, 4, 2));
        $ok = true;
    } elseif ($len === 7 && ctype_digit($digits)) {
        $hours = intval(substr($timezone, 1, 2));
        $minutes = intval(substr($timezone, 3, 2));
        $seconds = intval(substr($timezone, 5, 2));
        $ok = true;
    } elseif ($len === 9 && $timezone[3] === ":" && $timezone[6] === ":"
        && ctype_digit($timezone[1]) && ctype_digit($timezone[2])
        && ctype_digit($timezone[4]) && ctype_digit($timezone[5])
        && ctype_digit($timezone[7]) && ctype_digit($timezone[8])) {
        $hours = intval(substr($timezone, 1, 2));
        $minutes = intval(substr($timezone, 4, 2));
        $seconds = intval(substr($timezone, 7, 2));
        $ok = true;
    }
    if ($ok) {
        $total = $hours * 3600 + $minutes * 60 + $seconds;
        $hours = intdiv($total, 3600);
        $remaining = $total % 3600;
        $minutes = intdiv($remaining, 60);
        $seconds = $remaining % 60;
        if ($hours >= 100) {
            return "";
        }
        $sign = ($total === 0) ? "+" : $timezone[0];
        $hh = (($hours < 10) ? "0" : "") . (string)$hours;
        $mm = (($minutes < 10) ? "0" : "") . (string)$minutes;
        $name = $sign . $hh . ":" . $mm;
        if ($seconds !== 0) {
            $ss = (($seconds < 10) ? "0" : "") . (string)$seconds;
            $name = $name . ":" . $ss;
        }
        return $name;
    }
}
if (in_array(strtolower($timezone), [__TZ_IDENTIFIERS_LOWER__], true)) {
    return $timezone . "";
}
if (strlen($timezone) === 1) {
    $upper = strtoupper($timezone);
    $code = ord($upper);
    if ((($code >= 65 && $code <= 73) || ($code >= 75 && $code <= 90))) {
        return $upper;
    }
}
if (in_array(strtolower($timezone), [__TZ_ABBREVIATIONS__], true)) {
    return strtoupper($timezone);
}
return "";
"#;

/// PHP source backing `DateTimeZone::__construct`.
pub(super) const DATETIME_ZONE_CONSTRUCT_SRC: &str = r#"<?php
if (str_contains($timezone, chr(0))) {
    throw new ValueError(
        'DateTimeZone::__construct(): Argument #1 ($timezone) must not contain any null bytes'
    );
}
$this->__elephc_initialized = true;
$__normalized = DateTimeZone::__elephc_normalize_timezone($timezone);
if ($__normalized !== "") {
    $this->name = $__normalized;
    return;
}
$__length = strlen($timezone);
$__offsetOutOfRange = false;
if ($__length === 5 && ctype_digit(substr($timezone, 1))) {
    $__hours = intval(substr($timezone, 1, 2));
    $__minutes = intval(substr($timezone, 3, 2));
    $__offsetOutOfRange = $__hours * 3600 + $__minutes * 60 >= 100 * 3600;
} else if ($__length === 6 && $timezone[3] === ":") {
    $__hours = intval(substr($timezone, 1, 2));
    $__minutes = intval(substr($timezone, 4, 2));
    $__offsetOutOfRange = $__hours * 3600 + $__minutes * 60 >= 100 * 3600;
} else if ($__length === 7 && ctype_digit(substr($timezone, 1))) {
    $__hours = intval(substr($timezone, 1, 2));
    $__minutes = intval(substr($timezone, 3, 2));
    $__seconds = intval(substr($timezone, 5, 2));
    $__offsetOutOfRange = $__hours * 3600 + $__minutes * 60 + $__seconds >= 100 * 3600;
} else if ($__length === 9 && $timezone[3] === ":" && $timezone[6] === ":") {
    $__hours = intval(substr($timezone, 1, 2));
    $__minutes = intval(substr($timezone, 4, 2));
    $__seconds = intval(substr($timezone, 7, 2));
    $__offsetOutOfRange = $__hours * 3600 + $__minutes * 60 + $__seconds >= 100 * 3600;
}
if ($__offsetOutOfRange) {
    throw new DateInvalidTimeZoneException(
        "DateTimeZone::__construct(): Timezone offset is out of range (" . $timezone . ")"
    );
}
throw new DateInvalidTimeZoneException("DateTimeZone::__construct(): Unknown or bad timezone (" . $timezone . ")");
"#;

/// Builds the PHP string-literal list of every abbreviation key in php-src's
/// baked timelib table.
pub(super) fn timezone_abbreviation_literals() -> String {
    include_str!("../../../../../crates/elephc-tz/data/abbreviations.data")
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(abbr, _)| format!("\"{abbr}\"")))
        .collect::<Vec<_>>()
        .join(",")
}

/// Builds the case-folded `ALL_WITH_BC` identifier set accepted by `DateTimeZone::__construct()`.
///
/// `DateTimeZone::listIdentifiers()` intentionally omits backward-compatible aliases unless
/// `ALL_WITH_BC` is requested, while the constructor accepts them unconditionally. The bridge's
/// location table contains the complete php-src-derived set, including `Etc/Universal`,
/// `US/Eastern`, and the other compatibility identifiers.
pub(super) fn timezone_constructor_identifier_literals() -> String {
    include_str!("../../../../../crates/elephc-tz/data/location.data")
        .lines()
        .filter_map(|line| line.split_once('\t').map(|(identifier, _)| identifier.to_lowercase()))
        .map(|identifier| format!("\"{identifier}\""))
        .collect::<Vec<_>>()
        .join(",")
}

/// Builds the allocation-free timezone canonicalizer used by constructor suffix probing.
pub(super) fn datetime_zone_normalize_timezone() -> ClassMethod {
    let source = DATETIME_ZONE_NORMALIZE_SRC
        .replace(
            "__TZ_IDENTIFIERS_LOWER__",
            &timezone_constructor_identifier_literals(),
        )
        .replace("__TZ_ABBREVIATIONS__", &timezone_abbreviation_literals());
    let tokens = crate::lexer::tokenize(&source)
        .expect("DateTimeZone timezone canonicalizer source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTimeZone timezone canonicalizer source must parse");
    ClassMethod {
        name: "__elephc_normalize_timezone".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("timezone".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// `DateTimeZone::__construct(string $timezone)` — validates and stores the identifier.
/// Throws `DateInvalidTimeZoneException` (PHP 8.3+) on an unrecognized identifier.
pub(super) fn datetime_zone_constructor() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATETIME_ZONE_CONSTRUCT_SRC)
        .expect("DateTimeZone::__construct body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTimeZone::__construct body source must parse");
    ClassMethod {
        name: "__construct".to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![(
            "timezone".to_string(),
            Some(TypeExpr::Str),
            None,
            false,
        )],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: None,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the procedural `timezone_open()` wrapper that converts constructor
/// exceptions into php-src's warning plus `false` return.
pub(super) fn datetime_zone_procedural_open() -> ClassMethod {
    let source = r#"<?php
$timezone = (string) $timezone;
if (str_contains($timezone, chr(0))) {
    throw new ValueError(
        'timezone_open(): Argument #1 ($timezone) must not contain any null bytes'
    );
}
try {
    return new DateTimeZone($timezone);
} catch (DateInvalidTimeZoneException $exception) {
    __elephc_diag_warning(
        "\nWarning: timezone_open(): Unknown or bad timezone (" . $timezone . ")",
        $sourceLine
    );
    return false;
}
"#;
    let tokens = crate::lexer::tokenize(source)
        .expect("procedural timezone_open body must tokenize");
    let body =
        crate::parser::parse(&tokens).expect("procedural timezone_open body must parse");
    ClassMethod {
        name: "__elephc_timezone_open".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "timezone".to_string(),
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

/// `DateTimeZone::getName(): string` — returns the stored identifier.
pub(super) fn datetime_zone_get_name() -> ClassMethod {
    method("getName", Vec::new(), Some(TypeExpr::Str), vec![return_expr(this_property("name"))])
}

/// Builds the internal php-src-compatible comparison handler for `DateTimeZone` objects.
///
/// php-src treats unequal zones of the same representation kind as uncomparable (comparison code
/// `1`), throws for different kinds, and rejects comparisons involving an uninitialized subclass.
pub(super) fn datetime_zone_compare() -> ClassMethod {
    let src = r#"<?php
if (!$this->__elephc_initialized || !$other->__elephc_initialized) {
    throw new DateObjectError("Trying to compare uninitialized DateTimeZone objects");
}
$leftType = DateTime::__elephc_timezone_type($this->name);
$rightType = DateTime::__elephc_timezone_type($other->name);
if ($leftType !== $rightType) {
    throw new DateException("Cannot compare two different kinds of DateTimeZone objects");
}
return $this->name === $other->name ? 0 : 1;
"#;
    let tokens =
        crate::lexer::tokenize(src).expect("DateTimeZone comparison source must tokenize");
    let body = crate::parser::parse(&tokens).expect("DateTimeZone comparison source must parse");
    let mut result = method(
        "__elephc_compare",
        vec![(
            "other".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeZone"))),
            None,
            false,
        )],
        Some(TypeExpr::Int),
        body,
    );
    result.is_final = true;
    result
}

/// `DateTimeZone::getOffset(DateTimeInterface $datetime): int` — UTC offset (seconds) of this zone
/// at the given instant.
///
/// Temporarily applies this zone via `date_default_timezone_set`, reads the offset with the `date()`
/// `Z` specifier for `$datetime->getTimestamp()` (so it is daylight-saving correct), then restores
/// the previous default. Returns a positive value east of UTC, negative west.
pub(super) fn datetime_zone_get_offset() -> ClassMethod {
    let call = |name: &str, args: Vec<Expr>| {
        Expr::new(ExprKind::FunctionCall { name: Name::unqualified(name), args }, dummy())
    };
    let var = |n: &str| Expr::new(ExprKind::Variable(n.to_string()), dummy());
    let expr_stmt = |e: Expr| Stmt::new(StmtKind::ExprStmt(e), dummy());
    let runtime_zone = |zone: Expr| {
        Expr::new(
            ExprKind::StaticMethodCall {
                receiver: StaticReceiver::Named(Name::unqualified("DateTime")),
                method: "__elephc_runtime_timezone_name".to_string(),
                args: vec![zone],
            },
            dummy(),
        )
    };
    // $datetime->getTimestamp()
    let dt_ts = Expr::new(
        ExprKind::MethodCall {
            object: Box::new(var("datetime")),
            method: "getTimestamp".to_string(),
            args: Vec::new(),
        },
        dummy(),
    );
    let z_spec = Expr::new(ExprKind::StringLiteral("Z".to_string()), dummy());
    method(
        "getOffset",
        vec![(
            "datetime".to_string(),
            Some(TypeExpr::Named(Name::unqualified("DateTimeInterface"))),
            None,
            false,
        )],
        Some(TypeExpr::Int),
        vec![
            // $__saved = date_default_timezone_get();
            Stmt::assign("__saved", call("date_default_timezone_get", Vec::new())),
            // date_default_timezone_set($this->name);
            expr_stmt(call(
                "date_default_timezone_set",
                vec![runtime_zone(this_property("name"))],
            )),
            // $__off = intval(date("Z", $datetime->getTimestamp()));
            Stmt::assign("__off", call("intval", vec![call("date", vec![z_spec, dt_ts])])),
            // date_default_timezone_set($__saved);  (restore the previous default)
            expr_stmt(call("date_default_timezone_set", vec![var("__saved")])),
            return_expr(var("__off")),
        ],
    )
}

/// `DateTimeZone::listIdentifiers(int $timezoneGroup = DateTimeZone::ALL, ?string $countryCode = null): array`
/// — returns the embedded IANA timezone identifier list. The body is a parsed `return [ ... ];`
/// over the identifiers in `timezone_ids::TIMEZONE_IDENTIFIERS_ARRAY` (captured from PHP).
///
/// The `$timezoneGroup`/`$countryCode` filter parameters are declared for signature parity (so
/// reflection reports PHP's real signature), but the body returns the full unfiltered list: real
/// calls are desugared by the name resolver to the injected `__elephc_list_identifiers()` free
/// function (which performs the group/country filter), so this body only runs via reflection
/// invocation, where filtering is best-effort.
pub(super) fn datetime_zone_list_identifiers() -> ClassMethod {
    let body = super::bodies::list_identifiers(super::timezone_ids::TIMEZONE_IDENTIFIERS);
    ClassMethod {
        name: "listIdentifiers".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            (
                "timezoneGroup".to_string(),
                Some(TypeExpr::Int),
                // `DateTimeZone::ALL` (2047) as a literal: referencing the class's own constant in a
                // default triggers a circular-inheritance error, so the literal value is used.
                Some(Expr::new(ExprKind::IntLiteral(2047), dummy())),
                false,
            ),
            (
                "countryCode".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Str))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
        ],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: None,
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Parses a synthetic-method body from elephc-PHP source into statements. Used so
/// the introspection methods return array literals directly — the only shape
/// whose element type a synthetic method's inferred (`None`) return resolves to
/// (a call to a prelude helper would infer as a scalar). Panics on a
/// tokenize/parse failure, which is a compiler bug in the static source.
pub(super) fn parse_tz_body(src: &str) -> Vec<Stmt> {
    let tokens = crate::lexer::tokenize(src).expect("tz method body must tokenize");
    crate::parser::parse(&tokens).expect("tz method body must parse")
}

/// `DateTimeZone::getLocation(): array|false` — returns the zone's country code,
/// latitude, longitude, and comments (or `false` for the few zones without a
/// location). Calls the `elephc_tz` bridge directly and marshals the tab-joined
/// result into an array literal so inference resolves the return shape. Only added
/// to `DateTimeZone` when the introspection prelude is injected.
pub(super) fn datetime_zone_get_location() -> ClassMethod {
    method(
        "getLocation",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("mixed"))),
        parse_tz_body(
            r#"<?php
$raw = elephc_tz_location($this->name);
if ($raw === "") {
    return false;
}
$f = explode("\t", $raw);
return [
    "country_code" => $f[0],
    "latitude" => (float) $f[1],
    "longitude" => (float) $f[2],
    "comments" => $f[3],
];
"#,
        ),
    )
}

/// `DateTimeZone::getTransitions(int $timestampBegin = PHP_INT_MIN, int $timestampEnd = PHP_INT_MAX): array|false`
/// — returns the DST transition rows in the window. The defaults reproduce PHP's
/// full no-arg list: the synthetic first row coincides with the bridge's row 0, so
/// its precomputed `time` is reused rather than asking `gmdate` to format
/// `PHP_INT_MIN`.
#[allow(dead_code)]
pub(super) fn datetime_zone_get_transitions() -> ClassMethod {
    // PHP's defaults are PHP_INT_MIN and 2147483647. They are materialized as
    // integer literals because a `ConstRef` default is not evaluated at call sites.
    let int_literal = |v: i64| Expr::new(ExprKind::IntLiteral(v), dummy());
    method(
        "getTransitions",
        vec![
            (
                "timestampBegin".to_string(),
                Some(TypeExpr::Int),
                Some(int_literal(i64::MIN)),
                false,
            ),
            (
                "timestampEnd".to_string(),
                Some(TypeExpr::Int),
                Some(int_literal(2_147_483_647)),
                false,
            ),
        ],
        Some(TypeExpr::Named(Name::unqualified("mixed"))),
        parse_tz_body(
            r#"<?php
$raw = elephc_tz_transitions($this->name);
if ($raw === "") {
    return false;
}
$lines = explode("\n", $raw);
$all = [];
foreach ($lines as $line) {
    $g = explode("\t", $line);
    $all[] = [
        "ts" => (int) $g[0],
        "offset" => (int) $g[1],
        "isdst" => $g[2] === "1",
        "abbr" => $g[3],
        "time" => $g[4],
    ];
}
$n = count($all);
$result = [];
$active = -1;
for ($i = 0; $i < $n; $i++) {
    if ($all[$i]["ts"] <= $timestampBegin) {
        $active = $i;
    }
}
if ($active >= 0) {
    $a = $all[$active];
    // (int) unboxes the boxed array element to a plain int so the comparison with
    // the int param is reliable (a boxed element compared directly mis-evaluates).
    // $ats <= $timestampBegin by construction; when they are equal (the
    // PHP_INT_MIN default lands on row 0, or begin hits a transition exactly),
    // reuse the bridge's ts/time rather than formatting an extreme begin with
    // gmdate — gmdate(PHP_INT_MIN) exhausts the heap.
    $ats = (int) $a["ts"];
    if ($timestampBegin <= $ats) {
        // The bridge row stores `time` last, while php-src exposes transition rows
        // in `ts,time,offset,isdst,abbr` insertion order. Rebuild with the bridge's
        // preformatted time so PHP_INT_MIN never flows through gmdate here.
        $result[] = [
            "ts" => $a["ts"],
            "time" => $a["time"],
            "offset" => $a["offset"],
            "isdst" => $a["isdst"],
            "abbr" => $a["abbr"],
        ];
    } else {
        $result[] = [
            "ts" => $timestampBegin,
            "time" => gmdate("Y-m-d\TH:i:sP", $timestampBegin),
            "offset" => $a["offset"],
            "isdst" => $a["isdst"],
            "abbr" => $a["abbr"],
        ];
    }
}
for ($i = 0; $i < $n; $i++) {
    if ($all[$i]["ts"] > $timestampBegin && $all[$i]["ts"] <= $timestampEnd) {
        $r = $all[$i];
        $result[] = [
            "ts" => $r["ts"],
            "time" => $r["time"],
            "offset" => $r["offset"],
            "isdst" => $r["isdst"],
            "abbr" => $r["abbr"],
        ];
    }
}
return $result;
"#,
        ),
    )
}

/// Builds the transition scanner without storing associative rows inside a boxed indexed array.
///
/// Keeping transition fields in scalar locals avoids the nested Mixed-array comparison path that
/// otherwise drops every row after the synthetic window-start row.
pub(super) fn datetime_zone_get_transitions_flat() -> ClassMethod {
    let int_literal = |value: i64| Expr::new(ExprKind::IntLiteral(value), dummy());
    method(
        "getTransitions",
        vec![
            (
                "timestampBegin".to_string(),
                Some(TypeExpr::Int),
                Some(int_literal(i64::MIN)),
                false,
            ),
            (
                "timestampEnd".to_string(),
                Some(TypeExpr::Int),
                Some(int_literal(2_147_483_647)),
                false,
            ),
        ],
        Some(TypeExpr::Named(Name::unqualified("mixed"))),
        super::bodies::tz_get_transitions(),
    )
}

/// `DateTimeZone::listAbbreviations(): array` — returns PHP's static
/// abbreviation→offset/DST/zone table. Static method; calls the `elephc_tz` bridge
/// directly and marshals the result into the nested array literal.
pub(super) fn datetime_zone_list_abbreviations() -> ClassMethod {
    ClassMethod {
        name: "listAbbreviations".to_string(),
        visibility: Visibility::Public,
        is_static: true,
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
        body: super::bodies::tz_list_abbreviations(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source backing the `DateTime`/`DateTimeImmutable` constructor. With no timezone, parses the
/// string in the active default zone and records that as the display zone. With a `$timezone`, the
/// wall-clock string is interpreted in that zone (the default is temporarily switched so
/// `strtotime()` resolves the local time there — an explicit zone inside the string still wins),
/// and the zone becomes the display zone. `"now"` is the current instant regardless of zone.
#[cfg(test)]
pub(super) const CONSTRUCT_SRC: &str = r#"<?php
$__originalDateTime = $datetime . "";
if ($__originalDateTime === "" || $__originalDateTime === "now") {
    DateTime::$lastParseResult = "";
} else {
    $__parseResult = DateTime::__elephc_date_parse($__originalDateTime);
    if ($__parseResult["error_count"] === 0 && $__parseResult["warning_count"] === 0) {
        DateTime::$lastParseResult = "";
    } else {
        DateTime::$lastParseResult = $__parseResult;
    }
}
if ($datetime === "") {
    $datetime = "now";
}
// Capture a trailing fractional second (HH:MM:SS.ffffff) into the microsecond
// component and strip it before strtotime() (which does not accept it). The
// parsing lives in static helpers so the constructor body stays small (adding
// locals + a loop here corrupts the frame when a caller also formats the result).
$this->microsecond = DateTime::__elephc_extract_micros($datetime);
$datetime = DateTime::__elephc_strip_micros($datetime);
if (substr($__originalDateTime, 0, 1) === "@") {
    $__ts = strtotime($datetime);
    if ($__ts === false) {
        throw new DateMalformedStringException(
            DateTime::__elephc_malformed_time_message("", $__originalDateTime)
        );
    }
    $this->timestamp = $__ts;
    $this->timezone_name = "+00:00";
    $this->__elephc_initialized = true;
    return;
}
$__zoneData = explode("\t", DateTime::__elephc_extract_constructor_zone($datetime));
$__detectedZone = $__zoneData[0];
$datetime = $__zoneData[1];
if ($__detectedZone !== "") {
    if ($datetime === "now") {
        $__ts = microtime(true);
        $this->timestamp = intval($__ts);
        $this->microsecond = intval(($__ts - $this->timestamp) * 1000000);
        // php-src preserves the default zone's current wall clock when a
        // timezone-type 1/2 offset or abbreviation is the whole input. Named
        // timezone-type 3 identifiers instead preserve the current instant.
        if (DateTime::__elephc_timezone_type($__detectedZone) !== 3) {
            $__saved = date_default_timezone_get();
            $__wall = date("Y-m-d H:i:s", $this->timestamp);
            date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($__detectedZone));
            $this->timestamp = strtotime($__wall);
            date_default_timezone_set($__saved);
        }
    } else {
        $__saved = date_default_timezone_get();
        date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($__detectedZone));
        $__ts = strtotime($datetime);
        date_default_timezone_set($__saved);
        if ($__ts === false) {
            throw new DateMalformedStringException(
                DateTime::__elephc_malformed_time_message("", $__originalDateTime)
            );
        }
        $this->timestamp = $__ts;
    }
    $this->timezone_name = $__detectedZone;
} else if ($timezone === null) {
    if ($datetime === "now") {
        $__ts = microtime(true);
        $this->timestamp = intval($__ts);
        $this->microsecond = intval(($__ts - $this->timestamp) * 1000000);
    } else {
        $__ts = strtotime($datetime);
        if ($__ts === false) {
            throw new DateMalformedStringException(
                DateTime::__elephc_malformed_time_message("", $__originalDateTime)
            );
        }
        $this->timestamp = $__ts;
    }
    $this->timezone_name = date_default_timezone_get();
} else {
    $tzname = $timezone->getName();
    if ($datetime === "now") {
        $__ts = microtime(true);
        $this->timestamp = intval($__ts);
        $this->microsecond = intval(($__ts - $this->timestamp) * 1000000);
    } else {
        $saved = date_default_timezone_get();
        date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($tzname));
        $__ts = strtotime($datetime);
        if ($__ts === false) {
            date_default_timezone_set($saved);
            throw new DateMalformedStringException(
                DateTime::__elephc_malformed_time_message("", $__originalDateTime)
            );
        }
        $this->timestamp = $__ts;
        date_default_timezone_set($saved);
    }
    $this->timezone_name = $tzname;
}
$this->__elephc_initialized = true;
"#;

/// `DateTime`/`DateTimeImmutable::__construct(string $datetime = "now", ?DateTimeZone $timezone = null)`
/// — stores a UNIX timestamp and the object's display zone.
///
/// The direct AST body mirrors `CONSTRUCT_SRC`. `$timezone` is typed `?DateTimeZone` (defaulting to
/// `null`); the `=== null` discriminator selects the form and `$timezone->getName()` reads the
/// zone on the non-null arm. A later `setTimezone()` still overrides the zone. (A `mixed` default
/// of `null` here miscompiled when the constructor was called more than once per frame, so the
/// nullable-object typing is used instead — it also matches PHP's signature.)
pub(super) fn datetime_immutable_constructor() -> ClassMethod {
    let body = super::bodies::construct();
    method(
        "__construct",
        vec![
            (
                "datetime".to_string(),
                Some(TypeExpr::Str),
                Some(Expr::new(ExprKind::StringLiteral("now".to_string()), dummy())),
                false,
            ),
            (
                "timezone".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
                    "DateTimeZone",
                ))))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
        ],
        None,
        body,
    )
}

/// `DateTimeImmutable::getTimestamp(): int` — returns the stored UNIX timestamp.
pub(super) fn datetime_immutable_get_timestamp() -> ClassMethod {
    method("getTimestamp", Vec::new(), Some(TypeExpr::Int), vec![return_expr(this_property("timestamp"))])
}

/// `DateTime`/`DateTimeImmutable::getMicrosecond(): int` — returns the stored sub-second component
/// (0..999999), 0 unless set by `setMicrosecond()` or parsed from a fractional second.
pub(super) fn datetime_get_microsecond() -> ClassMethod {
    method("getMicrosecond", Vec::new(), Some(TypeExpr::Int), vec![return_expr(this_property("microsecond"))])
}

/// `DateTimeImmutable::getTimezone(): DateTimeZone` — re-materializes a zone from the stored name.
pub(super) fn datetime_immutable_get_timezone() -> ClassMethod {
    method(
        "getTimezone",
        Vec::new(),
        Some(TypeExpr::Named(Name::unqualified("DateTimeZone"))),
        vec![return_expr(Expr::new(
            ExprKind::NewObject {
                class_name: Name::unqualified("DateTimeZone"),
                args: vec![this_property("timezone_name")],
            },
            dummy(),
        ))],
    )
}

/// PHP source backing `format()`. Applies `$this->timezone_name` via `date_default_timezone_set`
/// around the `date()` call (saving/restoring the previous default) for per-object formatting, and
/// rewrites the unescaped `u` (microseconds, 6 digits) and `v` (milliseconds, 3 digits) specifiers
/// to the stored sub-second value before calling `date()` — those decimal digits pass through
/// `date()` literally (only letters are specifiers). Backslash escapes are preserved verbatim.
pub(super) const FORMAT_SRC: &str = r#"<?php
$saved = date_default_timezone_get();
date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($this->timezone_name));
__CIVIL_FORMAT__
$us = $this->microsecond;
$fmt = "";
$flen = strlen($format);
$k = 0;
while ($k < $flen) {
    $ch = $format[$k];
    if ($ch === "\\") {
        $fmt = $fmt . $ch;
        $k = $k + 1;
        if ($k < $flen) { $fmt = $fmt . $format[$k]; $k = $k + 1; }
        continue;
    }
    if ($ch === "u") {
        $s = "" . $us;
        while (strlen($s) < 6) { $s = "0" . $s; }
        $fmt = $fmt . $s;
        $k = $k + 1;
        continue;
    }
    if ($ch === "v") {
        $ms = intdiv($us, 1000);
        $s = "" . $ms;
        while (strlen($s) < 3) { $s = "0" . $s; }
        $fmt = $fmt . $s;
        $k = $k + 1;
        continue;
    }
    if ($ch === "T" && DateTime::__elephc_timezone_type($this->timezone_name) === 1) {
        $zoneLiteral = "GMT" . substr($this->timezone_name, 0, 3)
            . substr($this->timezone_name, 4, 2);
        $zoneLength = strlen($zoneLiteral);
        $zoneIndex = 0;
        while ($zoneIndex < $zoneLength) {
            $fmt = $fmt . "\\" . $zoneLiteral[$zoneIndex];
            $zoneIndex = $zoneIndex + 1;
        }
        $k = $k + 1;
        continue;
    }
    if ($ch === "e"
        || ($ch === "T"
            && DateTime::__elephc_timezone_type($this->timezone_name) === 2)) {
        $zoneLiteral = $this->timezone_name;
        $zoneLength = strlen($zoneLiteral);
        $zoneIndex = 0;
        while ($zoneIndex < $zoneLength) {
            $fmt = $fmt . "\\" . $zoneLiteral[$zoneIndex];
            $zoneIndex = $zoneIndex + 1;
        }
        $k = $k + 1;
        continue;
    }
    if ($ch === "X" || $ch === "x") {
        $year = intval(date("Y", $this->timestamp));
        if ($year < 0) {
            $year = -$year;
            $sign = "-";
        } else {
            $sign = "+";
        }
        $s = "" . $year;
        while (strlen($s) < 4) { $s = "0" . $s; }
        if ($ch === "x" && $sign === "+" && strlen($s) <= 4) {
            $fmt = $fmt . $s;
        } else {
            $fmt = $fmt . $sign . $s;
        }
        $k = $k + 1;
        continue;
    }
    $fmt = $fmt . $ch;
    $k = $k + 1;
}
$r = date($fmt, $this->timestamp);
date_default_timezone_set($saved);
return $r;
"#;

/// Timelib-only branch that formats separately retained civil fields after timestamp overflow.
pub(super) const CIVIL_FORMAT_SRC: &str = r#"if ($this->__elephc_civil_override) {
    $civil = $this->timezone_name . "\t"
        . $this->__elephc_civil_year . "\t"
        . $this->__elephc_civil_month . "\t"
        . $this->__elephc_civil_day;
    $r = elephc_tz_format_civil(
        $this->timestamp,
        $this->microsecond,
        $format,
        strlen($format),
        $civil,
        strlen($civil)
    );
    date_default_timezone_set($saved);
    return $r;
}
"#;

/// `DateTime`/`DateTimeImmutable::format(string $format): string` — formats the stored timestamp in
/// the object's own timezone, with `u`/`v` reflecting the stored microseconds. The timelib-only
/// civil-overflow branch is included only when the timezone prelude declares its bridge symbol.
pub(super) fn datetime_immutable_format(uses_timelib: bool) -> ClassMethod {
    let source = FORMAT_SRC.replace(
        "__CIVIL_FORMAT__",
        if uses_timelib { CIVIL_FORMAT_SRC } else { "" },
    );
    let tokens = crate::lexer::tokenize(&source).expect("format() body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("format() body source must parse");
    method(
        "format",
        vec![("format".to_string(), Some(TypeExpr::Str), None, false)],
        Some(TypeExpr::Str),
        body,
    )
}

/// Builds `(int) date($fmt, $this->timestamp)` — extracts a numeric component of the stored time.
pub(super) fn date_component_int(fmt: &str) -> Expr {
    Expr::new(
        ExprKind::Cast {
            target: crate::parser::ast::CastType::Int,
            expr: Box::new(Expr::new(
                ExprKind::FunctionCall {
                    name: Name::unqualified("date"),
                    args: vec![
                        Expr::new(ExprKind::StringLiteral(fmt.to_string()), dummy()),
                        this_property("timestamp"),
                    ],
                },
                dummy(),
            )),
        },
        dummy(),
    )
}

/// Builds an `__elephc_mktime_raw(hour, minute, second, month, day, year)` call expression — the
/// internal fixed-arity runtime entry that the `mktime()`/`gmmktime()` procedural aliases desugar
/// to. Synthetic method bodies call it directly (they are injected after the name resolver, so
/// the alias rewrite never runs on them); using the raw name avoids an unresolved `mktime` call.
pub(super) fn mktime_call(parts: [&str; 6]) -> Expr {
    Expr::new(
        ExprKind::FunctionCall {
            name: Name::unqualified("__elephc_mktime_raw"),
            args: parts
                .iter()
                .map(|n| Expr::new(ExprKind::Variable((*n).to_string()), dummy()))
                .collect(),
        },
        dummy(),
    )
}

/// Builds the statement tail that publishes a freshly computed timestamp.
///
/// Mutable classes (`DateTime`) assign `$this->timestamp` and return `$this`. Immutable classes
/// (`DateTimeImmutable`) construct a fresh instance, copy the new timestamp and the timezone name,
/// and return it — preserving copy-on-modify semantics.
pub(super) fn result_tail(result_ts: Expr, mutable: bool, class_name: &str) -> Vec<Stmt> {
    result_tail_micro(result_ts, None, mutable, class_name)
}

/// Like `result_tail`, but with an explicit sub-second value for the result. When
/// `result_micro` is `None` the existing `$this->microsecond` is carried through
/// (the common case); add()/sub() pass the recomputed microsecond instead.
pub(super) fn result_tail_micro(
    result_ts: Expr,
    result_micro: Option<Expr>,
    mutable: bool,
    class_name: &str,
) -> Vec<Stmt> {
    result_tail_micro_with_timezone(result_ts, result_micro, None, mutable, class_name)
}

/// Like `result_tail_micro`, with an optional explicit display timezone for
/// operations such as `modify("@timestamp")` that replace both the instant and
/// php-src's timezone representation.
pub(super) fn result_tail_micro_with_timezone(
    result_ts: Expr,
    result_micro: Option<Expr>,
    result_timezone: Option<Expr>,
    mutable: bool,
    class_name: &str,
) -> Vec<Stmt> {
    let micro = result_micro.unwrap_or_else(|| this_property("microsecond"));
    if mutable {
        let mut tail = vec![
            assign_this_property("microsecond", micro),
            assign_this_property("timestamp", result_ts),
            assign_this_property(
                "__elephc_civil_override",
                Expr::new(ExprKind::BoolLiteral(false), dummy()),
            ),
        ];
        if let Some(timezone) = result_timezone {
            tail.push(assign_this_property("timezone_name", timezone));
        }
        tail.push(return_expr(Expr::new(ExprKind::This, dummy())));
        tail
    } else {
        let new_var = || Expr::new(ExprKind::Variable("__new".to_string()), dummy());
        let timezone = result_timezone.unwrap_or_else(|| this_property("timezone_name"));
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
                    value: result_ts,
                },
                dummy(),
            ),
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "timezone_name".to_string(),
                    value: timezone,
                },
                dummy(),
            ),
            // Carry the sub-second component into the fresh immutable instance so it survives
            // setTimestamp/setTime/setDate/setTimezone/add/sub/modify.
            Stmt::new(
                StmtKind::PropertyAssign {
                    object: Box::new(new_var()),
                    property: "microsecond".to_string(),
                    value: micro,
                },
                dummy(),
            ),
            return_expr(new_var()),
        ]
    }
}

#[cfg(test)]
mod ast_migration_tests {
    use super::*;
    use crate::synthetic_class::transcribe::transcribe;

    /// Prints the direct constructor AST used while removing production PHP parsing.
    #[test]
    fn transcribes_datetime_constructor_body() {
        if std::env::var_os("ELEPHC_DUMP_DATETIME_AST").is_none() {
            return;
        }
        let tokens = crate::lexer::tokenize(CONSTRUCT_SRC)
            .expect("DateTime constructor source must tokenize in the migration test");
        let body = crate::parser::parse(&tokens)
            .expect("DateTime constructor source must parse in the migration test");
        eprintln!("{}", transcribe(&body));
    }
}
