//! Purpose:
//! Regressions for PHP extension validation across check, EIR and native output modes.
//!
//! Called from:
//! - The `phpext_tests` binary for SDK-independent checks and opt-in real-PHP pruning.
//!
//! Key details:
//! - An intentionally missing SDK proves source errors are reported before native build work.
//! - Nullable scalars and PHP arrays retain the producer's supported parameter shapes.
//! - Dead bridge declarations must disappear before validating final link requirements.

use super::*;

/// Native handles must retain their identity instead of boxing into PHP integer addresses.
#[test]
fn phpext_validation_rejects_native_handle_boxing() {
    let project = Project::new();
    let source = project.0.join("handles.php");
    for expression in ["buffer_new<int>(1)", "ptr($value)", "ptr_null()", "$records[0]"] {
        for (return_type, result) in [("mixed", expression.to_string()),
            ("array", format!("['handle' => {expression}]")),
            ("array", format!("[{expression}, {expression}]"))] {
            fs::write(&source, format!("<?php packed class NativeRecord {{ public int $value; }} #[Export] function php_handle(): {return_type} {{ $value = 1; $records = buffer_new<NativeRecord>(1); return {result}; }}")).unwrap();
            for mode in [Some("--check"), Some("--emit-ir"), None] {
                let mut command = Command::new(compiler_binary());
                command.args(["--emit", "phpext"]);
                if let Some(mode) = mode { command.arg(mode); }
                let output = command.arg(&source)
                    .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
                let diagnostic = String::from_utf8_lossy(&output.stderr);
                assert!(!output.status.success(), "{result}, {mode:?} boxed a native handle");
                let boxed_handle = diagnostic.contains("native handles cannot be boxed into PHP values");
                let pointer_policy = expression.starts_with("ptr") && diagnostic.contains("no audited request-local PHP extension lifecycle");
                assert!(boxed_handle || pointer_policy, "{result}, {mode:?}: {diagnostic}");
                assert!(!diagnostic.contains("query php-config"), "{result}, {mode:?}: {diagnostic}");
            }
        }
    }
}

/// Inferred storage types must not become declarations in the public PHP signature.
#[test]
fn phpext_validation_requires_declared_export_types() {
    let project = Project::new();
    let source = project.0.join("inferred.php");
    for (program, expected) in [
        ("function identity($value): int { return $value; }", "parameters require a declared type"),
        ("function identity($value = 'text'): string { return $value; }", "parameters require a declared type"),
        ("function identity(int $value) { return $value; }", "returns require a declared type"),
        ("function identity() { return 1; }", "returns require a declared type"),
    ] {
        fs::write(&source, format!("<?php #[Export] {program}")).unwrap();
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]);
            if let Some(mode) = mode { command.arg(mode); }
            let output = command.arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{program}, {mode:?} accepted an inferred declaration");
            assert!(diagnostic.contains(expected), "{program}, {mode:?}: {diagnostic}");
            assert!(!diagnostic.contains("query php-config"), "{program}, {mode:?}: {diagnostic}");
        }
    }
}

/// Null defaults need an explicit nullable native representation before any SDK work.
#[test]
fn phpext_validation_requires_explicit_nullable_defaults() {
    let project = Project::new();
    let source = project.0.join("implicit_nullable.php");
    for ty in ["int", "float", "bool", "string", "array", "false"] {
        fs::write(&source, format!("<?php #[Export] function optional({ty} $value = null): mixed {{ return $value; }}")).unwrap();
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]);
            if let Some(mode) = mode { command.arg(mode); }
            let output = command.arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{ty}, {mode:?} accepted an implicit nullable default");
            let nullable_error = diagnostic.contains("null defaults require an explicitly nullable parameter type");
            let checker_error = diagnostic.contains("parameter $value expects") && diagnostic.contains("got null");
            assert!(nullable_error || checker_error, "{ty}, {mode:?}: {diagnostic}");
            assert!(!diagnostic.contains("query php-config"), "{ty}, {mode:?}: {diagnostic}");
        }
    }
}

