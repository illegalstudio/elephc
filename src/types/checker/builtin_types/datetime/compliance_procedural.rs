//! Purpose:
//! Audited serialization, procedural date helpers, debug rendering, and interface metadata.
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

/// PHP source backing `DateTime::__serialize()` / `DateTimeImmutable::__serialize()`.
pub(super) const DATETIME_SERIALIZE_SRC: &str = r#"<?php
$__tz = (string)$this->timezone_name;
$__saved = date_default_timezone_get();
date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($__tz));
$__date = date("x-m-d H:i:s", $this->timestamp);
$__us = str_pad((string)$this->microsecond, 6, "0", 1);
$__date = $__date . "." . $__us;
date_default_timezone_set($__saved);
return [
    "date" => $__date,
    "timezone_type" => DateTime::__elephc_timezone_type($__tz),
    "timezone" => $__tz,
];
"#;

/// Synthetic-PHP helpers shared by serialization and php-src-compatible object debugging.
pub(super) const DATETIME_DEBUG_HELPERS_SRC: &str = r#"<?php
if ($timezone === "") {
    return 3;
}
$__first = $timezone[0];
if ($__first === "+" || $__first === "-") {
    return 1;
}
if ($timezone === "UTC"
    || strpos($timezone, "/") !== false
    || in_array(strtolower($timezone), [__TZ_DATABASE_IDENTIFIERS_NO_SLASH__], true)) {
    return 3;
}
return 2;
"#;

