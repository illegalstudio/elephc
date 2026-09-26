//! Purpose:
//! End-to-end tests for php-src's STARTUP VALIDATION of `opcache.file_cache` and
//! `opcache.file_cache_read_only` — the one part of the file-cache surface that is
//! observable without an on-disk opcode cache, because reference PHP refuses to start
//! on a bad setting rather than ignoring it.
//!
//! Called from:
//! - `cargo test --test opcache_file_cache_tests` through Rust's test harness.
//!
//! Key details:
//! - Every expectation here was VERIFIED against reference PHP 8.5.10 before being
//!   written; the probes are recorded in each test's own doc comment.
//! - THE VALIDATION IS GATED ON THE CACHE BEING ENABLED. That is reference behaviour,
//!   not an elephc shortcut, which is why the default-CLI test asserts a clean run with
//!   a deliberately broken path.
//! - The fatal is raised from the PROLOGUE, where the OPcache configuration is installed,
//!   so it precedes the program's first statement exactly as reference's MINIT fatal does.
//!   It used to be raised when the eval context was built, which is why older fixtures here
//!   all contain an `eval()`; that is now incidental rather than required. A binary with no
//!   dynamic tier still never configures the cache and so never validates.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an isolated
//!   temp dir, the same harness style as `opcache_runtime_cache_tests`. Host-target only.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_ID: AtomicUsize = AtomicUsize::new(0);

/// php-src's message for a `opcache.file_cache` that is not a usable directory.
const BAD_DIRECTORY: &str = "opcache.file_cache must be a full path of an accessible directory";

/// php-src's message for `opcache.file_cache_read_only` with no path to read from.
const READ_ONLY_WITHOUT_PATH: &str =
    "opcache.file_cache_read_only is set without a proper setting of opcache.file_cache";

/// The exit status php-src's `exit(-2)` produces, as the shell sees it.
const ACCEL_FATAL_STATUS: i32 = 254;

/// Creates an isolated temp dir unique across parallel test threads/processes.
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

/// Writes an entry script that reaches `eval()`, which is where the cache is configured.
fn write_fixture(dir: &Path) {
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\neval('include __DIR__ . \"/lib.php\";');\necho \"RAN\\n\";\n",
    )
    .unwrap();
}

/// Compiles `<dir>/main.php` with the supplied `--ini` assignments and returns the executable.
fn compile(dir: &Path, ini: &[&str]) -> PathBuf {
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg(dir.join("main.php"));
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

/// Returns the value of a `key=value` line the probe printed.
fn field<'a>(output: &'a str, key: &str) -> &'a str {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .unwrap_or_else(|| panic!("probe printed no `{key}=` line; output was:\n{output}"))
}

/// Builds a fixture with the given directives and runs it, returning the raw output.
fn compile_and_run(prefix: &str, ini: &[&str]) -> (PathBuf, Output) {
    let dir = make_test_dir(prefix);
    write_fixture(&dir);
    let bin = compile(&dir, ini);
    let output = Command::new(&bin).output().expect("failed to run binary");
    (dir, output)
}

/// Asserts the run produced php-src's accelerator fatal, on stderr, with its exit status.
///
/// The line shape is `zend_accelerator_debug.c`'s: a 24-character `asctime` timestamp, the
/// pid in parentheses, then `Fatal Error ` INCLUDING its trailing space, then the message.
fn assert_accel_fatal(output: &Output, message: &str) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(ACCEL_FATAL_STATUS),
        "expected php-src's exit(-2); stderr was:\n{stderr}"
    );
    assert!(
        stderr.contains(&format!("Fatal Error {message}")),
        "expected the accelerator fatal for {message:?}; stderr was:\n{stderr}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("RAN"),
        "the program ran despite a startup fatal"
    );
}

/// Asserts the run completed normally and the program body executed.
fn assert_ran_cleanly(output: &Output) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "expected a clean run; stderr was:\n{stderr}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("RAN"),
        "the program body did not run; stderr was:\n{stderr}"
    );
    assert!(
        !stderr.contains("Fatal Error"),
        "an accelerator fatal was reported on a valid setting:\n{stderr}"
    );
}

