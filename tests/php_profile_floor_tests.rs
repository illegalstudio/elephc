//! Purpose:
//! End-to-end tests for the minimum-version check: `--php-version` naming a profile older
//! than the program's own syntax is rejected, and every way of NOT needing the newer profile
//! still compiles.
//!
//! Called from:
//! - `cargo test --test php_profile_floor_tests` through Rust's test harness.
//!
//! Key details:
//!
//! - WHAT THIS CLOSES. elephc's parser is version-agnostic by design: it accepts the whole
//!   language whatever `--php-version` says (`rg "php_version" src/parser/` finds nothing).
//!   Before this check, a file using PHP 8.4 property hooks compiled under
//!   `--php-version 8.2` and baked `PHP_VERSION = "8.2.0"` into the binary — a version its
//!   own source could never have run under. `hooks_under_82_are_rejected` is that exact
//!   program, and `hooks_under_84_compile` is the control proving the check is about the
//!   PROFILE and not about the construct being unsupported.
//!
//! - THE FALSE-POSITIVE TESTS ARE THE IMPORTANT ONES. This check REJECTS a compile, so a
//!   wrong answer breaks a working build of valid code. `function_exists_guard_compiles` and
//!   `polyfill_compiles` pin the two idioms a naive name match would wrongly reject, and
//!   `default_profile_never_rejects` pins the structural guarantee that a default build
//!   cannot be broken by this feature at all.
//!
//! - Host-target only, same harness style as `php_profile_independence_tests`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Resolves the elephc CLI binary path (cargo env var, fallback next to the test binary).
fn elephc_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source`, optionally at an explicit profile, and returns the raw process output.
fn compile(name: &str, source: &str, profile: Option<&str>) -> Output {
    let dir = make_test_dir(&format!("elephc_floor_{name}"));
    fs::write(dir.join("prog.php"), source).expect("failed to write program");
    let mut args: Vec<String> = Vec::new();
    if let Some(profile) = profile {
        args.push("--php-version".to_string());
        args.push(profile.to_string());
    }
    args.push("prog.php".to_string());
    let output = Command::new(elephc_bin())
        .args(&args)
        .current_dir(&dir)
        .output()
        .expect("failed to spawn elephc");
    let _ = fs::remove_dir_all(&dir);
    output
}

/// A class whose property uses PHP 8.4 hooks.
const HOOKS: &str = "<?php\nclass C {\n    public string $x { get => 'v'; }\n}\necho (new C())->x;\n";

/// PHP 8.4 property hooks cannot be built for the 8.2 profile.
#[test]
fn hooks_under_82_are_rejected() {
    let out = compile("hooks82", HOOKS, Some("8.2"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "compiling 8.4 syntax for the 8.2 profile must fail, stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("needs PHP 8.4") && stderr.contains("property hooks"),
        "the error must name the required profile and the construct, got:\n{stderr}"
    );
}

/// The same file builds for the profile that introduced the construct — the control proving
/// the rejection is about the PROFILE, not about hooks being unsupported.
#[test]
fn hooks_under_84_compile() {
    let out = compile("hooks84", HOOKS, Some("8.4"));
    assert!(
        out.status.success(),
        "8.4 syntax must compile for the 8.4 profile, stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A default build can never be rejected: the default is the newest maintained profile and
/// the floor can never exceed it. This is a structural guarantee, so it is worth a test.
#[test]
fn default_profile_never_rejects() {
    let out = compile("hooksdefault", HOOKS, None);
    assert!(
        out.status.success(),
        "a default build must never be rejected by the floor check, stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// `function_exists()` around a newer function is the idiom for STAYING portable, so it must
/// not be read as requiring the newer profile.
#[test]
fn function_exists_guard_compiles() {
    let out = compile(
        "guard",
        "<?php\nif (function_exists('json_validate')) {\n    echo \"has it\";\n} else {\n    echo \"no\";\n}\n",
        Some("8.2"),
    );
    assert!(
        out.status.success(),
        "a function_exists guard must not be read as a requirement, stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// The guarded polyfill — the real portability idiom — compiles at the older profile.
///
/// The guard is inert inside elephc (these builtins exist at every profile, so
/// `function_exists` is always true and the inner declaration is dead), but rejecting this
/// program would mean rejecting the canonical way to stay compatible with older PHP. Note
/// that an UNGUARDED redeclaration is a different matter: elephc refuses to redeclare a
/// built-in function outright, with its own more specific error.
#[test]
fn guarded_polyfill_compiles() {
    let out = compile(
        "polyfill",
        "<?php\nif (!function_exists('json_validate')) {\n    function json_validate(string $json): bool { return $json !== ''; }\n}\nvar_dump(json_validate('{}'));\n",
        Some("8.2"),
    );
    assert!(
        out.status.success(),
        "a guarded polyfill must suppress the requirement, stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Actually CALLING an 8.3 function is rejected for the 8.2 profile, and the error names it.
#[test]
fn new_function_call_under_82_is_rejected() {
    let out = compile("newfn", "<?php\nvar_dump(json_validate('{}'));\n", Some("8.2"));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !out.status.success(),
        "calling an 8.3 function for the 8.2 profile must fail, stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("needs PHP 8.3") && stderr.contains("json_validate"),
        "the error must name the required profile and the function, got:\n{stderr}"
    );
}

/// Calling PHP 8.4's `array_first()` / `array_last()` is rejected for the 8.3 profile and
/// names the function, while the 8.4 profile builds the same program — the gating
/// `array_find` already has.
#[test]
fn array_first_and_last_require_84() {
    for name in ["array_first", "array_last"] {
        let source = format!("<?php\nvar_dump({name}([1, 2]));\n");
        let out = compile(&format!("{name}83"), &source, Some("8.3"));
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !out.status.success(),
            "calling {name} for the 8.3 profile must fail, stderr:\n{stderr}"
        );
        assert!(
            stderr.contains("needs PHP 8.4") && stderr.contains(name),
            "the error must name the required profile and {name}, got:\n{stderr}"
        );
        let out = compile(&format!("{name}84"), &source, Some("8.4"));
        assert!(
            out.status.success(),
            "{name} must compile for the 8.4 profile, stderr:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

/// An ordinary program is untouched at the oldest profile.
#[test]
fn plain_program_compiles_at_the_oldest_profile() {
    let out = compile(
        "plain",
        "<?php\n$a = [3, 1, 2];\nsort($a);\necho implode(',', $a);\n",
        Some("8.2"),
    );
    assert!(
        out.status.success(),
        "a plain program must compile at 8.2, stderr:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
