//! Purpose:
//! End-to-end tests for `opcache.restrict_api`, the compile-time OPcache API guard baked by
//! `src/opcache_prelude.rs`. Covers the denied path (verbatim `E_WARNING` + `false` from the
//! five restricted functions, `opcache_compile_file` untouched), the allowed path, the
//! default (directive absent) path, and the two matching rules that distinguish a plain byte
//! prefix from a path-component match.
//!
//! Called from:
//! - `cargo test --test opcache_restrict_api_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation here is PINNED FROM REFERENCE PHP 8.5.6 (Homebrew, `Zend OPcache`
//!   loaded), reproduced with
//!   `php -d opcache.enable=1 -d opcache.enable_cli=1 -d opcache.restrict_api=<prefix> probe.php`.
//!   The reference matrix under a denying prefix is: `opcache_get_status`,
//!   `opcache_get_configuration`, `opcache_reset`, `opcache_is_script_cached` and
//!   `opcache_invalidate` each emit `Warning: Zend OPcache API is restricted by "restrict_api"
//!   configuration directive` and return `false`, while `opcache_compile_file` is NOT guarded
//!   and returns normally with no warning.
//! - The prefix is compared against the ENTRY SCRIPT path (php-src reads
//!   `SG(request_info).path_translated`), and against its RESOLVED spelling — so the tests
//!   canonicalize the temp dir before building a prefix from it (on macOS `std::env::temp_dir()`
//!   is under `/var/folders/...`, which resolves to `/private/var/folders/...`).
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp dir,
//!   the same harness style as `opcache_ini_tests`. Host-target only (macOS aarch64 local).
//! - The probe program guards every array-returning call with `is_array()`. That is deliberate:
//!   it is the REGRESSION ANCHOR for the over-rejection the first cut of the restricted
//!   templates caused. Rendering the restricted `opcache_get_status` /
//!   `opcache_get_configuration` with a single `return false;` exit types them as plain `bool`,
//!   and the checker then rejects `count($s)` inside an `is_array($s)` guard with
//!   `count() argument must be array or Countable object` — a compile failure on correct
//!   defensive PHP. Keeping the dead array exit preserves reference's `array|false` signature.
//!   A regression shows up as a FAILED COMPILE in `compile`, not as a wrong value.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// The verbatim reference warning line elephc writes to STDOUT (php-src emits the same text at
/// `E_WARNING`, followed by ` in <file> on line <n>`, which elephc does not synthesize).
///
/// STDOUT, not stderr. MEASURED on the same reference build this file pins everything else from,
/// with stderr discarded — `php -d xdebug.mode=off -d opcache.enable=1 -d opcache.enable_cli=1
/// -d opcache.restrict_api=/nonexistent probe.php 2>/dev/null` prints all five warnings, and
/// `display_errors` is `STDOUT` there as it is by php CLI default. The pins below said otherwise
/// for as long as elephc wrote them to the wrong stream: a program's own output and its
/// diagnostics INTERLEAVE in php, which is what `DENIED_STDOUT` now carries.
const RESTRICT_WARNING: &str =
    "Warning: Zend OPcache API is restricted by \"restrict_api\" configuration directive";

/// The probe program: one line per OPcache API function, printing either the array's size or the
/// scalar return. `is_array()` guards are load-bearing (see the module preamble).
const PROBE: &str = r#"<?php
$s = opcache_get_status();
echo 'status=', (is_array($s) ? 'ARRAY' . count($s) : var_export($s, true)), "\n";
$c = opcache_get_configuration();
echo 'config=', (is_array($c) ? 'ARRAY' . count($c) : var_export($c, true)), "\n";
echo 'reset=', var_export(opcache_reset(), true), "\n";
echo 'cached=', var_export(opcache_is_script_cached(__FILE__), true), "\n";
echo 'invalidate=', var_export(opcache_invalidate(__FILE__, true), true), "\n";
echo 'compile=', var_export(opcache_compile_file(__FILE__), true), "\n";
"#;

/// Stdout of the probe when the API is DENIED. Matches reference PHP 8.5.6 exactly, including
/// `compile=true`: `opcache_compile_file` carries no restriction guard in php-src.
///
/// The five warnings are PART OF IT, in php's own places: a diagnostic is `\n` + the line, and it
/// lands where the call happens. So `status` and `config`, whose calls precede their `echo`, get
/// the warning first, while `reset`, `cached` and `invalidate` are echoed as `reset=` before the
/// call runs — the label, then the warning, then the value. Reproduced byte for byte from the
/// reference build (`2>/dev/null`), the only difference being php's ` in <file> on line <n>`
/// tail, which elephc does not synthesize.
fn denied_stdout() -> String {
    format!(
        "\n{w}\nstatus=false\n\n{w}\nconfig=false\nreset=\n{w}\nfalse\ncached=\n{w}\nfalse\n\
         invalidate=\n{w}\nfalse\ncompile=true\n",
        w = RESTRICT_WARNING
    )
}

