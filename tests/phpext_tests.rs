//! Purpose:
//! Real-PHP integration tests for producer SDK discovery and module registration.
//!
//! Called from:
//! - `cargo test --test phpext_tests -- --include-ignored` with a PHP 8.5 development SDK.
//!
//! Key details:
//! - ELEPHC_PHPEXT_PHP_CONFIG selects the SDK; the SDK selects the matching interpreter.
//! - Missing or incompatible SDKs fail explicitly when these tests are requested.
//! - Registration tests do not claim native invocation, request-heap isolation, or ZTS concurrency.
//! - Every fixture owns one private directory and removes it on success or panic.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use elephc::codegen_support::platform::Target;
use elephc::phpext::{render_registration, PhpSdk};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

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
