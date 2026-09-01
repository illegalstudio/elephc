//! Purpose:
//! End-to-end tests for PHP's version surface: the `PHP_VERSION*` / `PHP_SAPI` constants,
//! `phpversion()` / `phpversion($extension)`, `zend_version()`, `php_sapi_name()` and
//! `ini_restore()` — across `--php-version` profiles and in both CLI and `--web` mode.
//!
//! Called from:
//! - `cargo test --test php_version_surface_tests` through Rust's test harness.
//!
//! Key details:
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   compile a plain executable, run it, and assert stdout — the same harness style as
//!   `function_exists_tests` / `extension_loaded_tests` / `opcache_ini_tests`. Host-target only
//!   (macOS aarch64 local).
//!
//! - THE VERSION RULE UNDER TEST. elephc targets a PHP LANGUAGE PROFILE selected by
//!   `--php-version` (8.2/8.3/8.4/8.5, default 8.5). The default is pinned to the frozen
//!   php-src oracle (`8.5.10-dev` / `80510`); older profiles retain `8.<minor>.0` values.
//!   `PHP_VERSION_ID` uses the reference `major * 10000 + minor * 100 + release` formula.
//!
//! - WHERE REFERENCE IS MATCHED EXACTLY (values captured from the frozen php-src oracle):
//!   `PHP_EXTRA_VERSION` is `"-dev"`; `PHP_MAJOR_VERSION` is `8`; `PHP_SAPI` is `cli` for a CLI
//!   binary; `phpversion($unknown)` is `false`; extension-name matching is case-insensitive
//!   (`phpversion('core') === phpversion('Core')`); every bundled extension reports the
//!   interpreter's own version (`Core`, `json`, `pcre`, `Zend OPcache`, … all `8.5.10-dev`);
//!   `ini_restore()` returns `NULL`.
//!
//! - WHERE ELEPHC DELIBERATELY DIVERGES: `PHP_SAPI` under `--web`, which is
//!   `cli-server` — elephc's `--web` binary embeds its own HTTP listener with no external
//!   server, which is exactly what reference's built-in server is, and it is the only reference
//!   SAPI name that describes a standalone PHP binary speaking HTTP.
//!
//! - Compile-failure assertions filter stderr through `elephc_diagnostics` because the system
//!   linker (GNU `ld` on Linux) emits warnings macOS does not.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

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

