//! Purpose:
//! End-to-end tests for what `opcache_get_status()` reports about the RUNTIME SCRIPT CACHE —
//! the tier that holds files included at run time through the eval bridge, which the
//! compile-time manifest cannot contain.
//!
//! Called from:
//! - `cargo test --test opcache_runtime_cache_tests` through Rust's test harness.
//!
//! Key details:
//! - A runtime-dynamic include is a COMPILE ERROR at AOT top level, so every probe here
//!   performs its include from inside `eval()`. That is not an artifact of the test: it is
//!   the only way the tier is reachable at all.
//! - The `--ini opcache.enable_cli=1` binaries are the interesting ones. The DEFAULT CLI
//!   binary is pinned separately and must keep reporting `false` from
//!   `opcache_get_status()`, because its cache is disabled.
//! - The pay-for-use pin is the point of `a_binary_without_eval_reports_only_the_manifest`:
//!   a program with no eval bridge has no dynamic tier, and the lowering folds every
//!   bridge call to the empty-cache answer rather than linking the interpreter.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated temp
//!   dir, the same harness style as `opcache_manifest_tests`. Host-target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// Creates an isolated temp dir unique across parallel test threads/processes, CANONICALIZED
/// so the paths the probe builds match the spelling the cache stores.
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

/// Compiles `<dir>/main.php` with the supplied `--ini` assignments and returns the executable.
fn compile(dir: &Path, ini: &[&str]) -> PathBuf {
    compile_with_flags(dir, ini, &[])
}

/// The same, with extra CLI flags ahead of the `--ini` assignments.
///
/// `--php-version` is the one that matters here. A program whose behaviour depends on the
/// PHP profile must pin it, and the OPcache surface is profile-dependent in several places
/// at once — `opcache_get_configuration()`'s directive set, `opcache_get_status()`'s JIT
/// block, `phpversion()`, `zend_version()`. A fixture that reaches enough of them is asked
/// to be explicit rather than inheriting the default.
///
/// THE FIXTURES THAT NEED IT ARE THE `eval()` ONES, and that is a consequence of this
/// branch: a fragment the compiler cannot read counts as a mention of every watched name,
/// so those programs get the whole OPcache prelude injected and cross the threshold where
/// one or two of them would not. The pin is the right answer — the alternative was the
/// detector staying silent and the program getting a wrong value — but it is a real cost of
/// the conservative rule and worth seeing stated next to the tests that pay it.
fn compile_with_flags(dir: &Path, ini: &[&str], flags: &[&str]) -> PathBuf {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(dir.join("main.php"));
    for flag in flags {
        cmd.arg(flag);
    }
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
    dir.join("main")
}