/// Stdout of the probe when the API is ALLOWED and the cache is enabled.
///
/// `cached=true` is elephc's compile-time script manifest answering for the entry file (reference
/// PHP reports `false` here because a fresh CLI process has not cached the script yet). That is a
/// PRE-EXISTING, documented AOT design decision (see `opcache_prelude::ScriptEntry`), unrelated to
/// `restrict_api`; this file pins it only so the allowed path is compared against something exact.
const ALLOWED_STDOUT: &str = "status=ARRAY9\nconfig=ARRAY3\nreset=true\ncached=true\n\
                              invalidate=true\ncompile=true\n";

/// Creates an isolated temp dir unique across parallel test threads/processes, returned
/// CANONICALIZED so a prefix built from it matches the entry path elephc bakes.
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

/// Compiles `PROBE` in `dir` with the supplied `--ini` assignments and returns the executable.
///
/// A compile failure is a hard assert: the checker over-rejection this file anchors surfaces
/// exactly here, so the failure text is surfaced verbatim.
fn compile(dir: &Path, stem: &str, ini: &[&str]) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, PROBE).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    for assignment in ini {
        cmd.arg("--ini").arg(assignment);
    }
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed for {ini:?} (a `count() argument must be array` error means the \
         restricted bodies lost their array exit):\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Runs a compiled executable and returns `(stdout, stderr)`, asserting a clean exit.
