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
//!
//! Key details:
//! - There is no compiler-side class-name list any more: adding a builtin class means adding
//!   its contract (name, kind, module, route) to the shared catalog. The checker-side join test
//!   in this module proves the catalog and the checker's injections agree in both directions.

use std::collections::HashMap;
use std::sync::OnceLock;

use elephc_builtin_contract::{classes, lookup_class, PhpModule};

/// Returns the PHP spellings of every builtin class-like name, internal helpers included.
pub(crate) fn builtin_class_like_names() -> impl Iterator<Item = &'static str> {
    classes().iter().map(|class| class.name)
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use elephc_builtin_contract::{classes, ClassRoute, PhpModule};

    use crate::names::php_symbol_key;

    /// The AOT join for builtin classes: with every registration gate open (`introspects`),
    /// the checker must inject exactly the class-likes the catalog routes through the checker
    /// or the language front end — no catalogued name missing, no injected name uncatalogued.
    /// Prelude-declared classes are audited against their prelude declarations instead.
    #[test]
    fn checker_injects_exactly_the_catalogued_checker_classes() {
        let source = "<?php get_declared_classes();";
        let tokens = crate::lexer::tokenize(source).expect("tokenize");
        let program = crate::parser::parse(&tokens).expect("parse");
        let checked = crate::types::checker::check_types(
            &program,
            crate::codegen_support::platform::Target::new(
                crate::codegen_support::platform::Platform::MacOS,
                crate::codegen_support::platform::Arch::AArch64,
            ),
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
            "builtin class catalog and checker injections disagree.\n\
             injected by the checker but missing from the catalog: {uncatalogued:?}\n\
             catalogued as checker-provided but never injected: {uninjected:?}"
        );
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
