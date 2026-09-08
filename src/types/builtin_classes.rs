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

/// Identifies builtin classes whose runtime payload uses compact Throwable fields.
/// Generic property-default initialization must not write boxed defaults into this layout.
pub(crate) fn is_compact_throwable_class(class_name: &str) -> bool {
    matches!(
        class_name,
        "Error"
            | "TypeError"
            | "CompileError"
            | "ParseError"
            | "ArgumentCountError"
            | "ValueError"
            | "ArithmeticError"
            | "DivisionByZeroError"
            | "AssertionError"
            | "UnhandledMatchError"
            | "Exception"
            | "RuntimeException"
            | "ReflectionException"
            | "JsonException"
            | "FiberError"
            | "LogicException"
            | "BadFunctionCallException"
            | "BadMethodCallException"
            | "DomainException"
            | "InvalidArgumentException"
            | "LengthException"
            | "OutOfRangeException"
            | "OutOfBoundsException"
            | "OverflowException"
            | "RangeException"
            | "UnderflowException"
            | "UnexpectedValueException"
    )
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
