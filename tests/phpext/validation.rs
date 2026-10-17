//! Purpose:
//! Regressions for PHP extension validation across check, EIR and native output modes.
//!
//! Called from:
//! - The `phpext_tests` integration test binary, without requiring a PHP SDK.
//!
//! Key details:
//! - An intentionally missing SDK proves source errors are reported before native build work.
//! - Nullable scalars and PHP arrays retain the producer's supported parameter shapes.

use super::*;

/// Requires all producer terminal modes to reject user-class construction consistently.
#[test]
fn phpext_validation_rejects_user_classes_in_eir() {
    let project = Project::new();
    let source = project.0.join("objects.php");
    fs::write(&source, "<?php class PhpBox { public int $value = 42; } #[Export] function box_value(): int { $box = new PhpBox(); return $box->value; }").unwrap();
    for mode in [Some("--check"), Some("--emit-ir"), None] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_elephc"));
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
    let cdylib = Command::new(env!("CARGO_BIN_EXE_elephc"))
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
            let mut command = Command::new(env!("CARGO_BIN_EXE_elephc"));
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
        let output = Command::new(env!("CARGO_BIN_EXE_elephc"))
            .args(["--emit", "phpext", mode]).arg(&source)
            .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk"))
            .output().unwrap();
        assert!(output.status.success(), "{mode}: {}", String::from_utf8_lossy(&output.stderr));
    }
}