/// Verifies a missing `opcache.file_cache` directory fatals exactly as reference PHP does.
///
/// VERIFIED on 8.5.10: `-d opcache.enable=1 -d opcache.enable_cli=1
/// -d opcache.file_cache=/no/such/dir` prints the fatal and exits 254 without running.
#[test]
fn a_missing_file_cache_directory_is_a_startup_fatal() {
    let (_dir, output) = compile_and_run(
        "opcache_fc_missing",
        &[
            "opcache.enable_cli=1",
            "opcache.file_cache=/no/such/directory",
        ],
    );

    assert_accel_fatal(&output, BAD_DIRECTORY);
}

/// Verifies a DISABLED cache never validates the directory, however broken it is.
///
/// This is the branch that keeps a default CLI binary working. VERIFIED on 8.5.10 that
/// `-d opcache.enable=0` and the CLI default `enable_cli=0` both run the script with no
/// validation at all.
#[test]
fn a_disabled_cache_never_validates_the_directory() {
    let (_dir, output) = compile_and_run(
        "opcache_fc_disabled",
        &["opcache.file_cache=/no/such/directory"],
    );

    assert_ran_cleanly(&output);
}

/// Verifies a relative path is refused even though it exists and is a directory.
///
/// VERIFIED on 8.5.10: `-d opcache.file_cache=.` fatals. The directive is documented as
/// requiring a "full path", and php-src tests `IS_ABSOLUTE_PATH` before it stats.
#[test]
fn a_relative_file_cache_directory_is_refused() {
    let (_dir, output) = compile_and_run(
        "opcache_fc_relative",
        &["opcache.enable_cli=1", "opcache.file_cache=."],
    );

    assert_accel_fatal(&output, BAD_DIRECTORY);
}

/// Verifies an existing FILE is refused, since php-src requires the entry to be a directory.
///
/// VERIFIED on 8.5.10 with `-d opcache.file_cache=/etc/hosts`.
#[test]
fn a_file_instead_of_a_directory_is_refused() {
    let (_dir, output) = compile_and_run(
        "opcache_fc_file",
        &["opcache.enable_cli=1", "opcache.file_cache=/etc/hosts"],
    );

    assert_accel_fatal(&output, BAD_DIRECTORY);
}

/// Verifies a writable absolute directory is accepted and the program runs.
#[test]
fn a_writable_absolute_directory_is_accepted() {
    let dir = make_test_dir("opcache_fc_ok");
    write_fixture(&dir);
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            &format!("opcache.file_cache={}", cache.display()),
        ],
    );

    let output = Command::new(&bin).output().expect("failed to run binary");

    assert_ran_cleanly(&output);
}