/// Runs a compiled binary, asserting success, and returns its stdout.
fn run_binary(bin: &Path) -> String {
    let output = Command::new(bin).output().expect("failed to run binary");
    assert!(
        output.status.success(),
        "binary failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Runs a compiled binary with runtime-provided eval source and returns stdout/stderr.
fn run_binary_with_env(bin: &Path, name: &str, value: &str) -> (String, String) {
    let output = Command::new(bin)
        .env(name, value)
        .output()
        .expect("failed to run binary");
    assert!(
        output.status.success(),
        "binary failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Returns the value of a `key=value` line the probe printed.
fn field<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("probe printed no `{key}=` line; output was:\n{output}"))
}

/// Writes the dynamic-include fixture: an entry that includes a sibling from inside `eval()`.
fn write_dynamic_fixture(dir: &Path, probe: &str) {
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(dir.join("main.php"), probe).unwrap();
}

/// The probe: include a dynamic file from `eval()`, then report what the status array says.
const STATUS_PROBE: &str = r#"<?php
eval('include __DIR__ . "/lib.php";');
$s = opcache_get_status();
if (!is_array($s)) {
    echo "status=false\n";
    return;
}
echo 'status=array', "\n";
echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
echo 'keys=', $s['opcache_statistics']['num_cached_keys'], "\n";
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
echo 'rate=', $s['opcache_statistics']['opcache_hit_rate'], "\n";
echo 'scripts=', count($s['scripts']), "\n";
echo 'cached=', (opcache_is_script_cached(__DIR__ . '/lib.php') ? '1' : '0'), "\n";
$found = 0;
foreach ($s['scripts'] as $key => $entry) {
    if ($key === __DIR__ . '/lib.php') {
        $found = 1;
        echo 'entry_full_path=', $entry['full_path'], "\n";
        echo 'entry_keys=', count($entry), "\n";
        echo 'entry_ts_positive=', ($entry['timestamp'] > 0 ? '1' : '0'), "\n";
    }
}
echo 'found=', $found, "\n";
"#;

/// The "compiled without optional regex support" note is about EVALUATED code, so a program
/// that links the interpreter only for `opcache_compile_file()` does not get it.
///
/// `opcache_compile_file()` links the eval bridge — see the test below — but parses and caches
/// without running anything, so "evaluated code that uses preg_* will fail" described code the
/// program cannot contain. The control proves the note still reaches a program that CAN
/// evaluate code, so the silence is not the note having been removed. Reported by DeepSeek.
#[test]
fn the_regex_note_is_only_for_programs_that_evaluate_code() {
    const NOTE: &str = "dynamic eval was compiled without optional regex support";
    let compile_stderr = |name: &str, source: &str| -> String {
        let dir = make_test_dir(name);
        fs::write(dir.join("lib.php"), "<?php $lib = 1;\n").unwrap();
        fs::write(dir.join("main.php"), source).unwrap();
        let output = Command::new(elephc_bin())
            .env("XDG_CACHE_HOME", dir.join("cache-root"))
            .current_dir(&dir)
            .arg(dir.join("main.php"))
            .arg("--ini")
            .arg("opcache.enable_cli=1")
            .output()
            .expect("failed to spawn elephc");
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stderr).into_owned()
    };

    let opcache_only = compile_stderr(
        "opcache_rt_regex_note_opcache_only",
        "<?php\nvar_dump(opcache_compile_file(__DIR__ . '/lib.php'));\n",
    );
    let evaluates = compile_stderr(
        "opcache_rt_regex_note_eval",
        "<?php\neval('$x = 1;' . str_repeat(' ', count($argv) - 1));\n",
    );

    assert!(
        !opcache_only.contains(NOTE),
        "an OPcache-only bridge link runs no evaluated code:\n{opcache_only}"
    );
    assert!(
        evaluates.contains(NOTE),
        "PREMISE: a program that evaluates code still gets the note:\n{evaluates}"
    );
}

/// Verifies `opcache_compile_file()` works WITHOUT an `eval()` anywhere in the program.
///
/// Every other test in this file writes an `eval()` to force the eval bridge, which is
/// exactly what hid this: `opcache_compile_file()` folded to `0` in a binary that links no
/// bridge, and answered `false` for a readable, parsable file where reference answers `true`
/// and caches it. The fold is right for `is_cached` and `discard` — nothing can have been
/// cached without a dynamic tier — and wrong for the one operation whose job is to CREATE
/// the entry.
///
/// Calling it is now itself a reason to link the interpreter, which is what pay-for-use
/// means. The cost is bounded and was measured: 2.6 MB for this binary against 69 KB for
/// the same program with OPcache disabled, because the prelude's own gate short-circuits
/// before the call is ever emitted there.
///
/// THE ABSENCE OF `eval` IS THE TEST. If a future edit adds one to this source to make
/// something else work, the test keeps passing and stops meaning anything.
#[test]
fn compile_file_works_without_any_eval_in_the_program() {
    let dir = make_test_dir("opcache_rt_compile_no_eval");
    fs::write(dir.join("lib.php"), "<?php $lib = 1;\n").unwrap();
    let source = r#"<?php
$p = __DIR__ . '/lib.php';
echo 'c1=', (opcache_compile_file($p) ? '1' : '0'), "\n";
echo 'c2=', (opcache_compile_file($p) ? '1' : '0'), "\n";
echo 'cached=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
"#;
    assert!(
        !source.contains("eval("),
        "this test only means something while the program has no eval()"
    );
    fs::write(dir.join("main.php"), source).unwrap();

    let bin = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    let out = run_binary(&bin);

    assert_eq!(
        field(&out, "c1"),
        "1",
        "compile_file folded to false in a binary with no eval bridge:\n{out}"
    );
    assert_eq!(field(&out, "c2"), "1", "{out}");
    assert_eq!(
        field(&out, "cached"),
        "1",
        "the file must actually be cached, not merely reported compiled:\n{out}"
    );
}

/// Verifies `opcache_compile_file()` does not report success for a file that cannot parse.
///
/// This one came out of the interaction between two changes that were each correct alone.
/// The new guard in `fill_entry` stops caching a script with a syntax error, and returns
/// `Ok` while doing so — deliberately, because the segments it hands back carry the error so
/// a later `include` can raise it at the right moment. `compile_file` read that `Ok` as
/// "compiled". The result was the single worst answer available: `true` for a file that is
/// not cached and never will be, with no diagnostic anywhere.
///
/// DIVERGENCE, asserted as it stands rather than as it should be: reference PHP 8.5 THROWS a
/// `ParseError` here. Carrying the message and line across the bridge is out of reach of a
/// `-> u64` symbol, so `false` is the honest half of the answer. If that ever becomes a
/// throw, this assertion is what will fail and say so.
///
/// The `eval()` is load-bearing and not scaffolding: without one the binary links no eval
/// bridge, every `rt_*` call folds to `0`, and the test would pass for the wrong reason.
#[test]
fn compile_file_refuses_a_file_that_does_not_parse() {
    let dir = make_test_dir("opcache_rt_compile_broken");
    fs::write(dir.join("inc.php"), "<?php $unrelated = 1;\n").unwrap();
    fs::write(dir.join("good.php"), "<?php $ok = 1;\n").unwrap();
    fs::write(dir.join("broken.php"), "<?php $a = ;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
eval('include __DIR__ . "/inc.php";');
echo 'good=', (opcache_compile_file(__DIR__ . '/good.php') ? '1' : '0'), "\n";
echo 'broken=', (opcache_compile_file(__DIR__ . '/broken.php') ? '1' : '0'), "\n";
echo 'broken_cached=', (opcache_is_script_cached(__DIR__ . '/broken.php') ? '1' : '0'), "\n";
"#,
    )
    .unwrap();

    let bin = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    let out = run_binary(&bin);

    assert_eq!(
        field(&out, "good"),
        "1",
        "a parsable file outside the manifest must still compile:\n{out}"
    );
    assert_eq!(
        field(&out, "broken"),
        "0",
        "compile_file claimed success for a file with a syntax error:\n{out}"
    );
    assert_eq!(
        field(&out, "broken_cached"),
        "0",
        "a file that did not parse must not be cached:\n{out}"
    );
}

/// The EVAL `opcache_compile_file()` warns about a missing file, and ONLY a missing file.
///
/// Its handler printed `Failed to open stream: No such file or directory` whenever the
/// compile failed — including for a file that exists and merely does not parse. Reference
/// throws a `ParseError` there (the pinned divergence answers `false`); a "no such file"
/// warning names a failure that did not happen. Found by one reviewer and recorded as
/// unsettled by another.
///
/// THE MISSING-FILE HALF IS THE PREMISE. It proves the call reaches the eval handler — a
/// literal name would inject the native wrapper instead — so the silence on the broken file
/// means the handler stayed quiet, not that it was never asked.
#[test]
fn the_eval_compile_file_warns_only_about_a_missing_file() {
    let dir = make_test_dir("opcache_rt_eval_compile_broken");
    fs::write(dir.join("broken.php"), "<?php $a = ;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
$n = 'opcache_' . 'compile_file';
echo 'broken=', var_export(eval('return ' . $n . '(__DIR__ . "/broken.php");'), true), "\n";
echo 'missing=', var_export(eval('return ' . $n . '(__DIR__ . "/never-existed.php");'), true), "\n";
"#,
    )
    .unwrap();

    let bin = compile_with_flags(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
        &["--php-version", "8.5"],
    );
    let output = Command::new(&bin).output().expect("failed to run binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(field(&stdout, "broken"), "false", "a parse failure compiles nothing:\n{stdout}");
    assert_eq!(field(&stdout, "missing"), "false", "neither does a missing file:\n{stdout}");
    assert!(
        stderr.contains("never-existed.php): Failed to open stream"),
        "PREMISE: the missing file must warn, or the eval handler was never reached:\n{stderr}"
    );
    assert!(
        !stderr.contains("broken.php): Failed to open stream"),
        "a file that exists must not be reported missing:\n{stderr}"
    );
}

/// Verifies a FORCED `opcache_invalidate()` and `opcache_compile_file()` reach the runtime
/// tier, not just the manifest.
///
/// Both used to answer as though the work had happened while doing none of it. Forced
/// invalidation returned `true` and left the entry cached and served, and
/// `opcache_compile_file()` refused every path outside the manifest — which meant it could
/// only ever succeed for files that needed no compiling, the one case where it does nothing.
///
/// Each step is asserted through a SEPARATE observation rather than the call's own return
/// value, because the return value is exactly what was already correct while the effect was
/// missing: `2_inval` was `true` before this change too. `3_after` is what makes it real.
///
/// VERIFIED against reference PHP 8.5, which prints this sequence exactly.
#[test]
fn a_forced_invalidate_and_a_compile_reach_the_runtime_tier() {
    let dir = make_test_dir("opcache_rt_invalidate_compile");
    fs::write(dir.join("dyn.php"), "<?php $dyn = 1;\n").unwrap();
    fs::write(dir.join("compileme.php"), "<?php $cf = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
$p = __DIR__ . '/dyn.php';
eval('include $p;');
echo 'cached=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
echo 'inval=', (opcache_invalidate($p, true) ? '1' : '0'), "\n";
echo 'after_inval=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
$c = __DIR__ . '/compileme.php';
echo 'before_compile=', (opcache_is_script_cached($c) ? '1' : '0'), "\n";
echo 'compile=', (opcache_compile_file($c) ? '1' : '0'), "\n";
echo 'after_compile=', (opcache_is_script_cached($c) ? '1' : '0'), "\n";
"#,
    )
    .unwrap();

    let bin = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    let out = run_binary(&bin);

    assert_eq!(field(&out, "cached"), "1", "{out}");
    assert_eq!(field(&out, "inval"), "1", "{out}");
    assert_eq!(
        field(&out, "after_inval"),
        "0",
        "a forced invalidate returned true and discarded nothing:\n{out}"
    );
    assert_eq!(field(&out, "before_compile"), "0", "{out}");
    assert_eq!(
        field(&out, "compile"),
        "1",
        "compile_file refused a file outside the manifest:\n{out}"
    );
    assert_eq!(
        field(&out, "after_compile"),
        "1",
        "compile_file answered true without caching anything:\n{out}"
    );
}

/// Verifies `opcache_is_script_cached()` sees a DYNAMICALLY included file, natively and in
/// `eval()`, and that the two agree.
///
/// The manifest is only half the cache, and the native declaration used to be the only half
/// it knew: a natively written call answered `false` for a file the runtime tier was actively
/// serving, while the same call inside `eval()` answered `true`. One binary, one cache, two
/// answers decided by where the question was written.
///
/// VERIFIED against reference PHP 8.5, which reports `true` on both surfaces for an included
/// file and `false` for one that was never included.
///
/// THE NEGATIVE CASE IS NOT DECORATION. A lookup that answered `true` unconditionally would
/// satisfy the first two assertions, so "always true" and "correct" differ only on a path
/// that was never included.
///
/// This gap survived because `STATUS_PROBE` has emitted a `cached=` line all along and no test
/// ever asserted it: the evidence was on screen and unread.
#[test]
fn a_dynamically_included_file_is_reported_cached_on_both_surfaces() {
    let dir = make_test_dir("opcache_rt_is_cached");
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
$p = __DIR__ . '/lib.php';
eval('include $p;');
echo 'native=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
echo 'eval=', (eval('return opcache_is_script_cached($p);') ? '1' : '0'), "\n";
$q = __DIR__ . '/never-included.php';
echo 'absent_native=', (opcache_is_script_cached($q) ? '1' : '0'), "\n";
echo 'absent_eval=', (eval('return opcache_is_script_cached($q);') ? '1' : '0'), "\n";
"#,
    )
    .unwrap();

    let bin = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    let out = run_binary(&bin);

    assert_eq!(
        field(&out, "native"),
        "1",
        "the native surface did not see the runtime tier:\n{out}"
    );
    assert_eq!(field(&out, "eval"), "1", "{out}");
    assert_eq!(
        field(&out, "absent_native"),
        "0",
        "a file that was never included must not be reported cached:\n{out}"
    );
    assert_eq!(field(&out, "absent_eval"), "0", "{out}");
}

/// Verifies a dynamically included file appears in `opcache_get_status()` with live counters.
///
/// This is the whole point of the change: before it, the status array described only the
/// compile-time manifest and reported `hits`/`misses` as a permanent `0`, while the cache
/// behind `eval()`'s includes was actively serving them.
#[test]
fn a_dynamically_included_file_reaches_the_status_array() {
    let dir = make_test_dir("opcache_rt_status");
    write_dynamic_fixture(&dir, STATUS_PROBE);

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    assert_eq!(field(&output, "status"), "array");
    // The entry file is the manifest's single member; the dynamic include adds one more.
    assert_eq!(field(&output, "num"), "2");
    assert_eq!(field(&output, "keys"), "2");
    assert_eq!(field(&output, "scripts"), "2");
    assert_eq!(field(&output, "found"), "1");
    assert_eq!(
        field(&output, "entry_full_path"),
        dir.join("lib.php").display().to_string(),
        "the entry must carry the canonical path, not merely some path"
    );
    // The dynamic entry carries the reference 7-key shape on an 8.5 target.
    assert_eq!(field(&output, "entry_keys"), "7");
    assert_eq!(field(&output, "entry_ts_positive"), "1");
}

/// Verifies the miss from the first include is counted, and the hit from the second.
#[test]
fn the_counters_follow_the_actual_includes() {
    let dir = make_test_dir("opcache_rt_counters");
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('include __DIR__ . "/lib.php"; include __DIR__ . "/lib.php";');
$s = opcache_get_status();
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
echo 'rate=', $s['opcache_statistics']['opcache_hit_rate'], "\n";
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    // One cold fill, then one warm serve.
    assert_eq!(field(&output, "misses"), "1");
    assert_eq!(field(&output, "hits"), "1");
    // php-src reports the rate as a percentage of lookups: 1 hit of 2 lookups is 50.
    assert_eq!(field(&output, "rate"), "50");
}

/// Verifies the hit rate is `0`, not a division by zero, when nothing has been looked up.
#[test]
fn an_untouched_cache_reports_a_zero_hit_rate() {
    let dir = make_test_dir("opcache_rt_rate");
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('$x = 1;');
$s = opcache_get_status();
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
echo 'rate=', $s['opcache_statistics']['opcache_hit_rate'], "\n";
echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    assert_eq!(field(&output, "hits"), "0");
    assert_eq!(field(&output, "misses"), "0");
    assert_eq!(field(&output, "rate"), "0");
    assert_eq!(field(&output, "num"), "1");
}

/// Verifies a program with no eval bridge reports exactly the manifest, and nothing else.
///
/// This is the pay-for-use pin. Such a binary has no dynamic tier, so every bridge call is
/// folded to the empty-cache answer at lowering time rather than linking the interpreter.
#[test]
fn a_binary_without_eval_reports_only_the_manifest() {
    let dir = make_test_dir("opcache_rt_noeval");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$s = opcache_get_status();
echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
echo 'rate=', $s['opcache_statistics']['opcache_hit_rate'], "\n";
echo 'full=', ($s['cache_full'] ? '1' : '0'), "\n";
echo 'scripts=', count($s['scripts']), "\n";
echo 'restarts=', $s['opcache_statistics']['manual_restarts'], "\n";
echo 'last=', $s['opcache_statistics']['last_restart_time'], "\n";
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    assert_eq!(field(&output, "num"), "1");
    assert_eq!(field(&output, "scripts"), "1");
    assert_eq!(field(&output, "hits"), "0");
    assert_eq!(field(&output, "misses"), "0");
    assert_eq!(field(&output, "rate"), "0");
    assert_eq!(field(&output, "full"), "0");
    assert_eq!(field(&output, "restarts"), "0");
    assert_eq!(field(&output, "last"), "0");
}

/// Verifies a default CLI binary still answers `false`, cache or no cache.
///
/// The whole runtime cache is gated on the cache-enabled state, so a default CLI build must
/// behave exactly as it did before this tier existed.
#[test]
fn a_default_cli_binary_still_reports_no_status() {
    let dir = make_test_dir("opcache_rt_default");
    write_dynamic_fixture(&dir, STATUS_PROBE);

    let output = run_binary(&compile(&dir, &[]));

    assert_eq!(field(&output, "status"), "false");
}

/// Verifies `opcache_reset()` from inside `eval()` shows up as a pending restart natively,
/// and that nothing else moves until the restart is actually performed.
///
/// The native latch and the runtime cache's latch are different objects; reporting only the
/// native one would deny a restart the cache has actually scheduled.
///
/// The other three figures are the DEFERRAL: php-src schedules the restart and performs it
/// at the next request, so within this one the cache keeps its entries and neither
/// `manual_restarts` nor `last_restart_time` moves. VERIFIED on reference PHP 8.5.10, which
/// reports exactly this shape. A CLI program is a single request and so never reaches the
/// boundary that performs it — correctly, since reference would restart at a next request
/// a CLI process does not have.
#[test]
fn a_reset_issued_from_eval_is_reported_natively() {
    let dir = make_test_dir("opcache_rt_reset");
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('include __DIR__ . "/lib.php"; opcache_reset();');
$s = opcache_get_status();
echo 'pending=', ($s['restart_pending'] ? '1' : '0'), "\n";
echo 'restarts=', $s['opcache_statistics']['manual_restarts'], "\n";
echo 'last_positive=', ($s['opcache_statistics']['last_restart_time'] > 0 ? '1' : '0'), "\n";
echo 'num=', $s['opcache_statistics']['num_cached_scripts'], "\n";
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    assert_eq!(field(&output, "pending"), "1", "the latch is the only thing that moves");
    assert_eq!(field(&output, "restarts"), "0", "counted at the restart, not the schedule");
    assert_eq!(field(&output, "last_positive"), "0");
    // The dynamic entry SURVIVES the schedule: manifest (1) plus the cached include (1).
    assert_eq!(field(&output, "num"), "2");
}

/// Verifies `opcache_get_status()` answers the same thing natively and from inside `eval()`.
///
/// It did not. The eval interpreter carries its own handler for the name and dispatched it
/// BEFORE consulting the program's own declarations, so the same call in the same binary
/// answered an array natively and `false` from inside `eval()`. The interpreter now prefers
/// the prelude's declaration when the binary carries one — which is the body that knows both
/// the compile-time manifest and the live cache — and only falls back to its handler for a
/// program that has no such declaration.
///
/// The assertion is EQUALITY between the two sides, not a fixed value: what matters is that
/// one binary cannot hold two answers.
#[test]
fn the_status_array_is_the_same_written_natively_and_inside_eval() {
    let dir = make_test_dir("opcache_rt_sides");
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('include __DIR__ . "/lib.php";');
$native = opcache_get_status();
echo 'native_type=', (is_array($native) ? 'array' : 'false'), "\n";
echo 'native_num=', $native['opcache_statistics']['num_cached_scripts'], "\n";
echo 'native_misses=', $native['opcache_statistics']['misses'], "\n";
echo 'native_scripts=', count($native['scripts']), "\n";
echo 'native_keys=', implode(',', array_keys($native)), "\n";
eval('
$inside = opcache_get_status();
echo "inside_type=", (is_array($inside) ? "array" : "false"), "\n";
echo "inside_num=", $inside["opcache_statistics"]["num_cached_scripts"], "\n";
echo "inside_misses=", $inside["opcache_statistics"]["misses"], "\n";
echo "inside_scripts=", count($inside["scripts"]), "\n";
echo "inside_keys=", implode(",", array_keys($inside)), "\n";
');
"#,
    );

    let output = run_binary(&compile(&dir, &["opcache.enable_cli=1"]));

    for key in ["type", "num", "misses", "scripts", "keys"] {
        assert_eq!(
            field(&output, &format!("native_{key}")),
            field(&output, &format!("inside_{key}")),
            "`{key}` disagrees between the native and the eval-written call"
        );
    }
    // And the shared answer is the live one, not the empty fallback.
    assert_eq!(field(&output, "native_type"), "array");
    assert_eq!(field(&output, "native_num"), "2");
}

/// Verifies a name mentioned ONLY inside `eval()` fragments still reaches the live cache.
///
/// The detector that decides whether to inject the OPcache prelude matched a string literal
/// only when the string WAS the name. That covers `function_exists('opcache_get_status')`
/// and the callable spellings, and it never covers `eval('return opcache_get_status();')`,
/// where the name is embedded in code. A program whose only mention sat inside a fragment
/// therefore got no declaration, and the interpreter's own fallback answered instead — and
/// that fallback reports the compile-time CLI default rather than reading the live cache.
///
/// The symptom was not a missing function. It was a SELF-CONTRADICTING one: measured at the
/// head before the fix, this exact program was told `opcache_get_status()` was `false` — no
/// cache — and on the next two lines that `opcache_compile_file()` had cached the file and
/// that `opcache_is_script_cached()` saw it. Three siblings read the live cache and the
/// fourth read a constant, in one process, four lines apart.
///
/// `num` is asserted as well as the type, so a future fallback that returns an EMPTY array
/// instead of `false` cannot satisfy this test: only the live cache knows the count. It is
/// `2`, not `1`, and that is parity rather than drift — reference counts the running script
/// alongside the compiled one (`scripts` there is `lib.php,main.php`), and elephc's
/// compile-time manifest holds `main.php` for the same reason.
///
/// EVERY CALL HERE IS INSIDE A FRAGMENT, and that is the test. One native `opcache_*` call
/// added to this source injects the prelude for its own sake, and the assertions below keep
/// passing while pinning nothing.
#[test]
fn opcache_names_used_only_inside_eval_still_reach_the_live_cache() {
    let dir = make_test_dir("opcache_rt_fragment_only");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$p = __DIR__ . '/lib.php';
echo 'status=', (is_array(eval('return opcache_get_status();')) ? 'array' : 'false'), "\n";
echo 'compile=', (eval('return opcache_compile_file($p);') ? 'true' : 'false'), "\n";
echo 'cached=', (eval('return opcache_is_script_cached($p);') ? 'true' : 'false'), "\n";
echo 'status2=', (is_array(eval('return opcache_get_status();')) ? 'array' : 'false'), "\n";
echo 'num=', eval('$s = opcache_get_status(); return $s["opcache_statistics"]["num_cached_scripts"];'), "\n";
"#,
    );

    let output = run_binary(&compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    ));

    // Reference, same fixture: array / true / true / array, num 2.
    assert_eq!(
        field(&output, "status"),
        "array",
        "the FIRST status call read the stale fallback"
    );
    assert_eq!(field(&output, "compile"), "true");
    assert_eq!(field(&output, "cached"), "true");
    assert_eq!(
        field(&output, "status2"),
        "array",
        "status contradicts its own siblings: they cached a file it cannot see"
    );
    assert_eq!(
        field(&output, "num"),
        "2",
        "the status array is not the live one"
    );
}

