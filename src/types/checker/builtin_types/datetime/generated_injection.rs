//! Purpose:
//! Installs generated direct-AST declarations for the DateTime and DatePeriod families.
//!
//! Called from:
//! - The builtin-type checker initialization gates.
//!
//! Key details:
//! - Timelib and fallback declaration variants are selected without parsing PHP source.

use std::collections::HashMap;

use crate::names::{label_fragment, Name};
use crate::parser::ast::{
    ClassMethod, Expr, ExprKind, Program, Stmt, StmtKind, TypeExpr, Visibility,
};
use crate::types::traits::FlattenedClass;

use super::super::declarations::InterfaceDeclInfo;

/// Reserved internal method used to hydrate DateTime-subclass declared properties through AST
/// assignments instead of copying parsed `Mixed` cells directly into object storage.
const DATE_MAGIC_RESTORE_HELPER: &str = "__elephc_restore_date_properties";
/// Reserved internal method that removes serialized references before ext/date hydrates native slots.
const DATE_MAGIC_FILTER_REFERENCES_HELPER: &str = "__elephc_filter_date_references";
/// Prefix for non-PHP private method names invoked only through the date restore table.
const DATE_MAGIC_SLOT_HELPER_PREFIX: &str = "__elephc_date_magic_restore$";

/// Returns whether `name` is one of ext/date's five classes with native magic serialization.
fn is_builtin_datetime_class_name(name: &str) -> bool {
    matches!(
        name.trim_start_matches('\\'),
        "DateTime" | "DateTimeImmutable" | "DateTimeZone" | "DateInterval" | "DatePeriod"
    )
}

/// Builds a synthetic AST expression with the compiler's neutral source span.
fn expr(kind: ExprKind) -> Expr {
    Expr::new(kind, crate::span::Span::dummy())
}

/// Builds a synthetic AST variable expression.
fn var(name: &str) -> Expr {
    expr(ExprKind::Variable(name.to_string()))
}

/// Builds a synthetic AST string literal expression.
fn string(value: impl Into<String>) -> Expr {
    expr(ExprKind::StringLiteral(value.into()))
}

/// Builds the AST key lookup `$data[$key]` used by the date property hydrators.
fn data_item(key: &str) -> Expr {
    expr(ExprKind::ArrayAccess {
        array: Box::new(var("data")),
        index: Box::new(string(key)),
    })
}

/// Builds the private marker hook recognized by EIR lowering inside native date unserializers.
///
/// The body is intentionally inert: lowering replaces this call with the table-driven runtime
/// dispatcher before code generation, so no PHP-visible virtual method can intercept hydration.
fn date_magic_restore_root_method() -> ClassMethod {
    ClassMethod {
        name: DATE_MAGIC_RESTORE_HELPER.to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: true,
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
        return_type: Some(TypeExpr::Named(Name::unqualified("array"))),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(Some(var("data"))), crate::span::Span::dummy())],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Builds the private marker hook that filters references before native date hydration.
fn date_magic_filter_references_root_method() -> ClassMethod {
    ClassMethod {
        name: DATE_MAGIC_FILTER_REFERENCES_HELPER.to_string(),
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: true,
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
        return_type: Some(TypeExpr::Named(Name::unqualified("array"))),
        by_ref_return: false,
        body: vec![Stmt::new(StmtKind::Return(Some(var("data"))), crate::span::Span::dummy())],
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    }
}

/// Returns PHP's serialized-key spelling for one declared user property.
fn date_magic_property_key(class: &FlattenedClass, property: &crate::parser::ast::ClassProperty) -> String {
    match property.visibility {
        Visibility::Public => property.name.clone(),
        Visibility::Protected => format!("\0*\0{}", property.name),
        Visibility::Private => format!("\0{}\0{}", class.name, property.name),
    }
}

