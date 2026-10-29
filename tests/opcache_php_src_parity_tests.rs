//! Purpose:
//! Parity pins for php-src `ext/opcache` API tests that elephc already satisfies but did not
//! previously pin, derived from the maintained `PHP-8.5` branch of `php/php-src`.
//!
//! Called from:
//! - `cargo test --test opcache_php_src_parity_tests` through Rust's test harness.
//!
//! Key details:
//! - Each test names the php-src `.phpt` it mirrors so the source of truth stays traceable.
//! - The harness mirrors the sibling `opcache_*_tests.rs` files: the elephc CLI is spawned as
//!   a subprocess in an isolated temp dir. Host-target only.
//! - These are REGRESSION pins, not new behavior: every expectation was re-verified against
//!   `php -d xdebug.mode=off` on 8.5.10 before being written.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes, returned
/// CANONICALIZED so its paths match the spelling elephc bakes for `__FILE__`.
fn make_test_dir(prefix: &str) -> PathBuf {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{}_{}_{:?}_{}", prefix, pid, tid, id));
    fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
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

/// Compiles a single-file fixture with the supplied `--ini` assignments, asserting success,
/// and returns the executable path.
fn compile(dir: &Path, stem: &str, ini: &[&str]) -> PathBuf {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(dir.join(format!("{}.php", stem)));
    for assignment in ini {
        cmd.arg("--ini").arg(assignment);
    }
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "compilation failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    dir.join(stem)
}

/// Runs a compiled binary and returns the raw [`Output`] (stdout, stderr and status).
fn run(bin: &Path) -> Output {
    Command::new(bin).output().expect("failed to run binary")
}

/// Asserts stdout equals `expected`, reporting stderr on mismatch.
fn assert_stdout(output: &Output, expected: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        stdout.as_ref(),
        expected,
        "stdout mismatch:\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Mirrors php-src `ext/opcache/tests/bug69281.phpt`: `opcache_is_script_cached()` answers the
/// same under `opcache.validate_timestamps=0` as under `1` — a manifest member is cached, a
/// nonexistent path is not. The directive governs revalidation, not manifest membership.
#[test]
fn bug69281_is_script_cached_with_validate_timestamps_disabled() {
    let dir = make_test_dir("opcache_parity_bug69281");
    fs::write(
        dir.join("main.php"),
        "<?php\nvar_dump(opcache_is_script_cached(__FILE__));\nvar_dump(opcache_is_script_cached(\"nonexistent.php\"));\n",
    )
    .unwrap();
    let bin = compile(
        &dir,
        "main",
        &[
            "opcache.enable=1",
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.validate_timestamps=0",
            "opcache.file_cache_only=0",
        ],
    );

    assert_stdout(&run(&bin), "bool(true)\nbool(false)\n");
}

/// Mirrors php-src `ext/opcache/tests/issue0128.phpt`: `opcache_invalidate('1')` returns `false`,
/// because a bare relative name does not resolve to a path (`php-src`'s `zend_accel_invalidate`
/// answers "cached or resolvable"). The call must not crash or abort the program.
#[test]
fn issue0128_invalidate_of_a_bare_relative_name_is_false() {
    let dir = make_test_dir("opcache_parity_issue0128");
    fs::write(
        dir.join("main.php"),
        "<?php\nvar_dump(opcache_invalidate('1'));\nvar_dump(\"okey\");\n",
    )
    .unwrap();
    let bin = compile(
        &dir,
        "main",
        &[
            "opcache.enable=1",
            "opcache.enable_cli=1",
            "opcache.optimization_level=-1",
        ],
    );

    assert_stdout(&run(&bin), "bool(false)\nstring(4) \"okey\"\n");
}

/// Mirrors php-src `ext/opcache/tests/get_configuration_matches_ini.phpt`:
/// `opcache_get_configuration()['directives']` is a superset of `ini_get_all('zend opcache')`.
/// The probe compares key membership directly rather than through `array_diff_key`, which the
/// EIR backend does not implement for a `Mixed` first argument.
#[test]
fn get_configuration_matches_ini_lists_every_directive() {
    let dir = make_test_dir("opcache_parity_get_config");
    fs::write(
        dir.join("main.php"),
        "<?php\n$config = opcache_get_configuration();\n$inis = ini_get_all('zend opcache');\n$missing = 0;\nforeach ($inis as $k => $v) { if (!isset($config['directives'][$k])) { $missing++; } }\necho 'missing=', $missing, \"\\n\";\n",
    )
    .unwrap();
    let bin = compile(
        &dir,
        "main",
        &[
            "opcache.enable=1",
            "opcache.enable_cli=1",
            "opcache.opt_debug_level=0",
        ],
    );

    assert_stdout(&run(&bin), "missing=0\n");
}

/// Mirrors php-src `ext/opcache/tests/gh11715.phpt`: with `opcache.interned_strings_buffer=16`
/// the reported `buffer_size` is 16 MiB and `used_memory + free_memory` equals it exactly.
#[test]
fn gh11715_interned_buffer_reports_configured_bytes_and_sum() {
    let dir = make_test_dir("opcache_parity_gh11715");
    fs::write(
        dir.join("main.php"),
        "<?php\n$i = opcache_get_status()['interned_strings_usage'];\nvar_dump($i['buffer_size']);\nvar_dump($i['used_memory'] + $i['free_memory']);\n",
    )
    .unwrap();
    let bin = compile(
        &dir,
        "main",
        &[
            "opcache.enable=1",
            "opcache.enable_cli=1",
            "opcache.interned_strings_buffer=16",
        ],
    );

    assert_stdout(&run(&bin), "int(16777216)\nint(16777216)\n");
}

/// Mirrors php-src `ext/opcache/tests/bug78429.phpt`: with the cache disabled
/// (`opcache.enable_cli=0`), `opcache_compile_file()` writes php-src's notice and returns
/// `false`. elephc omits the ` in <file> on line <n>` suffix, a documented diagnostic
/// divergence, so the assertion is on the message body.
#[test]
fn bug78429_disabled_compile_file_notices_and_returns_false() {
    let dir = make_test_dir("opcache_parity_bug78429");
    fs::write(
        dir.join("main.php"),
        "<?php\nvar_dump(opcache_compile_file(__FILE__));\n",
    )
    .unwrap();
    let bin = compile(&dir, "main", &["opcache.enable_cli=0"]);

    let output = run(&bin);
    assert_stdout(&output, "bool(false)\n");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Notice: Zend OPcache has not been properly started, can't compile file"),
        "the disabled-cache notice must be emitted:\nstderr: {stderr:?}"
    );
}

/// Mirrors php-src `ext/opcache/tests/log_verbosity_bug.phpt`: an `ACCEL_LOG_FATAL` exits the
/// process even when the gate suppresses the line. The sibling
/// `opcache_file_cache_tests::a_fatal_survives_verbosity_zero` pins verbosity `0` (line printed);
/// this pins `-1`, where `0 <= -1` is false, so nothing is printed but the exit status is still
/// 254 and the program never runs.
#[test]
fn log_verbosity_bug_fatal_exits_at_minus_one_without_a_line() {
    let dir = make_test_dir("opcache_parity_logverbosity");
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\neval('include __DIR__ . \"/lib.php\";');\necho \"RAN\\n\";\n",
    )
    .unwrap();
    let bin = compile(
        &dir,
        "main",
        &[
            "opcache.enable=1",
            "opcache.enable_cli=1",
            "opcache.log_verbosity_level=-1",
            "opcache.file_cache=/no/such/directory",
        ],
    );

    let output = run(&bin);
    assert_eq!(
        output.status.code(),
        Some(254),
        "the accelerator fatal must exit with 254:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "the fatal line is suppressed at verbosity -1 and the program must not run:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}