/// A forced `opcache_invalidate()` on a MANIFEST file answers the same whichever surface
/// issued it — written natively, written in a literal fragment, or written in a computed one.
///
/// WHY THIS NEEDS ITS OWN TEST. The manifest tier's invalidate latch is a `static` inside
/// the INJECTED NATIVE function, and the eval interpreter's own handler cannot reach it: it
/// knows the runtime cache and the file store and nothing else. The surfaces agree only
/// because an eval'd call dispatches into the native body once the prelude is injected — so
/// the agreement is a CONSEQUENCE of the detector, not a property of the invalidate code,
/// and any future narrowing of the detector silently breaks it here instead of in
/// production. A reviewer raised exactly this route, reasoning that a computed fragment
/// escaped injection and would leave the latch unset — which is precisely what happens, and
/// is pinned by `a_fragment_that_never_spells_the_name_is_a_known_divergence` rather than
/// duplicated here. Reference retires the entry for that spelling and elephc does not;
/// closing that divergence closes this one, because both are the same missing declaration.
///
/// MEASURED against reference PHP 8.5: `before=1 after=0` for both spellings here.
///
/// A fragment whose text never spells the name at all — `eval('$f = "opcache_" .
/// "invalidate"; $f($p, true);')` — is NOT covered and cannot be: no compile-time scan can
/// see a name assembled at runtime. It is not a silent divergence either. That program
/// stops with `Fatal error: eval() fragment uses an unsupported construct`, because the
/// interpreter refuses a variable function call, which is a pre-existing limitation of
/// `eval` rather than anything this surface decides.
#[test]
fn a_forced_invalidate_agrees_across_every_surface_that_can_issue_it() {
    for (label, issue) in [
        ("native", r#"opcache_invalidate($p, true);"#),
        ("literal fragment", r#"eval("opcache_invalidate(\$p, true);");"#),
    ] {
        let dir = make_test_dir("opcache_rt_surface_agreement");
        write_dynamic_fixture(
            &dir,
            &format!(
                r#"<?php
$p = __FILE__;
echo 'before=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
{issue}
echo 'after=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
"#
            ),
        );

        let output = run_binary(&compile_with_flags(
            &dir,
            &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
            &["--php-version", "8.5"],
        ));

        assert_eq!(
            field(&output, "before"),
            "1",
            "{label}: the entry script is a manifest member"
        );
        assert_eq!(
            field(&output, "after"),
            "0",
            "{label}: the invalidate did not reach the manifest latch\n{output}"
        );
    }
}

/// The ROOT-QUALIFIED fragment spelling, alone in its program so nothing else can inject
/// the declaration for it.
///
/// `\opcache_get_status()` is what a namespaced file or a code generator emits, and it names
/// the global function unambiguously. It was rejected because `\` counted as an identifier
/// byte, so the name failed its own whole-word test — no declaration, and the interpreter's
/// own builtin answering from the compile-time CLI default while the program's other OPcache
/// calls read the live cache.
///
/// THE PROGRAM MENTIONS THE NAME EXACTLY ONCE. An earlier version of this test put two
/// spellings in one file and passed against the broken compiler, because the plain spelling
/// on line 1 injected the declaration that line 2 then used.
///
/// MEASURED against reference PHP 8.5: `array`.
#[test]
fn the_root_qualified_fragment_spelling_injects() {
    let dir = make_test_dir("opcache_rt_fragment_qualified");
    write_dynamic_fixture(
        &dir,
        r#"<?php
echo 'r=', (is_array(eval('return \opcache_get_status();')) ? 'array' : 'false'), "\n";
"#,
    );

    let output = run_binary(&compile_with_flags(
        &dir,
        &["opcache.enable_cli=1"],
        &["--php-version", "8.5"],
    ));

    assert_eq!(
        field(&output, "r"),
        "array",
        "the root-qualified spelling read the stale fallback:\n{output}"
    );
}

/// PINS A DIVERGENCE, not parity: a fragment whose text never spells the name.
///
/// ```php
/// $fn = 'opcache_get' . '_status';
/// eval('return ' . $fn . '();');
/// ```
///
/// Reference answers with the status array; elephc answers `false`, because no declaration is
/// injected and the call reaches the interpreter's own builtin, which reports the
/// compile-time CLI default rather than the live cache.
///
/// THE OBVIOUS FIX IS WORSE THAN THE BUG, and this test exists so nobody re-applies it.
/// Treating an unreadable fragment as a mention of every watched name — which is the rule
/// `args_select_subject` uses elsewhere, and which this detector briefly adopted — injects
/// the whole OPcache prelude into any program that calls `eval()` on a computed string.
/// MEASURED: `$c = "echo " . "1;"; eval($c);`, a program with nothing to do with OPcache,
/// stopped compiling — `fixup value out of range` from the assembler, an AArch64 conditional
/// branch no longer reaching its target across 1.8M lines of emitted assembly. A wrong value
/// in a rare spelling is a smaller harm than most `eval()` programs failing to build.
///
/// Closing it properly belongs in `crates/elephc-magician`'s own OPcache builtins, which
/// already run with the configuration this binary installs at startup, so they could read
/// the live cache instead of a baked default and cost nothing in code size.
///
/// FLIP THIS TO PARITY when that lands; do not delete it.
#[test]
fn a_fragment_that_never_spells_the_name_is_a_known_divergence() {
    let dir = make_test_dir("opcache_rt_fragment_computed");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$fn = 'opcache_get' . '_status';
$code = 'return ' . $fn . '();';
echo 'r=', (is_array(eval($code)) ? 'array' : 'false'), "\n";
"#,
    );

    let output = run_binary(&compile_with_flags(
        &dir,
        &["opcache.enable_cli=1"],
        &["--php-version", "8.5"],
    ));

    assert_eq!(
        field(&output, "r"),
        "false",
        "reference answers `array` here; if elephc now does too, this divergence is CLOSED \
         and the test should be flipped to assert parity rather than removed:\n{output}"
    );
}

/// A PENDING RESTART closes admission: nothing new joins the cache between
/// `opcache_reset()` and the restart it schedules.
///
/// `opcache_reset()` schedules rather than flushes — the entries already cached keep
/// answering for the rest of the request, which is pinned elsewhere and is php-src's
/// behaviour. What must NOT happen is a new entry being admitted in that window, because it
/// would survive the flush that follows and outlive the reset that was supposed to clear
/// everything.
///
/// MEASURED against reference PHP 8.5: `compiled=true cached=0`. The compile still
/// SUCCEEDS — php-src reports the compile, not the store — and elephc reported `cached=1`.
///
/// ASSERTING BOTH IS THE TEST. `cached=0` alone would be satisfied by a build where
/// `opcache_compile_file()` simply failed, which is a different and worse answer.
#[test]
fn a_pending_restart_admits_nothing_new() {
    let dir = make_test_dir("opcache_rt_restart_admission");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$p = __DIR__ . '/lib.php';
opcache_reset();
echo 'compiled=', (opcache_compile_file($p) ? 'true' : 'false'), "\n";
echo 'cached=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
"#,
    );

    let output = run_binary(&compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    ));

    assert_eq!(
        field(&output, "compiled"),
        "true",
        "the compile itself still succeeds:\n{output}"
    );
    assert_eq!(
        field(&output, "cached"),
        "0",
        "an entry was admitted while a restart was pending:\n{output}"
    );
}