/// Builds a collision-free private helper for one declaring class's own date properties.
///
/// A runtime descriptor invokes these helpers in ancestor-to-descendant order. Each helper stays
/// in its declaring class's scope, which preserves private-shadow slots and normal typed AST
/// assignment semantics without exposing an overrideable PHP hook.
fn date_magic_restore_subclass_method(
    class: &FlattenedClass,
    helper_name: String,
    native_public_keys: &[String],
) -> ClassMethod {
    let span = crate::span::Span::dummy();
    let mut body = Vec::new();

    for property in &class.properties {
        // ext/date's common public serialization fields stay owned by the native handler. A
        // protected/private property with the same short name has a mangled key and remains a
        // genuine user slot, so only the public collision is excluded.
        if property.visibility == Visibility::Public
            && native_public_keys.iter().any(|key| key == &property.name)
        {
            continue;
        }
        let key = date_magic_property_key(class, property);
        let has_key = expr(ExprKind::FunctionCall {
            name: Name::unqualified("array_key_exists"),
            args: vec![string(&key), var("data")],
        });
        let assign = Stmt::new(
            StmtKind::PropertyAssign {
                object: Box::new(expr(ExprKind::This)),
                property: property.name.clone(),
                value: data_item(&key),
            },
            span,
        );
        let remove = Stmt::new(
            StmtKind::ExprStmt(expr(ExprKind::FunctionCall {
                name: Name::unqualified("unset"),
                args: vec![data_item(&key)],
            })),
            span,
        );
        body.push(Stmt::new(
            StmtKind::If {
                condition: has_key,
                then_body: vec![assign, remove],
                elseif_clauses: Vec::new(),
                else_body: None,
            },
            span,
        ));
    }
    body.push(Stmt::new(StmtKind::Return(Some(var("data"))), span));

    ClassMethod {
        name: helper_name,
        visibility: Visibility::Private,
        is_static: false,
        is_abstract: false,
        is_final: true,
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
        return_type: Some(TypeExpr::Named(Name::unqualified("array"))),
        by_ref_return: false,
        body,
        span,
        attributes: Vec::new(),
    }
}

/// Returns the public wire keys owned by ext/date for a class's native magic handler.
fn date_magic_native_public_keys(
    class_map: &HashMap<String, FlattenedClass>,
    class: &FlattenedClass,
) -> Vec<String> {
    let mut current = Some(class.name.as_str());
    while let Some(name) = current {
        match name.trim_start_matches('\\') {
            "DateTime" | "DateTimeImmutable" => {
                return vec!["date".to_string(), "timezone_type".to_string(), "timezone".to_string()]
            }
            "DateTimeZone" => return vec!["timezone_type".to_string(), "timezone".to_string()],
            "DateInterval" => {
                return vec![
                    "from_string".to_string(), "date_string".to_string(), "y".to_string(),
                    "m".to_string(), "d".to_string(), "h".to_string(), "i".to_string(),
                    "s".to_string(), "f".to_string(), "invert".to_string(), "days".to_string(),
                ]
            }
            "DatePeriod" => {
                return vec![
                    "start".to_string(), "current".to_string(), "end".to_string(),
                    "interval".to_string(), "recurrences".to_string(),
                    "include_start_date".to_string(), "include_end_date".to_string(),
                ]
            }
            _ => {}
        }
        current = class_map.get(name).and_then(|parent| parent.extends.as_deref());
    }
    Vec::new()
}

/// Selects a parser-unrepresentable private helper name whose backend label cannot collide with
/// any user method declared on the same class.
fn date_magic_restore_slot_helper_name(class: &FlattenedClass) -> String {
    let existing_labels = class
        .methods
        .iter()
        .map(|method| label_fragment(&method.name))
        .collect::<std::collections::HashSet<_>>();
    for suffix in 0usize.. {
        let candidate = format!("{DATE_MAGIC_SLOT_HELPER_PREFIX}{suffix}");
        if !existing_labels.contains(&label_fragment(&candidate)) {
            return candidate;
        }
    }
    unreachable!("a finite class method list leaves a date restore helper name available")
}

/// Returns whether a user class descends from an internal date/time class.
fn has_builtin_datetime_ancestor(
    class_map: &HashMap<String, FlattenedClass>,
    class: &FlattenedClass,
) -> bool {
    let mut parent = class.extends.as_deref();
    while let Some(name) = parent {
        if is_builtin_datetime_class_name(name) {
            return true;
        }
        parent = class_map.get(name).and_then(|parent| parent.extends.as_deref());
    }
    false
}

