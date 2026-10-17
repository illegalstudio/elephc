//! Purpose:
//! Real-PHP integration tests for producer SDK discovery, registration and native invocation.
//!
//! Called from:
//! - `cargo test --test phpext_tests -- --include-ignored` with a PHP 8.5 development SDK.
//!
//! Key details:
//! - ELEPHC_PHPEXT_PHP_CONFIG selects the SDK; the SDK selects the matching interpreter.
//! - Missing or incompatible SDKs fail explicitly when these tests are requested.
//! - SDK-only registration tests and compiled-function tests exercise separate guarantees.
//! - Every fixture owns one private directory and removes it on success or panic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use elephc::codegen_support::platform::Target;
use elephc::phpext::{render_registration, PhpSdk};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[path = "phpext/validation.rs"]
mod validation;

struct Project(PathBuf);

impl Project {
    /// Creates an exclusive fixture directory without replacing an existing file or directory.
    fn new() -> Self {
        for _ in 0..100 {
            let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("elephc-phpext-tests-{}-{id}", std::process::id()));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create PHP extension test directory: {error}"),
            }
        }
        panic!("could not reserve PHP extension test directory");
    }

    /// Selects a real host SDK and requires its interpreter to load the validation module.
    fn sdk(&self) -> PhpSdk {
        let config = std::env::var_os("ELEPHC_PHPEXT_PHP_CONFIG").unwrap_or_else(|| "php-config".into());
        let sdk = PhpSdk::discover(Path::new(&config), Target::detect_host(), &self.0)
            .unwrap_or_else(|error| panic!("PHP SDK discovery: {error}"));
        if let Ok(expected) = std::env::var("ELEPHC_PHPEXT_EXPECT_ZTS") {
            assert!(matches!(expected.as_str(), "0" | "1"), "invalid expected ZTS flag");
            assert_eq!(sdk.build().zts, expected == "1", "the CI SDK must match the requested thread mode");
        }
        if let Ok(expected) = std::env::var("ELEPHC_PHPEXT_EXPECT_TARGET") {
            assert_eq!(sdk.build().target, expected, "the CI SDK must match the requested platform");
        }
        sdk
    }

    /// Asserts no private compiler staging directories survived a completed SDK operation.
    fn assert_clean(&self) {
        for path in fs::read_dir(&self.0).unwrap() {
            let path = path.unwrap();
            assert!(!path.file_name().to_string_lossy().starts_with(".elephc-phpext-"),
                "staging directory survived: {:?}", path.path());
        }
    }

    /// Compiles source with the real compiler binary and requires a loadable producer artifact.
    fn compile(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(format!("{name}.php"));
        fs::write(&path, source).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_elephc"))
            .args(["--emit", "phpext", "--heap-size=524288"]).arg(&path).output().unwrap();
        assert!(output.status.success(), "compiler failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout), String::from_utf8_lossy(&output.stderr));
        let module = self.0.join(format!("{name}.so"));
        assert!(module.is_file());
        self.assert_clean();
        module
    }
}

impl Drop for Project {
    /// Removes only this fixture's exclusively created directory, including failed test paths.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Checks the shared neutral Rust value types against real PHP headers rather than generated headers.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn real_host_value_layout_matches_shared_contract() {
    let project = Project::new();
    let sdk = project.sdk();
    let facts = elephc_zend::abi::values::value_layout_facts();
    assert_eq!(sdk.build().layout.len(), facts.len());
    for (expression, expected) in facts {
        assert_eq!(sdk.build().layout.get(&expression), Some(&expected), "{expression}");
    }
    assert_eq!(sdk.build().php_version_id / 100, 805);
    assert_eq!(sdk.abi_identity().len(), 64);
    project.assert_clean();
}

