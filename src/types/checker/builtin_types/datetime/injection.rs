//! Purpose:
//! Injects synthetic DateTime declarations and the PHP date exception hierarchy.
//!
//! Called from:
//! - The DateTime checker metadata facade during builtin type initialization.
//!
//! Key details:
//! - Existing user declarations are preserved and timezone introspection remains feature-gated.

use super::*;

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
                properties: vec![
                    interval_property("y"),
                    interval_property("m"),
                    interval_property("d"),
                    interval_property("h"),
                    interval_property("i"),
                    interval_property("s"),
                    // `f` (fraction of a second, 0.0..1.0) exists for API completeness; elephc works
                    // at second resolution so it stays 0.0 (sub-second durations are not parsed).
                    property("f", TypeExpr::Float, Expr::new(ExprKind::FloatLiteral(0.0), dummy())),
                    interval_property("invert"),
                    interval_property("days"),
                ],
                methods: vec![
                    date_interval_constructor(),
                    date_interval_format(),
                    date_interval_create_from_date_string(),
                ],
                attributes: Vec::new(),
                constants: Vec::new(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
            },
        );
    }

    if !class_map.contains_key("DateTimeZone") {
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
                properties: vec![property(
                    "name",
                    TypeExpr::Str,
                    Expr::new(ExprKind::StringLiteral("UTC".to_string()), dummy()),
                )],
                methods: {
                    let mut methods = vec![
                        datetime_zone_constructor(),
                        datetime_zone_get_name(),
                        datetime_zone_get_offset(),
                        datetime_zone_list_identifiers(),
                    ];
                    // getLocation/getTransitions/listAbbreviations call the
                    // tz_prelude marshalling helpers, which are only declared when
                    // the introspection prelude is injected. Adding them
                    // unconditionally would make every DateTimeZone program
                    // reference (and link) the elephc_tz bridge, since method
                    // bodies are type-checked eagerly. So they are gated on the
                    // prelude's presence.
                    if uses_tz_introspection {
                        methods.push(datetime_zone_get_location());
                        methods.push(datetime_zone_get_transitions());
                        methods.push(datetime_zone_list_abbreviations());
                    }
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
                    let mut m = datetime_shared_methods();
                    m.extend(datetime_setter_methods(false, "DateTimeImmutable"));
                    m.push(datetime_create_from_format("DateTimeImmutable"));
                    m.push(datetime_get_last_errors("DateTimeImmutable"));
                    m.push(datetime_create_from_timestamp("DateTimeImmutable"));
                    m.push(datetime_create_from_object("createFromInterface", "DateTimeImmutable"));
                    m.push(datetime_create_from_object("createFromMutable", "DateTimeImmutable"));
                    m.push(datetime_set_isodate("DateTimeImmutable"));
                    m
                },
                attributes: Vec::new(),
                constants: datetime_format_constants(),
                used_traits: Vec::new(),
                trait_aliases: Vec::new(),
            },
        );
    }

    if !class_map.contains_key("DateTime") {
        let mut methods = datetime_shared_methods();
        methods.extend(datetime_setter_methods(true, "DateTime"));
        methods.push(datetime_create_from_format("DateTime"));
        methods.push(datetime_get_last_errors("DateTime"));
        methods.push(datetime_create_from_timestamp("DateTime"));
        methods.push(datetime_create_from_object("createFromInterface", "DateTime"));
        methods.push(datetime_create_from_object("createFromImmutable", "DateTime"));
        methods.push(datetime_set_isodate("DateTime"));
        methods.push(datetime_date_parse_from_format());
        methods.push(datetime_date_parse());
        methods.push(datetime_gettimeofday());
        methods.push(datetime_strftime());
        methods.push(datetime_extract_micros());
        methods.push(datetime_strip_micros());
        methods.push(datetime_extract_modify_micros());
        methods.push(datetime_strip_modify_micros());
        methods.push(datetime_sun_rs());
        methods.push(datetime_sun_val());
        methods.push(datetime_sun_info());
        methods.push(datetime_sunfunc());
        methods.push(datetime_strptime());
        methods.push(datetime_tz_name_from_abbr());
        methods.extend(super::calendar::calendar_methods());
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
                constants: datetime_format_constants(),
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
