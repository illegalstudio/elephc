//! Purpose:
//! Verifies selected PHP constant exports and modules without native function handlers.
//!
//! Called from:
//! - The PHP producer integration test binary and real-SDK CI matrix.
//!
//! Key details:
//! - Selection errors must precede SDK work, and real modules keep Zend ownership.

use super::*;

/// Rejects undeclared selections in every terminal mode without consulting the PHP SDK.
#[test]
fn phpext_constants_reject_missing_selection_before_sdk() {
    let project = Project::new();
    let source = project.0.join("missing_constant.php");
    fs::write(&source, "<?php const PRESENT = 42;").unwrap();
    for mode in [Some("--check"), Some("--emit-ir"), None] {
        let mut command = Command::new(compiler_binary());
        command.args(["--emit", "phpext", "--export-constant=MISSING"]);
        if let Some(mode) = mode { command.arg(mode); }
        let output = command.arg(&source)
            .env("ELEPHC_PHPEXT_PHP_CONFIG", project.0.join("missing-sdk")).output().unwrap();
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(!output.status.success(), "{mode:?} accepted a missing constant");
        assert!(error.contains("not a top-level const declaration"), "{mode:?}: {error}");
        assert!(!error.contains("query php-config"), "{mode:?}: {error}");
    }
}

/// Refuses the producer-only option in executable mode before processing source.
#[test]
fn phpext_constants_reject_non_php_emit() {
    let output = Command::new(compiler_binary())
        .args(["--export-constant=VERSION", "missing.php"]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--export-constant requires --emit phpext"));
}

/// Registers a constant-only module with arithmetic aliases and binary-safe persistent values.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_constants_only_module_uses_host_owned_scalars() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile_with_options("constant_only", r#"<?php
namespace ConstDemo;
const BASE = 20;
const ANSWER = BASE + 22;
const BYTES = "a\0b";
const ENABLED = true;
const NOTHING = null;
const NEGATIVE_ZERO = -0.0;
"#, &["--heap-size=524288", "--export-constant=ConstDemo\\ANSWER",
        "--export-constant=ConstDemo\\BYTES", "--export-constant=ConstDemo\\ENABLED",
        "--export-constant=ConstDemo\\NOTHING", "--export-constant=ConstDemo\\NEGATIVE_ZERO"]);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
$r = new ReflectionExtension('constant_only');
$expected = ['ConstDemo\\ANSWER' => 42, 'ConstDemo\\BYTES' => "a\0b",
    'ConstDemo\\ENABLED' => true, 'ConstDemo\\NOTHING' => null, 'ConstDemo\\NEGATIVE_ZERO' => -0.0];
if ($r->getConstants() !== $expected || $r->getFunctions() !== [] || defined('ConstDemo\\BASE')) { exit(2); }
if (pack('d', ConstDemo\NEGATIVE_ZERO) !== pack('d', -0.0)) { exit(3); }
echo "constants ok\n";
"#]).output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"constants ok\n");
    project.assert_clean();
}

/// Retains ordinary compiled function invocation when a module also exports constants.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_constants_coexist_with_native_functions() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile_with_options("constant_function", "<?php const DEMO_ANSWER = 42; #[Export] function php_constant_answer(): int { return DEMO_ANSWER; }",
        &["--heap-size=524288", "--export-constant=DEMO_ANSWER"]);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display()))
        .args(["-r", "var_dump(DEMO_ANSWER, php_constant_answer());"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"int(42)\nint(42)\n");
    project.assert_clean();
}

/// Keeps immutable nested arrays, numeric keys and unpacking values safe through COW and shutdown.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_constants_immutable_arrays_preserve_host_cow() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile_with_options("constant_arrays", r#"<?php
namespace ConstArray;
const BASE = [5 => 'five', 'named' => 'old'];
const DATA = ['nested' => ['value' => 'original'], ...BASE, 'named' => 'new',
    '8' => 'eight', '08' => 'zeroeight', -2 => 'negative', 'tail', 'binary' => "a\0b"];
const NEGATIVE = [-2 => 'start', 'tail'];
"#, &["--heap-size=524288", "--export-constant=ConstArray\\DATA", "--export-constant=ConstArray\\NEGATIVE"]);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", module.display())).args(["-r", r#"
$expected = ['nested' => ['value' => 'original'], ...[5 => 'five', 'named' => 'old'],
    'named' => 'new', '8' => 'eight', '08' => 'zeroeight', -2 => 'negative', 'tail', 'binary' => "a\0b"];
$constants = (new ReflectionExtension('constant_arrays'))->getConstants();
if ($constants !== ['ConstArray\\DATA' => $expected, 'ConstArray\\NEGATIVE' => [-2 => 'start', 'tail']]) { exit(2); }
for ($i = 0; $i < 1000; $i++) {
    $copy = ConstArray\DATA;
    $copy['nested']['value'] = 'changed';
    $copy['binary'][0] = 'Z';
    $copy[] = $i;
    unset($copy);
    if (ConstArray\DATA !== $expected) { exit(3); }
}
echo "arrays ok\n";
"#]).output().unwrap();
    assert!(output.status.success(), "{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"arrays ok\n");
    project.assert_clean();
}

/// Checks every constant collision before registering any class from the failing module.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn phpext_constants_collision_does_not_publish_partial_classes() {
    let project = Project::new();
    let sdk = project.sdk();
    let first = project.compile_with_options("collision_first", "<?php const PHPEXT_COLLISION = 1;",
        &["--heap-size=524288", "--export-constant=PHPEXT_COLLISION"]);
    let second = project.compile_with_options("collision_second", "<?php const PHPEXT_COLLISION = 2; #[Export] final class CollisionClass {}",
        &["--heap-size=524288", "--export-constant=PHPEXT_COLLISION"]);
    let output = Command::new(sdk.php_binary()).args(["-n", "-d"])
        .arg(format!("extension={}", first.display())).arg("-d")
        .arg(format!("extension={}", second.display()))
        .args(["-r", "if (PHPEXT_COLLISION !== 1 || class_exists('CollisionClass', false)) { exit(2); } echo 'collision contained';"])
        .output().unwrap();
    let diagnostics = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
    // Zend can stop startup on MINIT failure. A signal is never an acceptable outcome.
    assert!(output.status.code().is_some(), "status {:?}: {diagnostics}", output.status);
    assert!(!diagnostics.contains("Segmentation fault") && !diagnostics.contains("Assertion"), "{diagnostics}");
    assert!(diagnostics.contains("PHP extension constant already defined"), "{diagnostics}");
    if output.status.success() { assert!(diagnostics.contains("collision contained"), "{diagnostics}"); }
    else { assert!(diagnostics.contains("Unable to start collision_second module"), "status {:?}: {diagnostics}", output.status); }
    project.assert_clean();
}
