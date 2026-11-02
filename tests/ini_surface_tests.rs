//! Purpose:
//! End-to-end tests for PHP's runtime-configuration surface in a PLAIN (non-`--web`) binary:
//! `ini_get()`, `ini_set()`, `ini_get_all()` and `error_log()`. The one thing every test here
//! turns on is WHERE the call is written — an AUTOLOADED class, not the entry file — because
//! that is the whole defect: the surface existed, and the gate that injects it could not see
//! the code that wanted it.
//!
//! Called from:
//! - `cargo test --test ini_surface_tests` through Rust's test harness.
//!
//! Key details:
//! - THE GAP. `opcache_prelude`'s CLI `ini_get`/`ini_set`/`ini_get_all` wrappers were gated in
//!   the `opcache-prelude` pipeline phase, which runs before `autoload-run`. Symfony's console
//!   entry is 21 lines that name no INI function; `ini_set()` is called from an autoloaded
//!   class, so the gate injected nothing and the binary died with `Call to undefined function
//!   ini_set()`. `opcache_prelude::inject_cli_ini_if_used` is the second gate, in the
//!   `compat-preludes` phase, and `tests/error_handling_surface_tests.rs` is the sibling suite
//!   for the family that hit the same wall first.
//! - `error_log()` was never undefined off `--web` — the REGISTRY builtin answered it — but it
//!   ignored `$message_type`, so `error_log($m, 3, $file)` logged to stderr and returned true.
//!   The `--web` prelude's PHP body moved to `crate::error_handling_prelude`, which now shadows
//!   the builtin in both SAPIs.
//! - Every expected string here was captured from `php 8.5.10` on the same source. The known
//!   divergences are asserted AS DIVERGENCES rather than hidden: elephc's CLI INI table knows
//!   only `opcache.*`, so `ini_get('precision')` answers `false` where php answers `"14"`, and
//!   `ini_set()` reports failure for every key because every directive is baked at compile
//!   time. Both predate this change; the tests pin them so the day they are fixed is loud.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir, compile a plain executable, run it, and assert stdout AND stderr. Host-target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
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
        let mut exe = std::env::current_exe().expect("failed to resolve current test binary");
        exe.pop();
        if exe.ends_with("deps") {
            exe.pop();
        }
        exe.join("elephc").to_string_lossy().into_owned()
    })
}

/// Compiles `source` to a plain (non-`--web`) executable and returns its path.
fn compile_cli(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    dir.join(stem)
}

/// Runs a compiled executable and returns `(stdout, stderr)`.
fn run_binary(bin: &Path) -> (String, String) {
    let output = Command::new(bin)
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "compiled binary exited non-zero:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    (stdout, stderr)
}

/// Writes an entry file whose ONLY mention of the surface is inside an autoloaded class, and
/// compiles it.
///
/// This is the shape the defect needs and the reason every test here costs a second file. The
/// entry registers an `spl_autoload_register` closure and calls one static method; `body` is
/// the method's PHP. An entry that spelled `ini_set` itself would be injected by the EARLY
/// (pre-autoload) gate and would pass even with the fix reverted — which is exactly how the
/// gap survived until a console binary hit it.
fn compile_autoloaded(prefix: &str, body: &str) -> (PathBuf, PathBuf) {
    let dir = make_test_dir(prefix);
    fs::create_dir_all(dir.join("lib")).unwrap();
    fs::write(
        dir.join("lib/Probe.php"),
        format!(
            "<?php\n\nnamespace App;\n\nclass Probe\n{{\n    public static function run(string $log): void\n    {{\n{body}\n    }}\n}}\n"
        ),
    )
    .unwrap();
    let entry = r#"<?php
spl_autoload_register(function (string $class): void {
    if ($class === 'App\\Probe') {
        require __DIR__ . '/lib/Probe.php';
    }
});
$log = __DIR__ . '/probe.log';
if (file_exists($log)) {
    unlink($log);
}
\App\Probe::run($log);
"#;
    let bin = compile_cli(&dir, entry, "app");
    (dir, bin)
}