/// Runs the compiler on `source` with extra flags and returns its raw output.
fn compile_raw(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> std::process::Output {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(flags).arg(&php);
    cmd.output().expect("failed to spawn elephc")
}

/// Compiles `source` to a plain executable with extra compiler flags and returns its path.
fn compile_with_flags(dir: &Path, source: &str, stem: &str, flags: &[&str]) -> PathBuf {
    let output = compile_raw(dir, source, stem, flags);
    assert_successful_compile(dir, stem, output)
}

/// Requires a successful compile and resolves the produced executable path.
fn assert_successful_compile(
    dir: &Path,
    stem: &str,
    output: std::process::Output,
) -> PathBuf {
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Runs a compiled executable and returns its stdout as a string.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin)
        .output()
        .expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Compiles and runs `source` for one `--php-version` profile, returning stdout.
fn run_for_profile(prefix: &str, source: &str, profile: &str) -> String {
    let dir = make_test_dir(prefix);
    let bin = compile_with_flags(&dir, source, "probe", &["--php-version", profile]);
    run_binary(&bin)
}

/// Keeps only elephc's own diagnostics so linker chatter (GNU `ld` on Linux emits warnings
/// macOS does not) cannot make a stderr assertion platform-dependent.
///
/// elephc emits `Warning: …` for the INI-override diagnostics (`src/main.rs`) and
/// `warning: …` / `warning[line:col]: …` / `error[line:col]: …` for compile diagnostics
/// (`src/errors/report.rs`).
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
                || line.starts_with("error:")
                || line.starts_with("error[")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Picks an ephemeral localhost port by binding :0 and releasing it.
fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// Spawns a `--web` binary on `addr` with one worker and blocks until it accepts connections.
///
/// Both output streams go to `/dev/null`: the server is a prefork parent, so an inherited pipe
/// would be held open by the worker and could outlive the test.
fn spawn_server(bin: &Path, addr: &str) -> std::process::Child {
    let child = Command::new(bin)
        .arg("--listen")
        .arg(addr)
        .arg("--workers")
        .arg("1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn web server");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(addr).is_ok() {
            return child;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("server did not start listening on {}", addr);
}

/// Stops a spawned server and every prefork worker it left behind.
///
/// `Child::kill()` only signals the parent; the forked worker keeps the listening socket alive
/// and is reparented, so the workers are matched by the `--listen <addr>` argument. The
/// ephemeral port makes the pattern unique to this test, and the leading `--` is omitted
/// because `pkill` would parse it as one of its own options.
fn stop_server(server: &mut std::process::Child, addr: &str) {
    let _ = server.kill();
    let _ = server.wait();
    let _ = Command::new("pkill")
        .arg("-f")
        .arg(format!("listen {}", addr))
        .status();
}

/// Sends one HTTP/1.1 GET and returns the response with any complete chunked body decoded.
fn http_get(addr: &str, path: &str) -> String {
    let mut stream = TcpStream::connect(addr).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        path, addr
    );
    stream.write_all(request.as_bytes()).unwrap();
    let mut response = Vec::with_capacity(4096);
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => response.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    normalize_complete_http_response(String::from_utf8_lossy(&response).into_owned())
}

/// Decodes a complete chunked response body while preserving the response headers.
fn normalize_complete_http_response(response: String) -> String {
    let Some((headers, body)) = response.split_once("\r\n\r\n") else {
        return response;
    };
    let is_chunked = headers.lines().any(|line| {
        let Some((name, value)) = line.split_once(':') else {
            return false;
        };
        name.eq_ignore_ascii_case("transfer-encoding")
            && value
                .split(',')
                .any(|encoding| encoding.trim().eq_ignore_ascii_case("chunked"))
    });
    if !is_chunked {
        return response;
    }
    let Some(decoded) = decode_complete_chunked_body(body.as_bytes()) else {
        return response;
    };
    format!("{headers}\r\n\r\n{}", String::from_utf8_lossy(&decoded))
}

/// Decodes one complete HTTP chunk stream and rejects truncated or malformed framing.
fn decode_complete_chunked_body(mut body: &[u8]) -> Option<Vec<u8>> {
    let mut decoded = Vec::new();
    loop {
        let size_end = body.windows(2).position(|window| window == b"\r\n")?;
        let size_line = std::str::from_utf8(&body[..size_end]).ok()?;
        let size_text = size_line.split(';').next()?.trim();
        let size = usize::from_str_radix(size_text, 16).ok()?;
        body = &body[size_end + 2..];
        if size == 0 {
            return body.starts_with(b"\r\n").then_some(decoded);
        }
        let chunk = body.get(..size)?;
        body = body.get(size..)?;
        if !body.starts_with(b"\r\n") {
            return None;
        }
        decoded.extend_from_slice(chunk);
        body = &body[2..];
    }
}

/// The probe every constant test uses: one pipe-separated line per surface.
const CONSTANTS_PROBE: &str = r#"<?php
echo PHP_VERSION, "|", PHP_VERSION_ID, "|", PHP_MAJOR_VERSION, "|", PHP_MINOR_VERSION,
     "|", PHP_RELEASE_VERSION, "|", PHP_EXTRA_VERSION, "|", PHP_SAPI, "\n";
"#;

/// Every constant reports the compile target's profile, for every maintained `--php-version`.
///
/// The default profile mirrors the frozen php-src snapshot, including its development suffix.
#[test]
fn version_constants_follow_the_compile_target_profile() {
    for (profile, expected) in [
        ("8.2", "8.2.0|80200|8|2|0||cli\n"),
        ("8.3", "8.3.0|80300|8|3|0||cli\n"),
        ("8.4", "8.4.0|80400|8|4|0||cli\n"),
        ("8.5", "8.5.10-dev|80510|8|5|10|-dev|cli\n"),
    ] {
        let out = run_for_profile("elephc_version_consts", CONSTANTS_PROBE, profile);
        assert_eq!(out, expected, "--php-version {profile}");
    }
}

/// The default profile is 8.5, so a flagless compile answers exactly like `--php-version 8.5`.
#[test]
fn version_constants_default_to_the_newest_profile() {
    let dir = make_test_dir("elephc_version_default");
    let bin = compile_with_flags(&dir, CONSTANTS_PROBE, "probe", &[]);
    assert_eq!(run_binary(&bin), "8.5.10-dev|80510|8|5|10|-dev|cli\n");
}

/// The constants must never contradict each other inside one binary.
///
/// This is the guard against the failure mode the whole design exists to avoid: a binary
/// reporting a version string whose components disagree with `PHP_VERSION_ID`. The formula is
/// reference PHP's, and the string
/// equality asserts the components spell out the reported version string. Both are asserted
/// INSIDE the compiled program, so the check runs against the baked literals.
#[test]
fn constants_are_internally_consistent() {
    let source = r#"<?php
$id = PHP_MAJOR_VERSION * 10000 + PHP_MINOR_VERSION * 100 + PHP_RELEASE_VERSION;
echo $id === PHP_VERSION_ID ? "id-ok" : "id-BAD", "\n";
$spelled = PHP_MAJOR_VERSION . "." . PHP_MINOR_VERSION . "." . PHP_RELEASE_VERSION . PHP_EXTRA_VERSION;
echo $spelled === PHP_VERSION ? "string-ok" : "string-BAD", "\n";
echo phpversion() === PHP_VERSION ? "phpversion-ok" : "phpversion-BAD", "\n";
echo php_sapi_name() === PHP_SAPI ? "sapi-ok" : "sapi-BAD", "\n";
"#;
    for profile in ["8.2", "8.5"] {
        let out = run_for_profile("elephc_version_consistent", source, profile);
        assert_eq!(
            out, "id-ok\nstring-ok\nphpversion-ok\nsapi-ok\n",
            "--php-version {profile}",
        );
    }
}

/// `PHP_VERSION_ID` supports the feature-detection comparisons real code writes.
///
/// This is the surface's actual job: `PHP_VERSION_ID >= 80300` must answer the same question
/// `--php-version` answers. Reference semantics are matched exactly here — the comparison is
/// against a profile boundary, which is where elephc's `.0` patch is indistinguishable from
/// reference's.
#[test]
fn version_id_answers_feature_detection_comparisons() {
    let source = r#"<?php
echo PHP_VERSION_ID >= 80200 ? "1" : "0";
echo PHP_VERSION_ID >= 80300 ? "1" : "0";
echo PHP_VERSION_ID >= 80400 ? "1" : "0";
echo PHP_VERSION_ID >= 80500 ? "1" : "0";
echo "\n";
"#;
    assert_eq!(run_for_profile("elephc_version_cmp", source, "8.2"), "1000\n");
    assert_eq!(run_for_profile("elephc_version_cmp", source, "8.5"), "1111\n");
}

/// The constants resolve inside a namespace, unqualified and fully qualified, and are `defined()`.
///
/// Reference PHP locks these as global core constants; elephc routes them through
/// `name_resolver::names::is_builtin_global_constant`, the same path `PHP_OS` takes, so a
/// namespaced file sees them without importing anything.
#[test]
fn constants_resolve_inside_a_namespace() {
    let source = r#"<?php
namespace App;
echo PHP_VERSION, "|", \PHP_VERSION_ID, "|", PHP_SAPI, "\n";
var_dump(defined('PHP_VERSION'), defined('PHP_SAPI'), defined('PHP_NOT_A_CONSTANT'));
"#;
    let out = run_for_profile("elephc_version_ns", source, "8.5");
    assert_eq!(
        out,
        "8.5.10-dev|80510|cli\nbool(true)\nbool(true)\nbool(false)\n"
    );
}

/// `PHP_OS` still reports the target platform and is unaffected by the version surface.
///
/// REGRESSION ANCHOR: the version constants were added through `PHP_OS`'s exact mechanism
/// (checker constant table + `is_builtin_global_constant` + `prescan::collect_constants`), so
/// this asserts the model was extended and not disturbed.
#[test]
fn php_os_is_not_regressed() {
    let source = r#"<?php
echo PHP_OS, "|", PHP_OS === "Darwin" || PHP_OS === "Linux" ? "known" : "BAD", "\n";
"#;
    let out = run_for_profile("elephc_version_os", source, "8.5");
    assert!(
        out == "Darwin|known\n" || out == "Linux|known\n",
        "unexpected PHP_OS line: {out:?}",
    );
}

/// `phpversion()` reports the PHP language version, not elephc's own package version.
///
/// REGRESSION ANCHOR: `phpversion()` used to return the COMPILER's version (`0.26.2`), which is
/// the bug this test pins shut. The default value is the frozen php-src oracle version.
#[test]
fn phpversion_reports_the_language_version_not_the_compiler_version() {
    let source = r#"<?php
echo phpversion(), "\n";
"#;
    assert_eq!(run_for_profile("elephc_pv", source, "8.2"), "8.2.0\n");
    assert_eq!(run_for_profile("elephc_pv", source, "8.5"), "8.5.10-dev\n");
}

/// `phpversion($extension)` answers `string|false` for literal names, case-insensitively.
///
/// Every bundled extension reports the interpreter's own version, while an unknown extension
/// returns `false`.
///
/// REGRESSION ANCHOR: `phpversion($extension)` used to be rejected at compile time with
/// "phpversion() takes no arguments".
#[test]
fn phpversion_with_a_literal_extension_matches_reference_shape() {
    let source = r#"<?php
var_dump(phpversion("json"));
var_dump(phpversion("Core"));
var_dump(phpversion("core"));
var_dump(phpversion("Zend OPcache"));
var_dump(phpversion("zend opcache"));
var_dump(phpversion("nope_xyz"));
var_dump(phpversion(""));
"#;
    let out = run_for_profile("elephc_pv_ext", source, "8.5");
    assert_eq!(
        out,
        concat!(
            "string(10) \"8.5.10-dev\"\n",
            "string(10) \"8.5.10-dev\"\n",
            "string(10) \"8.5.10-dev\"\n",
            "string(10) \"8.5.10-dev\"\n",
            "string(10) \"8.5.10-dev\"\n",
            "bool(false)\n",
            "bool(false)\n",
        )
    );
}

/// `phpversion($extension)` follows the compile target, like the zero-argument form.
#[test]
fn phpversion_with_an_extension_follows_the_profile() {
    let source = r#"<?php
var_dump(phpversion("json"));
"#;
    assert_eq!(
        run_for_profile("elephc_pv_ext_82", source, "8.2"),
        "string(5) \"8.2.0\"\n"
    );
}

/// `phpversion($e) !== false` and `extension_loaded($e)` answer over the SAME set.
///
/// The two must never disagree — that is why `lower_phpversion` reuses `extension_is_loaded`
/// rather than carrying its own table. The probe covers a non-literal (`foreach`) argument, the
/// always-present core set, a BRIDGE that is only linked because the program uses it (`hash`,
/// auto-detected from the `hash()` call at the bottom), a bridge that is NOT linked (`PDO`), and
/// an unknown name. Reference PHP would report `hash` and `PDO` as loaded; elephc reports what
/// this binary actually links, which is the same rule `extension_loaded` already documents.
#[test]
fn phpversion_and_extension_loaded_agree_over_the_same_set() {
    let source = r#"<?php
foreach (["json", "Core", "ZEND OPCACHE", "hash", "PDO", "nope_xyz"] as $name) {
    $version = phpversion($name);
    $loaded = extension_loaded($name);
    echo $name, "|", $loaded ? "loaded" : "absent", "|", var_export($version, true),
         "|", ($version !== false) === $loaded ? "agree" : "DISAGREE", "\n";
}
echo hash("md5", "x"), "\n";
"#;
    let out = run_for_profile("elephc_pv_agree", source, "8.5");
    assert_eq!(
        out,
        concat!(
            "json|loaded|'8.5.10-dev'|agree\n",
            "Core|loaded|'8.5.10-dev'|agree\n",
            "ZEND OPCACHE|loaded|'8.5.10-dev'|agree\n",
            "hash|loaded|'8.5.10-dev'|agree\n",
            "PDO|absent|false|agree\n",
            "nope_xyz|absent|false|agree\n",
            "9dd4e461268c8034f5c8564e155c67a6\n",
        )
    );
}

/// A non-literal `phpversion($name)` compiles and answers like the literal fold.
///
/// The dynamic path is a baked candidate table scanned with `__rt_strcasecmp` (the same shape
/// `extension_loaded` uses), so a drift between the two decision points would show up here as a
/// differing pair rather than a crash.
#[test]
fn dynamic_phpversion_matches_the_literal_fold() {
    let source = r#"<?php
$known = "json";
$unknown = "nope_xyz";
$mixedCase = "JSON";
echo phpversion($known) === phpversion("json") ? "known-ok" : "known-BAD", "\n";
echo phpversion($unknown) === phpversion("nope_xyz") ? "unknown-ok" : "unknown-BAD", "\n";
echo phpversion($mixedCase) === phpversion("json") ? "case-ok" : "case-BAD", "\n";
var_dump(phpversion($unknown));
"#;
    let out = run_for_profile("elephc_pv_dyn", source, "8.5");
    assert_eq!(out, "known-ok\nunknown-ok\ncase-ok\nbool(false)\n");
}

/// A non-string `phpversion()` argument and a wrong arity are rejected at compile time.
///
/// Reference PHP raises `TypeError` / `ArgumentCountError` at runtime; elephc rejects both
/// statically, which is strictly earlier. stderr is filtered to elephc's own diagnostics so the
/// host linker cannot influence the assertion.
#[test]
fn phpversion_rejects_bad_arguments_at_compile_time() {
    let dir = make_test_dir("elephc_pv_bad");
    let too_many = compile_raw(&dir, "<?php phpversion(\"a\", \"b\");", "too_many", &[]);
    assert!(!too_many.status.success());
    assert!(
        elephc_diagnostics(&String::from_utf8_lossy(&too_many.stderr))
            .contains("phpversion() takes 0 or 1 arguments"),
        "unexpected diagnostics: {}",
        String::from_utf8_lossy(&too_many.stderr),
    );

    let wrong_type = compile_raw(&dir, "<?php phpversion(1);", "wrong_type", &[]);
    assert!(!wrong_type.status.success());
    assert!(
        elephc_diagnostics(&String::from_utf8_lossy(&wrong_type.stderr))
            .contains("phpversion() extension argument must be string"),
        "unexpected diagnostics: {}",
        String::from_utf8_lossy(&wrong_type.stderr),
    );
}

/// `zend_version()` reports the Zend Engine track for the compile target.
///
/// The default profile mirrors the oracle's `4.5.10-dev`; older profiles retain `.0`.
#[test]
fn zend_version_tracks_the_profile() {
    let source = r#"<?php
echo zend_version(), "\n";
"#;
    assert_eq!(run_for_profile("elephc_zend", source, "8.2"), "4.2.0\n");
    assert_eq!(run_for_profile("elephc_zend", source, "8.3"), "4.3.0\n");
    assert_eq!(run_for_profile("elephc_zend", source, "8.4"), "4.4.0\n");
    assert_eq!(run_for_profile("elephc_zend", source, "8.5"), "4.5.10-dev\n");
}

/// `php_sapi_name()` reports `cli` for a plain binary — exactly as reference PHP does.
#[test]
fn php_sapi_name_reports_cli_for_a_plain_binary() {
    let source = r#"<?php
echo php_sapi_name(), "|", PHP_SAPI, "\n";
var_dump(php_sapi_name() === "cli");
"#;
    let out = run_for_profile("elephc_sapi_cli", source, "8.5");
    assert_eq!(out, "cli|cli\nbool(true)\n");
}

/// `ini_restore()` is a no-op that returns `void`, and leaves `ini_get()` untouched.
///
/// Reference PHP 8.5.6 verified: `var_dump(ini_restore('precision'))` prints `NULL`. In elephc
/// every INI value is baked at compile time and `ini_set()` already reports failure for every
/// key, so a directive is always already at its startup value — which makes "restore to the
/// startup value" exactly a no-op rather than an approximation. The test proves the *observable*
/// part: the value before and after is identical, for a real `opcache.*` directive and for an
/// unknown key.
#[test]
fn ini_restore_is_a_no_op_returning_void() {
    let source = r#"<?php
$before = ini_get('opcache.enable');
var_dump(ini_restore('opcache.enable'));
$after = ini_get('opcache.enable');
echo $before === $after ? "unchanged" : "CHANGED", "\n";
var_dump($after);
var_dump(ini_restore('not.a.real.directive'));
var_dump(ini_get('not.a.real.directive'));
"#;
    let out = run_for_profile("elephc_ini_restore", source, "8.5");
    assert_eq!(
        out,
        concat!(
            "NULL\n",
            "unchanged\n",
            "string(1) \"1\"\n",
            "NULL\n",
            "bool(false)\n",
        )
    );
}

/// `ini_restore` is consistent with `ini_set`: neither can move a compiled directive.
///
/// `ini_set()` returns `false` for every key (documented in `opcache_prelude`), and
/// `ini_restore()` therefore has nothing to undo. This asserts the pair together so the two
/// halves of the "INI is compile-time baked" story cannot drift apart.
#[test]
fn ini_restore_is_consistent_with_ini_set() {
    let source = r#"<?php
$before = ini_get('opcache.enable');
var_dump(ini_set('opcache.enable', '0'));
echo ini_get('opcache.enable') === $before ? "set-unchanged" : "set-CHANGED", "\n";
ini_restore('opcache.enable');
echo ini_get('opcache.enable') === $before ? "restore-unchanged" : "restore-CHANGED", "\n";
"#;
    let out = run_for_profile("elephc_ini_pair", source, "8.5");
    assert_eq!(out, "bool(false)\nset-unchanged\nrestore-unchanged\n");
}

/// The version-surface functions are declared, so `function_exists()` reports them.
///
/// They are injected PHP declarations rather than builtins (the same shape `ini_get` /
/// `ini_set` / `ini_get_all` use on a CLI binary), and being real declarations is what makes
/// this true.
#[test]
fn version_surface_functions_are_reported_by_function_exists() {
    let source = r#"<?php
var_dump(function_exists('zend_version'));
var_dump(function_exists('php_sapi_name'));
var_dump(function_exists('ini_restore'));
var_dump(function_exists('phpversion'));
echo zend_version(), php_sapi_name(), "\n";
ini_restore('x');
"#;
    let out = run_for_profile("elephc_fx", source, "8.5");
    assert_eq!(
        out,
        "bool(true)\nbool(true)\nbool(true)\nbool(true)\n4.5.10-devcli\n"
    );
}

/// A program that never mentions them carries none of the injected declarations.
///
/// Pay-for-use: an unrelated binary must not grow three functions it never asked for. The names
/// are ASSEMBLED AT RUNTIME rather than written as literals, because the detector deliberately
/// treats a matching string literal as a reference (that is what makes
/// `function_exists('zend_version')` and `call_user_func('zend_version')` work — see
/// `opcache_prelude::detect`). Concatenation is the only way to ask the question without
/// answering it, and it exercises the dynamic `function_exists` table, which reflects exactly
/// the declarations the binary carries.
#[test]
fn unreferenced_version_functions_are_not_injected() {
    let source = r#"<?php
$names = ['zend_' . 'version', 'php_sapi_' . 'name', 'ini_' . 'restore'];
foreach ($names as $name) {
    echo $name, "=", function_exists($name) ? "1" : "0", "\n";
}
"#;
    let out = run_for_profile("elephc_payperuse", source, "8.5");
    assert_eq!(
        out,
        "zend_version=0\nphp_sapi_name=0\nini_restore=0\n",
        "an unreferenced version-surface function must not be injected",
    );
}

/// A string-literal mention IS a reference, so the `function_exists` form still injects.
///
/// The mirror image of the previous test, and the reason it has to build its names at runtime:
/// the detector deliberately over-injects on a matching literal so that probing for a function
/// by name — `function_exists('zend_version')`, `call_user_func('zend_version')` — finds it.
#[test]
fn a_string_literal_mention_injects_the_function() {
    let source = r#"<?php
var_dump(function_exists('zend_version'));
echo call_user_func('zend_version'), "\n";
"#;
    let out = run_for_profile("elephc_literal_ref", source, "8.5");
    assert_eq!(out, "bool(true)\n4.5.10-dev\n");
}

/// A user declaration of `ini_restore` wins over the injected one.
#[test]
fn user_declaration_overrides_the_injected_function() {
    let source = r#"<?php
function ini_restore(string $option): void {
    echo "user:", $option, "\n";
}
ini_restore('precision');
"#;
    let out = run_for_profile("elephc_userdecl", source, "8.5");
    assert_eq!(out, "user:precision\n");
}

/// Under `--web`, `PHP_SAPI` and `php_sapi_name()` report `cli-server`.
///
/// elephc's DOCUMENTED choice, not reference's `cli` — a `--web` binary embeds its own HTTP
/// listener with no external server or module host, which is what reference's built-in server
/// (`php -S`, `cli-server`) is. It matters because library code gates on `PHP_SAPI === 'cli'`
/// to decide console-vs-request; reporting `cli` here would put every such library on the
/// console path inside an HTTP request. The rest of the version surface is mode-independent and
/// must read identically to the CLI build.
#[test]
fn web_mode_reports_cli_server_and_keeps_the_rest_of_the_surface() {
    let dir = make_test_dir("elephc_version_web");
    let source = r#"<?php
echo PHP_SAPI, "|", php_sapi_name(), "|", PHP_VERSION, "|", PHP_VERSION_ID,
     "|", zend_version(), "|", phpversion(), "\n";
var_dump(phpversion('json'), phpversion('nope_xyz'), ini_restore('precision'));
"#;
    let bin = compile_with_flags(&dir, source, "webprobe", &["--web"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&bin, &addr);
    let response = http_get(&addr, "/");
    stop_server(&mut server, &addr);

    assert!(
        response.contains("cli-server|cli-server|8.5.10-dev|80510|4.5.10-dev|8.5.10-dev\n"),
        "unexpected --web response:\n{response}",
    );
    assert!(
        response.contains("string(10) \"8.5.10-dev\"\nbool(false)\nNULL\n"),
        "unexpected --web response:\n{response}",
    );
}

/// `--web` honours `--php-version` for the whole version surface.
#[test]
fn web_mode_follows_the_compile_target_profile() {
    let dir = make_test_dir("elephc_version_web82");
    let source = r#"<?php
echo PHP_SAPI, "|", PHP_VERSION, "|", PHP_VERSION_ID, "|", zend_version(), "\n";
"#;
    let bin = compile_with_flags(&dir, source, "webprobe82", &["--web", "--php-version", "8.2"]);
    let addr = format!("127.0.0.1:{}", free_port());
    let mut server = spawn_server(&bin, &addr);
    let response = http_get(&addr, "/");
    stop_server(&mut server, &addr);

    assert!(
        response.contains("cli-server|8.2.0|80200|4.2.0\n"),
        "unexpected --web response:\n{response}",
    );
}

/// The OPcache version surface and the PHP version surface agree inside one binary.
///
/// This is the contract that keeps the two surfaces from ever splitting.
#[test]
fn opcache_version_and_php_version_agree() {
    let source = r#"<?php
$configuration = opcache_get_configuration();
$opcache = $configuration['version']['version'];
echo $opcache, "|", PHP_VERSION, "|", $opcache === PHP_VERSION ? "agree" : "DISAGREE", "\n";
"#;
    assert_eq!(
        run_for_profile("elephc_opcache_agree", source, "8.5"),
        "8.5.10-dev|8.5.10-dev|agree\n"
    );
    assert_eq!(
        run_for_profile("elephc_opcache_agree", source, "8.2"),
        "8.2.0|8.2.0|agree\n"
    );
}

/// `eval()` sees the same version surface the compiled code does on the default profile.
///
/// The eval interpreter is a separate crate that cannot read `--php-version` itself, so the
/// compiler forwards the profile to it through `__elephc_eval_set_php_version_id`. This pins
/// the default; `eval_follows_a_non_default_profile` pins that the forwarding is what makes it
/// true, rather than the bridge's own default happening to agree.
#[test]
fn eval_sees_the_same_version_surface_on_the_default_profile() {
    let source = r#"<?php
eval('echo PHP_VERSION, "|", PHP_VERSION_ID, "|", PHP_MAJOR_VERSION, "|", PHP_MINOR_VERSION,
      "|", PHP_RELEASE_VERSION, "|", PHP_EXTRA_VERSION, "|", PHP_SAPI, "|", phpversion(), "\n";
var_dump(phpversion("json"), phpversion("nope_xyz"));');
"#;
    let out = run_for_profile("elephc_eval_version", source, "8.5");
    assert_eq!(
        out,
        "8.5.10-dev|80510|8|5|10|-dev|cli|8.5.10-dev\nstring(10) \"8.5.10-dev\"\nbool(false)\n"
    );
}

/// `eval()` reports the profile the BINARY was compiled for, not the bridge's own default.
///
/// This is the assertion the default-profile test cannot make. The eval interpreter ships as an
/// archive linked into the produced binary and defaults to the newest profile, so a `8.5`
/// expectation is satisfied whether or not the compiler forwards anything. Compiling at `8.2`
/// and demanding `8.2.0` is what distinguishes a working bridge from a coincidence.
///
/// `PHP_MAJOR_VERSION`, `PHP_RELEASE_VERSION` and `PHP_EXTRA_VERSION` stay `8`, `0` and empty:
/// they are invariant across every maintained profile, which is why the bridge does not carry
/// them.
#[test]
fn eval_follows_a_non_default_profile() {
    let source = r#"<?php
eval('echo PHP_VERSION, "|", PHP_VERSION_ID, "|", PHP_MAJOR_VERSION, "|", PHP_MINOR_VERSION,
      "|", PHP_RELEASE_VERSION, "|", PHP_EXTRA_VERSION, "|", phpversion(), "\n";');
echo PHP_VERSION, "|", PHP_VERSION_ID, "|", phpversion(), "\n";
"#;
    let out = run_for_profile("elephc_eval_version_82", source, "8.2");
    assert_eq!(out, "8.2.0|80200|8|2|0||8.2.0\n8.2.0|80200|8.2.0\n");
}