/// Loads generated registration through extension= and inspects its host-owned metadata and MINFO.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn real_php_loads_generated_registration() {
    let project = Project::new();
    let sdk = project.sdk();
    let source = render_registration("elephc_producer_test", "0.1.0-test").unwrap();
    let module = project.0.join("producer.so");
    sdk.compile_extension(&source, &module).unwrap();
    let output = php(&sdk, &module).args(["-r", r#"
$ext = new ReflectionExtension('elephc_producer_test');
echo json_encode([$ext->getName(), $ext->getVersion(), $ext->getFunctions()], JSON_THROW_ON_ERROR);
"#]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert!(output.stderr.is_empty());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), r#"["elephc_producer_test","0.1.0-test",[]]"#);
    let output = php(&sdk, &module).args(["--ri", "elephc_producer_test"]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let info = String::from_utf8(output.stdout).unwrap();
    assert!(info.contains("Module version => 0.1.0-test"), "{info}");
    assert!(info.contains(&sdk.build().build_id), "{info}");
    project.assert_clean();
}

/// Ensures the module defines only get_module publicly and imports Zend from its real host.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK and nm"]
fn generated_module_exports_only_get_module() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.0.join("symbols.so");
    sdk.compile_extension(&render_registration("elephc_symbols", "1.0").unwrap(), &module).unwrap();
    let mut command = Command::new("nm");
    if cfg!(target_os = "macos") {
        command.arg("-gU");
    } else {
        command.args(["-D", "--defined-only"]);
    }
    let output = command.arg(&module).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let symbols = String::from_utf8(output.stdout).unwrap();
    let names: Vec<_> = symbols.lines().filter_map(|line| line.split_whitespace().last()).collect();
    assert_eq!(names, [if cfg!(target_os = "macos") { "_get_module" } else { "get_module" }]);
    project.assert_clean();
}

/// Cross-checks the four requested exported functions against the identical pure PHP source.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_exports_match_pure_php() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("main", include_str!("../examples/php-extension/main.php"));
    let driver = project.0.join("driver.php");
    fs::write(&driver, include_str!("../examples/php-extension/driver.php")).unwrap();
    let reference = Command::new(sdk.php_binary()).arg("-n").arg(&driver).output().unwrap();
    assert!(reference.status.success(), "{}", String::from_utf8_lossy(&reference.stderr));
    let native = php(&sdk, &module).arg(&driver).output().unwrap();
    assert!(native.status.success(), "native PHP failed: {:?}\n{}\n{}", native.status,
        String::from_utf8_lossy(&native.stdout), String::from_utf8_lossy(&native.stderr));
    assert!(native.stderr.is_empty(), "{}", String::from_utf8_lossy(&native.stderr));
    assert_eq!(native.stdout, reference.stdout);
}