/// Whether this process can actually create a file in `/`.
///
/// Measured rather than assumed: root bypasses permission checks, so `/` is writable for it
/// even though its mode says otherwise, and CI runs as root on the Linux runners. The probe
/// file is removed immediately and is only ever created when the answer is already yes.
fn root_directory_is_writable() -> bool {
    let probe = Path::new("/").join(format!(".elephc-write-probe-{}", std::process::id()));
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// Verifies the access mode follows `opcache.file_cache_read_only`.
///
/// `/` is the fixture because it is readable and, for an ordinary user, not writable — so it
/// separates the two modes without creating anything. VERIFIED on 8.5.10 BOTH ways:
/// read-write fatals, read-only runs. php-src asks `access()` for `R_OK | W_OK` normally and
/// `R_OK` alone when read-only.
///
/// The read-write half is skipped where `/` really is writable — as root, which is how the
/// Linux CI runners execute. Reference PHP accepts the directory there too, so a refusal
/// would be the wrong assertion rather than a stricter one. The read-only half runs
/// everywhere and is the half that discriminates the directive.
#[test]
fn the_access_mode_follows_read_only() {
    if !root_directory_is_writable() {
        let (_write_dir, write_output) = compile_and_run(
            "opcache_fc_rw",
            &["opcache.enable_cli=1", "opcache.file_cache=/"],
        );
        assert_accel_fatal(&write_output, BAD_DIRECTORY);
    }

    let (_read_dir, read_output) = compile_and_run(
        "opcache_fc_ro",
        &[
            "opcache.enable_cli=1",
            "opcache.file_cache=/",
            "opcache.file_cache_read_only=1",
        ],
    );

    assert_ran_cleanly(&read_output);
}

/// Verifies `opcache.file_cache_read_only` without a path raises its OWN distinct fatal.
///
/// VERIFIED on 8.5.10: the message differs from the directory one, and an EMPTY
/// `opcache.file_cache` triggers it just as an absent one does.
#[test]
fn read_only_without_a_path_has_its_own_fatal() {
    let (_dir, output) = compile_and_run(
        "opcache_fc_ro_nopath",
        &["opcache.enable_cli=1", "opcache.file_cache_read_only=1"],
    );

    assert_accel_fatal(&output, READ_ONLY_WITHOUT_PATH);
}

/// Verifies an unset `opcache.file_cache` validates nothing, matching php-src's NULL default.
#[test]
fn an_unset_file_cache_validates_nothing() {
    let (_dir, output) = compile_and_run("opcache_fc_unset", &["opcache.enable_cli=1"]);

    assert_ran_cleanly(&output);
}

/// Verifies the fatal still prints at `opcache.log_verbosity_level=0`.
///
/// VERIFIED on 8.5.10: the gate is `level <= verbosity` and FATAL is level 0, so it passes
/// even at 0. The error HANDLING is outside the gate in php-src regardless, so the exit
/// status is 254 either way.
#[test]
fn a_fatal_survives_verbosity_zero() {
    let (_dir, output) = compile_and_run(
        "opcache_fc_verbosity",
        &[
            "opcache.enable_cli=1",
            "opcache.log_verbosity_level=0",
            "opcache.file_cache=/no/such/directory",
        ],
    );

    assert_accel_fatal(&output, BAD_DIRECTORY);
}

/// Verifies the bad-directory fatal happens BEFORE the program runs, as reference does.
///
/// THIS TEST USED TO PIN THE OPPOSITE. elephc installed its OPcache configuration at the
/// first `eval()`, so the startup validation of `opcache.file_cache` ran there too: a
/// program that echoed before its first eval printed that output and only then fatalled,
/// while reference PHP validates during module startup and prints nothing at all. The test
/// recorded the ordering gap and existed to stop it widening.
///
/// Moving the configuration into the prologue closed it. The fatal now precedes the
/// program's first statement, so the assertion flips: `BEFORE` must NOT appear.
///
/// The message and exit status were already identical and stay asserted, because ordering
/// is not the only thing that could regress here.
#[test]
fn the_bad_directory_fatal_precedes_the_program() {
    let dir = make_test_dir("opcache_fc_order");
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\necho \"BEFORE\\n\";\neval('include __DIR__ . \"/lib.php\";');\necho \"RAN\\n\";\n",
    )
    .unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_cache=/no/such/directory",
        ],
    );

    let output = Command::new(&bin).output().expect("failed to run binary");

    assert_accel_fatal(&output, BAD_DIRECTORY);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("BEFORE"),
        "the fatal must precede the program's own output, as reference PHP's does: {stdout:?}"
    );
    assert!(!stdout.contains("RAN"), "the program must not run: {stdout:?}");
}

/// Verifies a CONST-FOLDED `eval()` never reaches the bridge, and so never validates.
///
/// `eval('$x = 1;')` with a constant argument is resolved at compile time and emits no
/// bridge call, so the program has no runtime cache to configure. Pinning it records the
/// real shape of the divergence: it is not "a binary without `eval()`", it is "a binary
/// that never reaches the bridge at run time", which is a strictly wider set.
#[test]
fn a_const_folded_eval_never_reaches_the_validation() {
    let dir = make_test_dir("opcache_fc_folded");
    fs::write(
        dir.join("main.php"),
        "<?php\neval('$x = 1;');\necho \"RAN\\n\";\n",
    )
    .unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_cache=/no/such/directory",
        ],
    );

    let output = Command::new(&bin).output().expect("failed to run binary");

    assert_ran_cleanly(&output);
}