/// Native coroutine stacks, foreign globals and suppression cannot escape hosted state checks.
#[test]
fn phpext_validation_rejects_unhosted_runtime_state() {
    let project = Project::new();
    let source = project.0.join("unhosted.php");
    for (program, expected) in [
        ("function php_items(array $value) { foreach ($value as $item) { echo $item; yield $item; } } #[Export] function php_unhosted(array $value): array { $result = []; foreach (php_items($value) as $item) { $result[] = $item; } return $result; }", "native generators"),
        ("extern global int $optind; #[Export] function php_unhosted(): int { global $optind; return $optind; }", "extern globals"),
        ("extern global int $optind; #[Export] function php_unhosted(): int { global $optind; $optind = 5; return 1; }", "extern globals"),
        ("#[Export] function php_unhosted(array $value): mixed { return @$value['missing']; }", "error suppression"),
    ] {
        fs::write(&source, format!("<?php {program}")).unwrap();
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]);
            if let Some(mode) = mode { command.arg(mode); }
            let output = command.arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{expected}, {mode:?} bypassed hosted state validation");
            assert!(diagnostic.contains(expected), "{expected}, {mode:?}: {diagnostic}");
            assert!(!diagnostic.contains("query php-config"), "{expected}, {mode:?}: {diagnostic}");
        }
    }
}

/// Include-selected functions need top-level activation that a PHP extension never runs.
#[test]
fn phpext_validation_rejects_include_selected_function_variants() {
    let project = Project::new();
    let source = project.0.join("variants.php");
    for (included, main, expected) in [
        (
            "function helper(): int { return 1; }",
            "if ($argc > 5) { require __DIR__ . '/a.php'; } else { require __DIR__ . '/b.php'; } #[Export] function php_variant(): int { return helper(); }",
            "include-selected function variant 'helper'",
        ),
        (
            "#[Export] function php_inner(): int { return 7; }",
            "require __DIR__ . '/a.php';",
            "PHP export 'php_inner' is an include-selected function variant",
        ),
        (
            "#[Export] function php_inner(): int { return 7; }",
            "if ($argc > 5) { require __DIR__ . '/a.php'; } else { require __DIR__ . '/b.php'; }",
            "PHP export 'php_inner' is an include-selected function variant",
        ),
        (
            "#[Export] function php_inner($value) { return $value; }",
            "require __DIR__ . '/a.php';",
            "PHP export 'php_inner' is an include-selected function variant",
        ),
    ] {
        for file in ["a.php", "b.php"] {
            fs::write(project.0.join(file), format!("<?php {included}")).unwrap();
        }
        fs::write(&source, format!("<?php {main}")).unwrap();
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]);
            if let Some(mode) = mode {
                command.arg(mode);
            }
            let output = command
                .arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
                .output()
                .unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(
                !output.status.success(),
                "{mode:?} accepted an include-selected function variant"
            );
            assert!(diagnostic.contains(expected), "{mode:?}: {diagnostic}");
            assert!(
                !diagnostic.contains("__elephc_include_variant_"),
                "{mode:?}: {diagnostic}"
            );
            assert!(
                diagnostic.contains("does not execute top-level include activation"),
                "{mode:?}: {diagnostic}"
            );
            assert!(
                !diagnostic.contains("query php-config"),
                "{mode:?}: {diagnostic}"
            );
        }
    }
}

/// Handler registration and direct diagnostic emitters cannot bypass hosted capability checks.
#[test]
fn phpext_validation_rejects_unaudited_core_builtins() {
    let project = Project::new();
    let source = project.0.join("core.php");
    for body in ["trigger_error('boom', E_USER_WARNING);", "error_reporting(0);",
        "debug_print_backtrace();", "array_count_values([[]]);",
        "set_error_handler(function () { file_put_contents('unsafe', 'value'); return true; });"] {
        fs::write(&source, format!("<?php #[Export] function php_core(): void {{ {body} }}")).unwrap();
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]);
            if let Some(mode) = mode { command.arg(mode); }
            let output = command.arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{body}, {mode:?} accepted an unaudited operation");
            assert!(diagnostic.contains("no audited request-local PHP extension lifecycle"), "{body}, {mode:?}: {diagnostic}");
            assert!(!diagnostic.contains("query php-config"), "{body}, {mode:?}: {diagnostic}");
        }
    }
}

/// Requires all producer terminal modes to reject user-class construction consistently.
#[test]
fn phpext_validation_rejects_user_classes_in_eir() {
    let project = Project::new();
    let source = project.0.join("objects.php");
    fs::write(&source, "<?php class PhpBox { public int $value = 42; } #[Export] function box_value(): int { $box = new PhpBox(); return $box->value; }").unwrap();
    for mode in [Some("--check"), Some("--emit-ir"), None] {
        let mut command = Command::new(compiler_binary());
        command.args(["--emit", "phpext"]);
        if let Some(mode) = mode { command.arg(mode); }
        let output = command.arg(&source)
            .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
            .output().unwrap();
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{mode:?} accepted the unsupported class");
        assert!(diagnostic.contains("constructs unsupported class 'PhpBox'"), "{mode:?}: {diagnostic}");
        assert!(!diagnostic.contains("query php-config"), "{mode:?}: {diagnostic}");
    }
    let cdylib = Command::new(compiler_binary())
        .args(["--emit", "cdylib", "--emit-ir"]).arg(&source).output().unwrap();
    assert!(cdylib.status.success(), "{}", String::from_utf8_lossy(&cdylib.stderr));
}