/// Adds private native markers and collision-free typed property helpers to date descendants.
fn install_date_magic_restore_helpers(class_map: &mut HashMap<String, FlattenedClass>) {
    for class in class_map.values_mut() {
        if is_builtin_datetime_class_name(&class.name)
            && !class
                .methods
                .iter()
                .any(|method| method.name.eq_ignore_ascii_case(DATE_MAGIC_RESTORE_HELPER))
        {
            class.methods.push(date_magic_restore_root_method());
        }
        if is_builtin_datetime_class_name(&class.name)
            && !class
                .methods
                .iter()
                .any(|method| method.name.eq_ignore_ascii_case(DATE_MAGIC_FILTER_REFERENCES_HELPER))
        {
            class.methods.push(date_magic_filter_references_root_method());
        }
    }

    let subclasses = class_map
        .values()
        .filter(|class| {
            !is_builtin_datetime_class_name(&class.name)
                && !class.properties.is_empty()
                && !class
                    .methods
                    .iter()
                    .any(|method| method.name.starts_with(DATE_MAGIC_SLOT_HELPER_PREFIX))
                && has_builtin_datetime_ancestor(class_map, class)
        })
        .map(|class| class.name.clone())
        .collect::<Vec<_>>();
    for name in subclasses {
        let native_public_keys = date_magic_native_public_keys(
            class_map,
            class_map
                .get(&name)
                .expect("selected date subclass must remain in the class map"),
        );
        let class = class_map
            .get_mut(&name)
            .expect("selected date subclass must remain in the class map");
        let helper_name = date_magic_restore_slot_helper_name(class);
        let method = date_magic_restore_subclass_method(class, helper_name, &native_public_keys);
        class.methods.push(method);
    }
}

/// Builds the selected generated declaration program.
fn declarations(uses_timelib: bool) -> Program {
    if uses_timelib {
        super::generated_declarations_timelib::generated_datetime_declarations()
    } else {
        super::generated_declarations_fallback::generated_datetime_declarations()
    }
}

/// Converts one generated class declaration into checker flattening metadata.
fn flatten_class(stmt: Stmt) -> Option<FlattenedClass> {
    let attributes = stmt.attributes;
    let span = stmt.span;
    let StmtKind::ClassDecl {
        name,
        extends,
        implements,
        is_abstract,
        is_final,
        is_readonly_class,
        trait_uses,
        properties,
        methods,
        constants,
    } = stmt.kind
    else {
        return None;
    };
    debug_assert!(trait_uses.is_empty(), "generated DateTime classes use no traits");
    Some(FlattenedClass {
        name,
        span,
        extends: extends.map(|name| name.as_str().to_string()),
        implements: implements
            .into_iter()
            .map(|name| name.as_str().to_string())
            .collect(),
        is_abstract,
        is_final,
        is_readonly_class,
        properties,
        methods,
        attributes,
        constants,
        used_traits: Vec::new(),
        trait_aliases: Vec::new(),
    })
}

/// Converts one generated interface declaration into checker metadata.
fn interface_info(stmt: Stmt) -> Option<InterfaceDeclInfo> {
    let span = stmt.span;
    let StmtKind::InterfaceDecl {
        name,
        extends,
        properties,
        methods,
        constants,
    } = stmt.kind
    else {
        return None;
    };
    Some(InterfaceDeclInfo {
        name,
        extends: extends
            .into_iter()
            .map(|name| name.as_str().to_string())
            .collect(),
        properties,
        methods,
        span,
        constants,
    })
}

/// Creates one empty php-src date exception subclass.
fn date_exception_class(name: &str, parent: &str) -> FlattenedClass {
    FlattenedClass {
        name: name.to_string(),
        span: crate::span::Span::dummy(),
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

/// Injects DateTimeInterface plus the four core DateTime classes from generated AST.
pub(crate) fn inject_builtin_datetime(
    interface_map: &mut HashMap<String, InterfaceDeclInfo>,
    class_map: &mut HashMap<String, FlattenedClass>,
    uses_timelib: bool,
) {
    for stmt in declarations(uses_timelib) {
        if let Some(info) = interface_info(stmt.clone()) {
            interface_map.entry(info.name.clone()).or_insert(info);
            continue;
        }
        let Some(class) = flatten_class(stmt) else {
            continue;
        };
        if class.name != "DatePeriod" {
            class_map.entry(class.name.clone()).or_insert(class);
        }
    }
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
        class_map
            .entry(name.to_string())
            .or_insert_with(|| date_exception_class(name, parent));
    }
    install_date_magic_restore_helpers(class_map);
    super::native_procedural::install(class_map);
}

/// Injects DatePeriod from the same generated declaration variant.
pub(crate) fn inject_builtin_date_period(
    class_map: &mut HashMap<String, FlattenedClass>,
    uses_timelib: bool,
) {
    if class_map.contains_key("DatePeriod") {
        return;
    }
    for stmt in declarations(uses_timelib) {
        let Some(class) = flatten_class(stmt) else {
            continue;
        };
        if class.name == "DatePeriod" {
            class_map.insert(class.name.clone(), class);
            install_date_magic_restore_helpers(class_map);
            return;
        }
    }
}