/// Builds the case-folded list of database identifiers without `/` that php-src classifies as
/// timezone type 3. Timelib marks abbreviation-only compatibility entries with `F`; those remain
/// type 2 even though they also occur in the location table.
pub(super) fn timezone_database_identifiers_without_slash_literals() -> String {
    include_str!("../../../../../crates/elephc-tz/data/location.data")
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let identifier = fields.next()?;
            let marker = fields.next().unwrap_or_default();
            (!identifier.contains('/') && marker != "F")
                .then(|| format!("\"{}\"", identifier.to_lowercase()))
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Returns php-src's timezone representation discriminator for a stored display timezone.
pub(super) fn datetime_timezone_type() -> ClassMethod {
    let source = DATETIME_DEBUG_HELPERS_SRC.replace(
        "__TZ_DATABASE_IDENTIFIERS_NO_SLASH__",
        &timezone_database_identifiers_without_slash_literals(),
    );
    let tokens = crate::lexer::tokenize(&source)
        .expect("DateTime timezone type helper source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime timezone type helper source must parse");
    ClassMethod {
        name: "__elephc_timezone_type".to_string(),
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
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal instance renderer used when `var_dump()` receives a date/time object.
pub(super) fn datetime_debug_dump(class_name: &str) -> ClassMethod {
    let src = r#"<?php
$pad = str_repeat(" ", __elephc_var_dump_indent(0));
$field_pad = $pad . "  ";
$property_count = __elephc_var_dump_object_property_count($this);
echo $pad . "object(" . get_class($this) . ")#" . spl_object_id($this) . " (" . ($property_count + 3) . ") {\n";
__elephc_var_dump_indent(2);
__elephc_var_dump_object_properties($this);
__elephc_var_dump_indent(-2);
echo $field_pad . "[\"date\"]=>\n";
echo $field_pad; var_dump($this->format("x-m-d H:i:s.u"));
echo $field_pad . "[\"timezone_type\"]=>\n";
echo $field_pad; var_dump(DateTime::__elephc_timezone_type($this->timezone_name));
echo $field_pad . "[\"timezone\"]=>\n";
echo $field_pad; var_dump($this->timezone_name);
echo $pad . "}\n";
"#
    .replace("__CLASS__", class_name);
    let tokens = crate::lexer::tokenize(&src)
        .expect("DateTime debug dump source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime debug dump source must parse");
    ClassMethod {
        name: "__elephc_debug_dump".to_string(),
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
        attributes: Vec::new(),
    }
}

/// Builds the internal renderer used when `print_r()` receives a date/time object.
pub(super) fn datetime_print_r_dump() -> ClassMethod {
    let src = r#"<?php
echo get_class($this) . " Object\n(\n";
__elephc_print_r_object_properties($this);
echo "    [date] => " . $this->format("x-m-d H:i:s.u") . "\n";
echo "    [timezone_type] => " . DateTime::__elephc_timezone_type($this->timezone_name) . "\n";
echo "    [timezone] => " . $this->timezone_name . "\n";
echo ")\n";
"#;
    let tokens =
        crate::lexer::tokenize(src).expect("DateTime print_r dump source must tokenize");
    let body = crate::parser::parse(&tokens).expect("DateTime print_r dump source must parse");
    ClassMethod {
        name: "__elephc_print_r_dump".to_string(),
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
        attributes: Vec::new(),
    }
}

/// Builds the internal renderer for php-src's two-field `DateTimeZone` debug shape.
pub(super) fn datetimezone_debug_dump() -> ClassMethod {
    let src = r#"<?php
$pad = str_repeat(" ", __elephc_var_dump_indent(0));
$field_pad = $pad . "  ";
$property_count = __elephc_var_dump_object_property_count($this);
echo $pad . "object(" . get_class($this) . ")#" . spl_object_id($this) . " (" . ($property_count + 2) . ") {\n";
__elephc_var_dump_indent(2);
__elephc_var_dump_object_properties($this);
__elephc_var_dump_indent(-2);
echo $field_pad . "[\"timezone_type\"]=>\n";
echo $field_pad; var_dump(DateTime::__elephc_timezone_type($this->name));
echo $field_pad . "[\"timezone\"]=>\n";
echo $field_pad; var_dump($this->name);
echo $pad . "}\n";
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DateTimeZone debug dump source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTimeZone debug dump source must parse");
    ClassMethod {
        name: "__elephc_debug_dump".to_string(),
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
        attributes: Vec::new(),
    }
}

/// Builds the internal `print_r()` renderer for php-src's two-field `DateTimeZone` shape.
pub(super) fn datetimezone_print_r_dump() -> ClassMethod {
    let src = r#"<?php
echo get_class($this) . " Object\n(\n";
__elephc_print_r_object_properties($this);
echo "    [timezone_type] => " . DateTime::__elephc_timezone_type($this->name) . "\n";
echo "    [timezone] => " . $this->name . "\n";
echo ")\n";
"#;
    let tokens =
        crate::lexer::tokenize(src).expect("DateTimeZone print_r dump source must tokenize");
    let body =
        crate::parser::parse(&tokens).expect("DateTimeZone print_r dump source must parse");
    ClassMethod {
        name: "__elephc_print_r_dump".to_string(),
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
        attributes: Vec::new(),
    }
}

/// Builds the internal renderer for php-src's component or relative-string
/// `DateInterval` debug shapes.
pub(super) fn dateinterval_debug_dump() -> ClassMethod {
    let src = r#"<?php
$pad = str_repeat(" ", __elephc_var_dump_indent(0));
$field_pad = $pad . "  ";
$property_count = __elephc_var_dump_object_property_count($this);
if ($this->_from_string) {
    echo $pad . "object(" . get_class($this) . ")#" . spl_object_id($this) . " (" . ($property_count + 2) . ") {\n";
    __elephc_var_dump_indent(2);
    __elephc_var_dump_object_properties($this);
    __elephc_var_dump_indent(-2);
    echo $field_pad . "[\"from_string\"]=>\n";
    echo $field_pad; var_dump(true);
    echo $field_pad . "[\"date_string\"]=>\n";
    echo $field_pad; var_dump($this->_date_string);
    echo $pad . "}\n";
    return;
}
echo $pad . "object(" . get_class($this) . ")#" . spl_object_id($this) . " (" . ($property_count + 10) . ") {\n";
__elephc_var_dump_indent(2);
__elephc_var_dump_object_properties($this);
__elephc_var_dump_indent(-2);
echo $field_pad . "[\"y\"]=>\n"; echo $field_pad; var_dump($this->y);
echo $field_pad . "[\"m\"]=>\n"; echo $field_pad; var_dump($this->m);
echo $field_pad . "[\"d\"]=>\n"; echo $field_pad; var_dump($this->d);
echo $field_pad . "[\"h\"]=>\n"; echo $field_pad; var_dump($this->h);
echo $field_pad . "[\"i\"]=>\n"; echo $field_pad; var_dump($this->i);
echo $field_pad . "[\"s\"]=>\n"; echo $field_pad; var_dump($this->s);
echo $field_pad . "[\"f\"]=>\n"; echo $field_pad; var_dump($this->f);
echo $field_pad . "[\"invert\"]=>\n"; echo $field_pad; var_dump($this->invert);
echo $field_pad . "[\"days\"]=>\n"; echo $field_pad; var_dump($this->days);
echo $field_pad . "[\"from_string\"]=>\n"; echo $field_pad; var_dump(false);
echo $pad . "}\n";
"#;
    let tokens = crate::lexer::tokenize(src)
        .expect("DateInterval debug dump source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateInterval debug dump source must parse");
    ClassMethod {
        name: "__elephc_debug_dump".to_string(),
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
        attributes: Vec::new(),
    }
}

/// Builds the internal `print_r()` renderer for php-src's `DateInterval` shapes.
pub(super) fn dateinterval_print_r_dump() -> ClassMethod {
    let src = r#"<?php
echo get_class($this) . " Object\n(\n";
__elephc_print_r_object_properties($this);
if ($this->_from_string) {
    echo "    [from_string] => 1\n";
    echo "    [date_string] => " . $this->_date_string . "\n";
    echo ")\n";
    return;
}
echo "    [y] => " . $this->y . "\n";
echo "    [m] => " . $this->m . "\n";
echo "    [d] => " . $this->d . "\n";
echo "    [h] => " . $this->h . "\n";
echo "    [i] => " . $this->i . "\n";
echo "    [s] => " . $this->s . "\n";
echo "    [f] => " . $this->f . "\n";
echo "    [invert] => " . $this->invert . "\n";
echo "    [days] => " . $this->days . "\n";
echo "    [from_string] => \n";
echo ")\n";
"#;
    let tokens =
        crate::lexer::tokenize(src).expect("DateInterval print_r dump source must tokenize");
    let body =
        crate::parser::parse(&tokens).expect("DateInterval print_r dump source must parse");
    ClassMethod {
        name: "__elephc_print_r_dump".to_string(),
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
        attributes: Vec::new(),
    }
}

/// PHP source backing `DateTime::__unserialize()` / `DateTimeImmutable::__unserialize()`.
/// Restores the serialized wall clock directly so internal reconstruction does not allocate
/// user-visible `DateTimeZone`/`DateTime` handles ahead of the object being unserialized.
pub(super) const DATETIME_UNSERIALIZE_SRC: &str = r#"<?php
if (!array_key_exists("date", $data)
    || !array_key_exists("timezone_type", $data)
    || !array_key_exists("timezone", $data)
    || !is_string($data["date"])
    || !is_int($data["timezone_type"])
    || !is_string($data["timezone"])) {
    throw new Error("Invalid serialization data for __CLASS__ object");
}
$__date = $data["date"];
$__tz = $data["timezone"];
$__tzType = $data["timezone_type"];
$__normalizedTz = DateTimeZone::__elephc_normalize_timezone($__tz);
if ($__normalizedTz === ""
    || $__tzType !== DateTime::__elephc_timezone_type($__normalizedTz)) {
    throw new Error("Invalid serialization data for __CLASS__ object");
}
$__tz = $__normalizedTz;
$this->microsecond = DateTime::__elephc_extract_micros($__date);
$__dateWithoutMicros = DateTime::__elephc_strip_micros($__date);
$__saved = date_default_timezone_get();
if ($__tzType === 1) {
    date_default_timezone_set("UTC");
    $__timestamp = strtotime($__dateWithoutMicros);
    $__offsetSeconds = intval(substr($__tz, 1, 2)) * 3600
        + intval(substr($__tz, 4, 2)) * 60;
    if (strlen($__tz) === 9) {
        $__offsetSeconds = $__offsetSeconds + intval(substr($__tz, 7, 2));
    }
    if ($__tz[0] === "-") {
        $__offsetSeconds = -$__offsetSeconds;
    }
    if ($__timestamp !== false) {
        $__timestamp = $__timestamp - $__offsetSeconds;
    }
} else {
    if (!@date_default_timezone_set(DateTime::__elephc_runtime_timezone_name($__tz))) {
        date_default_timezone_set($__saved);
        throw new Error("Invalid serialization data for __CLASS__ object");
    }
    $__timestamp = strtotime($__dateWithoutMicros);
}
date_default_timezone_set($__saved);
if ($__timestamp === false) {
    throw new Error("Invalid serialization data for __CLASS__ object");
}
$this->timestamp = $__timestamp;
$this->timezone_name = $__tz;
$this->__elephc_initialized = true;
"#;

/// PHP source backing `DateTime::__set_state()` / `DateTimeImmutable::__set_state()`.
/// `__CLASS__` is substituted with the concrete class.
pub(super) const DATETIME_SET_STATE_SRC: &str = r#"<?php
$__d = new __CLASS__;
$__d->__unserialize($array);
return $__d;
"#;

/// PHP source backing `__wakeup()`.
///
/// `__CLASS__` is replaced with the concrete class. Date/time classes except
/// `DateInterval` reject a direct wakeup as invalid serialization data.
pub(super) const DATETIME_WAKEUP_SRC: &str = r#"<?php
__elephc_diag_warning("Deprecated: Method __CLASS__::__wakeup() is deprecated since 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()\n", 0, E_DEPRECATED);
if ("__CLASS__" !== "DateInterval") {
    throw new Error("Invalid serialization data for __CLASS__ object");
}
"#;

/// Builds `__serialize(): array` for the given date/time class.
pub(super) fn datetime_serialize() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATETIME_SERIALIZE_SRC)
        .expect("DateTime::__serialize body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime::__serialize body source must parse");
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

/// Builds `__unserialize(array $data): void` for the given date/time class. `class_name` is
/// substituted into the body for the `__CLASS__` token.
pub(super) fn datetime_unserialize(class_name: &str) -> ClassMethod {
    let src = DATETIME_UNSERIALIZE_SRC.replace("__CLASS__", class_name);
    let tokens = crate::lexer::tokenize(&src)
        .expect("DateTime::__unserialize body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime::__unserialize body source must parse");
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

/// Builds `static __set_state(array $array): static` for the given date/time class.
pub(super) fn datetime_set_state(class_name: &str) -> ClassMethod {
    let src = DATETIME_SET_STATE_SRC.replace("__CLASS__", class_name);
    let tokens = crate::lexer::tokenize(&src)
        .expect("DateTime::__set_state body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime::__set_state body source must parse");
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
        return_type: Some(TypeExpr::Named(Name::unqualified(class_name))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds `__wakeup(): void` for the given date/time class (no-op in elephc).
pub(super) fn datetime_wakeup(class_name: &str) -> ClassMethod {
    let src = DATETIME_WAKEUP_SRC.replace("__CLASS__", class_name);
    let tokens = crate::lexer::tokenize(&src)
        .expect("DateTime::__wakeup body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTime::__wakeup body source must parse");
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
        attributes: deprecated_attribute(
            "8.5",
            "this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()",
        ),
    }
}

/// Returns the 4 serialization methods for a date/time class.
pub(super) fn datetime_serialize_methods(class_name: &str) -> Vec<ClassMethod> {
    vec![
        datetime_wakeup(class_name),
        datetime_serialize(),
        datetime_unserialize(class_name),
        datetime_set_state(class_name),
    ]
}

/// PHP source backing `DateTimeZone::__serialize()`, retaining php-src's
/// offset/abbreviation/identifier discriminator.
pub(super) const DATETIMEZONE_SERIALIZE_SRC: &str = r#"<?php
return [
    "timezone_type" => DateTime::__elephc_timezone_type($this->name),
    "timezone" => $this->name,
];
"#;

/// PHP source backing `DateTimeZone::__set_state()`. Creates a new zone from the array's `timezone` key.
pub(super) const DATETIMEZONE_SET_STATE_SRC: &str = r#"<?php
if (!array_key_exists("timezone_type", $array)
    || !array_key_exists("timezone", $array)
    || !is_int($array["timezone_type"])
    || !is_string($array["timezone"])) {
    throw new Error("Invalid serialization data for DateTimeZone object");
}
$__normalized = DateTimeZone::__elephc_normalize_timezone($array["timezone"]);
if ($__normalized === ""
    || $array["timezone_type"] !== DateTime::__elephc_timezone_type($__normalized)) {
    throw new Error("Invalid serialization data for DateTimeZone object");
}
return new DateTimeZone($__normalized);
"#;

/// Builds `DateTimeZone::__serialize(): array`.
pub(super) fn datetimezone_serialize() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATETIMEZONE_SERIALIZE_SRC)
        .expect("DateTimeZone::__serialize body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTimeZone::__serialize body source must parse");
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

/// Builds `DateTimeZone::__unserialize(array $data): void`.
pub(super) fn datetimezone_unserialize() -> ClassMethod {
    let src = r#"<?php
if (!array_key_exists("timezone_type", $data)
    || !array_key_exists("timezone", $data)
    || !is_int($data["timezone_type"])
    || !is_string($data["timezone"])) {
    throw new Error("Invalid serialization data for DateTimeZone object");
}
$__normalized = DateTimeZone::__elephc_normalize_timezone($data["timezone"]);
if ($__normalized === ""
    || $data["timezone_type"] !== DateTime::__elephc_timezone_type($__normalized)) {
    throw new Error("Invalid serialization data for DateTimeZone object");
}
$this->name = $__normalized;
$this->__elephc_initialized = true;
"#;
    let tokens = crate::lexer::tokenize(src).expect("DateTimeZone::__unserialize body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("DateTimeZone::__unserialize body source must parse");
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

/// Builds `static DateTimeZone::__set_state(array $array): static`.
pub(super) fn datetimezone_set_state() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATETIMEZONE_SET_STATE_SRC)
        .expect("DateTimeZone::__set_state body source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DateTimeZone::__set_state body source must parse");
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
        return_type: Some(TypeExpr::Named(Name::unqualified("DateTimeZone"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Returns the serialization methods for `DateTimeZone`.
pub(super) fn datetimezone_serialize_methods() -> Vec<ClassMethod> {
    vec![
        datetime_wakeup("DateTimeZone"),
        datetimezone_serialize(),
        datetimezone_unserialize(),
        datetimezone_set_state(),
    ]
}

/// PHP source backing `date_create()` / `date_create_immutable()`. The procedural aliases return
/// `DateTime|false` (false on an unparseable string), unlike `new DateTime()` which throws
/// `DateMalformedStringException` (PHP 8.3+). The wrapper catches the exception and returns `false`.
/// `__CLASS__` is substituted with the concrete class so each alias builds its own type.
pub(super) const DATE_CREATE_SRC: &str = r#"<?php
try {
    if ($timezone === null) {
        return new __CLASS__($datetime);
    }
    try {
        $timezone->__elephc_assert_initialized();
    } catch (\DateObjectError $e) {
        throw new \Error("The DateTimeZone object has not been correctly initialized by its constructor");
    }
    return new __CLASS__($datetime, $timezone);
} catch (\DateMalformedStringException $e) {
    return false;
}
"#;

/// Builds the internal static `__elephc_date_create($datetime = "now", $timezone = null)` method on
/// the given class, backing the `date_create()` / `date_create_immutable()` procedural aliases. They
/// return the constructed instance or `false` on an unparseable string (catching the ctor's
/// `DateMalformedStringException`). Self-contained parsed source.
pub(super) fn datetime_date_create(class_name: &str) -> ClassMethod {
    let src = DATE_CREATE_SRC.replace("__CLASS__", class_name);
    let tokens = crate::lexer::tokenize(&src).expect("date_create body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("date_create body source must parse");
    ClassMethod {
        name: "__elephc_date_create".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
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

/// PHP source backing `date_modify()`. The procedural alias returns `DateTime|false` (false on an
/// unparseable modifier), unlike `DateTime::modify()` which throws `DateMalformedStringException`
/// (PHP 8.3+). The wrapper catches the exception, emits php-src's suppressible procedural warning,
/// and returns `false`.
pub(super) const DATE_MODIFY_SRC: &str = r#"<?php
try {
    return $object->modify($modifier);
} catch (\DateMalformedStringException $e) {
    __elephc_diag_warning(
        "\nWarning: date_modify(): " . substr($e->getMessage(), 20),
        $sourceLine
    );
    return false;
}
"#;

/// Builds the internal static `__elephc_date_modify($object, $modifier)` method on `DateTime`, backing
/// the `date_modify()` procedural alias. Returns the modified object or `false` on an unparseable
/// modifier after emitting the matching suppressible warning (catching `modify()`'s
/// `DateMalformedStringException`). `$object` is typed `mixed` so the alias composes with
/// `date_create()` (which returns `DateTime|false` aka `mixed`). Self-contained parsed source.
pub(super) fn datetime_date_modify() -> ClassMethod {
    let tokens = crate::lexer::tokenize(DATE_MODIFY_SRC).expect("date_modify body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("date_modify body source must parse");
    ClassMethod {
        name: "__elephc_date_modify".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("object".to_string(), Some(TypeExpr::Named(Name::unqualified("mixed"))), None, false),
            ("modifier".to_string(), Some(TypeExpr::Str), None, false),
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

/// Translates the strftime `%`-format into a `date()` format, then calls `date()`/`gmdate()`.
/// Common specifiers map 1:1 (or to a composite like `%T` -> `H:i:s`); `%j`/`%C` are computed and
/// inlined as literal digits (digits pass through `date()`). Literal letters are backslash-escaped so
/// `date()` keeps them literal. Locale-dependent `%c`/`%x`/`%X` reproduce PHP's default C/POSIX
/// locale byte-for-byte (elephc has no `setlocale()`, so the C locale is the only reachable behavior;
/// locale-aware output would require a separate locale system, which is out of scope here);
/// week-number `%U`/`%V`/`%W` are computed to match PHP; space-padded `%e`/`%k`/`%l` are space-padded
/// from the non-padded `date()` specifier.
pub(super) const STRFTIME_SRC: &str = r#"<?php
if ($utc) {
    __elephc_diag_warning("\nDeprecated: Function gmstrftime() is deprecated since 8.1, use IntlDateFormatter::format() instead", $sourceLine, E_DEPRECATED);
} else {
    __elephc_diag_warning("\nDeprecated: Function strftime() is deprecated since 8.1, use IntlDateFormatter::format() instead", $sourceLine, E_DEPRECATED);
}
if ($format === "") {
    return false;
}
$out = "";
$flen = strlen($format);
$k = 0;
while ($k < $flen) {
    $ch = $format[$k];
    if ($ch !== "%") {
        $cc = ord($ch);
        if (($cc >= 65 && $cc <= 90) || ($cc >= 97 && $cc <= 122)) {
            $out = $out . "\\" . $ch;
        } else {
            $out = $out . $ch;
        }
        $k = $k + 1;
        continue;
    }
    $k = $k + 1;
    if ($k >= $flen) { break; }
    $spec = $format[$k];
    $k = $k + 1;
    if ($spec === "a") { $out = $out . "D"; }
    else if ($spec === "A") { $out = $out . "l"; }
    else if ($spec === "d") { $out = $out . "d"; }
    else if ($spec === "e") {
        if ($utc) { $dd = intval(gmdate("j", $timestamp)); } else { $dd = intval(date("j", $timestamp)); }
        $ds = "" . $dd;
        if (strlen($ds) < 2) { $ds = " " . $ds; }
        $out = $out . $ds;
    }
    else if ($spec === "j") {
        if ($utc) { $z = intval(gmdate("z", $timestamp)); } else { $z = intval(date("z", $timestamp)); }
        $z = $z + 1;
        $zs = "" . $z;
        while (strlen($zs) < 3) { $zs = "0" . $zs; }
        $out = $out . $zs;
    }
    else if ($spec === "u") { $out = $out . "N"; }
    else if ($spec === "w") { $out = $out . "w"; }
    else if ($spec === "V") { $out = $out . "W"; }
    else if ($spec === "U" || $spec === "W") {
        if ($utc) { $wd = intval(gmdate("w", $timestamp)); $yd = intval(gmdate("z", $timestamp)); }
        else { $wd = intval(date("w", $timestamp)); $yd = intval(date("z", $timestamp)); }
        // %U counts weeks from the first Sunday; %W from the first Monday.
        if ($spec === "W") { if ($wd === 0) { $wd = 6; } else { $wd = $wd - 1; } }
        $wk = intdiv($yd + 7 - $wd, 7);
        $ws = "" . $wk;
        while (strlen($ws) < 2) { $ws = "0" . $ws; }
        $out = $out . $ws;
    }
    else if ($spec === "G") { $out = $out . "o"; }
    else if ($spec === "g") {
        if ($utc) { $iy = intval(gmdate("o", $timestamp)); } else { $iy = intval(date("o", $timestamp)); }
        $g2 = $iy % 100;
        $gs = "" . $g2;
        while (strlen($gs) < 2) { $gs = "0" . $gs; }
        $out = $out . $gs;
    }
    else if ($spec === "b" || $spec === "h") { $out = $out . "M"; }
    else if ($spec === "B") { $out = $out . "F"; }
    else if ($spec === "m") { $out = $out . "m"; }
    else if ($spec === "y") { $out = $out . "y"; }
    else if ($spec === "Y") { $out = $out . "Y"; }
    else if ($spec === "C") {
        if ($utc) { $yy = intval(gmdate("Y", $timestamp)); } else { $yy = intval(date("Y", $timestamp)); }
        $cen = intdiv($yy, 100);
        $cs = "" . $cen;
        while (strlen($cs) < 2) { $cs = "0" . $cs; }
        $out = $out . $cs;
    }
    else if ($spec === "H") { $out = $out . "H"; }
    else if ($spec === "k") {
        if ($utc) { $kh = intval(gmdate("G", $timestamp)); } else { $kh = intval(date("G", $timestamp)); }
        $ks = "" . $kh;
        if (strlen($ks) < 2) { $ks = " " . $ks; }
        $out = $out . $ks;
    }
    else if ($spec === "I") { $out = $out . "h"; }
    else if ($spec === "l") {
        if ($utc) { $hh = intval(gmdate("g", $timestamp)); } else { $hh = intval(date("g", $timestamp)); }
        $hs = "" . $hh;
        if (strlen($hs) < 2) { $hs = " " . $hs; }
        $out = $out . $hs;
    }
    else if ($spec === "M") { $out = $out . "i"; }
    else if ($spec === "p") { $out = $out . "A"; }
    else if ($spec === "P") { $out = $out . "a"; }
    else if ($spec === "r") { $out = $out . "h:i:s A"; }
    else if ($spec === "R") { $out = $out . "H:i"; }
    else if ($spec === "S") { $out = $out . "s"; }
    else if ($spec === "T" || $spec === "X") { $out = $out . "H:i:s"; }
    else if ($spec === "D" || $spec === "x") { $out = $out . "m/d/y"; }
    else if ($spec === "F") { $out = $out . "Y-m-d"; }
    else if ($spec === "s") { $out = $out . "U"; }
    else if ($spec === "z") { $out = $out . "O"; }
    else if ($spec === "Z") { $out = $out . "T"; }
    else if ($spec === "c") {
        if ($utc) { $cd = intval(gmdate("j", $timestamp)); } else { $cd = intval(date("j", $timestamp)); }
        $cs = "" . $cd;
        if (strlen($cs) < 2) { $cs = " " . $cs; }
        $out = $out . "D M " . $cs . " H:i:s Y";
    }
    else if ($spec === "n") { $out = $out . "\n"; }
    else if ($spec === "t") { $out = $out . "\t"; }
    else if ($spec === "%") { $out = $out . "%"; }
    else {
        $sc = ord($spec);
        if (($sc >= 65 && $sc <= 90) || ($sc >= 97 && $sc <= 122)) {
            $out = $out . "\\" . $spec;
        } else {
            $out = $out . $spec;
        }
    }
}
if ($utc) { return gmdate($out, $timestamp); }
return date($out, $timestamp);
"#;

/// PHP source for `DateTime::__elephc_extract_micros($s)` — returns the normalized
/// microseconds (0..999999) of an epoch literal or a trailing `HH:MM:SS.ffffff`.
/// Negative epoch fractions complement the decimal digits because timelib stores
/// the whole seconds using floor semantics (`@-0.4` is second `-1`, microsecond `600000`).
/// The ordinary-date dot must follow `:SS` so a `DD.MM.YYYY` separator is not mistaken
/// for a fraction. `substr` reads chars to avoid a computed string-index miscompile.
pub(super) const EXTRACT_MICROS_SRC: &str = r#"<?php
$__dot = strrpos($s, ".");
if (substr($s, 0, 1) === "@" && $__dot !== false) {
    $__fd = substr($s, $__dot + 1);
    while (strlen($__fd) < 6) { $__fd = $__fd . "0"; }
    $__micro = intval(substr($__fd, 0, 6));
    if (substr($s, 1, 1) === "-" && $__micro !== 0) {
        return 1000000 - $__micro;
    }
    return $__micro;
}
if ($__dot !== false && $__dot >= 3 && substr($s, $__dot - 3, 1) === ":") {
    $__fd = "";
    $__k = $__dot + 1;
    $__len = strlen($s);
    while ($__k < $__len) {
        $__c = substr($s, $__k, 1);
        if ($__c >= "0" && $__c <= "9") { $__fd = $__fd . $__c; $__k = $__k + 1; }
        else { break; }
    }
    if ($__fd !== "") {
        while (strlen($__fd) < 6) { $__fd = $__fd . "0"; }
        return intval(substr($__fd, 0, 6));
    }
}
return 0;
"#;

/// PHP source for `DateTime::__elephc_strip_micros($s)` — returns the string with a
/// trailing fractional second removed, so `strtotime()` can parse the remainder. Always
/// returns a freshly allocated string (never the borrowed argument) so the constructor's
/// `$datetime = __elephc_strip_micros($datetime)` self-reassignment cannot free-then-reuse
/// an owned source string.
pub(super) const STRIP_MICROS_SRC: &str = r#"<?php
$__dot = strrpos($s, ".");
if ($__dot !== false && $__dot >= 3 && substr($s, $__dot - 3, 1) === ":") {
    $__k = $__dot + 1;
    $__len = strlen($s);
    while ($__k < $__len) {
        $__c = substr($s, $__k, 1);
        if ($__c >= "0" && $__c <= "9") { $__k = $__k + 1; }
        else { break; }
    }
    return substr($s, 0, $__dot) . substr($s, $__k);
}
// Return a fresh copy (concat with "") rather than `$s` itself: the constructor
// self-reassigns `$datetime = __elephc_strip_micros($datetime)`, and returning the
// borrowed argument would make that assignment release the owned source string and
// then store the same freed pointer (use-after-free) when the source is an owned
// temporary, e.g. a Mixed datetime string materialized from an untyped argument.
return $s . "";
"#;

/// PHP source for detecting a timezone suffix in a free-form DateTime constructor string.
///
/// The result keeps php-src's canonical display spelling and removes the suffix from the
/// wall-clock input so elephc can parse it under the matching runtime timezone. Named zones,
/// abbreviations, military zones, `Z`, and every numeric offset spelling accepted by the
/// synthetic `DateTimeZone` constructor share the same validation path.
pub(super) const EXTRACT_CONSTRUCTOR_ZONE_SRC: &str = r#"<?php
$__display = "";
$__base = $datetime . "";
$__len = strlen($datetime);
$__normalized = DateTimeZone::__elephc_normalize_timezone($datetime);
if ($__normalized !== "") {
    $__display = "" . $__normalized;
    $__base = "now";
}
if ($__display === "" && $__len >= 4
    && strtoupper(substr($datetime, $__len - 4)) === " GMT") {
    $__display = "GMT";
    $__base = substr($datetime, 0, $__len - 4);
}
if ($__display === "" && strpos($datetime, " GMT ") !== false) {
    $__display = "GMT";
    $__base = str_replace(" GMT ", " ", $datetime);
}
$__space = strrpos($datetime, " ");
if ($__display === "" && $__space !== false && $__space + 1 < $__len) {
    $__candidate = substr($datetime, $__space + 1);
    $__normalized = DateTimeZone::__elephc_normalize_timezone($__candidate);
    if ($__normalized !== "") {
        $__display = "" . $__normalized;
        $__base = substr($datetime, 0, $__space);
    }
}
if ($__display === "" && $__len > 1) {
    $__last = strtoupper(substr($datetime, $__len - 1, 1));
    $__lastCode = ord($__last);
    $__previous = substr($datetime, $__len - 2, 1);
    $__military = ($__lastCode >= 65 && $__lastCode <= 73)
        || ($__lastCode >= 75 && $__lastCode <= 90);
    if ($__military && ctype_digit($__previous)) {
        $__normalized = DateTimeZone::__elephc_normalize_timezone($__last);
        if ($__normalized !== "") {
            $__display = "" . $__normalized;
            $__base = substr($datetime, 0, $__len - 1);
        }
    }
}
if ($__display === "") {
    $__plus = strrpos($datetime, "+");
    $__minus = strrpos($datetime, "-");
    $__offset = $__plus;
    if ($__minus !== false && ($__offset === false || $__minus > $__offset)) {
        $__offset = $__minus;
    }
    if ($__offset !== false && $__offset > 0
        && strrpos(substr($datetime, 0, $__offset), ":") !== false) {
        $__candidate = substr($datetime, $__offset);
        $__normalized = DateTimeZone::__elephc_normalize_timezone($__candidate);
        if ($__normalized !== "") {
            $__display = "" . $__normalized;
            $__base = substr($datetime, 0, $__offset);
        }
    }
}
return $__display . "\t" . $__base;
"#;

/// Builds the internal static `DateTime::__elephc_extract_micros(string $s): int`.
pub(super) fn datetime_extract_micros() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(EXTRACT_MICROS_SRC).expect("extract_micros body must tokenize");
    let body = crate::parser::parse(&tokens).expect("extract_micros body must parse");
    ClassMethod {
        name: "__elephc_extract_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("s".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal constructor timezone-suffix parser.
pub(super) fn datetime_extract_constructor_zone() -> ClassMethod {
    let tokens = crate::lexer::tokenize(EXTRACT_CONSTRUCTOR_ZONE_SRC)
        .expect("constructor timezone parser source must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("constructor timezone parser source must parse");
    ClassMethod {
        name: "__elephc_extract_constructor_zone".to_string(),
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
        return_type: Some(TypeExpr::Str),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// PHP source for `DateTime::__elephc_extract_modify_micros($m)` — sums the
/// microsecond and millisecond deltas in a modify() string, including timelib's
/// `usec`, `µs`, `msec`, and long-form aliases.
pub(super) const EXTRACT_MODIFY_MICROS_SRC: &str = r#"<?php
$__toks = explode(" ", $m);
$__n = count($__toks);
$__sum = 0;
$__i = 0;
while ($__i < $__n) {
    $__t = strtolower($__toks[$__i]);
    $__factor = 0;
    if ($__t === "microsecond" || $__t === "microseconds" || $__t === "usec" ||
        $__t === "usecs" || $__t === "µs" || $__t === "µsec" || $__t === "µsecs") {
        $__factor = 1;
    } else if ($__t === "millisecond" || $__t === "milliseconds" || $__t === "ms" ||
               $__t === "msec" || $__t === "msecs") {
        $__factor = 1000;
    }
    if ($__factor !== 0 && $__i > 0) {
        $__sum = $__sum + intval($__toks[$__i - 1]) * $__factor;
    }
    $__i = $__i + 1;
}
return $__sum;
"#;

/// PHP source for `DateTime::__elephc_strip_modify_micros($m)` — returns the
/// modify() string with every supported sub-second clause removed, so the
/// remainder can be parsed by strtotime() without applying it twice.
pub(super) const STRIP_MODIFY_MICROS_SRC: &str = r#"<?php
$__toks = explode(" ", $m);
$__n = count($__toks);
$__out = "";
$__i = 0;
while ($__i < $__n) {
    $__unit = 0;
    if ($__i + 1 < $__n) {
        $__nt = strtolower($__toks[$__i + 1]);
        if ($__nt === "microsecond" || $__nt === "microseconds" || $__nt === "usec" ||
            $__nt === "usecs" || $__nt === "µs" || $__nt === "µsec" || $__nt === "µsecs" ||
            $__nt === "millisecond" || $__nt === "milliseconds" || $__nt === "ms" ||
            $__nt === "msec" || $__nt === "msecs") {
            $__unit = 1;
        }
    }
    if ($__unit === 1) {
        $__i = $__i + 2;
    } else {
        if ($__out !== "") { $__out = $__out . " "; }
        $__out = $__out . $__toks[$__i];
        $__i = $__i + 1;
    }
}
return $__out;
"#;

/// PHP source for the detailed php-src/timelib parse error used by DateTime constructors and
/// `modify()`. The first parser error supplies the byte position and message; an exhausted input
/// uses a single display space for the character field, matching php-src's `( )` rendering.
pub(super) const MALFORMED_TIME_MESSAGE_SRC: &str = r#"<?php
$__parsed = DateTime::__elephc_date_parse($input);
$__position = 0;
$__message = "Unknown or bad format";
if ($__parsed["error_count"] > 0) {
    $__errors = $__parsed["errors"];
    $__position = intval(array_key_first($__errors));
    $__message = $__errors[$__position];
}
$__character = substr($input, $__position, 1);
if ($__character === "") {
    $__character = " ";
}
return $context
    . "Failed to parse time string (" . $input . ") at position "
    . $__position . " (" . $__character . "): " . $__message;
"#;

/// Builds the internal static `DateTime::__elephc_extract_modify_micros(string $m): int`.
pub(super) fn datetime_extract_modify_micros() -> ClassMethod {
    let tokens = crate::lexer::tokenize(EXTRACT_MODIFY_MICROS_SRC)
        .expect("extract_modify_micros body must tokenize");
    let body = crate::parser::parse(&tokens).expect("extract_modify_micros body must parse");
    ClassMethod {
        name: "__elephc_extract_modify_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("m".to_string(), Some(TypeExpr::Str), None, false)],
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type: Some(TypeExpr::Int),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the internal static `DateTime::__elephc_strip_modify_micros(string $m): string`.
pub(super) fn datetime_strip_modify_micros() -> ClassMethod {
    let tokens = crate::lexer::tokenize(STRIP_MODIFY_MICROS_SRC)
        .expect("strip_modify_micros body must tokenize");
    let body = crate::parser::parse(&tokens).expect("strip_modify_micros body must parse");
    ClassMethod {
        name: "__elephc_strip_modify_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("m".to_string(), Some(TypeExpr::Str), None, false)],
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

/// Builds the internal timelib-backed parse-error formatter shared by constructors and `modify()`.
pub(super) fn datetime_malformed_time_message() -> ClassMethod {
    let tokens = crate::lexer::tokenize(MALFORMED_TIME_MESSAGE_SRC)
        .expect("malformed-time message body must tokenize");
    let body = crate::parser::parse(&tokens).expect("malformed-time message body must parse");
    ClassMethod {
        name: "__elephc_malformed_time_message".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("context".to_string(), Some(TypeExpr::Str), None, false),
            ("input".to_string(), Some(TypeExpr::Str), None, false),
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

/// Builds the internal static `DateTime::__elephc_strip_micros(string $s): string`.
pub(super) fn datetime_strip_micros() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(STRIP_MICROS_SRC).expect("strip_micros body must tokenize");
    let body = crate::parser::parse(&tokens).expect("strip_micros body must parse");
    ClassMethod {
        name: "__elephc_strip_micros".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("s".to_string(), Some(TypeExpr::Str), None, false)],
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

/// Builds the internal static `DateTime::__elephc_strftime($format, $timestamp, $utc, $sourceLine)` method
/// backing the `strftime()`/`gmstrftime()` procedural functions (the name resolver desugars the
/// calls to it, injecting `time()` for the default timestamp and the local/UTC flag). Self-contained
/// parsed source.
pub(super) fn datetime_strftime() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(STRFTIME_SRC).expect("strftime body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("strftime body source must parse");
    ClassMethod {
        name: "__elephc_strftime".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("format".to_string(), Some(TypeExpr::Str), None, false),
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            ("utc".to_string(), Some(TypeExpr::Bool), None, false),
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

/// Synthetic-PHP body of the shared solar "rise/set" core, a faithful port of timelib's
/// `astro.c` (Paul Schlyter's algorithm). Given the UTC-midnight timestamp of a day, an observer
/// longitude/latitude, a target altitude (degrees), and an upper-limb flag, it returns the
/// diurnal-arc result as an associative array `["rc"=>int, "hr"=>float, "hs"=>float, "ts"=>float]`:
/// `rc` is 0 (sun crosses the altitude), +1 (always above), or -1 (always below); `hr`/`hs` are the
/// rise/set hours UT (valid only when `rc==0`); `ts` is the south-transit hour UT. All angles are in
/// degrees, matching the original; `M_PI` provides the exact conversion factor PHP's C code uses.
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
pub(super) const SUN_INFO_SRC: &str = r#"<?php
if (!is_finite($latitude)) {
    throw new ValueError('date_sun_info(): Argument #2 ($latitude) must be finite');
}
if (!is_finite($longitude)) {
    throw new ValueError('date_sun_info(): Argument #3 ($longitude) must be finite');
}
$y = intval(date("Y", $timestamp));
$mo = intval(date("n", $timestamp));
$dy = intval(date("j", $timestamp));
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
pub(super) const SUNFUNC_SRC: &str = r#"<?php
if ($which == 0) {
    if ($line > 0) {
        __elephc_diag_warning("\nDeprecated: Function date_sunrise() is deprecated since 8.1, use date_sun_info() instead", $line, E_DEPRECATED);
    } else {
        __elephc_diag_warning("\nDeprecated: Function date_sunrise() is deprecated since 8.1, use date_sun_info() instead\n", 0, E_DEPRECATED);
    }
} else {
    if ($line > 0) {
        __elephc_diag_warning("\nDeprecated: Function date_sunset() is deprecated since 8.1, use date_sun_info() instead", $line, E_DEPRECATED);
    } else {
        __elephc_diag_warning("\nDeprecated: Function date_sunset() is deprecated since 8.1, use date_sun_info() instead\n", 0, E_DEPRECATED);
    }
}
if ($returnFormat !== 0 && $returnFormat !== 1 && $returnFormat !== 2) {
    if ($which == 0) {
        throw new ValueError("date_sunrise(): Argument #2 (\$returnFormat) must be one of SUNFUNCS_RET_TIMESTAMP, SUNFUNCS_RET_STRING, or SUNFUNCS_RET_DOUBLE");
    }
    throw new ValueError("date_sunset(): Argument #2 (\$returnFormat) must be one of SUNFUNCS_RET_TIMESTAMP, SUNFUNCS_RET_STRING, or SUNFUNCS_RET_DOUBLE");
}
$lat = ($latitude === null) ? 31.7667 : $latitude;
$lon = ($longitude === null) ? 35.2333 : $longitude;
$zen = ($zenith === null) ? (90.0 + 50.0 / 60.0) : $zenith;
if (!is_finite($lat) || !is_finite($lon) || !is_finite($zen)) {
    return false;
}
$offset = ($utcOffset === null) ? intval(date("Z")) / 3600.0 : $utcOffset;
$y = intval(date("Y", $timestamp));
$mo = intval(date("n", $timestamp));
$dy = intval(date("j", $timestamp));
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
    $N = $r["hr"] + $offset;
} else {
    $N = $r["hs"] + $offset;
}
if ($N > 24.0 || $N < 0.0) {
    $N = $N - floor($N / 24.0) * 24.0;
}
if (!($N <= 24.0 && $N >= 0.0)) {
    return false;
}
if ($returnFormat == 2) {
    return $N;
}
$hh = intval($N);
$mm = intval(60.0 * ($N - $hh));
return sprintf("%02d:%02d", $hh, $mm);
"#;

/// Synthetic-PHP body converting PHP's canonical fixed-offset zone names (`+02:00`) into the
/// inverted-sign POSIX `TZ` form (`UTC-2`) expected by libc. Named IANA/abbreviation zones pass
/// through unchanged; the original PHP name remains stored on the object for `getTimezone()`.
pub(super) const RUNTIME_TIMEZONE_NAME_SRC: &str = r#"<?php
$upper = strtoupper($zone);
if ($upper === "UTC" || $upper === "GMT" || $upper === "Z"
    || $zone === "+00:00" || $zone === "-00:00") {
    return "UTC";
}
if ((strlen($zone) === 6 || strlen($zone) === 9)
    && ($zone[0] === "+" || $zone[0] === "-")
    && $zone[3] === ":"
    && ctype_digit($zone[1]) && ctype_digit($zone[2])
    && ctype_digit($zone[4]) && ctype_digit($zone[5])) {
    $hours = intval(substr($zone, 1, 2));
    $minutes = substr($zone, 4, 2);
    $sign = ($zone[0] === "+") ? "-" : "+";
    $runtime = "UTC" . $sign . $hours;
    if ($minutes !== "00") {
        $runtime = $runtime . ":" . $minutes;
    }
    if (strlen($zone) === 9 && $zone[6] === ":"
        && ctype_digit($zone[7]) && ctype_digit($zone[8])) {
        if ($minutes === "00") {
            $runtime = $runtime . ":00";
        }
        $runtime = $runtime . ":" . substr($zone, 7, 2);
    }
    return $runtime;
}
if (strlen($upper) === 1) {
    $code = ord($upper);
    $offset = 0;
    if ($code >= 65 && $code <= 73) {
        $offset = $code - 64;
    } else if ($code >= 75 && $code <= 77) {
        $offset = $code - 65;
    } else if ($code >= 78 && $code <= 89) {
        $offset = 77 - $code;
    } else if ($upper === "Z") {
        return "UTC";
    }
    if ($offset !== 0) {
        $sign = ($offset > 0) ? "-" : "+";
        return "UTC" . $sign . (string)abs($offset);
    }
}
$length = strlen($zone);
if ($length >= 2 && $length <= 6) {
    $alpha = true;
    for ($i = 0; $i < $length; $i++) {
        $code = ord($zone[$i]);
        if (!(($code >= 65 && $code <= 90) || ($code >= 97 && $code <= 122))) {
            $alpha = false;
        }
    }
    if ($alpha) {
        $abbrZones = [__TZ_ABBR_RUNTIME_MAP__];
        $key = strtolower($zone);
        if (isset($abbrZones[$key])) {
            return "" . $abbrZones[$key];
        }
    }
}
return "" . $zone;
"#;

/// Builds PHP abbreviation names mapped to their fixed type-2 POSIX runtime offsets.
pub(super) fn timezone_runtime_abbreviation_map() -> String {
    include_str!("../../../../../crates/elephc-tz/data/abbreviations.data")
        .lines()
        .filter_map(|line| {
            let (abbr, rows) = line.split_once('\t')?;
            let first = rows.split(';').next()?;
            let mut fields = first.splitn(3, ':');
            fields.next()?;
            let total_offset = fields.next()?.parse::<i64>().ok()?;
            let runtime = if total_offset == 0 {
                "UTC".to_string()
            } else {
                let sign = if total_offset > 0 { '-' } else { '+' };
                let absolute = total_offset.unsigned_abs();
                let hours = absolute / 3_600;
                let minutes = (absolute % 3_600) / 60;
                let seconds = absolute % 60;
                if seconds != 0 {
                    format!("UTC{sign}{hours}:{minutes:02}:{seconds:02}")
                } else if minutes != 0 {
                    format!("UTC{sign}{hours}:{minutes:02}")
                } else {
                    format!("UTC{sign}{hours}")
                }
            };
            Some(format!("\"{abbr}\"=>\"{runtime}\""))
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Builds `DateTime::__elephc_runtime_timezone_name(string $zone): string`, the internal adapter
/// used immediately before calls to libc-backed `date_default_timezone_set()`.
pub(super) fn datetime_runtime_timezone_name() -> ClassMethod {
    let source = RUNTIME_TIMEZONE_NAME_SRC.replace(
        "__TZ_ABBR_RUNTIME_MAP__",
        &timezone_runtime_abbreviation_map(),
    );
    let tokens = crate::lexer::tokenize(&source)
        .expect("runtime timezone-name helper must tokenize");
    let body = crate::parser::parse(&tokens).expect("runtime timezone-name helper must parse");
    ClassMethod {
        name: "__elephc_runtime_timezone_name".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![("zone".to_string(), Some(TypeExpr::Str), None, false)],
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

/// Synthetic-PHP body of `timezone_name_from_abbr($abbr, $utcOffset, $isDST)`. Reproduces
/// timelib's `abbr_search`: search PHP's complete abbreviation table case-insensitively, return
/// the first row when no offset is supplied, prefer the first exact offset match otherwise, and
/// fall back to timelib's offset/DST map only when the abbreviation itself is unknown.
pub(super) const TZ_NAME_FROM_ABBR_SRC: &str = r#"<?php
$key = strtolower($abbr);
if ($key === "utc" || $key === "gmt") {
    return "UTC";
}
$lines = explode("\n", elephc_tz_abbreviations());
foreach ($lines as $line) {
    $parts = explode("\t", $line);
    if ($parts[0] === $key) {
        $rows = explode(";", $parts[1]);
        $first = "";
        $firstIsNull = false;
        $haveFirst = false;
        foreach ($rows as $row) {
            $columns = explode(":", $row);
            $zone = $columns[2];
            if (!$haveFirst) {
                $first = "" . $zone;
                $firstIsNull = $zone === "NULL";
                $haveFirst = true;
                if ($utcOffset == -1) {
                    return $firstIsNull ? false : $first;
                }
            }
            if (intval($columns[1]) == $utcOffset) {
                return $zone === "NULL" ? false : ("" . $zone);
            }
        }
        return $firstIsNull ? false : $first;
    }
}
$fallback = [
    "-39600:0" => "Pacific/Apia",
    "-36000:0" => "Pacific/Honolulu",
    "-32400:0" => "America/Anchorage",
    "-28800:1" => "America/Anchorage",
    "-28800:0" => "America/Los_Angeles",
    "-25200:1" => "America/Los_Angeles",
    "-25200:0" => "America/Denver",
    "-21600:1" => "America/Denver",
    "-21600:0" => "America/Chicago",
    "-18000:1" => "America/Chicago",
    "-18000:0" => "America/New_York",
    "-16200:0" => "America/Caracas",
    "-14400:1" => "America/New_York",
    "-14400:0" => "America/Halifax",
    "-10800:1" => "America/Halifax",
    "-10800:0" => "America/Sao_Paulo",
    "-7200:1" => "America/Sao_Paulo",
    "-3600:0" => "Atlantic/Azores",
    "0:1" => "Atlantic/Azores",
    "0:0" => "Europe/London",
    "3600:1" => "Europe/London",
    "3600:0" => "Europe/Paris",
    "7200:1" => "Europe/Paris",
    "7200:0" => "Europe/Helsinki",
    "10800:1" => "Europe/Helsinki",
    "10800:0" => "Europe/Moscow",
    "14400:1" => "Europe/Moscow",
    "14400:0" => "Asia/Dubai",
    "18000:0" => "Asia/Karachi",
    "19800:0" => "Asia/Kolkata",
    "20700:0" => "Asia/Katmandu",
    "21600:1" => "Asia/Yekaterinburg",
    "25200:1" => "Asia/Novosibirsk",
    "25200:0" => "Asia/Krasnoyarsk",
    "28800:0" => "Asia/Shanghai",
    "28800:1" => "Asia/Krasnoyarsk",
    "32400:0" => "Asia/Tokyo",
    "36000:0" => "Australia/Melbourne",
    "37800:1" => "Australia/Adelaide",
    "39600:1" => "Australia/Melbourne",
    "43200:0" => "Pacific/Auckland",
    "46800:1" => "Pacific/Auckland",
];
$fallbackKey = $utcOffset . ":" . $isDST;
return isset($fallback[$fallbackKey]) ? $fallback[$fallbackKey] : false;
"#;

/// Builds the internal static `__elephc_timezone_name_from_abbr(...)` method on `DateTime` backing
/// the `timezone_name_from_abbr()` procedural function. See `TZ_NAME_FROM_ABBR_SRC`.
pub(super) fn datetime_tz_name_from_abbr() -> ClassMethod {
    let tokens =
        crate::lexer::tokenize(TZ_NAME_FROM_ABBR_SRC).expect("tz_name_from_abbr must tokenize");
    let body = crate::parser::parse(&tokens).expect("tz_name_from_abbr must parse");
    ClassMethod {
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

/// Synthetic-PHP body of `strptime($timestamp, $format)`, the inverse of `strftime()`. Walks the
/// C `strftime` `%`-specifiers in `$format` against `$timestamp`, filling a `struct tm` array.
/// Supports `%Y %y %m %d %e %H %M %S %j %B %b %h %A %a %p %P`, the week specifiers `%u %w %U %W %V`
/// (consumed but not used to build the instant — `tm_wday`/`tm_yday` are derived from the date),
/// the timezone specifiers `%z` (offset) and `%Z` (name) (consumed only), the whitespace metas
/// `%n`/`%t`, `%%`, flexible spaces, and literal characters. Returns PHP's nine-key array
/// (`tm_sec`/`tm_min`/`tm_hour`/`tm_mday`/`tm_mon` (0-based)/`tm_year` (since 1900)/`tm_wday`/
/// `tm_yday`/`unparsed`) or `false` on mismatch. Unparsed date fields stay 0 and `tm_wday`/`tm_yday`
/// are computed (via `gmmktime`/`gmdate`) only when a full year+month+day was parsed, matching glibc.
pub(super) const STRPTIME_SRC: &str = r#"<?php
__elephc_diag_warning("Deprecated: Function strptime() is deprecated since 8.2, use date_parse_from_format() (for locale-independent parsing), or IntlDateFormatter::parse() (for locale-dependent parsing) instead\n", 0, E_DEPRECATED);
$slen = strlen($timestamp);
$flen = strlen($format);
$sec = 0; $min = 0; $hour = 0; $mday = 0; $mon = 0; $year = 0;
$gotY = false; $gotMon = false; $gotMday = false;
$sp = 0; $fp = 0; $ok = true;
while ($fp < $flen) {
    $fc = $format[$fp];
    if ($fc === "%") {
        $fp = $fp + 1;
        if ($fp >= $flen) { $ok = false; break; }
        $spec = $format[$fp];
        $fp = $fp + 1;
        if ($spec === "%") {
            if ($sp >= $slen || $timestamp[$sp] !== "%") { $ok = false; break; }
            $sp = $sp + 1;
        } else if ($spec === "n" || $spec === "t") {
            while ($sp < $slen && ($timestamp[$sp] === " " || $timestamp[$sp] === "\t" || $timestamp[$sp] === "\n")) { $sp = $sp + 1; }
        } else if ($spec === "Y" || $spec === "y" || $spec === "m" || $spec === "d" || $spec === "e" || $spec === "H" || $spec === "M" || $spec === "S" || $spec === "j") {
            if ($spec === "e") { while ($sp < $slen && $timestamp[$sp] === " ") { $sp = $sp + 1; } }
            $num = 0; $cnt = 0;
            $maxd = ($spec === "Y") ? 4 : (($spec === "j") ? 3 : 2);
            while ($cnt < $maxd && $sp < $slen && ctype_digit($timestamp[$sp])) {
                $num = $num * 10 + (ord($timestamp[$sp]) - 48);
                $sp = $sp + 1; $cnt = $cnt + 1;
            }
            if ($cnt === 0) { $ok = false; break; }
            if ($spec === "Y") { $year = $num; $gotY = true; }
            else if ($spec === "y") { $year = ($num < 69) ? (2000 + $num) : (1900 + $num); $gotY = true; }
            else if ($spec === "m") { $mon = $num; $gotMon = true; }
            else if ($spec === "d" || $spec === "e") { $mday = $num; $gotMday = true; }
            else if ($spec === "H") { $hour = $num; }
            else if ($spec === "M") { $min = $num; }
            else if ($spec === "S") { $sec = $num; }
        } else if ($spec === "B" || $spec === "b" || $spec === "h") {
            $sub = "";
            while ($sp < $slen) {
                $io = ord($timestamp[$sp]);
                $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
                if (!$a) { break; }
                $sub = $sub . $timestamp[$sp];
                $sp = $sp + 1;
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
            if ($mv === 0) { $ok = false; break; }
            $mon = $mv; $gotMon = true;
        } else if ($spec === "A" || $spec === "a") {
            while ($sp < $slen) {
                $io = ord($timestamp[$sp]);
                $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
                if (!$a) { break; }
                $sp = $sp + 1;
            }
        } else if ($spec === "p" || $spec === "P") {
            $two = strtoupper(substr($timestamp, $sp, 2));
            if ($two === "PM") { if ($hour < 12) { $hour = $hour + 12; } $sp = $sp + 2; }
            else if ($two === "AM") { if ($hour === 12) { $hour = 0; } $sp = $sp + 2; }
            else { $ok = false; break; }
        } else if ($spec === "u" || $spec === "w" || $spec === "U" || $spec === "W" || $spec === "V") {
            $num = 0; $cnt = 0;
            $maxd = ($spec === "u" || $spec === "w") ? 1 : 2;
            while ($cnt < $maxd && $sp < $slen && ctype_digit($timestamp[$sp])) {
                $num = $num * 10 + (ord($timestamp[$sp]) - 48);
                $sp = $sp + 1; $cnt = $cnt + 1;
            }
            if ($cnt === 0) { $ok = false; break; }
        } else if ($spec === "z" || $spec === "Z") {
            if ($spec === "z") {
                if ($sp < $slen && ($timestamp[$sp] === "+" || $timestamp[$sp] === "-")) { $sp = $sp + 1; }
                $cnt = 0;
                while ($cnt < 4 && $sp < $slen && (ctype_digit($timestamp[$sp]) || $timestamp[$sp] === ":")) {
                    $sp = $sp + 1; $cnt = $cnt + 1;
                }
            } else {
                while ($sp < $slen) {
                    $io = ord($timestamp[$sp]);
                    $a = ($io >= 65 && $io <= 90) || ($io >= 97 && $io <= 122);
                    if (!$a) { break; }
                    $sp = $sp + 1;
                }
            }
        } else {
            $ok = false; break;
        }
    } else if ($fc === " ") {
        while ($sp < $slen && $timestamp[$sp] === " ") { $sp = $sp + 1; }
        $fp = $fp + 1;
    } else {
        if ($sp >= $slen || $timestamp[$sp] !== $fc) { $ok = false; break; }
        $sp = $sp + 1; $fp = $fp + 1;
    }
}
if (!$ok) { return false; }
$wday = 0; $yday = 0; $tmMon = 0; $tmYear = 0;
if ($gotMon) { $tmMon = $mon - 1; }
if ($gotY) { $tmYear = $year - 1900; }
if ($gotY && $gotMon && $gotMday) {
    $ts = __elephc_gmmktime_raw($hour, $min, $sec, $mon, $mday, $year);
    $wday = intval(gmdate("w", $ts));
    $yday = intval(gmdate("z", $ts));
}
return [
    "tm_sec" => $sec,
    "tm_min" => $min,
    "tm_hour" => $hour,
    "tm_mday" => $mday,
    "tm_mon" => $tmMon,
    "tm_year" => $tmYear,
    "tm_wday" => $wday,
    "tm_yday" => $yday,
    "unparsed" => substr($timestamp, $sp),
];
"#;

/// Builds the internal static `__elephc_strptime($timestamp, $format)` method on `DateTime` backing
/// the `strptime()` procedural function (the name resolver desugars the call to it). See
/// `STRPTIME_SRC` for the supported specifiers and return shape.
pub(super) fn datetime_strptime() -> ClassMethod {
    let tokens = crate::lexer::tokenize(STRPTIME_SRC).expect("strptime body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("strptime body source must parse");
    ClassMethod {
        name: "__elephc_strptime".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("timestamp".to_string(), Some(TypeExpr::Str), None, false),
            ("format".to_string(), Some(TypeExpr::Str), None, false),
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

/// Builds the internal static `__elephc_sun_rs(...)` core shared by `date_sun_info()`,
/// `date_sunrise()`, and `date_sunset()`. See `SUN_RS_SRC` for the algorithm and return shape.
pub(super) fn datetime_sun_rs() -> ClassMethod {
    let tokens = crate::lexer::tokenize(SUN_RS_SRC).expect("sun_rs body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("sun_rs body source must parse");
    ClassMethod {
        name: "__elephc_sun_rs".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("t_utc_sse".to_string(), Some(TypeExpr::Int), None, false),
            ("lon".to_string(), Some(TypeExpr::Float), None, false),
            ("lat".to_string(), Some(TypeExpr::Float), None, false),
            ("altit".to_string(), Some(TypeExpr::Float), None, false),
            ("limb".to_string(), Some(TypeExpr::Int), None, false),
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

/// Builds the internal static `__elephc_sun_val($rc, $tsval)` selector shared by `date_sun_info()`.
/// Returns `bool` for the polar all-day/all-night edge cases and the precomputed `int` timestamp
/// otherwise; the `mixed` return type preserves each branch's runtime tag. See `SUN_VAL_SRC`.
pub(super) fn datetime_sun_val() -> ClassMethod {
    let tokens = crate::lexer::tokenize(SUN_VAL_SRC).expect("sun_val body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("sun_val body source must parse");
    ClassMethod {
        name: "__elephc_sun_val".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("rc".to_string(), Some(TypeExpr::Int), None, false),
            ("tsval".to_string(), Some(TypeExpr::Int), None, false),
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

/// Builds the internal static `__elephc_date_sun_info($timestamp, $latitude, $longitude)` method on
/// `DateTime` backing the `date_sun_info()` procedural function. See `SUN_INFO_SRC`.
pub(super) fn datetime_sun_info() -> ClassMethod {
    let tokens = crate::lexer::tokenize(SUN_INFO_SRC).expect("sun_info body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("sun_info body source must parse");
    ClassMethod {
        name: "__elephc_date_sun_info".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            ("latitude".to_string(), Some(TypeExpr::Float), None, false),
            ("longitude".to_string(), Some(TypeExpr::Float), None, false),
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

/// Builds the internal static `__elephc_date_sunfunc(...)` method on `DateTime` backing both
/// `date_sunrise()` (`$which == 0`) and `date_sunset()` (`$which == 1`). See `SUNFUNC_SRC`. The
/// optional latitude/longitude/zenith/offset parameters default to null so the body can substitute
/// PHP's ini coordinates and current timezone offset; `$returnFormat` defaults to string (1).
pub(super) fn datetime_sunfunc() -> ClassMethod {
    let tokens = crate::lexer::tokenize(SUNFUNC_SRC).expect("sunfunc body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("sunfunc body source must parse");
    ClassMethod {
        name: "__elephc_date_sunfunc".to_string(),
        visibility: Visibility::Public,
        is_static: true,
        is_abstract: false,
        is_final: false,
        has_body: true,
        params: vec![
            ("which".to_string(), Some(TypeExpr::Int), None, false),
            (
                "line".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(0), dummy())),
                false,
            ),
            ("timestamp".to_string(), Some(TypeExpr::Int), None, false),
            (
                "returnFormat".to_string(),
                Some(TypeExpr::Int),
                Some(Expr::new(ExprKind::IntLiteral(1), dummy())),
                false,
            ),
            (
                "latitude".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Float))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
            (
                "longitude".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Float))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
            (
                "zenith".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Float))),
                Some(Expr::new(ExprKind::Null, dummy())),
                false,
            ),
            (
                "utcOffset".to_string(),
                Some(TypeExpr::Nullable(Box::new(TypeExpr::Float))),
                Some(Expr::new(ExprKind::Null, dummy())),
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

/// Builds the internal static `__elephc_date_parse(string $datetime)` method on `DateTime` backing
/// the `date_parse()` procedural function (the name resolver desugars the call to it). Returns the
/// same component array as `date_parse_from_format`. Self-contained parsed-source body.
pub(super) fn datetime_date_parse(uses_timelib: bool) -> ClassMethod {
    let source = if uses_timelib {
        TIMELIB_DATE_PARSE_SRC
    } else {
        DATE_PARSE_SRC
    };
    let tokens = crate::lexer::tokenize(source).expect("date_parse body source must tokenize");
    let body = crate::parser::parse(&tokens).expect("date_parse body source must parse");
    ClassMethod {
        name: "__elephc_date_parse".to_string(),
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
        return_type: Some(TypeExpr::Named(Name::unqualified("mixed"))),
        by_ref_return: false,
        body,
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the `timestamp` (int) and `timezone_name` (str, default "UTC") backing properties.
pub(super) fn datetime_backing_properties() -> Vec<ClassProperty> {
    let mut properties = vec![
        date_object_initialized_property(),
        private_property(
            "timestamp",
            TypeExpr::Int,
            Expr::new(ExprKind::IntLiteral(0), dummy()),
        ),
        private_property(
            "timezone_name",
            TypeExpr::Str,
            Expr::new(ExprKind::StringLiteral("UTC".to_string()), dummy()),
        ),
        // Sub-second component (0..999999) preserved across operations; surfaced by getMicrosecond()
        // and the `u`/`v` format specifiers. elephc otherwise works at libc second resolution.
        private_property(
            "microsecond",
            TypeExpr::Int,
            Expr::new(ExprKind::IntLiteral(0), dummy()),
        ),
        private_property(
            "__elephc_civil_override",
            TypeExpr::Bool,
            Expr::new(ExprKind::BoolLiteral(false), dummy()),
        ),
        private_property(
            "__elephc_civil_year",
            TypeExpr::Int,
            Expr::new(ExprKind::IntLiteral(1970), dummy()),
        ),
        private_property(
            "__elephc_civil_month",
            TypeExpr::Int,
            Expr::new(ExprKind::IntLiteral(1), dummy()),
        ),
        private_property(
            "__elephc_civil_day",
            TypeExpr::Int,
            Expr::new(ExprKind::IntLiteral(1), dummy()),
        ),
        // Shared parser state is stored on DateTime. The same backing layout remains on
        // DateTimeImmutable so both synthetic declarations stay structurally compatible.
        {
            let mut p =
                property("lastErrorCount", TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy()));
            p.is_static = true;
            p
        },
        // Scalar parser state avoids retaining refcounted arrays across synthetic static-property
        // assignments; getLastErrors() reconstructs PHP's public arrays on demand.
        {
            let mut p = property(
                "lastErrorPosition",
                TypeExpr::Int,
                Expr::new(ExprKind::IntLiteral(0), dummy()),
            );
            p.is_static = true;
            p
        },
        {
            let mut p = property(
                "lastErrorMessage",
                TypeExpr::Str,
                Expr::new(ExprKind::StringLiteral(String::new()), dummy()),
            );
            p.is_static = true;
            p
        },
        {
            let mut p =
                property("lastWarningCount", TypeExpr::Int, Expr::new(ExprKind::IntLiteral(0), dummy()));
            p.is_static = true;
            p
        },
        {
            let mut p = property(
                "lastWarningPosition",
                TypeExpr::Int,
                Expr::new(ExprKind::IntLiteral(0), dummy()),
            );
            p.is_static = true;
            p
        },
        {
            let mut p = property(
                "lastWarningMessage",
                TypeExpr::Str,
                Expr::new(ExprKind::StringLiteral(String::new()), dummy()),
            );
            p.is_static = true;
            p
        },
        {
            let mut p = property(
                "lastParseResult",
                TypeExpr::Named(Name::unqualified("mixed")),
                Expr::new(ExprKind::StringLiteral(String::new()), dummy()),
            );
            p.is_static = true;
            p
        },
    ];
    properties.extend(date_constructor_unpack_properties());
    properties
}

/// Builds an abstract (bodyless) interface method declaration.
pub(super) fn abstract_method(
    name: &str,
    params: Vec<(String, Option<TypeExpr>, Option<Expr>, bool)>,
    return_type: Option<TypeExpr>,
) -> ClassMethod {
    ClassMethod {
        name: name.to_string(),
        visibility: Visibility::Public,
        is_static: false,
        is_abstract: true,
        is_final: false,
        has_body: false,
        params,
        param_attributes: Vec::new(),
        variadic: None,
        variadic_by_ref: false,
        variadic_type: None,
        return_type,
        by_ref_return: false,
        body: Vec::new(),
        span: dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the complete PHP 8.5 `DateTimeInterface` method contract.
pub(super) fn datetime_interface_methods() -> Vec<ClassMethod> {
    vec![
        abstract_method(
            "format",
            vec![("format".to_string(), Some(TypeExpr::Str), None, false)],
            Some(TypeExpr::Str),
        ),
        abstract_method("getTimestamp", Vec::new(), Some(TypeExpr::Int)),
        // PHP 8.4 promoted getMicrosecond() onto the interface; both concrete
        // classes implement it, and diff() reads it through the interface.
        abstract_method("getMicrosecond", Vec::new(), Some(TypeExpr::Int)),
        abstract_method(
            "getTimezone",
            Vec::new(),
            Some(TypeExpr::Union(vec![
                TypeExpr::Named(Name::unqualified("DateTimeZone")),
                TypeExpr::False,
            ])),
        ),
        abstract_method("getOffset", Vec::new(), Some(TypeExpr::Int)),
        abstract_method(
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
        ),
        {
            let mut wakeup = abstract_method("__wakeup", Vec::new(), Some(TypeExpr::Void));
            wakeup.attributes = deprecated_attribute(
                "8.5",
                "this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()",
            );
            wakeup
        },
        abstract_method(
            "__serialize",
            Vec::new(),
            Some(TypeExpr::Named(Name::unqualified("array"))),
        ),
        abstract_method(
            "__unserialize",
            vec![(
                "data".to_string(),
                Some(TypeExpr::Named(Name::unqualified("array"))),
                None,
                false,
            )],
            Some(TypeExpr::Void),
        ),
        // Compiler-only hook used by DatePeriod's recursive debug renderer.
        // Reflection and source-level lookup filter `__elephc_*` methods.
        abstract_method(
            "__elephc_debug_dump",
            Vec::new(),
            Some(TypeExpr::Void),
        ),
        abstract_method(
            "__elephc_print_r_dump",
            Vec::new(),
            Some(TypeExpr::Void),
        ),
        abstract_method(
            "__elephc_assert_comparable",
            Vec::new(),
            Some(TypeExpr::Void),
        ),
    ]
}

/// `DateTime`/`DateTimeImmutable::getOffset(): int` — UTC offset (seconds) of the object's own zone
/// at its stored instant, daylight-saving aware.
///
/// Like `DateTimeZone::getOffset` but reads `$this->timezone_name`/`$this->timestamp`: temporarily
/// applies the object's zone, reads the `date()` `Z` specifier, then restores the previous default.
pub(super) fn datetime_get_offset() -> ClassMethod {
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
    let z_spec = Expr::new(ExprKind::StringLiteral("Z".to_string()), dummy());
    method(
        "getOffset",
        Vec::new(),
        Some(TypeExpr::Int),
        vec![
            // $__saved = date_default_timezone_get();
            Stmt::assign("__saved", call("date_default_timezone_get", Vec::new())),
            // date_default_timezone_set($this->timezone_name);
            expr_stmt(call(
                "date_default_timezone_set",
                vec![runtime_zone(this_property("timezone_name"))],
            )),
            // $__off = intval(date("Z", $this->timestamp));
            Stmt::assign(
                "__off",
                call("intval", vec![call("date", vec![z_spec, this_property("timestamp")])]),
            ),
            // date_default_timezone_set($__saved);  (restore the previous default)
            expr_stmt(call("date_default_timezone_set", vec![var("__saved")])),
            return_expr(var("__off")),
        ],
    )
}