/// Staleness for `opcache_invalidate()` is the TIMESTAMP ALONE, as php-src's
/// `do_validate_timestamps` is.
///
/// php-src passes a `NULL` size output and compares the mtime, so a rewrite that changes the
/// LENGTH while preserving the timestamp leaves the entry valid. elephc compared the size
/// too — borrowing the warm-hit path's mtime-or-size pair — and retired an entry reference
/// keeps.
///
/// The fixture restores the mtime after rewriting to a different length, and asserts
/// `same_mtime=1` first: without that check the test would pass on a machine where the
/// rewrite happened to land in a new second, for a reason that has nothing to do with the
/// predicate.
///
/// MEASURED against reference PHP 8.5: `still=1`. elephc reported `still=0`.
#[test]
fn a_size_only_rewrite_is_not_stale_to_a_non_forced_invalidate() {
    let dir = make_test_dir("opcache_rt_size_only");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$p = __DIR__ . '/lib.php';
eval('include $p;');
$mtime = intval(filemtime($p));
file_put_contents($p, "<?php \$lib_marker = 2; // a different length entirely\n");
touch($p, $mtime);
echo 'same_mtime=', (filemtime($p) === $mtime ? '1' : '0'), "\n";
echo 'soft=', (opcache_invalidate($p) ? 'true' : 'false'), "\n";
echo 'still=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
"#,
    );

    let output = run_binary(&compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.validate_timestamps=1",
        ],
    ));

    assert_eq!(
        field(&output, "same_mtime"),
        "1",
        "the fixture failed to restore the mtime, so it pins nothing:\n{output}"
    );
    assert_eq!(
        field(&output, "still"),
        "1",
        "a size-only rewrite was treated as stale:\n{output}"
    );
}