/// Verifies `opcache.error_log` redirects the accelerator channel away from stderr.
///
/// php-src writes accelerator diagnostics to that file when it is set, falling back to
/// stderr only when it cannot be opened. The exit status is unchanged.
#[test]
fn error_log_receives_the_fatal_instead_of_stderr() {
    let dir = make_test_dir("opcache_fc_errorlog");
    write_fixture(&dir);
    let log = dir.join("accel.log");
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_cache=/no/such/directory",
            &format!("opcache.error_log={}", log.display()),
        ],
    );

    let output = Command::new(&bin).output().expect("failed to run binary");

    assert_eq!(
        output.status.code(),
        Some(ACCEL_FATAL_STATUS),
        "expected php-src's exit(-2)"
    );
    let logged = fs::read_to_string(&log).expect("opcache.error_log was never written");
    assert!(
        logged.contains(&format!("Fatal Error {BAD_DIRECTORY}")),
        "the fatal did not reach opcache.error_log; it held:\n{logged}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("Fatal Error"),
        "the fatal was also written to stderr"
    );
}

/// The probe: include a file from `eval()`, and report whether the ON-DISK cache holds it.
///
/// `opcache_is_script_cached_in_file_cache()` is asked BEFORE the include on purpose — that
/// is what separates a cold first process from a warm second one sharing the directory.
const FILE_CACHE_PROBE: &str = r#"<?php
eval('
$f = __DIR__ . "/lib.php";
echo "before=", (opcache_is_script_cached_in_file_cache($f) ? "T" : "F"), "\n";
include $f;
echo "after=", (opcache_is_script_cached_in_file_cache($f) ? "T" : "F"), "\n";
');
"#;

/// Verifies the on-disk cache survives the process that wrote it.
///
/// This is the whole point of `opcache.file_cache`: the runtime script cache is per-process
/// and dies with it, so a worker that starts cold re-reads and re-parses every dynamically
/// included file. An entry on disk is what makes the SECOND process start warm.
///
/// Run twice, same binary, same cache directory:
/// - run 1 finds nothing on disk (`before=F`), parses, and writes the entry (`after=T`);
/// - run 2 is a brand-new process with an empty in-memory cache, and finds the entry
///   already there (`before=T`).
///
/// `before=T` on the second run is the assertion that cannot pass without a real file
/// cache — no in-process state survives between two `Command` invocations.
#[test]
fn the_file_cache_outlives_the_process_that_wrote_it() {
    let dir = make_test_dir("opcache_fc_persist");
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(dir.join("main.php"), FILE_CACHE_PROBE).unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            &format!("opcache.file_cache={}", cache.display()),
        ],
    );

    let first = run_binary(&bin);
    let second = run_binary(&bin);

    assert_eq!(field(&first, "before"), "F", "a cold directory holds nothing");
    assert_eq!(field(&first, "after"), "T", "the include writes the entry");
    assert_eq!(
        field(&second, "before"),
        "T",
        "a NEW process must find the entry the previous one left:\n{second}"
    );
}

/// Verifies a NON-forced invalidate still drops the on-disk entry, even when it
/// deliberately KEEPS the in-memory one.
///
/// php-src calls `zend_file_cache_invalidate` OUTSIDE the
/// `force || !validate_timestamps || stale` test, so the two tiers are not retired together:
/// an unchanged file under `validate_timestamps=1` keeps its memory entry and loses its disk
/// entry. elephc returned before reaching the removal, so the disk copy survived — and the
/// memory entry dies with the process anyway, which makes the surviving disk copy the one
/// that decides what the next process runs.
///
/// MEASURED against reference PHP 8.5: `mem_after=1 disk_after=0`. elephc reported
/// `disk_after=1`.
///
/// THE PAIR IS THE TEST. `disk_after=0` alone would be satisfied by a build that retired
/// both tiers, which is the OTHER wrong answer — reference keeps the memory entry here, and
/// `mem_after=1` is what says so.
#[test]
fn a_non_forced_invalidate_still_drops_the_on_disk_entry() {
    let dir = make_test_dir("opcache_fc_soft_invalidate");
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
$p = __DIR__ . '/lib.php';
eval('include $p;');
echo "disk_before=", (opcache_is_script_cached_in_file_cache($p) ? "T" : "F"), "\n";
opcache_invalidate($p);
echo "mem_after=", (opcache_is_script_cached($p) ? "T" : "F"), "\n";
echo "disk_after=", (opcache_is_script_cached_in_file_cache($p) ? "T" : "F"), "\n";
"#,
    )
    .unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.validate_timestamps=1",
            &format!("opcache.file_cache={}", cache.display()),
        ],
    );

    let out = run_binary(&bin);

    assert_eq!(field(&out, "disk_before"), "T", "the include writes the entry");
    assert_eq!(
        field(&out, "mem_after"),
        "T",
        "an unchanged file keeps its memory entry, as reference does:\n{out}"
    );
    assert_eq!(
        field(&out, "disk_after"),
        "F",
        "the disk entry must go regardless of the memory predicate:\n{out}"
    );
}