/// Exercises deep copies, append history, input ownership and bounded repeated-call heap usage.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_array_ownership_and_repeated_calls() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("ownership", r#"<?php
#[Export] function php_copy(array $value): array { return $value; }
#[Export] function php_append(array $value): array { $value[] = 'added'; return $value; }
#[Export] function php_upper(string $value): string { return strtoupper($value); }
#[Export] function php_failure(): void { throw new RuntimeException("bad\0message", 123); }
"#);
    let result = php(&sdk, &module).args(["-r", r#"
$value = [-4 => 'negative', 2 => "two\0bytes", '08' => [true, null, 3.5], "key\0bytes" => false];
$value[100] = 'deleted'; unset($value[100]);
$copy = php_copy($value);
if ($copy !== $value || array_key_last(php_append($value)) !== 101 || $copy !== $value) { exit(10); }
$packed = range(0, 63);
$large = str_repeat('x', 16384);
$start = memory_get_usage();
for ($i = 0; $i < 600; $i++) {
    if (php_copy($packed) !== $packed || php_copy($value) !== $value || php_upper($large) !== strtoupper($large)) { exit(11); }
    try { php_failure(); exit(12); } catch (RuntimeException $e) {
        if ($e->getCode() !== 123 || $e->getMessage() !== "bad\0message") { exit(13); }
    }
    unset($e);
}
gc_collect_cycles();
if (memory_get_usage() - $start > 65536) { exit(14); }
$ref = 1; $references = [&$ref];
try { php_copy($references); exit(15); } catch (Error $e) {
    if (!str_contains($e->getMessage(), 'references')) { exit(16); }
}
echo 'ok';
"#]).output().unwrap();
    assert!(result.status.success(), "PHP failed: {:?}\n{}\n{}", result.status,
        String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, b"ok");
    assert!(result.stderr.is_empty(), "{}", String::from_utf8_lossy(&result.stderr));
}

/// PHP output handlers can invoke another compiled export after native state is parked.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_output_can_reenter_exports() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("output", r#"<?php
#[Export] function php_announce(string $value): string { echo $value; return 'result'; }
#[Export] function php_sum(int $a, int $b): int { return $a + $b; }
"#);
    let result = php(&sdk, &module).args(["-r", r#"
ob_start(function ($text) { return $text . ':' . php_sum(20, 22); }, 1);
$result = php_announce("captured\0bytes");
ob_end_flush();
echo '|', $result;
"#]).output().unwrap();
    assert!(result.status.success(), "{:?}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, b"captured\0bytes:42:42|result");
    assert!(result.stderr.is_empty());
}

/// Zend validates scalar strictness, nullable values, defaults and named arguments.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_scalar_types_and_metadata() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("scalars", r#"<?php
#[Export] function php_scalars(?int $integer, float $float, bool $boolean, ?string $string = null): array {
    return [$integer, $float, $boolean, $string];
}


#[Export] function php_null(null $value): null { return $value; }
#[Export] function php_default(string $value = "default\0bytes"): string { return $value; }
#[Export] function php_void(): void {}
#[Export] function php_double(float $value): float { return $value * 2.0; }
#[Export] function php_invert(bool $value): bool { return !$value; }
#[Export] function php_nullable(?int $value): ?int { return $value; }
#[Export] function php_false(false $value): false { return $value; }
#[Export] function php_empty(array $value = []): array { return $value; }
#[Export] function php_float_default(float $value = 1): float { return $value; }
#[Export] function php_integer(int $value): int { return $value; }
"#);
    let result = php(&sdk, &module).args(["-r", r#"
declare(strict_types=1);
if (php_scalars(boolean: true, float: 2, integer: null) !== [null, 2.0, true, null]) { exit(10); }
if (php_scalars(5, 3.5, false, "a\0b") !== [5, 3.5, false, "a\0b"]) { exit(11); }
if (php_null(null) !== null || php_void() !== null || php_default() !== "default\0bytes") { exit(12); }
if (php_double(1.25) !== 2.5 || php_invert(true) !== false || php_invert(false) !== true) { exit(17); }
if (php_nullable(null) !== null || php_nullable(PHP_INT_MAX - 1) !== PHP_INT_MAX - 1) { exit(18); }
if (php_false(false) !== false || php_empty() !== []) { exit(19); }
if (php_float_default() !== 1.0 || (new ReflectionFunction('php_float_default'))->getParameters()[0]->getDefaultValue() !== 1.0) { exit(20); }
try { php_scalars('5', 1.0, true); exit(13); } catch (TypeError $e) {}
$function = new ReflectionFunction('php_scalars');
if ($function->getNumberOfRequiredParameters() !== 3 || $function->getParameters()[3]->getDefaultValue() !== null) { exit(14); }
if ((string)(new ReflectionFunction('php_void'))->getReturnType() !== 'void') { exit(15); }
if ((new ReflectionFunction('php_default'))->getParameters()[0]->getDefaultValue() !== "default\0bytes") { exit(16); }
echo 'ok';
"#]).output().unwrap();
    assert!(result.status.success(), "{:?}\n{}\n{}", result.status,
        String::from_utf8_lossy(&result.stdout), String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, b"ok");
    assert!(result.stderr.is_empty());
    let weak = php(&sdk, &module).args(["-r", r#"
if (php_double('1.25') !== 2.5 || php_invert(1) !== false || php_default(42) !== '42' || php_integer('42') !== 42) { exit(21); }
foreach (['php_double', 'php_invert', 'php_default', 'php_integer'] as $function) {
    try { $function(null); exit(22); } catch (TypeError $e) {}
}
try { php_scalars(null, 1.0, null); exit(23); } catch (TypeError $e) {}
echo 'ok';
"#]).output().unwrap();
    assert!(weak.status.success(), "{:?}: {}", weak.status, String::from_utf8_lossy(&weak.stderr));
    assert_eq!(weak.stdout, b"ok");
    assert!(weak.stderr.is_empty());
}

/// Failed partial imports roll back native allocation state and leave the next call usable.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_allocation_failure_recovers() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("allocation", r#"<?php
#[Export] function php_large_copy(array $value): array { return $value; }
"#);
    let result = php(&sdk, &module).args(["-r", r#"
$large = ['first' => str_repeat('x', 65536), 'second' => str_repeat('y', 1048576)];
for ($i = 0; $i < 32; $i++) {
    try { php_large_copy($large); exit(10); } catch (Error $e) {
        if (!str_contains($e->getMessage(), 'allocation failed')) { throw $e; }
    }
    if (php_large_copy(['ok' => 42]) !== ['ok' => 42]) { exit(11); }
}
echo 'ok';
"#]).output().unwrap();
    assert!(result.status.success(), "{:?}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, b"ok");
    assert!(result.stderr.is_empty());
}

/// Output callbacks observe the native state at echo and their exceptions stop the native call.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_output_preserves_callback_order_and_exceptions() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("ordering", r#"<?php
#[Export] function php_stage(int $mode): int {
    static $stage = 0;
    if ($mode === 1) { $stage = 1; echo 'chunk'; $stage = 2; }
    return $stage;
}
"#);
    let driver = r#"
ob_start(function ($text) { return $text === '' ? '' : $text . ':' . php_stage(0); }, 1);
$result = php_stage(1);
ob_end_flush();
echo '|', $result, '|';
ob_start(function ($text) { if ($text !== '') { throw new LogicException('callback'); } return ''; }, 1);
try { php_stage(1); exit(10); } catch (LogicException $e) {}
ob_end_clean();
if (php_stage(0) !== 1) { exit(11); }
echo 'recovered';
"#;
    let result = php(&sdk, &module).args(["-r", driver]).output().unwrap();
    let reference_driver = project.0.join("reference.php");
    fs::write(&reference_driver, format!("<?php require __DIR__ . '/ordering.php';\n{driver}")).unwrap();
    let reference = Command::new(sdk.php_binary()).arg("-n").arg(reference_driver).output().unwrap();
    assert!(reference.status.success(), "{}", String::from_utf8_lossy(&reference.stderr));

    assert!(result.status.success(), "{:?}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, reference.stdout);
    assert!(result.stderr.is_empty());
}

/// Independently generated modules hide their native runtimes and coexist in one host.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK and nm"]
fn compiled_php_modules_hide_native_symbols_and_coexist() {
    let project = Project::new();
    let sdk = project.sdk();
    let first = project.compile("first", "<?php #[Export] function php_first(string $value): string { return 'first:' . $value; }");
    let second = project.compile("second", "<?php #[Export] function php_second(string $value): string { return 'second:' . $value; }");
    for module in [&first, &second] {
        let mut command = Command::new("nm");
        if cfg!(target_os = "macos") { command.arg("-gU"); }
        else { command.args(["-D", "--defined-only"]); }
        let output = command.arg(module).output().unwrap();
        assert!(output.status.success());
        let symbols = String::from_utf8(output.stdout).unwrap();
        let names = symbols.lines().filter_map(|line| line.split_whitespace().last()).collect::<Vec<_>>();
        assert_eq!(names, [if cfg!(target_os = "macos") { "_get_module" } else { "get_module" }]);
    }
    let mut extension = std::ffi::OsString::from("extension=");
    extension.push(&second);
    let output = php(&sdk, &first).arg("-d").arg(extension).args(["-r", r#"
for ($i = 0; $i < 32; $i++) {
    if (php_first("a\0b") !== "first:a\0b" || php_second("a\0b") !== "second:a\0b") { exit(1); }
}
echo 'ok';
"#]).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    assert_eq!(output.stdout, b"ok");
    assert!(output.stderr.is_empty());
}

/// Native result packing is bounded before the transport crosses the PHP boundary.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_result_depth_is_bounded() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("depth", r#"<?php
#[Export] function php_deep(int $depth, mixed $value): mixed {
    for ($i = 0; $i < $depth; $i++) { $value = [$value]; }
    return $value;
}
"#);
    let result = php(&sdk, &module).args(["-r", r#"
for ($i = 0; $i < 16; $i++) {
    try { php_deep(500, 1); exit(10); } catch (Error $e) {
        if (!str_contains($e->getMessage(), 'nesting depth')) { throw $e; }
    }
    if (php_deep(2, 1) !== [[1]]) { exit(11); }
}
echo 'ok';
"#]).output().unwrap();
    assert!(result.status.success(), "{:?}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, b"ok");
    assert!(result.stderr.is_empty());
}

/// Native warnings execute PHP error handlers with safe reentry and contained exceptions.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn compiled_php_warnings_use_host_error_handlers() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.compile("warnings", r#"<?php
#[Export] function php_missing(array $value): mixed { return $value['missing']; }
#[Export] function php_warning_sum(int $a, int $b): int { return $a + $b; }
"#);
    let result = php(&sdk, &module).args(["-r", r#"
set_error_handler(function ($severity, $message) {
    if ($severity !== E_WARNING || !str_contains($message, 'missing')) { throw new Exception('wrong warning'); }
    echo 'warning:', php_warning_sum(20, 22);
    return true;
});
if (php_missing([]) !== null) { exit(10); }
restore_error_handler();
set_error_handler(function () { throw new LogicException('host warning'); });
try { php_missing([]); exit(11); } catch (LogicException $e) {}
restore_error_handler();
if (php_warning_sum(2, 3) !== 5) { exit(12); }
echo '|recovered';
"#]).output().unwrap();
    assert!(result.status.success(), "{:?}: {}", result.status, String::from_utf8_lossy(&result.stderr));
    assert_eq!(result.stdout, b"warning:42|recovered");
    assert!(result.stderr.is_empty());
}

/// Preserves a previous artifact and cleans compiler staging after a failed C adapter compilation.
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn failed_adapter_build_does_not_replace_existing_artifact() {
    let project = Project::new();
    let sdk = project.sdk();
    let module = project.0.join("previous.so");
    fs::write(&module, b"previous artifact").unwrap();
    let error = sdk.compile_extension("#error intentional adapter failure\n", &module).unwrap_err();
    assert!(error.contains("intentional adapter failure"), "{error}");
    assert_eq!(fs::read(&module).unwrap(), b"previous artifact");
    project.assert_clean();
}

/// Refuses a symlink artifact before the compiler can write through it.
#[cfg(unix)]
#[test]
#[ignore = "requires a real PHP 8.5 development SDK"]
fn adapter_build_refuses_symlink_destinations() {
    let project = Project::new();
    let sdk = project.sdk();
    let victim = project.0.join("victim");
    fs::write(&victim, b"unchanged").unwrap();
    let module = project.0.join("unsafe.so");
    std::os::unix::fs::symlink(&victim, &module).unwrap();
    let source = render_registration("elephc_symlink", "1.0").unwrap();
    assert!(sdk.compile_extension(&source, &module).unwrap_err().contains("unsafe"));
    assert_eq!(fs::read(&victim).unwrap(), b"unchanged");
    project.assert_clean();
}

/// Creates an interpreter command with an absolute module path and no ambient php.ini settings.
fn php(sdk: &PhpSdk, module: &Path) -> Command {
    let mut command = Command::new(sdk.php_binary());
    let mut extension = std::ffi::OsString::from("extension=");
    extension.push(module);
    command.args(["-n", "-d"]).arg(extension);
    command
}