/// The NATIVE `opcache_compile_file()` serves a deleted script that is still a warm entry.
///
/// php-src serves the cached script without reopening the file when
/// `validate_timestamps=0`. The native wrapper warned and answered `false` the moment
/// `realpath()` failed. MEASURED: compile, `unlink()`, compile — reference `true, true`.
///
/// The name is literal on purpose: only a literal spelling injects the native wrapper.
#[test]
fn the_native_compile_file_serves_a_deleted_cached_script() {
    let dir = make_test_dir("opcache_rt_native_compile_deleted");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$p = realpath(__DIR__ . '/lib.php');
echo 'first=', var_export(opcache_compile_file($p), true), "\n";
unlink($p);
echo 'second=', var_export(opcache_compile_file($p), true), "\n";
"#,
    );

    let output = run_binary(&compile_with_flags(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.validate_timestamps=0",
        ],
        &["--php-version", "8.5"],
    ));

    assert_eq!(field(&output, "first"), "true", "the file is compiled:\n{output}");
    assert_eq!(field(&output, "second"), "true", "and still served once deleted:\n{output}");
}

/// `opcache_compile_file('')` throws `ValueError: Path must not be empty` once the cache is
/// enabled, on both surfaces — and a disabled cache still answers with its notice.
///
/// Reference raises the error from the open, after the "not properly started" notice. elephc
/// answered `false`: its path normalization turned `''` into the working directory. MEASURED:
/// enabled, reference throws; disabled, it notices and answers `false`. Found by Kimi.
///
/// Separate programs for the two surfaces: a literal name injects the native wrapper, and
/// every eval'd spelling in that program then resolves to it.
#[test]
fn compile_file_refuses_an_empty_path() {
    let run = |name: &str, probe: &str, ini: &[&str]| -> String {
        let dir = make_test_dir(name);
        write_dynamic_fixture(&dir, probe);
        run_binary(&compile_with_flags(&dir, ini, &["--php-version", "8.5"]))
    };
    let native = r#"<?php
eval('$unrelated = 1;' . str_repeat(' ', count($argv) - 1));
try { $r = var_export(opcache_compile_file(''), true); }
catch (\ValueError $e) { $r = $e->getMessage(); }
echo 'r=', $r, "\n";
"#;
    let eval = r#"<?php
$n = 'opcache_' . 'compile_file';
try { $r = var_export(eval('return ' . $n . '("");'), true); }
catch (\ValueError $e) { $r = $e->getMessage(); }
echo 'r=', $r, "\n";
"#;
    let enabled = ["opcache.enable_cli=1"];

    assert_eq!(
        field(&run("opcache_rt_empty_native", native, &enabled), "r"),
        "Path must not be empty"
    );
    assert_eq!(
        field(&run("opcache_rt_empty_eval", eval, &enabled), "r"),
        "Path must not be empty"
    );
    assert_eq!(
        field(&run("opcache_rt_empty_disabled", native, &[]), "r"),
        "false",
        "a disabled cache answers with its notice, before the path is looked at"
    );
}