/// Verifies `opcache.validate_timestamps=0` reaches the ON-DISK cache, not just memory.
///
/// Under that directive php-src does not re-check timestamps at all, and a stored entry is
/// served even when the source has moved on — that IS the setting's purpose in production:
/// the deployment swaps files and restarts, and until it does, the cache is authoritative.
///
/// elephc validated the disk entry's mtime and size unconditionally, on the reasoning that
/// the directive "governs how often an in-memory entry is re-checked" and does not license
/// running a stale file from disk. Reference disagrees, and the difference is visible as the
/// WRONG VERSION OF THE SCRIPT RUNNING — the second process re-read the changed file where
/// reference replayed the stored one.
///
/// THE OUTPUT IS THE ASSERTION, not just the `before` flag: a build could report the entry
/// present and still recompile it. MEASURED against reference PHP 8.5: run 2 prints
/// `VERSION-A`, the stored text, after the source became `VERSION-B`.
///
/// The path identity check stays unconditional and is unaffected — entries are keyed by a
/// hash of the canonical path, and this directive says nothing about collisions.
#[test]
fn validate_timestamps_off_serves_the_stored_disk_entry() {
    let dir = make_test_dir("opcache_fc_novalidate");
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
eval('
$f = __DIR__ . "/lib.php";
echo "before=", (opcache_is_script_cached_in_file_cache($f) ? "T" : "F"), "\n";
include $f;
');
"#,
    )
    .unwrap();
    fs::write(dir.join("lib.php"), "<?php echo \"VERSION-A\\n\";\n").unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            "opcache.validate_timestamps=0",
            &format!("opcache.file_cache={}", cache.display()),
        ],
    );

    let first = run_binary(&bin);
    fs::write(
        dir.join("lib.php"),
        "<?php echo \"VERSION-B-is-a-longer-file\\n\";\n",
    )
    .unwrap();
    let second = run_binary(&bin);

    assert_eq!(field(&first, "before"), "F", "a cold directory holds nothing");
    assert!(first.contains("VERSION-A"), "run 1 must run the original:\n{first}");
    assert_eq!(
        field(&second, "before"),
        "T",
        "the stored entry must still be found after the source changed:\n{second}"
    );
    assert!(
        second.contains("VERSION-A"),
        "with validate_timestamps=0 the STORED text must run, not the rewritten file:\n{second}"
    );
}

/// `call_user_func('opcache_is_script_cached_in_file_cache', $f)` agrees with the direct call.
///
/// The by-values dispatch arm answered a constant `false`, under a comment calling it terminal
/// because "the file cache does not exist" — which this branch made untrue by shipping one.
/// Its direct twin had been migrated to read the disk; the arm had not, so the interpreter's
/// two spellings of one name disagreed about one file in one process.
///
/// AN ENTRY MUST ACTUALLY BE ON DISK, and the name must be computed. With no entry both
/// spellings rightly answer `false`, and with a literal name the native declaration takes both
/// calls — either way the test would pass against the stub.
///
/// MEASURED against reference PHP 8.5: `direct=1 cuf=1`. elephc printed `direct=1 cuf=0`.
#[test]
fn call_user_func_agrees_with_the_direct_file_cache_query() {
    let dir = make_test_dir("opcache_fc_cuf");
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
$lib = __DIR__ . '/lib.php';
eval('include $lib;');
$n = 'opcache_' . 'is_script_cached_in_file_cache';
echo 'direct=', (eval('return ' . $n . '($lib);') ? '1' : '0'), "\n";
echo 'cuf=', (eval('return call_user_func($n, $lib);') ? '1' : '0'), "\n";
"#,
    )
    .unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            "opcache.file_update_protection=0",
            &format!("opcache.file_cache={}", cache.display()),
        ],
    );

    let out = run_binary(&bin);

    assert_eq!(field(&out, "direct"), "1", "the entry is on disk:\n{out}");
    assert_eq!(
        field(&out, "cuf"),
        "1",
        "call_user_func must reach the same disk the direct call reads:\n{out}"
    );
}

