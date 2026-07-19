//! Purpose:
//! Corpus validation tests for AST-to-EIR lowering over real example programs.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Exercises the full frontend ordering, including resolver and autoload, on
//!   each default-profile `examples/*/main.php` fixture before EIR validation.

use std::path::{Path, PathBuf};

/// Verifies every checked example program lowers to validated printable EIR.
///
/// The `strict-php` example is lowered with strict-PHP mode enabled, matching
/// its documented `elephc --strict-php` invocation: it deliberately declares a
/// user function named after an extension builtin, which only PHP-compatible
/// (strict) resolution accepts.
#[test]
fn lowers_examples_corpus() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut fixtures = example_main_files(root);
    fixtures.sort();
    assert!(!fixtures.is_empty(), "expected example PHP fixtures");

    for fixture in fixtures {
        let strict = fixture
            .parent()
            .and_then(|dir| dir.file_name())
            .is_some_and(|name| name == "strict-php");
        // RAII guard: if lowering a strict fixture panics, the guard still
        // restores the state during unwinding, so no later fixture can
        // accidentally run with strict mode inherited.
        let _guard = strict.then(crate::strict_php::scoped_enable);
        let module = super::lower_file(&fixture);
        assert!(
            !module.functions.is_empty(),
            "expected at least main function for {}",
            fixture.display()
        );
    }
}

/// Returns all example `main.php` fixtures in deterministic order, excluding
/// examples that require a non-default compiler profile.
///
/// The corpus lowers each fixture in the default CLI build. Web examples rely on
/// the `--web` request prelude, while optional PDO driver examples may reference
/// constants and methods exposed only by their `pdo-*` Cargo feature. Those
/// profiles have dedicated tests, so this default-profile corpus skips them.
fn example_main_files(root: &Path) -> Vec<PathBuf> {
    let examples = root.join("examples");
    std::fs::read_dir(&examples)
        .expect("examples directory should exist")
        .map(|entry| entry.expect("example entry").path().join("main.php"))
        .filter(|path| path.exists())
        .filter(|path| !example_requires_non_default_profile(path))
        .collect()
}

/// Returns true when an example needs web mode or an optional PDO driver feature.
fn example_requires_non_default_profile(main_php: &Path) -> bool {
    const NON_DEFAULT_PROFILE_EXAMPLES: &[&str] = &[
        "pdo-cubrid",
        "pdo-dblib",
        "pdo-firebird",
        "pdo-ibm",
        "pdo-informix",
        "pdo-oci",
        "pdo-odbc",
        "pdo-sqlsrv",
        "web-session",
        "web-session-trans-sid",
        "web-session-upload",
    ];
    main_php
        .parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| NON_DEFAULT_PROFILE_EXAMPLES.contains(&name))
}