/// The NATIVE `opcache_compile_file()` names the real failure for a file under a directory
/// it cannot search.
///
/// `realpath()` fails there, so the native body used to take its "missing file" arm and print
/// `No such file or directory`, where reference says `Permission denied`. The warning now comes
/// from the bridge, which holds the real `io::Error`. MEASURED as a non-root user.
///
/// Skipped when the process can traverse the directory anyway (root).
#[test]
fn compile_file_under_an_unsearchable_directory_reports_permission_denied() {
    use std::os::unix::fs::PermissionsExt;
    let dir = make_test_dir("opcache_rt_compile_locked_parent");
    let locked = dir.join("locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("lib.php"), "<?php\n").unwrap();
    write_dynamic_fixture(
        &dir,
        r#"<?php
eval('$unrelated = 1;' . str_repeat(' ', count($argv) - 1));
echo 'r=', var_export(opcache_compile_file(__DIR__ . '/locked/lib.php'), true), "\n";
"#,
    );
    let bin = compile_with_flags(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
        &["--php-version", "8.5"],
    );

    fs::set_permissions(&locked, fs::Permissions::from_mode(0o600)).unwrap();
    let searchable_anyway = fs::metadata(locked.join("lib.php")).is_ok();
    let output = Command::new(&bin).output().expect("failed to run binary");
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
    if searchable_anyway {
        return;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(field(&stdout, "r"), "false", "{stdout}");
    assert!(
        stderr.contains("lib.php): Failed to open stream: Permission denied"),
        "the failure is a permission, not a missing file:\n{stderr}"
    );
    assert!(
        !stderr.contains("No such file or directory"),
        "the file exists; it must not be reported missing:\n{stderr}"
    );
}

/// Both `opcache_compile_file()` surfaces warn `Permission denied` for a file that EXISTS but
/// cannot be read.
///
/// `realpath()` succeeds for it, so the native wrapper's unresolved-path warnings never ran
/// and it answered `false` in silence; the eval handler warned, but always "No such file or
/// directory". MEASURED as a non-root user on a mode-`0000` file: reference prints
/// `Failed to open stream: Permission denied` on both surfaces.
///
/// TWO PROGRAMS, because one cannot reach both: a literal `opcache_compile_file` injects the
/// native declaration, and every eval'd spelling in that program then resolves to it. The
/// eval program never spells the name.
///
/// Skipped when the process can read the file anyway (root), which would make it meaningless.
#[test]
fn compile_file_reports_permission_denied_on_both_surfaces() {
    use std::os::unix::fs::PermissionsExt;
    let run = |name: &str, probe: &str| -> (String, String) {
        let dir = make_test_dir(name);
        write_dynamic_fixture(&dir, probe);
        let bin = compile_with_flags(
            &dir,
            &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
            &["--php-version", "8.5"],
        );
        fs::set_permissions(dir.join("lib.php"), fs::Permissions::from_mode(0)).unwrap();
        let output = Command::new(&bin).output().expect("failed to run binary");
        fs::set_permissions(dir.join("lib.php"), fs::Permissions::from_mode(0o644)).unwrap();
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    let probe_dir = make_test_dir("opcache_rt_compile_denied_probe");
    let probe_file = probe_dir.join("probe.php");
    fs::write(&probe_file, "<?php
").unwrap();
    fs::set_permissions(&probe_file, fs::Permissions::from_mode(0)).unwrap();
    let readable_anyway = fs::read(&probe_file).is_ok();
    fs::set_permissions(&probe_file, fs::Permissions::from_mode(0o644)).unwrap();
    if readable_anyway {
        return;
    }

    let (native_out, native_err) = run(
        "opcache_rt_compile_denied_native",
        r#"<?php
eval('$unrelated = 1;' . str_repeat(' ', count($argv) - 1));
echo 'r=', var_export(opcache_compile_file(__DIR__ . '/lib.php'), true), "
";
"#,
    );
    let (eval_out, eval_err) = run(
        "opcache_rt_compile_denied_eval",
        r#"<?php
$p = __DIR__ . '/lib.php';
$n = 'opcache_' . 'compile_file';
echo 'r=', var_export(eval('return ' . $n . '($p);'), true), "
";
"#,
    );

    for (surface, out, err) in [("native", &native_out, &native_err), ("eval", &eval_out, &eval_err)] {
        assert_eq!(field(out, "r"), "false", "{surface}:
{out}");
        assert!(
            err.contains("lib.php): Failed to open stream: Permission denied"),
            "{surface} must name the real failure:
{err}"
        );
        assert!(
            !err.contains("No such file or directory"),
            "{surface}: the file exists; it must not be reported missing:
{err}"
        );
    }
}

/// The runtime capacity is php-src's hash prime, less the slots the compiled-in scripts hold.
///
/// With `max_accelerated_files=200`, php-src sizes the hash at the prime 223 and admits scripts
/// until it is full — the entry script included. The runtime tier used the raw 200 and refused
/// the 201st; it then used the full 223 and reported more scripts than keys. MEASURED on
/// reference PHP 8.5 with an absolute entry path: 222 dynamic scripts are cached beside the
/// entry, the 223rd compile finds the cache full, and `max_cached_keys` is 223.
#[test]
fn the_runtime_capacity_is_the_prime_less_the_compiled_scripts() {
    let dir = make_test_dir("opcache_rt_capacity_boundary");
    fs::write(
        dir.join("main.php"),
        r#"<?php
$d = __DIR__ . "/many";
@mkdir($d);
for ($i = 0; $i < 230; $i++) {
    $f = "$d/f$i.php";
    file_put_contents($f, "<?php return 1;\n");
    touch($f, 1000000);
}
clearstatcache();
$firstFull = -1;
for ($i = 0; $i < 230; $i++) {
    opcache_compile_file("$d/f$i.php");
    if ($firstFull < 0 && opcache_get_status(false)["cache_full"]) { $firstFull = $i; }
}
$cached = 0;
for ($i = 0; $i < 230; $i++) { if (opcache_is_script_cached("$d/f$i.php")) { $cached++; } }
$s = opcache_get_status(false);
echo "cached=$cached\n";
echo "first_full=$firstFull\n";
echo "scripts=", $s["opcache_statistics"]["num_cached_scripts"], "\n";
echo "keys=", $s["opcache_statistics"]["max_cached_keys"], "\n";
"#,
    )
    .unwrap();

    let out = run_binary(&compile_with_flags(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.max_accelerated_files=200",
        ],
        &["--php-version", "8.5"],
    ));

    assert_eq!(field(&out, "cached"), "222", "{out}");
    assert_eq!(field(&out, "first_full"), "222", "{out}");
    assert_eq!(field(&out, "scripts"), "223", "never more scripts than keys:\n{out}");
    assert_eq!(field(&out, "keys"), "223", "{out}");
}

/// The NATIVE `opcache_compile_file()` warns about a file it cannot open, as the eval one does.
///
/// Reference prints `Failed to open stream`, then `Failed opening ... for inclusion`, and
/// returns `false`. The eval surface already did; the native wrapper returned `false` in
/// silence, so one call spelled two ways disagreed about whether anything went wrong.
///
/// The name is literal on purpose: only a literal spelling injects the native wrapper.
#[test]
fn compile_file_warns_about_a_missing_file() {
    let dir = make_test_dir("opcache_rt_compile_missing_warns");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$gone = __DIR__ . '/never-existed.php';
echo 'r=', var_export(opcache_compile_file($gone), true), "\n";
"#,
    );

    let bin = compile_with_flags(&dir, &["opcache.enable_cli=1"], &["--php-version", "8.5"]);
    let output = Command::new(&bin).output().expect("failed to run binary");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let gone = dir.join("never-existed.php");

    assert_eq!(field(&stdout, "r"), "false", "a missing file is not compiled:\n{stdout}");
    assert!(
        stderr.contains(&format!(
            "Warning: opcache_compile_file({}): Failed to open stream: No such file or directory",
            gone.display()
        )),
        "the open failure must be reported:\n{stderr}"
    );
    assert!(
        stderr.contains(&format!(
            "Warning: opcache_compile_file(): Failed opening '{}' for inclusion",
            gone.display()
        )),
        "and so must the inclusion failure:\n{stderr}"
    );
}

/// The NATIVE `opcache_is_script_cached()` still finds a cached script whose source was deleted.
///
/// php-src looks the entry up before validating, so with `validate_timestamps=0` an unlinked
/// script is still reported cached. The native wrapper normalized the path with `realpath()`
/// and answered `false` the moment that failed, without reaching the cache.
///
/// THE NAMES ARE LITERAL ON PURPOSE: this pins the native wrapper, which only a literal
/// spelling injects.
///
/// MEASURED against reference PHP 8.5: `bool(true)`.
#[test]
fn the_native_query_finds_a_cached_script_whose_source_was_deleted() {
    let dir = make_test_dir("opcache_rt_native_deleted_query");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$p = realpath(__DIR__ . '/lib.php');
opcache_compile_file($p);
unlink($p);
echo 'r=', (opcache_is_script_cached($p) ? 'true' : 'false'), "\n";
"#,
    );

    let output = run_binary(&compile_with_flags(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.validate_timestamps=0",
        ],
        &["--php-version", "8.5"],
    ));

    assert_eq!(field(&output, "r"), "true", "the entry outlives its source:\n{output}");
}

/// A VARIABLE call and `call_user_func_array` reach the eval OPcache handlers, as
/// `call_user_func` always did.
///
/// Inside eval(), `$f()` and `call_user_func_array($f, ...)` resolve a string callee through a
/// dispatch that never consulted the runtime handlers these names live in, so both died on an
/// unsupported construct — for the very spelling two review rounds had already probed.
///
/// THE NAME MUST NEVER APPEAR LITERALLY, here or in any other string of the program: a literal
/// injects the native declaration, the call resolves through it, and the test would pass
/// against the fatal.
///
/// MEASURED against reference PHP 8.5: `var=true cufa=true`.
#[test]
fn a_variable_call_reaches_the_eval_opcache_handlers() {
    let dir = make_test_dir("opcache_rt_eval_variable_call");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$p = __DIR__ . '/lib.php';
eval('include $p;');
echo 'var=', var_export(eval('$f = "opcache_" . "is_script_cached"; return $f($p);'), true), "\n";
echo 'cufa=', var_export(eval('$f = "opcache_" . "is_script_cached"; return call_user_func_array($f, [$p]);'), true), "\n";
"#,
    );

    let output = run_binary(&compile_with_flags(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
        &["--php-version", "8.5"],
    ));

    assert_eq!(field(&output, "var"), "true", "a variable call must reach the handler:\n{output}");
    assert_eq!(field(&output, "cufa"), "true", "so must call_user_func_array:\n{output}");
}