/// Rejects unsupported scalar parameter unions before SDK discovery in every terminal mode.
#[test]
fn phpext_validation_rejects_parameter_unions_before_sdk() {
    let project = Project::new();
    let source = project.0.join("unions.php");
    for ty in ["int|float", "int|float|null", "string|bool"] {
        fs::write(&source, format!("<?php #[Export] function identity({ty} $value): {ty} {{ return $value; }}")).unwrap();
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]);
            if let Some(mode) = mode { command.arg(mode); }
            let output = command.arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
                .output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{ty}, {mode:?} accepted the unsupported union");
            assert!(diagnostic.contains("parameter union is not yet supported"), "{ty}, {mode:?}: {diagnostic}");
            assert!(diagnostic.contains("unions.php:1:"), "{ty}, {mode:?}: {diagnostic}");
            assert!(!diagnostic.contains("query php-config"), "{ty}, {mode:?}: {diagnostic}");
        }
    }
}

/// Keeps nullable scalars and nullable PHP arrays accepted in SDK-independent terminal modes.
#[test]
fn phpext_validation_accepts_nullable_parameter_shapes() {
    let project = Project::new();
    let source = project.0.join("nullable.php");
    fs::write(&source, "<?php #[Export] function nullable(?int $integer, ?float $float, ?bool $boolean, ?string $string, ?array $value): ?array { return $value; }").unwrap();
    for mode in ["--check", "--emit-ir"] {
        let output = Command::new(compiler_binary())
            .args(["--emit", "phpext", mode]).arg(&source)
            .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
            .output().unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Reports unsupported defaults as source errors before native SDK discovery in every mode.
#[test]
fn phpext_validation_rejects_defaults_before_sdk() {
    let project = Project::new();
    let source = project.0.join("defaults.php");
    for (ty, default, expected) in [
        ("array", "[1]", "defaults currently require scalar, null or empty array constants"),
        ("array", "['key' => 2]", "defaults currently require scalar, null or empty array constants"),
        ("bool", "1", "default value does not match the declared parameter type"),
    ] {
        fs::write(&source, format!("<?php #[Export] function php_default_array({ty} $value = {default}): {ty} {{ return $value; }}")).unwrap();
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]);
            if let Some(mode) = mode { command.arg(mode); }
            let output = command.arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
                .output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{default}, {mode:?} accepted an unsupported default");
            assert!(diagnostic.contains(expected), "{default}, {mode:?}: {diagnostic}");
            assert!(diagnostic.contains("defaults.php:1:"), "{default}, {mode:?}: {diagnostic}");
            assert!(!diagnostic.contains("query php-config"), "{default}, {mode:?}: {diagnostic}");
        }
    }
}

/// Preserves scalar, nullable and empty-array defaults in the SDK-independent validation paths.
#[test]
fn phpext_validation_accepts_supported_defaults() {
    let project = Project::new();
    let source = project.0.join("supported_defaults.php");
    fs::write(&source, "<?php #[Export] function defaults(int $integer = -42, float $float = 1.5, bool $boolean = false, string $string = 'text', ?int $nullable = null, array $values = []): array { return [$integer, $float, $boolean, $string, $nullable, $values]; }").unwrap();
    for mode in ["--check", "--emit-ir"] {
        let output = Command::new(compiler_binary())
            .args(["--emit", "phpext", mode]).arg(&source)
            .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
            .output().unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}

/// Rejects unsupported producer configuration before any terminal output path returns success.
#[test]
fn phpext_validation_rejects_options_before_sdk() {
    let project = Project::new();
    let source = project.0.join("options.php");
    fs::write(&source, "<?php #[Export] function sum(int $a, int $b): int { return $a + $b; }").unwrap();
    for options in [
        &["--gc-stats"][..], &["--counters"][..], &["--with-crypto"][..],
        &["--with-regex"][..], &["-l", "unavailable"][..],
        &["-L", "unavailable"][..], &["--framework", "unavailable"][..],
    ] {
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]).args(options);
            if let Some(mode) = mode { command.arg(mode); }
            let output = command.arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
                .output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{options:?}, {mode:?} accepted unsupported options");
            assert!(diagnostic.contains("PHP extension output currently requires the core runtime"), "{options:?}, {mode:?}: {diagnostic}");
            assert!(!diagnostic.contains("query php-config"), "{options:?}, {mode:?}: {diagnostic}");
        }
    }
}

