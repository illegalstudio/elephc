//! Purpose:
//! Corpus validation tests for AST-to-EIR lowering over real example programs.
//!
//! Called from:
//! - `crate::ir_lower::tests`.
//!
//! Key details:
//! - Exercises the full frontend ordering, including resolver and autoload, on
//!   each `examples/*/main.php` or `main.lfc` fixture before EIR validation.

use std::path::{Path, PathBuf};

/// Lowers the PCNTL example for each desktop target without requiring cross-target linking.
#[test]
fn lowers_pcntl_example_on_every_desktop_target() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/pcntl/main.php");
    let source = std::fs::read_to_string(&path).expect("PCNTL example");
    let parent = path.parent().expect("example directory");
    for target in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
        let target = crate::codegen::platform::Target::parse(target).expect("target");
        let module = super::lower_source_at_for_target(&source, &path, parent, target);
        assert!(!module.functions.is_empty(), "expected lowered main for {}", target.as_str());
    }
}

/// Checks nonliteral polyfill guards through the corpus pipeline on every supported target.
#[test]
fn lowers_target_guarded_polyfill_on_every_target() {
    let source = r#"<?php
$name = 'pcntl_getcpu';
if (!function_exists($name)) {
    function pcntl_getcpu(): int { return -1; }
}
echo pcntl_getcpu();
"#;
    for target in [
        "macos-aarch64",
        "ios-arm64",
        "ios-sim-arm64",
        "linux-aarch64",
        "linux-x86_64",
    ] {
        let target = crate::codegen::platform::Target::parse(target).expect("target");
        let module = super::lower_source_at_for_target(
            source,
            Path::new("main.php"),
            Path::new("."),
            target,
        );
        let has_polyfill = module.functions.iter().any(|function| function.name == "pcntl_getcpu");
        assert_eq!(
            has_polyfill,
            target.platform != crate::codegen::platform::Platform::Linux,
            "polyfill availability on {}",
            target.as_str(),
        );
    }
}

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

/// Returns all example `main.php` and `main.lfc` fixtures in deterministic order, excluding
/// examples that require a feature prelude or optional-driver build profile.
///
/// The corpus lowers each fixture in plain (CLI) mode, which does not inject the
/// feature preludes and optional PDO driver surfaces that the pipeline adds during a
/// real compile. Those profiles have dedicated tests, so their examples are skipped
/// rather than treated as failures in default-profile lowering.
fn example_main_files(root: &Path) -> Vec<PathBuf> {
    let examples = root.join("examples");
    std::fs::read_dir(&examples)
        .expect("examples directory should exist")
        .filter_map(|entry| {
            let directory = entry.expect("example entry").path();
            ["main.php", "main.lfc"]
                .into_iter()
                .map(|name| directory.join(name))
                .find(|path| path.exists())
        })
        .filter(|path| !example_requires_non_default_profile(path))
        .collect()
}

/// Returns true when an example needs a feature prelude or optional PDO driver profile.
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
        // This fixture intentionally exercises PHP_WIN32-only GD capture
        // functions. The host corpus is compiled for the detected host;
        // Windows PE has a dedicated target test for this example.
        "windows-image-capture",
        // This fixture exercises PHP_WIN32-only SAPI helpers. It is compiled
        // by the dedicated Windows PE example gate instead of the host corpus.
        "windows-sapi",
        // OPcache introspection functions are provided by the pay-for-use OPcache
        // prelude, which the plain CLI-mode corpus lowering does not inject.
        "opcache_get_configuration",
    ];
    main_php
        .parent()
        .and_then(|dir| dir.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| NON_DEFAULT_PROFILE_EXAMPLES.contains(&name))
}