/// The EVAL builtin's `opcache_invalidate()` answers "cached OR resolvable", as the native
/// surface does, for a cached file that has since been DELETED.
///
/// It answered "resolvable" alone, on the reasoning that a cached path always resolves because
/// it was canonicalized when stored — false for exactly one case, the deleted file, which is
/// the case an invalidate most often exists for. Two reviewers found it independently.
///
/// THE NAME IS COMPUTED ON PURPOSE. A literal `opcache_invalidate` anywhere injects the native
/// declaration and the eval'd call dispatches into it, which already answered `true`; only a
/// runtime-assembled name reaches the eval builtin. Spelling it literally here would make this
/// test pass against the broken builtin.
///
/// MEASURED against reference PHP 8.5: `true`.
#[test]
fn the_eval_builtin_reports_invalidating_a_deleted_cached_file() {
    let dir = make_test_dir("opcache_rt_eval_invalidate_deleted");
    write_dynamic_fixture(
        &dir,
        r#"<?php
$p = __DIR__ . '/lib.php';
eval('include $p;');
unlink($p);
$f = 'opcache_' . 'invalidate';
echo 'r=', (eval('return ' . $f . '($p, true);') ? 'true' : 'false'), "\n";
"#,
    );

    let output = run_binary(&compile_with_flags(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
        &["--php-version", "8.5"],
    ));

    assert_eq!(
        field(&output, "r"),
        "true",
        "the entry was cached and is retired; the file being gone does not change that"
    );
}