/// A binary that never calls `eval()` still sees an entry another process left on disk.
///
/// Reference answers `true`: a persistent file cache does not need this reader to have filled
/// a memory cache first. The query folded to a constant `false` in a binary that links no eval
/// bridge, because the pay-for-use rule assumed nothing could have been cached without a
/// dynamic tier — which a cache SHARED WITH OTHER PROCESSES makes untrue. Naming a file-cache
/// operation under a configured `opcache.file_cache` now links the bridge, as
/// `opcache_compile_file()` already did; a build with no file cache still folds.
///
/// THE ABSENCE OF `eval` IN THE READER IS THE TEST, and the premise check below that the seed
/// left an entry is what keeps a `true` from being luck.
#[test]
fn a_reader_without_eval_sees_the_disk_cache() {
    let root = make_test_dir("opcache_fc_standalone_reader");
    let cache = root.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    let lib = root.join("lib.php");
    fs::write(&lib, "<?php $lib_marker = 1;\n").unwrap();
    let ini = [
        "opcache.enable_cli=1".to_string(),
        "opcache.file_update_protection=0".to_string(),
        format!("opcache.file_cache={}", cache.display()),
    ];
    let ini: Vec<&str> = ini.iter().map(String::as_str).collect();

    let seed = root.join("seed");
    fs::create_dir_all(&seed).unwrap();
    fs::write(
        seed.join("main.php"),
        format!("<?php\n$lib = '{}';\neval('include $lib;');\n", lib.display()),
    )
    .unwrap();
    run_binary(&compile(&seed, &ini));
    // THE PREMISE: without an entry on disk `0` is simply right, and this would pin nothing.
    let entries = fs::read_dir(&cache)
        .unwrap()
        .flatten()
        .filter_map(|build| fs::read_dir(build.path()).ok())
        .flatten()
        .flatten()
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "bin"))
        .count();
    assert_eq!(entries, 1, "the seed process must leave exactly one disk entry");

    let reader = root.join("reader");
    fs::create_dir_all(&reader).unwrap();
    fs::write(
        reader.join("main.php"),
        format!(
            "<?php\necho 'r=', (opcache_is_script_cached_in_file_cache('{}') ? '1' : '0'), \"\\n\";\n",
            lib.display()
        ),
    )
    .unwrap();
    let out = run_binary(&compile(&reader, &ini));

    assert!(
        !fs::read_to_string(reader.join("main.php")).unwrap().contains("eval("),
        "PREMISE: the reader must not call eval(), or the bridge was linked for another reason"
    );
    assert_eq!(field(&out, "r"), "1", "the entry another process left is on disk:\n{out}");
}

