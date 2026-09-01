//! Purpose:
//! DatePeriod method aggregation, serialization, virtual properties, and class-map injection.
//!
//! Called from:
//! - The DatePeriod checker metadata facade and sibling compliance module.
//!
//! Key details:
//! - Preserves the audited php-src DatePeriod semantics in the split checker layout.

#[allow(unused_imports)]
use super::{
    BinOp, CastType, ClassConst, ClassMethod, ClassProperty, Expr, ExprKind, FlattenedClass,
    HashMap, Name, PropertyHooks, Stmt, StmtKind, TypeExpr, Visibility,
};
use super::compliance_core::*;
/// Builds the full `DatePeriod` method list.
pub(super) fn date_period_methods(uses_timelib: bool) -> Vec<ClassMethod> {
    let mut methods = vec![
        date_period_constructor(),
        date_period_initialize_end_components(),
        date_period_initialize_recurrence_components(),
        date_period_weak_string_argument(),
        date_period_clone_datetime_interface(),
        date_period_clone_datetime_interface_storage(),
        date_period_clone_iterator_value(),
        date_period_datetime_interface_timestamp(),
        date_period_add_interval(),
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
        date_period_initialize_from_iso8601_string(uses_timelib),
        date_period_initialize_from_argument_array(),
        date_period_begin_argument_array(),
        date_period_append_one_argument(),
        date_period_append_argument_chunk(),
        date_period_finish_argument_array(),
        date_period_factory_result(),
        date_period_weak_options(),
        date_period_debug_dump(),
        date_period_assert_initialized(),
        date_period_assert_iterable_initialized(),
        date_period_assert_foreach_by_reference(),
    ];
    methods.extend(date_period_property_getters());
    methods.extend(date_period_serialize_methods());
    guard_date_period_payload_methods(&mut methods);
    methods
}

/// PHP source backing `DatePeriod::__serialize()`. Returns the period's state as an array with
/// `start`, `current`, `end`, `interval`, `recurrences`, `include_start_date`, `include_end_date`.
pub(super) const DATEPERIOD_SERIALIZE_SRC: &str = r#"<?php
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

/// PHP source backing `DatePeriod::__set_state()`. Clears a valid allocation into the same empty
/// internal shell php-src creates before sequential restoration.
pub(super) const DATEPERIOD_SET_STATE_SRC: &str = r#"<?php
$result = new DatePeriod(new DateTime("@0"), new DateInterval("P1D"), 1);
$result->_start = null;
$result->_current = null;
$result->_cursor = null;
$result->_end = null;
$result->_interval = null;
$result->_recurrences = 0;
$result->_include_start_date = false;
$result->_include_end_date = false;
$result->startTs = 0;
$result->endTs = 0;
$result->startIsImmutable = false;
$result->excludeStart = 0;
$result->includeEnd = 0;
$result->curTs = 0;
$result->idx = 0;
$result->useCount = 0;
$result->_recurrence_count = 0;
$result->__elephc_initialized = false;
$result->__unserialize($array);
return $result;
"#;

/// Builds `DatePeriod::__wakeup(): void` (no-op, reusing the datetime wakeup builder).
pub(super) fn date_period_wakeup() -> ClassMethod {
    let tokens = crate::lexer::tokenize(r#"<?php
__elephc_diag_warning("Deprecated: Method DatePeriod::__wakeup() is deprecated since 8.5, this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()\n", 0, E_DEPRECATED);
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
        attributes: super::super::datetime::deprecated_attribute(
            "8.5",
            "this method is obsolete, as serialization hooks are provided by __unserialize() and __serialize()",
        ),
    }
}