/// `opcache_invalidate()` WITHOUT `$force` follows php-src's predicate, not "do nothing".
///
/// `accel_invalidate` is `force || !validate_timestamps || the source moved on`, and only
/// the first clause was implemented — so a non-forced call retired nothing, ever. The
/// clause that matters most in production is the second: under
/// `opcache.validate_timestamps=0` nothing is ever re-stated, which makes an explicit
/// `opcache_invalidate()` the ONLY way to retire a script. It did not.
///
/// MEASURED against reference PHP 8.5, the identical sequence under each setting:
///
/// ```text
/// validate_timestamps=1   soft=true  still_cached=1   <- both agree: the file is unchanged
/// validate_timestamps=0   soft=true  still_cached=0   <- reference; elephc reported 1
/// ```
///
/// BOTH SETTINGS ARE ASSERTED and the pair is the test. Making the non-forced call always
/// retire would satisfy the `=0` row on its own while breaking the `=1` row, where
/// reference deliberately keeps an unchanged script cached.
///
/// The return value is unrelated to the eviction: php-src answers whether the PATH
/// RESOLVES, so `soft=true` holds either way and asserting it alone would pin nothing.
#[test]
fn a_non_forced_invalidate_follows_the_timestamp_directive() {
    for (validate, still_cached) in [("1", "1"), ("0", "0")] {
        let dir = make_test_dir("opcache_rt_soft_invalidate");
        write_dynamic_fixture(
            &dir,
            r#"<?php
$p = __DIR__ . '/lib.php';
eval('include $p;');
echo 'cached_before=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
echo 'soft=', (opcache_invalidate($p) ? 'true' : 'false'), "\n";
echo 'still_cached=', (opcache_is_script_cached($p) ? '1' : '0'), "\n";
"#,
        );

        let output = run_binary(&compile(
            &dir,
            &[
                "opcache.enable_cli=1",
                "opcache.file_update_protection=0",
                &format!("opcache.validate_timestamps={validate}"),
            ],
        ));

        assert_eq!(field(&output, "cached_before"), "1", "nothing was cached");
        assert_eq!(field(&output, "soft"), "true", "the path resolves");
        assert_eq!(
            field(&output, "still_cached"),
            still_cached,
            "validate_timestamps={validate}: wrong eviction decision\n{output}"
        );
    }
}

/// The probe: include the same freshly written file twice from `eval()`, then report
/// whether the cache kept it. A stored entry makes the second include a HIT.
const FRESH_FILE_PROBE: &str = r#"<?php
eval('include __DIR__ . "/lib.php"; include __DIR__ . "/lib.php";');
$s = opcache_get_status();
echo 'hits=', $s['opcache_statistics']['hits'], "\n";
echo 'misses=', $s['opcache_statistics']['misses'], "\n";
"#;

/// Verifies `opcache.file_update_protection` refuses to CACHE a just-written file, while
/// still running it.
///
/// The window is set ENORMOUS rather than left at the default 2s, and that is what makes
/// this deterministic: with 2s the assertion depends on the binary starting within two
/// seconds of the fixture being written, which loses under parallel test load and fails for
/// a reason that has nothing to do with the directive. The exact boundary is pinned
/// precisely, and without a clock, by
/// `script_cache::config::tests::file_update_protection_refuses_only_younger_files`.
///
/// `lib.php` is still written AFTER the compile: compilation takes seconds, and a fixture
/// written before it would be measurably older than the test intends.
///
/// Both includes must therefore MISS, and the file must still run: refusing to cache is not
/// refusing to execute.
#[test]
fn a_freshly_written_file_is_run_but_not_cached() {
    let dir = make_test_dir("opcache_fup_fresh");
    write_dynamic_fixture(&dir, FRESH_FILE_PROBE);
    let binary = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=100000"],
    );
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();

    let output = run_binary(&binary);

    // `hits`/`misses` are the signal here, but no longer because the alternative is broken:
    // `opcache_is_script_cached()` DOES answer for the runtime tier now, through
    // `__elephc_opcache_rt_is_cached`. It is simply the weaker probe for this property —
    // it cannot distinguish "ran but was refused storage" from "never included", which is
    // exactly the distinction `file_update_protection` turns on.
    assert_eq!(
        field(&output, "hits"),
        "0",
        "a protected file must never hit; probe said:\n{output}"
    );
    assert_eq!(field(&output, "misses"), "2", "both includes miss");
}

/// Verifies `opcache.file_update_protection=0` disables the guard, so the identically fresh
/// fixture caches immediately and the second include hits.
///
/// This is the differential that proves the refusal above comes from the directive rather
/// than from the file being uncacheable for some other reason.
#[test]
fn zero_file_update_protection_caches_a_fresh_file() {
    let dir = make_test_dir("opcache_fup_off");
    write_dynamic_fixture(&dir, FRESH_FILE_PROBE);
    let binary = compile(
        &dir,
        &["opcache.enable_cli=1", "opcache.file_update_protection=0"],
    );
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();

    let output = run_binary(&binary);

    assert_eq!(
        field(&output, "hits"),
        "1",
        "the second include must hit; probe said:\n{output}"
    );
    assert_eq!(field(&output, "misses"), "1", "only the first include misses");
}

/// Runtime admission accepts the full prime-rounded table capacity, not the raw directive.
#[test]
fn runtime_cache_admits_the_rounded_accelerated_file_capacity() {
    let dir = make_test_dir("opcache_capacity_rounded");
    for index in 0..210 {
        fs::write(
            dir.join(format!("script-{index}.php")),
            format!("<?php return {index};\n"),
        )
        .unwrap();
    }
    fs::write(
        dir.join("main.php"),
        r#"<?php
$cached = 0;
for ($i = 0; $i < 210; $i++) {
    $path = __DIR__ . "/script-" . $i . ".php";
    opcache_compile_file($path);
    if (opcache_is_script_cached($path)) { $cached++; }
}
$status = opcache_get_status();
echo "cached=", $cached, "\n";
echo "num=", $status["opcache_statistics"]["num_cached_scripts"], "\n";
echo "max=", $status["opcache_statistics"]["max_cached_keys"], "\n";
echo "full=", $status["cache_full"] ? "1" : "0", "\n";
"#,
    )
    .unwrap();

    let output = run_binary(&compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.max_accelerated_files=200",
        ],
    ));
    assert_eq!(field(&output, "cached"), "210", "output:\n{output}");
    assert_eq!(field(&output, "num"), "211", "output:\n{output}");
    assert_eq!(field(&output, "max"), "223", "output:\n{output}");
    assert_eq!(field(&output, "full"), "0", "output:\n{output}");
}

/// Runtime-provided eval source cannot bypass `opcache.restrict_api` when no static call is visible.
#[test]
fn runtime_only_eval_opcache_reset_obeys_restrict_api() {
    let dir = make_test_dir("opcache_restrict_eval_only");
    fs::write(
        dir.join("main.php"),
        "<?php $code = getenv('REVIEW_CODE'); eval($code);\n",
    )
    .unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.restrict_api=/nonexistent",
        ],
    );
    let (stdout, stderr) =
        run_binary_with_env(&bin, "REVIEW_CODE", "var_dump(opcache_reset());");

    assert_eq!(stdout, "bool(false)\n");
    assert!(
        stderr.contains("Zend OPcache API is restricted by \"restrict_api\" configuration directive"),
        "restricted eval call emitted no OPcache warning: {stderr}"
    );
}