/// The file-cache operations link the eval bridge ONLY when a file cache is configured.
///
/// Both sides of the gate are the test. Without `opcache.file_cache` — php-src's default —
/// the query and `opcache_invalidate()` must keep folding to constants, or every program
/// that merely names them would carry the interpreter. With it, they must reach the bridge,
/// or the disk another process shares would be invisible (see
/// `a_reader_without_eval_sees_the_disk_cache`). Read from the emitted assembly, which names
/// the configure bridge exactly when the interpreter is linked.
#[test]
fn file_cache_operations_link_the_bridge_only_under_a_file_cache() {
    let emit = |name: &str, with_file_cache: bool| -> String {
        let dir = make_test_dir(name);
        let cache = dir.join("file-cache");
        fs::create_dir_all(&cache).unwrap();
        fs::write(
            dir.join("main.php"),
            "<?php\nvar_dump(opcache_is_script_cached_in_file_cache(__FILE__));\nvar_dump(opcache_invalidate(__FILE__));\n",
        )
        .unwrap();
        let mut cmd = Command::new(elephc_bin());
        cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
        cmd.current_dir(&dir);
        cmd.arg(dir.join("main.php")).arg("--emit-asm");
        cmd.arg("--ini").arg("opcache.enable_cli=1");
        if with_file_cache {
            cmd.arg("--ini")
                .arg(format!("opcache.file_cache={}", cache.display()));
        }
        let output = cmd.output().expect("failed to spawn elephc");
        assert!(
            output.status.success(),
            "compilation failed:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        fs::read_to_string(dir.join("main.s")).expect("--emit-asm writes main.s")
    };

    assert!(
        !emit("opcache_fc_gate_off", false).contains("__elephc_eval_configure_opcache"),
        "with no file cache configured, naming these functions must not link the interpreter"
    );
    assert!(
        emit("opcache_fc_gate_on", true).contains("__elephc_eval_configure_opcache"),
        "under a configured file cache they must reach the bridge"
    );
}

/// A `--web` worker keeps serving after its `opcache.file_cache` directory is removed.
///
/// The directory is validated at STARTUP in php-src, once. The configure bridge also runs at
/// the top of every request, and it re-ran that validation: removing the directory after the
/// first request made the next one exit the worker with the startup fatal, and the server
/// stopped accepting connections. MEASURED — `LIB OK`, then an empty reply, then a refused
/// connection.
#[test]
fn a_web_worker_survives_its_file_cache_directory_being_removed() {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::time::{Duration, Instant};

    let dir = make_test_dir("opcache_fc_web_rmdir");
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(dir.join("lib.php"), "<?php echo \"LIB\\n\";\n").unwrap();
    fs::write(
        dir.join("main.php"),
        "<?php\n$p = __DIR__ . '/lib.php';\neval('include $p;');\necho \"OK\\n\";\n",
    )
    .unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(&dir);
    cmd.arg(dir.join("main.php")).arg("--web");
    for assignment in [
        "opcache.enable_cli=1".to_string(),
        "opcache.file_update_protection=0".to_string(),
        format!("opcache.file_cache={}", cache.display()),
    ] {
        cmd.arg("--ini").arg(assignment);
    }
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "compilation failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let port = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let addr = format!("127.0.0.1:{port}");
    let mut server = Command::new(dir.join("main"))
        .arg("--listen")
        .arg(&addr)
        .arg("--workers")
        .arg("1")
        .current_dir(&dir)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("failed to spawn web server");
    let deadline = Instant::now() + Duration::from_secs(10);
    while TcpStream::connect(&addr).is_err() {
        assert!(Instant::now() < deadline, "server did not start listening on {addr}");
        std::thread::sleep(Duration::from_millis(25));
    }
    let get = || -> String {
        let Ok(mut stream) = TcpStream::connect(&addr) else {
            return "<refused>".to_string();
        };
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let request = format!("GET / HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
        if stream.write_all(request.as_bytes()).is_err() {
            return "<write failed>".to_string();
        }
        let mut response = Vec::new();
        let _ = stream.read_to_end(&mut response);
        String::from_utf8_lossy(&response).into_owned()
    };

    let first = get();
    fs::remove_dir_all(&cache).unwrap();
    let second = get();
    let third = get();
    let _ = server.kill();
    let _ = server.wait();
    let _ = Command::new("pkill").arg("-f").arg(format!("listen {addr}")).status();

    assert!(first.contains("OK"), "PREMISE: the first request is served:\n{first}");
    assert!(
        second.contains("OK"),
        "the request after the directory vanished must still run:\n{second}"
    );
    assert!(third.contains("OK"), "and the server must keep accepting:\n{third}");
}

/// Verifies a FORCED invalidate also removes the entry from disk, not just from memory.
///
/// php-src's `accel_invalidate` calls `zend_file_cache_invalidate` alongside the in-memory
/// eviction. elephc dropped only the in-memory entry, so the on-disk copy survived — and
/// the in-memory cache dies with the process anyway. The script therefore came back from
/// the dead in the very next process, which is the one outcome an invalidate must prevent.
///
/// THE SECOND PROCESS IS THE ASSERTION. Checking the first run proves nothing: it would
/// report the file uncached from the in-memory eviction alone, which already worked. Only a
/// new process with an empty in-memory cache can tell whether the DISK entry went.
///
/// MEASURED against reference: invalidating removes one entry from the cache directory
/// (reference holds two there, having also cached its own entry script).
#[test]
fn a_forced_invalidate_removes_the_on_disk_entry_too() {
    let dir = make_test_dir("opcache_fc_invalidate");
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(
        dir.join("main.php"),
        r#"<?php
eval('
$f = __DIR__ . "/lib.php";
echo "before=", (opcache_is_script_cached_in_file_cache($f) ? "T" : "F"), "\n";
include $f;
echo "after=", (opcache_is_script_cached_in_file_cache($f) ? "T" : "F"), "\n";
if (getenv("ELEPHC_PROBE_INVALIDATE") !== false) {
    opcache_invalidate($f, true);
}
echo "post=", (opcache_is_script_cached_in_file_cache($f) ? "T" : "F"), "\n";
');
"#,
    )
    .unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            &format!("opcache.file_cache={}", cache.display()),
        ],
    );

    // Run 1 writes the entry and then invalidates it.
    let first = {
        let output = Command::new(&bin)
            .env("ELEPHC_PROBE_INVALIDATE", "1")
            .output()
            .expect("failed to run binary");
        assert!(output.status.success(), "run 1 failed: {output:?}");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    // Run 2 is a fresh process: its in-memory cache is empty, so `before` reads DISK.
    let second = run_binary(&bin);

    assert_eq!(field(&first, "after"), "T", "the include must write the entry");
    assert_eq!(
        field(&first, "post"),
        "F",
        "the forced invalidate must drop the on-disk entry:\n{first}"
    );
    assert_eq!(
        field(&second, "before"),
        "F",
        "a NEW process still found the invalidated entry on disk:\n{second}"
    );
}

/// Verifies an edited source is not served from disk by a later process.
///
/// The stored entry records the source's mtime and size; rewriting the file invalidates it,
/// so the second process must find nothing rather than run the previous contents. This is
/// the check that keeps the file cache from turning a code change into a silent no-op.
#[test]
fn an_edited_source_invalidates_its_disk_entry() {
    let dir = make_test_dir("opcache_fc_edit");
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(dir.join("main.php"), FILE_CACHE_PROBE).unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            &format!("opcache.file_cache={}", cache.display()),
        ],
    );
    let first = run_binary(&bin);
    assert_eq!(field(&first, "after"), "T", "the entry was written");

    // THE TIMESTAMP IS MOVED EXPLICITLY. Freshness is the mtime alone, as php-src's
    // `do_validate_timestamps` is, and mtime has one-second resolution — so a rewrite landing
    // in the same second is not a change to either engine. This used to rely on the new
    // length instead, which pinned a rule reference does not have: reference replays the
    // stored script when only the size moved.
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 2; $extra = 'changed';\n").unwrap();
    fs::File::options()
        .write(true)
        .open(dir.join("lib.php"))
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(900_000))
        .unwrap();
    let second = run_binary(&bin);

    assert_eq!(
        field(&second, "before"),
        "F",
        "an edited source must not be served from disk:\n{second}"
    );
}

/// Verifies `opcache.file_cache_read_only` reads entries but never creates them.
#[test]
fn read_only_never_writes_an_entry() {
    let dir = make_test_dir("opcache_fc_ro");
    let cache = dir.join("file-cache");
    fs::create_dir_all(&cache).unwrap();
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 1;\n").unwrap();
    fs::write(dir.join("main.php"), FILE_CACHE_PROBE).unwrap();
    let bin = compile(
        &dir,
        &[
            "opcache.enable_cli=1",
            &format!("opcache.file_cache={}", cache.display()),
            "opcache.file_cache_read_only=1",
        ],
    );

    let first = run_binary(&bin);
    let second = run_binary(&bin);

    assert_eq!(field(&first, "after"), "F", "read-only stores nothing");
    assert_eq!(field(&second, "before"), "F", "so the next process finds nothing");
}