/// Builds `DatePeriod::__serialize(): array`.
pub(super) fn date_period_serialize() -> ClassMethod {
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

/// Builds `DatePeriod::__unserialize(array $data): void`. Restores fields in php-src order
/// without rollback, so mutations completed before a later invalid field remain observable.
pub(super) fn date_period_unserialize() -> ClassMethod {
    let src = r#"<?php
if (!array_key_exists("start", $data)) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$serializedStart = $data["start"];
if ($serializedStart !== null) {
    if ($serializedStart instanceof DateTimeImmutable) {
        if (!$serializedStart->__elephc_is_initialized()) {
            throw new Error("Invalid serialization data for DatePeriod object");
        }
        $startSnapshot = $serializedStart->__elephc_clone_for_period_storage();
    } else if ($serializedStart instanceof DateTime) {
        if (!$serializedStart->__elephc_is_initialized()) {
            throw new Error("Invalid serialization data for DatePeriod object");
        }
        $startSnapshot = $serializedStart->__elephc_clone_for_period_storage();
    } else {
        throw new Error("Invalid serialization data for DatePeriod object");
    }
    $this->_start = $startSnapshot;
    $this->startTs = $this->__elephc_datetime_interface_timestamp($startSnapshot);
    $this->startIsImmutable = $startSnapshot instanceof DateTimeImmutable;
    $this->curTs = $this->startTs;
    $this->idx = 0;
}

if (!array_key_exists("end", $data)) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$serializedEnd = $data["end"];
if ($serializedEnd !== null) {
    if ($this->_start === null) {
        throw new Error("Invalid serialization data for DatePeriod object");
    }
    if ($serializedEnd instanceof DateTimeImmutable) {
        if (!$serializedEnd->__elephc_is_initialized()) {
            throw new Error("Invalid serialization data for DatePeriod object");
        }
        $endSnapshot = $serializedEnd->__elephc_clone_for_period_storage();
    } else if ($serializedEnd instanceof DateTime) {
        if (!$serializedEnd->__elephc_is_initialized()) {
            throw new Error("Invalid serialization data for DatePeriod object");
        }
        $endSnapshot = $serializedEnd->__elephc_clone_for_period_storage();
    } else {
        throw new Error("Invalid serialization data for DatePeriod object");
    }
    $this->_end = $endSnapshot;
    $this->endTs = $this->__elephc_datetime_interface_timestamp($endSnapshot);
}

if (!array_key_exists("current", $data)) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$serializedCurrent = $data["current"];
if ($serializedCurrent !== null) {
    if ($this->_start === null) {
        throw new Error("Invalid serialization data for DatePeriod object");
    }
    if ($serializedCurrent instanceof DateTimeImmutable) {
        if (!$serializedCurrent->__elephc_is_initialized()) {
            throw new Error("Invalid serialization data for DatePeriod object");
        }
        $currentSnapshot = $serializedCurrent->__elephc_clone_for_period_storage();
    } else if ($serializedCurrent instanceof DateTime) {
        if (!$serializedCurrent->__elephc_is_initialized()) {
            throw new Error("Invalid serialization data for DatePeriod object");
        }
        $currentSnapshot = $serializedCurrent->__elephc_clone_for_period_storage();
    } else {
        throw new Error("Invalid serialization data for DatePeriod object");
    }
    $this->_current = $currentSnapshot;
}

if (!array_key_exists("interval", $data)) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$serializedInterval = $data["interval"];
if (!($serializedInterval instanceof DateInterval)
    || get_class($serializedInterval) !== "DateInterval"
    || !$serializedInterval->__elephc_is_initialized()) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$intervalSnapshot = $serializedInterval->__elephc_clone_storage();
$this->_interval = $intervalSnapshot;
$this->iv_y = $intervalSnapshot->y;
$this->iv_m = $intervalSnapshot->m;
$this->iv_d = $intervalSnapshot->d;
$this->iv_h = $intervalSnapshot->h;
$this->iv_i = $intervalSnapshot->i;
$this->iv_s = $intervalSnapshot->s;
$this->iv_invert = $intervalSnapshot->invert;

if (!array_key_exists("recurrences", $data)
    || !is_int($data["recurrences"])
    || $data["recurrences"] < 0
    || $data["recurrences"] > 2147483647) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$this->_recurrences = $data["recurrences"];
$this->_recurrence_count = $this->_recurrences
    - ($this->_include_start_date ? 1 : 0)
    - ($this->_include_end_date ? 1 : 0);

if (!array_key_exists("include_start_date", $data)
    || !is_bool($data["include_start_date"])) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$this->_include_start_date = $data["include_start_date"];
$this->excludeStart = $this->_include_start_date ? 0 : 1;
$this->_recurrence_count = $this->_recurrences
    - ($this->_include_start_date ? 1 : 0)
    - ($this->_include_end_date ? 1 : 0);

if (!array_key_exists("include_end_date", $data)
    || !is_bool($data["include_end_date"])) {
    throw new Error("Invalid serialization data for DatePeriod object");
}
$this->_include_end_date = $data["include_end_date"];
$this->includeEnd = $this->_include_end_date ? 2 : 0;
$this->_recurrence_count = $this->_recurrences
    - ($this->_include_start_date ? 1 : 0)
    - ($this->_include_end_date ? 1 : 0);
$this->useCount = $this->_end === null ? 1 : 0;
$this->_cursor = null;
$this->__elephc_initialized = true;
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
pub(super) fn date_period_set_state() -> ClassMethod {
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
pub(super) fn date_period_serialize_methods() -> Vec<ClassMethod> {
    vec![
        date_period_wakeup(),
        date_period_serialize(),
        date_period_unserialize(),
        date_period_set_state(),
    ]
}

/// Builds a public object property defaulting to `null` (for the `DateTimeInterface`/`DateInterval`
/// mirror properties exposed by PHP's `DatePeriod`).
pub(super) fn nullable_object_property(name: &str, class_name: &str, visibility: Visibility) -> ClassProperty {
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
pub(super) fn virtual_property(name: &str, type_expr: TypeExpr) -> ClassProperty {
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

/// Builds one scalar synthetic getter for a public `DatePeriod` virtual property.
pub(super) fn virtual_scalar_property_getter(
    name: &str,
    backing: &str,
    return_type: TypeExpr,
) -> ClassMethod {
    method(
        &crate::names::property_hook_get_method(name),
        Vec::new(),
        Some(return_type),
        vec![ret(this_prop(backing))],
    )
}

/// Builds a datetime virtual getter that returns a fresh clone on every property read.
pub(super) fn virtual_datetime_property_getter(name: &str, backing: &str) -> ClassMethod {
    let source = format!(
        r#"<?php
$value = $this->{backing};
if ($value === null) {{ return null; }}
return $this->__elephc_clone_datetime_interface($value);
"#
    );
    let tokens = crate::lexer::tokenize(&source)
        .expect("DatePeriod datetime virtual getter must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod datetime virtual getter must parse");
    method(
        &crate::names::property_hook_get_method(name),
        Vec::new(),
        Some(TypeExpr::Nullable(Box::new(
            date_period_datetime_implementation_type(),
        ))),
        body,
    )
}

/// Builds the interval virtual getter that clones php-src's materialized value.
pub(super) fn virtual_interval_property_getter() -> ClassMethod {
    let tokens = crate::lexer::tokenize(
        r#"<?php
$value = $this->_interval;
if ($value === null) { return null; }
return $value->__elephc_clone();
"#,
    )
    .expect("DatePeriod interval virtual getter must tokenize");
    let body = crate::parser::parse(&tokens)
        .expect("DatePeriod interval virtual getter must parse");
    method(
        &crate::names::property_hook_get_method("interval"),
        Vec::new(),
        Some(TypeExpr::Nullable(Box::new(TypeExpr::Named(Name::unqualified(
            "DateInterval",
        ))))),
        body,
    )
}

/// Builds the seven php-src virtual property getters.
pub(super) fn date_period_property_getters() -> Vec<ClassMethod> {
    vec![
        virtual_datetime_property_getter("start", "_start"),
        virtual_datetime_property_getter("current", "_current"),
        virtual_datetime_property_getter("end", "_end"),
        virtual_interval_property_getter(),
        virtual_scalar_property_getter("recurrences", "_recurrences", TypeExpr::Int),
        virtual_scalar_property_getter(
            "include_start_date",
            "_include_start_date",
            TypeExpr::Bool,
        ),
        virtual_scalar_property_getter(
            "include_end_date",
            "_include_end_date",
            TypeExpr::Bool,
        ),
    ]
}

/// Builds the `DatePeriod` integer state properties.
pub(super) fn date_period_properties() -> Vec<ClassProperty> {
    let mut props = vec![
        int_property("startTs"),
        int_property("endTs"),
        bool_property("startIsImmutable"),
        date_period_initialized_property(),
    ];
    props.push(mixed_property("__elephc_arguments"));
    props.push(bool_property("__elephc_seen_named_argument"));
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
        "_cursor",
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