/// THE DEFECT, in one test: all four names, reached only from an autoloaded class.
///
/// Before the second injection gate this binary compiled and then died on its first line with
/// `Call to undefined function ini_set()`. `function_exists()` is asserted alongside the calls
/// because the two can disagree — the catalog answers `function_exists`, the injected
/// declaration answers the call — and it is the CALL that was broken.
///
/// php 8.5.10 prints `1111` for the four `function_exists` probes.
#[test]
fn the_ini_surface_is_callable_from_an_autoloaded_class() {
    let (_dir, bin) = compile_autoloaded(
        "inisurface_autoloaded",
        r#"        foreach (['ini_get', 'ini_set', 'ini_get_all', 'error_log'] as $name) {
            echo function_exists($name) ? '1' : '0';
        }
        echo "\n";
        ini_get('opcache.enable');
        ini_set('opcache.enable', '1');
        $all = ini_get_all();
        echo is_array($all) ? "all-array\n" : "all-bad\n";
        error_log("reached\n", 3, $log);
        echo "done\n";"#,
    );
    let (stdout, stderr) = run_binary(&bin);
    assert_eq!(
        stdout, "1111\nall-array\ndone\n",
        "every name must be callable from an autoloaded class: {stdout:?} / {stderr:?}"
    );
}

/// `ini_get()` / `ini_get_all()` answer the `opcache.*` table, and answer it the same way php
/// does for the keys elephc knows.
///
/// php 8.5.10 on this source prints `enable=1`, `access=7`, `has-key=1`, `plain-is-string=1`.
/// The `access` bitmask is `PHP_INI_ALL`, which is what php reports for `opcache.enable`.
#[test]
fn ini_get_and_ini_get_all_report_the_opcache_table() {
    let (_dir, bin) = compile_autoloaded(
        "inisurface_opcache_table",
        r#"        echo 'enable=', ini_get('opcache.enable'), "\n";
        $all = ini_get_all();
        echo 'access=', $all['opcache.enable']['access'], "\n";
        echo 'has-key=', isset($all['opcache.enable']) ? '1' : '0', "\n";
        $plain = ini_get_all(null, false);
        echo 'plain-is-string=', is_string($plain['opcache.enable']) ? '1' : '0', "\n";"#,
    );
    let (stdout, stderr) = run_binary(&bin);
    assert_eq!(
        stdout, "enable=1\naccess=7\nhas-key=1\nplain-is-string=1\n",
        "the opcache INI table must survive the late injection: {stdout:?} / {stderr:?}"
    );
}

