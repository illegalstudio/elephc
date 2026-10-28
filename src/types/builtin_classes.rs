//! Purpose:
//! The compiler's single view over the shared builtin class-like catalog
//! (`elephc_builtin_contract::classes()`): every class, interface, and enum elephc provides,
//! grouped by the PHP module that owns it.
//!
//! Called from:
//! - `crate::autoload` and `crate::name_resolver::symbols`, which must never treat a builtin
//!   class-like as an autoload demand or an undeclared user symbol.
//! - Prelude demand detectors (`curl_prelude`, `image_prelude`) and checker gates
//!   (`builtin_types::datetime::gate`), which look for a module's class names in the program.
//! - `spl_classes()`, which lists the `ext/spl` module.
//! - EIR eval probes and codegen existence queries, which also expose intrinsic classes
//!   without an injected object layout.
//!
//! Key details:
//! - There is no compiler-side class-name list any more: adding a builtin class means adding
//!   its contract (name, kind, module, route) to the shared catalog. The checker-side join test
//!   in this module proves the catalog and the checker's injections agree in both directions.

use std::collections::HashMap;
use std::sync::OnceLock;

use elephc_builtin_contract::{classes, lookup_class, ClassKind, ClassRoute, PhpModule};

/// Returns the PHP spellings of every builtin class-like name, internal helpers included.
pub(crate) fn builtin_class_like_names() -> impl Iterator<Item = &'static str> {
    classes().iter().map(|class| class.name)
}

/// Returns public, target-independent intrinsic classes, including callable-backed classes.
/// Target-specific classes remain subject to the checker's target-filtered metadata.
pub(crate) fn intrinsic_class_names() -> impl Iterator<Item = &'static str> {
    classes()
        .iter()
        .filter(|class| {
            !class.internal
                && class.kind == ClassKind::Class
                && class.aot == ClassRoute::LanguageIntrinsic
                && class.target_support.is_none()
        })
        .map(|class| class.name)
}

/// Returns the PHP-visible class-like names one PHP module owns, in canonical order.
pub(crate) fn class_names_in_module(module: PhpModule) -> &'static [&'static str] {
    static BY_MODULE: OnceLock<HashMap<PhpModule, Vec<&'static str>>> = OnceLock::new();
    BY_MODULE
        .get_or_init(|| {
            let mut by_module: HashMap<PhpModule, Vec<&'static str>> = HashMap::new();
            for class in classes().iter().filter(|class| !class.internal) {
                by_module.entry(class.module).or_default().push(class.name);
            }
            by_module
        })
        .get(&module)
        .map_or(&[], Vec::as_slice)
}

/// Returns whether `name` (case-insensitive, optional leading `\`) is a catalogued
/// PHP-visible class-like owned by one of `modules`.
pub(crate) fn is_class_like_in_modules(name: &str, modules: &[PhpModule]) -> bool {
    lookup_class(name).is_some_and(|class| !class.internal && modules.contains(&class.module))
}

/// Recognizes internal dimension handlers without declaring the PHP ArrayAccess interface.
/// Subclasses inherit the native handler, while unrelated same-named user classes do not.
pub(crate) fn has_native_dimensions(name: &str, classes: &HashMap<String, super::ClassInfo>) -> bool {
    let mut current = Some(name.trim_start_matches('\\'));
    for _ in 0..=classes.len() {
        let Some(name) = current else { return false; };
        let Some(info) = classes.get(name) else { return false; };
        if info.is_internal && lookup_class(name).is_some_and(|class| class.module == PhpModule::Simplexml) {
            return true;
        }
        current = info.parent.as_deref();
    }
    false
}

/// Recognizes non-null object sinks which can receive a nullable native XML property read.
pub(crate) fn native_object_sink_needs_null_guard(
    ty: &super::PhpType, classes: &HashMap<String, super::ClassInfo>,
    interfaces: &HashMap<String, super::InterfaceInfo>,
) -> bool {
    if let super::PhpType::Union(members) = ty {
        return !members.iter().any(native_boundary_accepts_null)
            && members.iter().any(|member| native_object_sink_needs_null_guard(member, classes, interfaces));
    }
    let super::PhpType::Object(name) = ty.codegen_repr() else { return false; };
    if has_native_dimensions(&name, classes) { return true; }
    (name.is_empty() || name.eq_ignore_ascii_case("object") || interfaces.contains_key(&name))
        && classes.keys().any(|candidate| has_native_dimensions(candidate, classes))
}

/// Nullable and mixed union arms accept PHP null without narrowing it to an object.
fn native_boundary_accepts_null(ty: &super::PhpType) -> bool {
    match ty {
        super::PhpType::Void | super::PhpType::Mixed | super::PhpType::TaggedScalar => true,
        super::PhpType::Union(members) => members.iter().any(native_boundary_accepts_null),
        _ => false,
    }
}