/// Rejects live source dependencies consistently after declaration reachability is reconciled.
#[test]
fn phpext_validation_rejects_detected_libraries_before_sdk() {
    let project = Project::new();
    let source = project.0.join("libraries.php");
    for body in ["return hash('sha256', $value);", "return php_dependency($value);"] {
        fs::write(&source, format!("<?php function php_dependency(string $value): string {{ return hash('sha256', $value); }} #[Export] function php_live(string $value): string {{ {body} }}")).unwrap();
        for mode in [Some("--check"), Some("--emit-ir"), None] {
            let mut command = Command::new(compiler_binary());
            command.args(["--emit", "phpext"]);
            if let Some(mode) = mode { command.arg(mode); }
            let output = command.arg(&source)
                .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
                .output().unwrap();
            let diagnostic = String::from_utf8_lossy(&output.stderr);
            assert!(!output.status.success(), "{mode:?} accepted a detected bridge");
            assert!(diagnostic.contains("PHP extension output currently requires the core runtime"), "{mode:?}: {diagnostic}");
            assert!(!diagnostic.contains("query php-config"), "{mode:?}: {diagnostic}");
        }
    }
}

/// Preserves unused library declarations that native emission already removes before linking.
#[test]
fn phpext_validation_accepts_pruned_dependencies() {
    let project = Project::new();
    let source = project.0.join("pruned.php");
    fs::write(&source, "<?php function unused(string $value): string { return hash('sha256', $value); } #[Export] function php_ok(): int { return 1; }").unwrap();
    for mode in ["--check", "--emit-ir"] {
        let output = Command::new(compiler_binary())
            .args(["--emit", "phpext", mode]).arg(&source)
            .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
            .output().unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
    let output = Command::new(compiler_binary())
        .args(["--emit", "phpext"]).arg(&source)
        .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
        .output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("query php-config"), "{}", String::from_utf8_lossy(&output.stderr));
}

/// Enforces the native output's required export list before check/EIR can return success.
#[test]
fn phpext_validation_rejects_missing_exports_before_sdk() {
    let project = Project::new();
    let source = project.0.join("missing_exports.php");
    fs::write(&source, "<?php function php_unexported(): int { return 1; }").unwrap();
    for mode in [Some("--check"), Some("--emit-ir"), None] {
        let mut command = Command::new(compiler_binary());
        command.args(["--emit", "phpext"]);
        if let Some(mode) = mode { command.arg(mode); }
        let output = command.arg(&source)
            .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
            .output().unwrap();
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{mode:?} accepted missing exports");
        assert!(diagnostic.contains("requires at least one #[Export] function"), "{mode:?}: {diagnostic}");
        assert!(!diagnostic.contains("query php-config"), "{mode:?}: {diagnostic}");
    }
}

/// Loads a core-only extension after pruning a declaration that would otherwise need crypto.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_prunes_unused_bridge_requirements() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("pruned_bridge", "<?php function php_unused_bridge(string $value): string { return hash('sha256', $value); } #[Export] function php_pruned_ok(): int { return 42; }");
    let output = php(&sdk, &module).args(["-r", "echo php_pruned_ok();"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"42");
    assert!(output.stderr.is_empty());
}

/// Rejects an installed but unused hosted extension before either check/EIR early return.
#[test]
fn phpext_validation_rejects_hosted_extensions_before_sdk() {
    let project = Project::new();
    let source = project.0.join("hosted.php");
    fs::write(&source, "<?php #[Export] function php_ok(): int { return 1; }").unwrap();
    let cache = project.0.join("cache");
    let fixture = fixture_dir().join("demo");
    let installed = Command::new(compiler_binary())
        .current_dir(&project.0).args(["extension", "add", "elephc_demo", "--path"])
        .arg(fixture).env("ELEPHC_NATIVE_CACHE", &cache).output().unwrap();
    assert!(installed.status.success(), "{}", String::from_utf8_lossy(&installed.stderr));
    for mode in [Some("--check"), Some("--emit-ir"), None] {
        let mut command = Command::new(compiler_binary());
        command.args(["--emit", "phpext"]);
        if let Some(mode) = mode { command.arg(mode); }
        let output = command.arg(&source)
            .env("ELEPHC_NATIVE_CACHE", &cache)
            .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
            .output().unwrap();
        let diagnostic = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{mode:?} accepted a hosted extension");
        assert!(diagnostic.contains("PHP extension output currently requires the core runtime"), "{mode:?}: {diagnostic}");
        assert!(!diagnostic.contains("query php-config"), "{mode:?}: {diagnostic}");
    }
}