fn run_binary(bin: &Path) -> (String, String) {
    let output = Command::new(bin).output().expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// A denying prefix makes the five restricted functions warn with the VERBATIM reference text and
/// return `false`, while `opcache_compile_file` is untouched — exactly reference PHP 8.5.6's
/// matrix. The restriction also OVERRIDES the enabled cache: `opcache.enable_cli=1` is set here,
/// yet every restricted function still returns `false` (php-src runs the guard before the
/// accelerator-enabled check for these five).
#[test]
fn denied_prefix_warns_and_returns_false() {
    let dir = make_test_dir("opcache_restrict_denied");
    let bin = compile(
        &dir,
        "app",
        &["opcache.enable_cli=1", "opcache.restrict_api=/nonexistent"],
    );
    let (out, err) = run_binary(&bin);

    assert_eq!(out, denied_stdout(), "denied output must match reference PHP");
    assert_eq!(
        out.matches(RESTRICT_WARNING).count(),
        5,
        "exactly five functions warn; opcache_compile_file must not: {out:?}"
    );
    assert!(err.is_empty(), "php writes its diagnostics to stdout: {err:?}");
}

/// A prefix that matches the entry script's directory ALLOWS every call: normal return values and
/// a completely silent stderr.
#[test]
fn allowed_prefix_keeps_normal_behavior() {
    let dir = make_test_dir("opcache_restrict_allowed");
    let bin = compile(
        &dir,
        "app",
        &[
            "opcache.enable_cli=1",
            &format!("opcache.restrict_api={}", dir.display()),
        ],
    );
    let (out, err) = run_binary(&bin);

    assert_eq!(out, ALLOWED_STDOUT, "an allowing prefix must not change behavior");
    assert!(err.is_empty(), "an allowed call must not warn: {err:?}");
}

/// Omitting the directive entirely (the default `restrict_api = ""`) is indistinguishable from an
/// allowing prefix — the default path is untouched by this feature.
#[test]
fn default_directive_imposes_no_restriction() {
    let dir = make_test_dir("opcache_restrict_default");
    let bin = compile(&dir, "app", &["opcache.enable_cli=1"]);
    let (out, err) = run_binary(&bin);

    assert_eq!(out, ALLOWED_STDOUT, "the default must behave exactly as allowed");
    assert!(err.is_empty(), "the default must not warn: {err:?}");

    // An explicitly EMPTY prefix is the same thing (php-src's `*restrict_api` emptiness check).
    let empty_dir = make_test_dir("opcache_restrict_empty");
    let empty_bin = compile(
        &empty_dir,
        "app",
        &["opcache.enable_cli=1", "opcache.restrict_api="],
    );
    let (empty_out, empty_err) = run_binary(&empty_bin);
    assert_eq!(empty_out, ALLOWED_STDOUT);
    assert!(empty_err.is_empty());
}

/// The comparison is a PLAIN BYTE PREFIX, not a path-component match: an entry at
/// `<dir>/foobar/app.php` is ALLOWED by the prefix `<dir>/foo`, which is not a directory at all.
/// VERIFIED on reference PHP 8.5.6 (php-src uses `memcmp`, not a component walk).
#[test]
fn partial_path_component_prefix_allows() {
    let dir = make_test_dir("opcache_restrict_partial");
    let nested = dir.join("foobar");
    fs::create_dir_all(&nested).unwrap();

    let bin = compile(
        &nested,
        "app",
        &[
            "opcache.enable_cli=1",
            // `<dir>/foo` — a byte prefix of `<dir>/foobar/app.php`, not a path component.
            &format!("opcache.restrict_api={}/foo", dir.display()),
        ],
    );
    let (out, err) = run_binary(&bin);

    assert_eq!(
        out, ALLOWED_STDOUT,
        "a partial path component is still a byte prefix and must ALLOW"
    );
    assert!(err.is_empty(), "a byte-prefix match must not warn: {err:?}");
}

/// The comparison is CASE-SENSITIVE even though macOS's filesystem is not: uppercasing the
/// directory name in the prefix DENIES. VERIFIED on reference PHP 8.5.6 (memcmp on the path
/// string, never a filesystem lookup).
#[test]
fn case_changed_prefix_denies() {
    let dir = make_test_dir("opcache_restrict_case");
    let nested = dir.join("foobar");
    fs::create_dir_all(&nested).unwrap();

    let bin = compile(
        &nested,
        "app",
        &[
            "opcache.enable_cli=1",
            &format!("opcache.restrict_api={}/Foobar", dir.display()),
        ],
    );
    let (out, err) = run_binary(&bin);

    assert_eq!(out, denied_stdout(), "a case-changed prefix must DENY");
    assert!(err.is_empty(), "php writes its diagnostics to stdout: {err:?}");
}

/// A prefix LONGER than the entry path can never match (php-src's `strlen(path) < len` arm), and
/// a prefix equal to the WHOLE entry path allows.
#[test]
fn prefix_longer_than_entry_denies_and_exact_path_allows() {
    let long_dir = make_test_dir("opcache_restrict_long");
    let long_bin = compile(
        &long_dir,
        "app",
        &[
            "opcache.enable_cli=1",
            &format!("opcache.restrict_api={}/app.php/deeper", long_dir.display()),
        ],
    );
    let (long_out, long_err) = run_binary(&long_bin);
    assert_eq!(long_out, denied_stdout(), "an over-long prefix must DENY");
    assert!(long_err.is_empty(), "php writes its diagnostics to stdout: {long_err:?}");

    let exact_dir = make_test_dir("opcache_restrict_exact");
    let exact_bin = compile(
        &exact_dir,
        "app",
        &[
            "opcache.enable_cli=1",
            &format!("opcache.restrict_api={}/app.php", exact_dir.display()),
        ],
    );
    let (exact_out, exact_err) = run_binary(&exact_bin);
    assert_eq!(exact_out, ALLOWED_STDOUT, "the exact entry path must ALLOW");
    assert!(exact_err.is_empty());
}

/// The restriction is decided against the ENTRY SCRIPT, not the executing file — php-src compares
/// `SG(request_info).path_translated`. PROVEN on reference PHP 8.5.6 with an entry in one
/// directory that requires a script in another and calls the API from there: the ENTRY's prefix
/// allowed, the includee's prefix denied.
///
/// Here the API call lives in an included file under `<dir>/incdir` while the entry is
/// `<dir>/entrydir/app.php`. A prefix naming the INCLUDEE's directory must DENY, and one naming
/// the ENTRY's directory must ALLOW.
///
/// The probe is reduced to `opcache_reset()` — the same single call the reference experiment
/// used. The file-taking functions would report on the INCLUDEE's `__FILE__`, which is not in the
/// compile-time script manifest, muddying the signal with an unrelated concern.
#[test]
fn restriction_follows_the_entry_script_not_the_executing_file() {
    let dir = make_test_dir("opcache_restrict_entry");
    let entrydir = dir.join("entrydir");
    let incdir = dir.join("incdir");
    fs::create_dir_all(&entrydir).unwrap();
    fs::create_dir_all(&incdir).unwrap();
    fs::write(
        incdir.join("inc.php"),
        "<?php\necho 'reset=', var_export(opcache_reset(), true), \"\\n\";\n",
    )
    .unwrap();

    let entry_src = format!(
        "<?php\nrequire '{}';\n",
        incdir.join("inc.php").display()
    );

    let build = |stem: &str, prefix: &Path| -> (String, String) {
        let php = entrydir.join(format!("{stem}.php"));
        fs::write(&php, &entry_src).unwrap();
        let mut cmd = Command::new(elephc_bin());
        cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
        cmd.current_dir(&entrydir);
        cmd.arg(&php);
        cmd.arg("--ini").arg("opcache.enable_cli=1");
        cmd.arg("--ini")
            .arg(format!("opcache.restrict_api={}", prefix.display()));
        let output = cmd.output().expect("failed to spawn elephc");
        assert!(
            output.status.success(),
            "elephc compile failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        run_binary(&entrydir.join(stem))
    };

    // Prefix = the EXECUTING file's directory → the entry is not under it → DENY.
    let (inc_out, inc_err) = build("by_includee", &incdir);
    assert_eq!(
        inc_out,
        format!("reset=\n{RESTRICT_WARNING}\nfalse\n"),
        "the executing file's directory must NOT satisfy the restriction"
    );
    assert!(inc_err.is_empty(), "php writes its diagnostics to stdout: {inc_err:?}");

    // Prefix = the ENTRY script's directory → ALLOW, even though the call executes elsewhere.
    let (entry_out, entry_err) = build("by_entry", &entrydir);
    assert_eq!(
        entry_out, "reset=true\n",
        "the entry script's directory must satisfy the restriction"
    );
    assert!(entry_err.is_empty(), "entry-prefixed build must not warn: {entry_err:?}");
}