/// Resolves a native dimension adapter through the declaring internal ancestor.
pub(crate) fn native_dimension_method(
    name: &str, operation: elephc_builtin_contract::ObjectDimension,
    classes: &HashMap<String, super::ClassInfo>,
) -> Option<&'static str> {
    let mut current = Some(name.trim_start_matches('\\'));
    for _ in 0..=classes.len() {
        let info = classes.get(current?)?;
        if info.is_internal {
            if let Some(method) = lookup_class(current?).and_then(|class| class.operators)
                .and_then(|operators| operators.dimension_method(operation)) { return Some(method); }
        }
        current = info.parent.as_deref();
    }
    None
}

/// Resolves a native property adapter while reserving the prelude's own storage names.
pub(crate) fn native_property_method(
    name: &str, property: &str, operation: elephc_builtin_contract::ObjectProperty,
    classes: &HashMap<String, super::ClassInfo>,
) -> Option<&'static str> {
    if property.starts_with("__elephc_") { return None; }
    let mut current = Some(name.trim_start_matches('\\'));
    for _ in 0..=classes.len() {
        let info = classes.get(current?)?;
        if info.is_internal {
            if let Some(method) = lookup_class(current?).and_then(|class| class.operators)
                .and_then(|operators| operators.property_method(operation))
            { return Some(method); }
        }
        current = info.parent.as_deref();
    }
    None
}

/// Checks inherited PHP internal clone restrictions without rejecting user declarations.
pub(crate) fn class_allows_clone(name: &str, classes: &HashMap<String, crate::types::ClassInfo>) -> bool {
    let mut current = name.trim_start_matches('\\');
    for _ in 0..=classes.len() {
        let Some(info) = classes.get(current) else { return true; };
        if info.is_internal && lookup_class(current).is_some_and(|contract| !contract.cloneable) {
            return false;
        }
        let Some(parent) = info.parent.as_deref() else { return true; };
        current = parent.trim_start_matches('\\');
    }
    true
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use elephc_builtin_contract::{classes, ClassRoute, PhpModule};

    use crate::names::php_symbol_key;

    /// Checks catalog/checker parity on every target, excluding intrinsic callable storage.
    /// Prelude declarations are audited separately against their source declarations.
    #[test]
    fn checker_injects_exactly_the_catalogued_checker_classes() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            assert_checker_class_catalog_for_target(target);
        }
    }

    /// Compares injected object metadata with the catalog filtered to one target.
    fn assert_checker_class_catalog_for_target(target: &str) {
        let source = "<?php get_declared_classes();";
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let checked = crate::types::checker::check_types(
            &program,
            crate::codegen_support::platform::Target::parse(target).expect("target"),
        )
        .expect("check");

        let injected: BTreeSet<String> = checked
            .classes
            .keys()
            .chain(checked.interfaces.keys())
            .chain(checked.enums.keys())
            .map(|name| php_symbol_key(name))
            .filter(|key| !key.starts_with("__elephc"))
            .collect();
        let catalogued: BTreeSet<String> = classes()
            .iter()
            .filter(|class| {
                !class.internal
                    // Closures use callable storage, not a checker-injected object class.
                    && class.name != "Closure"
                    && class.target_support.is_none_or(|targets| targets.contains(&target))
                    && matches!(
                        class.aot,
                        ClassRoute::CheckerInjected | ClassRoute::LanguageIntrinsic
                    )
            })
            .map(|class| php_symbol_key(class.name))
            .collect();

        let uncatalogued: Vec<&String> = injected.difference(&catalogued).collect();
        let uninjected: Vec<&String> = catalogued.difference(&injected).collect();
        assert!(
            uncatalogued.is_empty() && uninjected.is_empty(),
            "builtin class catalog and checker injections disagree on {target}.\n\
             injected by the checker but missing from the catalog: {uncatalogued:?}\n\
             catalogued as checker-provided but never injected: {uninjected:?}"
        );
    }

    /// Retains the Closure symbol used by callable lowering without an object-class injection.
    #[test]
    fn callable_intrinsic_is_in_the_shared_class_catalog() {
        let closure = elephc_builtin_contract::lookup_class("\\cLoSuRe").expect("Closure");
        assert_eq!(closure.aot, ClassRoute::LanguageIntrinsic);
        assert!(super::builtin_class_like_names().any(|name| name == "Closure"));
    }

    /// Every module view is non-empty for the modules the compiler detects by class name.
    #[test]
    fn module_views_cover_the_detected_modules() {
        for module in [PhpModule::Spl, PhpModule::Curl, PhpModule::Date, PhpModule::Gd] {
            assert!(!super::class_names_in_module(module).is_empty(), "{module:?}");
        }
        assert!(super::is_class_like_in_modules("\\imagick", &[PhpModule::Imagick]));
        assert!(!super::is_class_like_in_modules("ArrayIterator", &[PhpModule::Gd]));
    }
}
