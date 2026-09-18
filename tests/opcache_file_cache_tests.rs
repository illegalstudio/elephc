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
//! - The fatal is raised while the eval context is built, so every fixture reaches an
//!   `eval()`. A binary with no dynamic tier never configures the cache and so never
//!   validates — the residual divergence documented in `docs/php/opcache.md`.
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

/// Verifies output written before the first bridge-reaching `eval()` precedes the fatal.
///
/// This is the documented position divergence: reference PHP validates before the script
/// runs and so prints nothing, while elephc validates where the cache is configured. The
/// MESSAGE and the EXIT STATUS are identical — only the ordering differs, which is what
/// this pins so a future change cannot quietly widen the gap.
#[test]
fn output_before_the_first_eval_precedes_the_fatal() {
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
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("BEFORE"),
        "output before the eval should already be flushed"
    );
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

    // A different length, so the size check alone is enough even if the mtime lands in the
    // same whole second.
    fs::write(dir.join("lib.php"), "<?php $lib_marker = 2; $extra = 'changed';\n").unwrap();
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