/// `ini_get_all('nosuchext')` warns and returns `false`; a KNOWN module with no directives
/// returns `[]`.
///
/// This is the one arm that needs `__elephc_ini_module_known`, which travels with
/// `ini_get_all` rather than with the shared helpers — so it is the arm that catches a late
/// injection that forgot it. php 8.5.10 warns `ini_get_all(): Extension "nosuchext" cannot be
/// found` and prints `bad=false` / `known=0`; elephc's warning text matches, without php's
/// ` in <file> on line <n>` suffix.
#[test]
fn ini_get_all_tells_an_unknown_extension_from_a_known_one() {
    let (_dir, bin) = compile_autoloaded(
        "inisurface_extension_filter",
        r#"        $bad = ini_get_all('nosuchext');
        echo 'bad=', $bad === false ? 'false' : 'other', "\n";
        $known = ini_get_all('core');
        echo 'known=', is_array($known) ? 'array' : 'other', "\n";"#,
    );
    let (stdout, stderr) = run_binary(&bin);
    assert_eq!(
        stdout, "bad=false\nknown=array\n",
        "the extension filter must distinguish unknown from empty: {stdout:?}"
    );
    assert!(
        stderr.contains(r#"ini_get_all(): Extension "nosuchext" cannot be found"#),
        "the unknown-extension warning must reach stderr: {stderr:?}"
    );
}

/// `error_log($m, 3, $file)` APPENDS TO THE FILE off `--web`, and reports an unopenable
/// destination as `false`.
///
/// This is the fidelity half of the change. The registry builtin that used to answer
/// `error_log()` on CLI writes to stderr and ignores `$message_type` entirely, so this program
/// silently wrote nothing to `$log`, returned `true`, and put both lines on the console.
///
/// php 8.5.10 leaves `one\ntwo\n` in the file and prints `t=1`, `bad=`.
#[test]
fn error_log_type_three_writes_to_the_destination_file() {
    let (dir, bin) = compile_autoloaded(
        "inisurface_errorlog_file",
        r#"        echo 't=', error_log("one\n", 3, $log) ? '1' : '0', "\n";
        error_log("two\n", 3, $log);
        echo 'bad=', error_log("x\n", 3, '/nonexistent-dir-xyz/log.txt') ? '1' : '0', "\n";"#,
    );
    let (stdout, _stderr) = run_binary(&bin);
    assert_eq!(
        stdout, "t=1\nbad=0\n",
        "error_log(type 3) must report success for a writable file and failure otherwise: {stdout:?}"
    );
    let logged = fs::read_to_string(dir.join("probe.log")).expect("error_log must create the file");
    assert_eq!(
        logged, "one\ntwo\n",
        "error_log(type 3) must append each message verbatim: {logged:?}"
    );
}

/// `error_log($m)` puts the message on stderr WITH A TRAILING NEWLINE, always.
///
/// php appends unconditionally: `error_log("b\n")` writes `b\n\n` (verified on 8.5.10, and
/// `__rt_error_log` matches byte for byte). The `--web` prelude body this declaration was
/// moved from appended only when the message did not already end in one, which would have made
/// the move a REGRESSION on CLI — so the condition was dropped. This test is what keeps it
/// dropped.
#[test]
fn error_log_type_zero_always_appends_a_newline() {
    let (_dir, bin) = compile_autoloaded(
        "inisurface_errorlog_stderr",
        r#"        error_log("a");
        error_log("b\n");
        echo "done\n";"#,
    );
    let (stdout, stderr) = run_binary(&bin);
    assert_eq!(stdout, "done\n", "stdout must carry nothing: {stdout:?}");
    assert_eq!(
        stderr, "a\nb\n\n",
        "error_log() must append a newline unconditionally, as php does: {stderr:?}"
    );
}

/// THE TWO KNOWN DIVERGENCES, pinned rather than hidden.
///
/// elephc's CLI INI table is the compile-time `opcache.*` directive set and nothing else, so a
/// core directive is simply absent, and every directive is baked into the binary, so nothing is
/// settable. php 8.5.10 prints `precision=14` and `set=14`; elephc prints `precision=` (the
/// `false` return) and `set=` (likewise). Neither is caused by the injection gate — both were
/// already true for a program that named `ini_get` in its entry file — and both become wrong
/// assertions the day elephc grows a core INI table, which is when this test should be
/// rewritten rather than deleted.
#[test]
fn a_core_ini_directive_is_not_in_the_table_yet() {
    let (_dir, bin) = compile_autoloaded(
        "inisurface_core_divergence",
        // `var_export()` is deliberately NOT used to render these: it is supplied by
        // `var_export_prelude`, another late compatibility prelude, and it is absent from
        // `name_resolver::canonical_compat_prelude_function_name` — so a bare `var_export()`
        // inside `namespace App` dies with `Call to undefined function App\var_export()`.
        // Measured while writing this test; a separate gap, in a family out of scope here.
        r#"        echo 'precision=', ini_get('precision') === false ? 'false' : ini_get('precision'), "\n";
        echo 'set=', ini_set('precision', '3') === false ? 'false' : 'other', "\n";
        echo 'set-opcache=', ini_set('opcache.enable', '0') === false ? 'false' : 'other', "\n";"#,
    );
    let (stdout, _stderr) = run_binary(&bin);
    assert_eq!(
        stdout,
        "precision=false\nset=false\nset-opcache=false\n",
        "the known divergences must stay exactly this shape: {stdout:?}"
    );
}

/// PAY-FOR-USE. A program that names none of the four must not grow by a byte.
///
/// The second injection gate reads the WHOLE post-autoload program, which is precisely the
/// input a careless gate would over-match on. Compares the emitted assembly for a trivial
/// script against the same script with one `ini_get()` call: the first must be strictly
/// smaller and must carry none of the INI symbols.
#[test]
fn a_program_that_names_nothing_emits_no_ini_code() {
    let dir = make_test_dir("inisurface_payforuse");
    let emit_asm = |stem: &str, source: &str| -> String {
        let php = dir.join(format!("{}.php", stem));
        fs::write(&php, source).unwrap();
        let output = Command::new(elephc_bin())
            .env("XDG_CACHE_HOME", dir.join("cache-root"))
            .current_dir(&dir)
            .arg("--emit-asm")
            .arg(&php)
            .output()
            .expect("failed to spawn elephc");
        assert!(
            output.status.success(),
            "elephc --emit-asm failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(dir.join(format!("{}.s", stem))).expect("emitted assembly")
    };

    let bare = emit_asm("bare", "<?php echo 1;\n");
    let using = emit_asm("using", "<?php echo 1; ini_get('opcache.enable');\n");

    for symbol in ["ini_u_get", "elephc_u_opcache_u_ini_u_string"] {
        assert!(
            !bare.contains(symbol),
            "a program that names no INI function must not carry {symbol}"
        );
    }
    assert!(
        using.contains("elephc_u_opcache_u_ini_u_string"),
        "a program that does name one must carry the dispatcher"
    );
    assert!(
        bare.len() < using.len(),
        "the bare program must be smaller: {} vs {}",
        bare.len(),
        using.len()
    );
}
