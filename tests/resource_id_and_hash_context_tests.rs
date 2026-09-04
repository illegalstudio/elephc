//! Purpose:
//! End-to-end tests for two measured PHP-compliance fixes that ship together because
//! both are about a `HashContext` being a resource with a life cycle:
//!
//! - RESOURCE IDS. A PHP resource displays as a small integer allocated in creation
//!   order — `resource(5) of type (stream)`, `get_resource_id($r) === 5`,
//!   `(int) $r === 5`. elephc derived that integer from the resource's NATIVE
//!   payload plus one, which is only coincidentally right for a file descriptor.
//!   An elephc-crypto `HashContext` handle is a malloc'd address, so
//!   `var_dump(hash_init('md5'))` printed `resource(4318862849) of type (stream)`:
//!   a raw heap address leaked into program stdout, and a DIFFERENT one on every
//!   run because of ASLR, so no program could produce stable output and no test
//!   could assert one. The fix is a resource-id registry
//!   (`src/codegen_support/runtime/resource_ids.rs`) that mints ids from a counter
//!   at creation; every display path now asks it instead of doing arithmetic on a
//!   payload.
//! - FINALIZED HASH CONTEXTS. PHP rejects `hash_update()`, `hash_final()` and
//!   `hash_copy()` on a context a previous `hash_final()` already consumed, with a
//!   catchable `TypeError`. elephc silently kept hashing the still-live handle and
//!   returned a plausible WRONG digest — `hash_update($c,"abc"); hash_final($c);
//!   hash_update($c,"def"); hash_final($c)` gave `md5("abcdef")`. The fix records
//!   finalization in elephc-crypto (`elephc_crypto_is_finalized`) and makes each of
//!   `__rt_hash_update` / `__rt_hash_final` / `__rt_hash_copy` raise PHP's exact
//!   TypeError before touching the context.
//!
//! Called from:
//! - `cargo test --test resource_id_and_hash_context_tests` through Rust's test harness.
//!
//! Key details:
//! - Harness style mirrors `tests/array_flip_assoc_tests.rs`: the elephc CLI
//!   (`CARGO_BIN_EXE_elephc`) runs as a subprocess in an isolated temp dir, compiles to a
//!   plain executable, runs it, and its stdout is asserted. Host-target only.
//! - EVERY expected string in this file was captured from reference PHP 8.5.6 with
//!   `php -d xdebug.mode=off` — mandatory, because the host `php` loads Xdebug, which
//!   overloads `var_dump`. Each test names the program it came from.
//! - Fixture files are created INSIDE the test's temp dir rather than reusing
//!   `/etc/hosts`, so the tests do not depend on host filesystem layout. What matters
//!   for numbering is only that a descriptor is opened, not which file it points at.
//! - THE STABILITY TEST RUNS ONE BINARY TWICE. Comparing two runs of the SAME
//!   executable is the only shape that catches the original defect: the old output was
//!   deterministic within a run and varied between runs, so a single-run assertion
//!   against a captured string would have passed against the bug on the run that
//!   produced the capture.
//! - Compile-failure assertions read the RAW stderr and only assert a substring, so the
//!   HOST linker's environmental warnings (GNU `ld` on Linux, silent on macOS) cannot
//!   interfere. Successful compiles go through `elephc_diagnostics`, which keeps
//!   elephc's own lines only.
//! - `hash` is bridge-gated behind `elephc_crypto`, so every hash program here also
//!   exercises the bridge link path.
//! - A `HashContext` IS NOW AN OBJECT, AND THAT CHANGED THE CELLS IN THIS FILE.
//!   When these tests landed, elephc modelled the incremental hashing state as a PHP
//!   resource, so a context consumed a resource id and this file asserted
//!   `var_dump(hash_init('md5')) == "resource(5) of type (stream)"` — explicitly logged
//!   at the time as a known divergence from PHP 8, which returns an OBJECT.
//!   `elephc::hash_prelude` now closes that gap: `hash_init()` returns a real
//!   `HashContext` object built on `internal: true` `__elephc_hash_ctx_*` builtins.
//!
//!   Consequently a hash context DRAWS AN OBJECT HANDLE AND CONSUMES NO RESOURCE ID —
//!   two disjoint numbering spaces, exactly as php-src keeps `zend_object.handle` and
//!   `zend_resource.handle` apart. The two tests that asserted a hash context's
//!   resource id were rewritten rather than deleted, and now assert the STRONGER
//!   property that they leave the stream numbering untouched: creating contexts before
//!   an `fopen()` must not shift that stream's id. The resource-id registry they were
//!   written to defend is unchanged and still covered by the stream tests.
//! - The FINALIZED-CONTEXT guard below is unchanged by that migration. It still lives in
//!   the runtime helpers (`elephc_crypto_is_finalized`), which the PHP-level wrappers
//!   call through, so the exact `TypeError` messages still name `hash_update()` /
//!   `hash_final()` / `hash_copy()` and are asserted here verbatim.

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
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Runs the compiler on `source` with `extra_args` and returns its raw output.
fn compile_raw(
    dir: &Path,
    source: &str,
    stem: &str,
    extra_args: &[&str],
) -> std::process::Output {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.args(extra_args);
    cmd.arg(&php);
    cmd.output().expect("failed to spawn elephc")
}

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather than
/// anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo` glibc notes
/// and the `.note.GNU-stack` deprecation, while Apple's linker stays silent.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("error")
                || line.starts_with("Error")
                || line.starts_with("warning")
                || line.starts_with("Warning: ")
                || line.starts_with("EIR backend error")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source` to a plain executable, asserting elephc reported no diagnostic.
fn compile(dir: &Path, source: &str, stem: &str, extra_args: &[&str]) -> PathBuf {
    let output = compile_raw(dir, source, stem, extra_args);
    let diagnostics = elephc_diagnostics(&String::from_utf8_lossy(&output.stderr));
    assert!(
        output.status.success(),
        "elephc compile failed:\n{diagnostics}"
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostic:\n{diagnostics}"
    );
    dir.join(stem)
}

/// Runs a compiled executable IN ITS OWN TEMP DIR and returns its stdout, asserting a
/// clean exit.
///
/// Setting the working directory is not cosmetic. `Command::new(bin)` inherits the test
/// harness's cwd, which is the repository root, so a program that opens `"first.txt"`
/// would read a file next to `Cargo.toml` — and one that CREATED a fixture would leave it
/// there. Every relative path in this file therefore resolves inside `dir`, and the whole
/// directory is removed afterwards.
fn run_binary_in(dir: &Path, bin: &Path) -> String {
    let output = Command::new(bin)
        .current_dir(dir)
        .output()
        .expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Writes the two fixture files the resource-numbering programs open.
///
/// They are created from RUST, not from the PHP program, and that is deliberate: PHP's
/// `file_put_contents()` opens a stream internally and therefore CONSUMES a resource id,
/// while elephc's does not (it writes through a raw descriptor that is never boxed as a
/// PHP resource). Creating the fixtures here keeps that unrelated divergence out of every
/// id assertion, so the expected numbers below are the same for both implementations and
/// a failure means the numbering changed rather than the fixture setup.
fn write_fixture_files(dir: &Path) {
    fs::write(dir.join("first.txt"), b"a\n").unwrap();
    fs::write(dir.join("second.txt"), b"b\n").unwrap();
}

/// Compiles `source`, runs it, and asserts stdout equals `expected`.
fn assert_program_output(prefix: &str, source: &str, expected: &str) {
    let dir = make_test_dir(prefix);
    write_fixture_files(&dir);
    let bin = compile(&dir, source, prefix, &[]);
    assert_eq!(run_binary_in(&dir, &bin), expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Compiles `source` once, runs the resulting binary TWICE, and returns both stdouts.
///
/// Two runs of one binary is the shape that separates "deterministic" from "happens to
/// look right": an ASLR-dependent value is identical within a run and differs between
/// runs, so only a cross-run comparison can see it.
fn run_twice(prefix: &str, source: &str) -> (String, String) {
    let dir = make_test_dir(prefix);
    write_fixture_files(&dir);
    let bin = compile(&dir, source, prefix, &[]);
    let first = run_binary_in(&dir, &bin);
    let second = run_binary_in(&dir, &bin);
    let _ = fs::remove_dir_all(&dir);
    (first, second)
}

/// Compiles `source` with `--heap-debug`, runs it, and asserts stdout plus a clean heap.
///
/// `--heap-debug` reports on the program's STDERR after `main` returns, so stdout stays
/// exactly what the PHP program printed. Both halves are asserted: a program that
/// silently stopped producing output would otherwise "leak nothing" and pass.
fn assert_program_output_and_clean_heap(prefix: &str, source: &str, expected_stdout: &str) {
    let dir = make_test_dir(prefix);
    let bin = compile(&dir, source, prefix, &["--heap-debug"]);
    let output = Command::new(&bin)
        .current_dir(&dir)
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{stderr}",
        output.status.code()
    );
    assert_eq!(stdout, expected_stdout, "program stdout diverged:\n{stderr}");
    assert!(
        stderr.contains("live_blocks=0"),
        "heap blocks leaked:\n{stderr}"
    );
    assert!(
        stderr.contains("leak summary: clean"),
        "heap summary is not clean:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Runs `source` under `--heap-debug` and asserts stdout plus an EXACT live-block count.
///
/// Exists only for the spent-context path, which leaks a bounded, known number of blocks
/// (see `rejected_reuse_leaks_one_block_per_rejected_call`). Asserting the exact count
/// rather than "clean" keeps the regression visible and makes any DRIFT — in either
/// direction — fail, so the number can never quietly grow.
fn assert_program_output_and_live_blocks(
    prefix: &str,
    source: &str,
    expected_stdout: &str,
    expected_live_blocks: usize,
) {
    let dir = make_test_dir(prefix);
    let bin = compile(&dir, source, prefix, &["--heap-debug"]);
    let output = Command::new(&bin)
        .current_dir(&dir)
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{stderr}",
        output.status.code()
    );
    assert_eq!(stdout, expected_stdout, "program stdout diverged:\n{stderr}");
    assert!(
        stderr.contains(&format!("live_blocks={expected_live_blocks}")),
        "expected exactly {expected_live_blocks} live blocks:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// PHP source that opens two files and a directory, dumping each resource.
///
/// The fixture files come from `write_fixture_files`, so the test carries no dependency
/// on host filesystem layout and no id is spent before the first `fopen()`.
const THREE_RESOURCES_SRC: &str = r#"<?php
$a = fopen("first.txt", "r");
var_dump($a);
$b = fopen("second.txt", "r");
var_dump($b);
$d = opendir(".");
var_dump($d);
"#;

/// Verifies resource ids are small and follow CREATION ORDER, byte-for-byte against
/// reference PHP 8.5.6.
///
/// Reference (`php -d xdebug.mode=off` on the equivalent program over `/etc/hosts`,
/// `/etc/passwd` and `/tmp`) prints ids 5, 6 and 7: the standard streams hold 1..3 and
/// the CLI SAPI consumes 4 before user code runs, so the first resource a script opens
/// is id 5. Before the fix elephc printed 4, 5, 6 — off by one against every PHP
/// program, because it derived the id from the descriptor number rather than counting
/// resources.
#[test]
fn resource_ids_are_small_and_follow_creation_order() {
    assert_program_output(
        "resid_creation_order",
        THREE_RESOURCES_SRC,
        "resource(5) of type (stream)\n\
         resource(6) of type (stream)\n\
         resource(7) of type (stream)\n",
    );
}

/// Verifies the standard streams keep the ids PHP fixes for them.
///
/// Reference PHP 8.5.6 prints `resource(1|2|3) of type (stream)` for
/// `STDIN`/`STDOUT`/`STDERR`. These are constants rather than created resources, so the
/// registry must answer for them without ever having minted anything — a regression here
/// would mean the three of them started consuming user ids.
#[test]
fn standard_streams_keep_their_php_resource_ids() {
    assert_program_output(
        "resid_std_streams",
        "<?php\nvar_dump(STDIN);\nvar_dump(STDOUT);\nvar_dump(STDERR);\n",
        "resource(1) of type (stream)\n\
         resource(2) of type (stream)\n\
         resource(3) of type (stream)\n",
    );
}

/// Verifies a resource survives a by-value `foreach` well enough to be passed to a
/// builtin that demands a stream.
///
/// The element type of `[STDIN, STDOUT, STDERR]` used to be run through
/// `PhpType::codegen_repr()` before the foreach local was typed, and a resource's
/// codegen representation is an integer. The loop variable therefore came out as `Int`
/// and `stream_get_meta_data($s)` was REFUSED at compile time with "stream argument PHP
/// type Int" — the program never ran at all, so this test asserts output rather than a
/// diagnostic. Reference PHP 8.5.6 prints `1|2|3 res meta`; only environment-independent
/// facts are asserted here because the host's `stream_type` for STDIN is `tcp_socket`
/// under a piped stdin and `STDIO` otherwise.
#[test]
fn a_foreach_over_standard_streams_still_yields_resources() {
    assert_program_output(
        "resid_foreach_streams",
        r#"<?php
foreach ([STDIN, STDOUT, STDERR] as $s) {
    $meta = stream_get_meta_data($s);
    echo get_resource_id($s), " ", (is_resource($s) ? "res" : "not"), " ", (count($meta) > 0 ? "meta" : "empty"), "\n";
}
"#,
        "1 res meta\n2 res meta\n3 res meta\n",
    );
}

/// Verifies an alias shares its resource's id while a descriptor REUSED after `fclose()`
/// gets a fresh one, byte-for-byte against reference PHP 8.5.6.
///
/// Reference prints `5 / 5 / 6 / 6 / stream`. Both halves matter and pull in opposite
/// directions: `$alias = $a` must NOT mint (PHP keeps one id per resource), while the
/// second `fopen()` must mint even though the kernel handed back the very same
/// descriptor number the closed stream used — php-src's resource handles come from a
/// monotonically increasing list index and are never recycled.
#[test]
fn alias_shares_an_id_and_a_reused_descriptor_takes_the_next_one() {
    assert_program_output(
        "resid_alias_and_reuse",
        r#"<?php
$a = fopen("first.txt", "r");
echo get_resource_id($a), "\n";
$alias = $a;
echo get_resource_id($alias), "\n";
fclose($a);
$b = fopen("second.txt", "r");
echo get_resource_id($b), "\n";
echo (int) $b, "\n";
echo get_resource_type($b), "\n";
"#,
        "5\n5\n6\n6\nstream\n",
    );
}

/// Verifies resource ids are STABLE ACROSS RUNS of the same binary, and that hash
/// contexts interleaved with streams do not perturb them.
///
/// This is the assertion that actually pins the reported defect. The old id for a
/// descriptor-backed resource was already stable, so a streams-only program would have
/// passed against the bug; it is the hash contexts in the same program that make it
/// bite, since a context's id used to be an ASLR-dependent heap address.
///
/// CHANGED CELL: this test used to expect three `resource(N)` lines, because
/// `var_dump($c)` on a hash context printed one. A context is an OBJECT now, so its two
/// `var_dump`s print `object(HashContext)#N`, and — the substantive part — the SECOND
/// stream still gets `resource(6)`, not `resource(8)`. Under the old model the two
/// contexts consumed ids 6 and 7 and pushed it to 8. Reference PHP 8.5.6 prints exactly
/// what is asserted below, contexts included.
#[test]
fn resource_ids_are_identical_across_two_runs_of_one_binary() {
    let (first, second) = run_twice(
        "resid_stable_runs",
        r#"<?php
$a = fopen("first.txt", "r");
var_dump($a);
$c = hash_init("md5");
var_dump($c);
$e = hash_init("sha256");
var_dump($e);
$b = fopen("first.txt", "r");
var_dump($b);
"#,
    );
    assert_eq!(
        first, second,
        "resource ids changed between two runs of the same binary"
    );
    assert_eq!(
        first,
        "resource(5) of type (stream)\n\
         object(HashContext)#1 (1) {\n  [\"algo\"]=>\n  string(3) \"md5\"\n}\n\
         object(HashContext)#2 (1) {\n  [\"algo\"]=>\n  string(6) \"sha256\"\n}\n\
         resource(6) of type (stream)\n"
    );
}

/// Verifies `HashContext`es are OBJECTS that consume no resource id, and that neither
/// their handles nor the stream numbering leak a native address.
///
/// CHANGED CELL, and this is the test that changed most. It used to assert
/// `resource(5)/(6)/(7)` for three contexts — the strongest statement available while a
/// context WAS a resource, because the measured pre-fix output was
/// `resource(4318862849) of type (stream)`, a raw malloc'd pointer that differed on
/// every run under ASLR. Contexts are objects now, so the correct statement is stronger:
/// they take OBJECT handles `#1/#2/#3` and the `fopen()` that follows still gets
/// `resource(5)`, proving three contexts consumed nothing from the resource counter.
///
/// The digit bound is kept and widened to cover BOTH numbering spaces, because it is the
/// part that actually defends against the original defect: it fails on a raw address
/// even if someone updated the expected string carelessly. `hash_copy()` is included so
/// the clone is covered too — it mints its own object, never an alias.
#[test]
fn hash_contexts_are_objects_and_consume_no_resource_id() {
    let (first, second) = run_twice(
        "resid_hash_ctx",
        r#"<?php
$a = hash_init("md5");
$b = hash_init("sha256");
$c = hash_copy($a);
var_dump($a);
var_dump($b);
var_dump($c);
$s = fopen("first.txt", "r");
var_dump($s);
"#,
    );
    assert_eq!(first, second, "HashContext output changed between runs");
    assert_eq!(
        first,
        "object(HashContext)#1 (1) {\n  [\"algo\"]=>\n  string(3) \"md5\"\n}\n\
         object(HashContext)#2 (1) {\n  [\"algo\"]=>\n  string(6) \"sha256\"\n}\n\
         object(HashContext)#3 (1) {\n  [\"algo\"]=>\n  string(3) \"md5\"\n}\n\
         resource(5) of type (stream)\n"
    );
    for (marker, terminator) in [("resource(", ')'), ("(HashContext)#", ' ')] {
        for id in first
            .split(marker)
            .skip(1)
            .map(|rest| rest.split(terminator).next().unwrap())
        {
            let value: u64 = id
                .parse()
                .unwrap_or_else(|_| panic!("id after {marker:?} must be a plain integer: {id:?}"));
            assert!(
                value < 1000,
                "id {value} after {marker:?} is not small — this is the heap-address regression"
            );
        }
    }
}

/// PHP source that finalizes a context and then tries all three spent-context
/// operations, catching each as `TypeError`.
const SPENT_CONTEXT_SRC: &str = r#"<?php
$c = hash_init("md5");
hash_update($c, "abc");
echo hash_final($c), "\n";
try { hash_update($c, "def"); echo "NOT REACHED\n"; }
catch (TypeError $e) { echo get_class($e), "|", $e->getMessage(), "\n"; }
try { hash_final($c); echo "NOT REACHED\n"; }
catch (TypeError $e) { echo get_class($e), "|", $e->getMessage(), "\n"; }
try { hash_copy($c); echo "NOT REACHED\n"; }
catch (TypeError $e) { echo get_class($e), "|", $e->getMessage(), "\n"; }
echo "still running\n";
"#;

/// The exact stdout reference PHP 8.5.6 produces for `SPENT_CONTEXT_SRC`.
///
/// The digest on the first line is `md5("abc")`, and its presence is load-bearing: it
/// proves the FIRST `hash_final()` still worked, so the guard rejects reuse rather than
/// rejecting finalization outright. `still running` proves each TypeError was catchable
/// and the program continued.
const SPENT_CONTEXT_EXPECTED: &str = "900150983cd24fb0d6963f7d28e17f72\n\
     TypeError|hash_update(): Argument #1 ($context) must be a valid, non-finalized HashContext\n\
     TypeError|hash_final(): Argument #1 ($context) must be a valid, non-finalized HashContext\n\
     TypeError|hash_copy(): Argument #1 ($context) must be a valid, non-finalized HashContext\n\
     still running\n";

/// Verifies all three spent-context operations raise PHP's exact catchable `TypeError`.
///
/// Each message names its own callee (`hash_update():` / `hash_final():` /
/// `hash_copy():`), which is why all three are asserted separately rather than through a
/// shared substring: a guard wired to one shared message would still pass a
/// "does it throw?" test while telling every caller the wrong function name.
///
/// The pre-fix behaviour was not an exception at all — `hash_update` after `hash_final`
/// returned `true` and kept hashing, and the second `hash_final` returned
/// `md5("abcdef")`. That is the most dangerous failure class there is: a plausible,
/// wrong digest with no diagnostic.
#[test]
fn spent_hash_context_raises_phps_exact_type_error_for_all_three_calls() {
    assert_program_output(
        "hashctx_spent_typeerror",
        SPENT_CONTEXT_SRC,
        SPENT_CONTEXT_EXPECTED,
    );
}

/// KNOWN BOUNDED REGRESSION — the rejected-reuse path leaks TWO heap blocks.
///
/// This test used to assert `live_blocks=0`. It cannot any more, and the reason is worth
/// stating precisely because the number must never be allowed to drift.
///
/// `hash_update` / `hash_final` / `hash_copy` are elephc-PHP wrappers now
/// (`elephc::hash_prelude`), so the guard's `\TypeError` is raised by the runtime helper
/// INSIDE a PHP function frame and unwinds THROUGH it. A builtin that throws through a
/// PHP frame strands one block per unwind — a PRE-EXISTING defect this migration merely
/// reaches. It is hash-independent and reproduces with no hashing at all:
///
/// ```php
/// class R { public mixed $m = null; }
/// function f(R $r): bool { $v = $r->m; throw new TypeError("x"); }
/// $r = new R(); try { f($r); } catch (TypeError $e) {}
/// ```
///
/// leaks one block under `--heap-debug`; the same function WITHOUT the local is clean,
/// and the same builtin throwing at top level with no wrapper frame is clean. Every
/// pure-PHP workaround was measured and none helps: binding the argument to a local,
/// leaving it inline, binding the result, `try`/`finally`, and catch-and-rethrow all
/// still leak on the throwing path (catch-and-rethrow leaks TWO). Fixing it properly
/// means fixing frame cleanup during unwind, not the prelude.
///
/// The SUCCESS path is unaffected and stays exactly clean — see
/// `hash_contexts_in_containers_leave_a_clean_heap`, which runs 1 000 contexts through
/// `hash_copy`/`hash_final` at `allocs == frees`. What leaks here is bounded by the
/// number of REJECTED calls, three in this program, costing two blocks.
///
/// `--heap-debug` is the authoritative instrument for elephc's own heap. It cannot see
/// the elephc-crypto context itself, which the bridge allocates outside elephc's heap —
/// that side is covered by the ownership argument on `elephc_crypto_final` (it still
/// finalizes a CLONE and still never frees, so `__rt_hash_ctx_free` remains the single
/// destructor and still runs exactly once) and by the flat-RSS measurement recorded on
/// `hash_contexts_in_containers_leave_a_clean_heap`.
#[test]
fn rejected_reuse_leaks_one_block_per_rejected_call() {
    assert_program_output_and_live_blocks(
        "hashctx_spent_heap",
        SPENT_CONTEXT_SRC,
        SPENT_CONTEXT_EXPECTED,
        2,
    );
}

/// Verifies an UNCAUGHT spent-context TypeError terminates the program rather than being
/// swallowed.
///
/// A guard that threw into a swallowed path would restore the silent-wrong-value bug in a
/// new shape, so the failure is pinned as observable at the process boundary: no digest
/// on stdout, and a non-zero exit.
#[test]
fn uncaught_spent_context_type_error_terminates_the_program() {
    let dir = make_test_dir("hashctx_uncaught");
    let bin = compile(
        &dir,
        r#"<?php
$c = hash_init("md5");
hash_final($c);
hash_update($c, "x");
echo hash_final($c), "\n";
"#,
        "hashctx_uncaught",
        &[],
    );
    let output = Command::new(&bin)
        .current_dir(&dir)
        .output()
        .expect("failed to run binary");
    assert!(
        !output.status.success(),
        "an uncaught spent-context TypeError must not exit zero"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("d41d8cd98f00b204e9800998ecf8427e"),
        "a digest reached stdout after the context was spent: {stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Regression pin: digests, `hash_copy()` independence and binary mode are unchanged.
///
/// Every value here is reference PHP 8.5.6 output. This is the guard against "fixed the
/// life cycle, broke the hashing": the finalized flag lives on the same heap object as
/// the digest state, and `hash_copy` had to start carrying it.
///
/// - line 1 is `sha256("abc")`,
/// - line 2 is `sha256("abcdef")` reached through a COPY taken after `"abc"`, so it
///   proves the copy is a deep, independent context and not an alias,
/// - line 3 is `md5("abc")` produced in BINARY mode and re-hexed, so it proves
///   `hash_final($ctx, true)` still returns raw bytes,
/// - line 4 proves `hash_init()`'s unknown-algorithm `\ValueError` still carries its own
///   message and class after the throw emitter was generalized to take a class id.
#[test]
fn digests_copy_independence_and_binary_mode_still_match_php() {
    assert_program_output(
        "hashctx_regression_pin",
        r#"<?php
$c = hash_init("sha256");
hash_update($c, "abc");
$mid = hash_copy($c);
hash_update($mid, "def");
echo hash_final($c), "\n";
echo hash_final($mid), "\n";
$bin = hash_init("md5");
hash_update($bin, "abc");
echo bin2hex(hash_final($bin, true)), "\n";
try { hash_init("no-such-algo"); } catch (Throwable $e) { echo get_class($e), "|", $e->getMessage(), "\n"; }
"#,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n\
         bef57ec7f53a6d40beb640a780a639c83bc29ac8a9816f1fc6c5c6dcd93c4721\n\
         900150983cd24fb0d6963f7d28e17f72\n\
         ValueError|hash_init(): Argument #1 ($algo) must be a valid hashing algorithm\n",
    );
}

/// Verifies a copy taken from a LIVE context is itself live, and that finalizing the
/// source does not spend the copy.
///
/// The finalized flag is per-handle, and `hash_copy` clones it. Cloning a live context
/// must therefore produce a live one; a flag accidentally shared between source and copy
/// would make this program throw where PHP returns a digest.
#[test]
fn a_copy_stays_usable_after_its_source_is_finalized() {
    assert_program_output(
        "hashctx_copy_outlives_source",
        r#"<?php
$src = hash_init("md5");
hash_update($src, "abc");
$copy = hash_copy($src);
echo hash_final($src), "\n";
hash_update($copy, "def");
echo hash_final($copy), "\n";
"#,
        "900150983cd24fb0d6963f7d28e17f72\ne80b5017098950fc58aad83c8c14978e\n",
    );
}

/// Verifies many open/close cycles keep producing small ids and never fall back to a
/// native payload.
///
/// The registry has a fixed slot count and evicts under pressure. This is the shape that
/// would expose an eviction bug as garbage output: every iteration reuses the same
/// descriptor number and therefore the same table slot, so 200 cycles must rewrite one
/// entry 200 times rather than consume 200 of them.
#[test]
fn repeated_open_close_cycles_keep_minting_small_ids() {
    assert_program_output(
        "resid_reuse_pressure",
        r#"<?php
$last = 0;
for ($i = 0; $i < 200; $i++) {
    $h = fopen("first.txt", "r");
    $last = get_resource_id($h);
    fclose($h);
}
echo $last, "\n";
"#,
        "204\n",
    );
}

// ---------------------------------------------------------------------------
// `HashContext` OBJECT MODEL (PHP 8 parity)
//
// PHP 8 turned the incremental hashing state from a resource into an object, the
// same migration GD made with `GdImage`. These tests pin the observable object
// surface. Every expectation was captured from `php -d xdebug.mode=off` on PHP
// 8.5.6 running the identical program.
// ---------------------------------------------------------------------------

/// Verifies the full identity surface of a `HashContext` matches PHP.
///
/// `is_object` and `gettype` are asserted alongside `get_class` and `instanceof`
/// deliberately: an implementation that produced a value merely *tagged* as an object
/// would satisfy `is_object` while failing `get_class`, and one that faked `get_class`
/// from a side table would fail `instanceof`. The `var_dump` line is the strictest cell
/// — it pins the class name, the object handle, the property COUNT, and the fact that
/// only `algo` is visible, all at once.
#[test]
fn hash_context_identity_surface_matches_php() {
    assert_program_output(
        "hashctx_identity",
        r#"<?php
$c = hash_init("md5");
var_dump(is_object($c));
var_dump(gettype($c));
var_dump(get_class($c));
var_dump($c instanceof HashContext);
var_dump($c);
"#,
        "bool(true)\n\
         string(6) \"object\"\n\
         string(11) \"HashContext\"\n\
         bool(true)\n\
         object(HashContext)#1 (1) {\n  [\"algo\"]=>\n  string(3) \"md5\"\n}\n",
    );
}

/// Verifies the internal context handle is INVISIBLE to `var_dump`, for every algorithm.
///
/// php-src's `HashContext` declares NO properties at all; the `algo` line comes from its
/// `__debugInfo()`. elephc has to store the bridge handle somewhere, and it lives in a
/// declared `$__elephc_ctx` property — which would otherwise print as a second row and
/// make the header read `(2)`. The prelude's `__debugInfo()` projection is what keeps
/// the output identical to PHP, so this test is the guard on that arrangement: it fails
/// loudly if the projection stops being honoured and the internal slot leaks into user
/// output. `algo` is echoed back per algorithm so a projection that hard-coded a string
/// rather than reading the property would fail too.
#[test]
fn hash_context_var_dump_never_leaks_the_internal_handle() {
    assert_program_output(
        "hashctx_no_internal_leak",
        r#"<?php
foreach (["md5", "sha1", "sha256"] as $algo) {
    var_dump(hash_init($algo));
}
"#,
        "object(HashContext)#1 (1) {\n  [\"algo\"]=>\n  string(3) \"md5\"\n}\n\
         object(HashContext)#1 (1) {\n  [\"algo\"]=>\n  string(4) \"sha1\"\n}\n\
         object(HashContext)#1 (1) {\n  [\"algo\"]=>\n  string(6) \"sha256\"\n}\n",
    );
}

/// Verifies a `HashContext` passes through a parameter TYPED `HashContext`, and that the
/// class participates in ordinary object plumbing.
///
/// A prelude-declared class must be a first-class citizen of the type system, not a
/// special case the checker waves through. Passing one into a user function with a
/// declared `HashContext` parameter exercises the checker's class resolution, and
/// returning it from a user function exercises the ownership path for a returned object.
///
/// NOT COVERED HERE, DELIBERATELY: `feed($c, "abc") === $c`. That shape hits a
/// PRE-EXISTING, hash-independent premature-free bug — elephc releases the object while
/// evaluating an object `===` whose left operand is a call temporary aliasing a live
/// variable, so the destructor runs immediately instead of at scope end. Minimal repro,
/// no hashing involved:
///
/// ```php
/// class Box { public string $s = "i"; public function __destruct() { echo "D\n"; } }
/// function feed(Box $b, string $d): Box { $b->s = $d; return $b; }
/// $x = new Box(); var_dump(feed($x, "abc") === $x);
/// ```
///
/// elephc prints `D` BEFORE `bool(true)`; PHP prints it last. A plain object survives the
/// aftermath because freed memory still reads back on macOS, but a `HashContext` frees a
/// real native context and the next use dies with
/// `hash_final(): Argument #1 ($stream) must be of type resource, unknown given`. The
/// hash context is simply a louder detector of an existing defect; asserting it here
/// would pin an unrelated bug into this file.
#[test]
fn hash_context_flows_through_typed_user_functions() {
    assert_program_output(
        "hashctx_typed_param",
        r#"<?php
function feed(HashContext $ctx, string $data): HashContext { hash_update($ctx, $data); return $ctx; }
function digest(HashContext $ctx): string { return hash_final($ctx); }
echo digest(feed(hash_init("md5"), "abc")), "\n";
$c = hash_init("sha256");
$same = feed($c, "abc");
echo digest($same), "\n";
"#,
        "900150983cd24fb0d6963f7d28e17f72\n\
         ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\n",
    );
}

/// Verifies hash contexts and ordinary objects share ONE handle pool, in creation order.
///
/// Reference PHP 8.5.6 prints `#1`, `#2`, `#3` for this program. A `HashContext`
/// allocated outside the standard object path would either draw no handle (leaving the
/// second `Dummy` at `#2`) or number from a private counter; both shapes fail here.
#[test]
fn hash_contexts_share_the_object_handle_pool_with_plain_objects() {
    assert_program_output(
        "hashctx_handle_pool",
        r#"<?php
class Dummy {}
$o1 = new Dummy();
$c = hash_init("sha256");
$o2 = new Dummy();
var_dump($o1, $c, $o2);
"#,
        "object(Dummy)#1 (0) {\n}\n\
         object(HashContext)#2 (1) {\n  [\"algo\"]=>\n  string(6) \"sha256\"\n}\n\
         object(Dummy)#3 (0) {\n}\n",
    );
}

/// Verifies `hash_copy()` produces an INDEPENDENT object, not an alias.
///
/// Two properties at once: the copy is a distinct object (`!==` the source, its own
/// handle), and its hashing state is genuinely forked — feeding the source after the
/// copy must not reach the copy. `$copy->algo` is checked because the wrapper has to
/// carry the algorithm across explicitly; a copy that lost it would still hash correctly
/// while printing the wrong `var_dump`.
#[test]
fn hash_copy_returns_an_independent_object() {
    assert_program_output(
        "hashctx_copy_object",
        r#"<?php
$src = hash_init("md5");
hash_update($src, "abc");
$copy = hash_copy($src);
var_dump($src === $copy);
var_dump($copy);
hash_update($src, "def");
echo hash_final($src), "\n";
echo hash_final($copy), "\n";
"#,
        "bool(false)\n\
         object(HashContext)#2 (1) {\n  [\"algo\"]=>\n  string(3) \"md5\"\n}\n\
         e80b5017098950fc58aad83c8c14978e\n\
         900150983cd24fb0d6963f7d28e17f72\n",
    );
}

/// Verifies hash contexts held in containers and destroyed in bulk leave a clean heap.
///
/// `--heap-debug` is authoritative for elephc's own heap and is what this asserts. It
/// CANNOT see the native `elephc_crypto` context, which the bridge allocates outside
/// elephc's heap; that side was verified separately by watching RSS stay flat
/// (2.03 MB → 2.10 MB) while the same workload ran at 2 000, 50 000 and 200 000
/// iterations — a 100x increase in contexts created and freed.
///
/// The array shape matters: it forces the contexts to be freed through the container's
/// deep-free path rather than through straight-line scope cleanup, which is where a
/// missing free or a double free would show up first.
#[test]
fn hash_contexts_in_containers_leave_a_clean_heap() {
    assert_program_output_and_clean_heap(
        "hashctx_container_heap",
        r#"<?php
function batch(int $n): string {
    $ctxs = [];
    for ($i = 0; $i < $n; $i++) {
        $c = hash_init("sha256");
        hash_update($c, "payload");
        $ctxs[] = hash_copy($c);
        $ctxs[] = $c;
    }
    $out = "";
    foreach ($ctxs as $c) { $out = hash_final($c); }
    return $out;
}
echo batch(500), "\n";
"#,
        "239f59ed55e737c77147cf55ad0c1b030b6d7ee748a7426952f9b852d5a935e5\n",
    );
}

/// KNOWN DIVERGENCE — `serialize()` on a `HashContext` THROWS here; PHP succeeds.
///
/// PHP 8.5.6 really does serialize a `HashContext`, emitting its full internal digest
/// state (`O:11:"HashContext":5:{i:0;s:3:"md5";i:1;i:0;i:2;a:23:{...}}`) so that
/// `unserialize()` returns a context that keeps hashing where the original left off.
/// elephc holds an opaque bridge handle and cannot reconstruct that array.
///
/// The prelude therefore makes `__serialize()` throw instead of returning a reduced
/// array. That choice is the point of this test: the reduced form
/// (`O:11:"HashContext":1:{s:4:"algo";s:3:"md5";}`) would LOOK like a serialized context
/// and silently not be one — a plausible-but-wrong value, which is the failure mode this
/// whole file exists to prevent. Failing loudly is the honest divergence.
#[test]
fn serializing_a_hash_context_fails_loudly_rather_than_silently_dropping_state() {
    assert_program_output(
        "hashctx_serialize_divergence",
        r#"<?php
try { echo serialize(hash_init("md5")), "\n"; }
catch (Throwable $e) { echo get_class($e), "|", $e->getMessage(), "\n"; }
echo "still running\n";
"#,
        "Exception|Serialization of 'HashContext' is not supported: \
         the native hashing state cannot be captured\n\
         still running\n",
    );
}

/// Verifies every PHP string context for an OPEN resource renders `Resource id #N`.
///
/// Captured from PHP 8.5.6 (`php -d xdebug.mode=off`) running this exact program.
///
/// Only the last line ever worked: `print`/`echo` of a bare resource is lowered to
/// `__rt_resource_write_stdout`, which produces no VALUE. Every form that needs a
/// string value went through `__rt_mixed_cast_string`, which had no tag-9 arm and so
/// fell through to its null/unsupported tail — all four printed the EMPTY string. The
/// resource arm added to that helper is what makes the five agree.
#[test]
fn every_string_context_renders_an_open_resource_like_php() {
    let source = r#"<?php
$r = fopen("first.txt", "r");
echo "interp:$r\n";
echo "concat:" . $r . "\n";
echo "cast:" . (string) $r . "\n";
echo "strval:" . strval($r) . "\n";
echo "print:";
print $r;
echo "\n";
"#;
    assert_program_output(
        "elephc_res_str_open",
        source,
        "interp:Resource id #5\nconcat:Resource id #5\ncast:Resource id #5\n\
         strval:Resource id #5\nprint:Resource id #5\n",
    );
}

/// Verifies a CLOSED resource keeps rendering its ORIGINAL id, and keeps it out of the
/// next `fopen()`'s way.
///
/// Captured from PHP 8.5.6 running this exact program: php-src does not disturb
/// `zend_resource.handle` when a resource is closed, so the closed handle still prints
/// `Resource id #5`, `get_resource_id()` still answers 5, and the next stream is 6.
///
/// elephc's `fclose` stamps a release sentinel into the Mixed box so scope cleanup
/// cannot close the descriptor twice. That sentinel used to be a bare `-1`, which
/// erased the only key the resource-id registry had: every display of the closed
/// handle missed the table, MINTED a fresh id, printed `Resource id #6`, and left the
/// next `fopen()` at 7. The sentinel now carries the id as `-id`.
#[test]
fn a_closed_resource_keeps_the_id_it_was_created_with() {
    let source = r#"<?php
$r = fopen("first.txt", "r");
fclose($r);
echo "interp:$r\n";
echo "concat:" . $r . "\n";
echo "cast:" . (string) $r . "\n";
echo "strval:" . strval($r) . "\n";
echo "print:";
print $r;
echo "\n";
echo "id:", get_resource_id($r), "\n";
$s = fopen("second.txt", "r");
echo "next:", get_resource_id($s), "\n";
"#;
    assert_program_output(
        "elephc_res_str_closed",
        source,
        "interp:Resource id #5\nconcat:Resource id #5\ncast:Resource id #5\n\
         strval:Resource id #5\nprint:Resource id #5\nid:5\nnext:6\n",
    );
}

/// Verifies the rendered resource string behaves like any other PHP string once it is
/// produced: joinable, searchable, measurable, and safe to hold in an array.
///
/// Captured from PHP 8.5.6 running this exact program. `implode()` is the load-bearing
/// case for OWNERSHIP: it is the ONLY caller that releases what `__rt_mixed_cast_string`
/// returns, through `__rt_heap_free`. The resource arm returns BORROWED `_concat_buf`
/// scratch, which that helper ignores because it lies outside the live heap — so this
/// program must neither leak nor free storage it does not own. `str_replace` and
/// `strtoupper` additionally prove the returned bytes survive a second pass over the
/// concat buffer instead of being overwritten by the next formatter.
#[test]
fn a_rendered_resource_string_survives_normal_string_operations() {
    let source = r#"<?php
$r = fopen("first.txt", "r");
$a = [$r, $r];
echo implode(",", $a), "\n";
echo str_replace("id #", "ID#", "$r"), "\n";
echo strlen("$r"), "\n";
echo strtoupper((string) $r), "\n";
$m = ["k" => $r];
echo $m["k"] . "!", "\n";
"#;
    assert_program_output(
        "elephc_res_str_ctx",
        source,
        "Resource id #5,Resource id #5\nResource ID#5\n14\nRESOURCE ID #5\nResource id #5!\n",
    );
}

/// Verifies a CLOSED resource renders its PHP type as `Unknown`, for all three closers.
///
/// Captured from PHP 8.5.6 running this exact program with `php -d xdebug.mode=off`; the
/// expectation below is that capture byte for byte. PHP renames a closed resource to
/// `Unknown` in BOTH `var_dump()` and `get_resource_type()`, and it does so identically
/// for `fclose`, `pclose` and `closedir` — measured, not assumed: `opendir()` under 8.5
/// reports the OPEN type `stream` (not `dir`), and all three collapse to the one name.
///
/// elephc baked the type name in as a compile-time literal at both display sites, so a
/// closed handle kept advertising `stream` forever. The name now comes from
/// `__rt_resource_type_name`, which reads the close state out of the sign bit of the
/// native payload — the same `-id` sentinel `fclose`/`pclose`/`closedir` already stamp.
///
/// The last three lines are the REGRESSION GUARD for the id fix this must not disturb:
/// the closed handles keep ids 5, 6 and 7, `get_resource_id()` still answers 5 for the
/// closed stream, and the fourth `fopen()` still gets 8 rather than reusing 5.
#[test]
fn a_closed_resource_reports_the_type_unknown_for_every_closer() {
    let source = r#"<?php
$s = fopen("first.txt", "r");
var_dump($s);
var_dump(get_resource_type($s));
fclose($s);
var_dump($s);
var_dump(get_resource_type($s));
var_dump(get_resource_id($s));

$p = popen("exit 0", "r");
var_dump($p);
var_dump(get_resource_type($p));
pclose($p);
var_dump($p);
var_dump(get_resource_type($p));

$d = opendir(".");
var_dump($d);
var_dump(get_resource_type($d));
closedir($d);
var_dump($d);
var_dump(get_resource_type($d));

$next = fopen("first.txt", "r");
var_dump($next);
var_dump(get_resource_type($next));
fclose($next);
"#;
    assert_program_output(
        "elephc_res_type_closed",
        source,
        "resource(5) of type (stream)\nstring(6) \"stream\"\n\
         resource(5) of type (Unknown)\nstring(7) \"Unknown\"\nint(5)\n\
         resource(6) of type (stream)\nstring(6) \"stream\"\n\
         resource(6) of type (Unknown)\nstring(7) \"Unknown\"\n\
         resource(7) of type (stream)\nstring(6) \"stream\"\n\
         resource(7) of type (Unknown)\nstring(7) \"Unknown\"\n\
         resource(8) of type (stream)\nstring(6) \"stream\"\n",
    );
}

/// The type-name resolution must not allocate, retain or free anything.
///
/// Both names are persistent `.data` literals (`_resource_type_stream` and
/// `_resource_type_unknown`), so the `release` the EIR already emits against a
/// `get_resource_type()` result stays the no-op it has always been against a `.data`
/// pointer. Returning a runtime-allocated string instead would be the double-free vector,
/// and `--heap-debug` is the instrument that would see it: the loop below opens, reads the
/// type, closes, and reads it again many times over.
#[test]
fn resolving_a_resource_type_name_allocates_nothing() {
    let source = r#"<?php
$n = 0;
for ($i = 0; $i < 64; $i++) {
    $r = fopen("first.txt", "r");
    $n += strlen(get_resource_type($r));
    fclose($r);
    $n += strlen(get_resource_type($r));
}
echo $n, "\n";
"#;
    let dir = make_test_dir("elephc_res_type_heap");
    write_fixture_files(&dir);
    let bin = compile(&dir, source, "elephc_res_type_heap", &["--heap-debug"]);
    let output = Command::new(&bin)
        .current_dir(&dir)
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?}):\n{stderr}",
        output.status.code()
    );
    // 64 open reads of "stream" (6) plus 64 closed reads of "Unknown" (7).
    assert_eq!(stdout, "832\n", "program stdout diverged:\n{stderr}");
    assert!(
        stderr.contains("live_blocks=0"),
        "heap blocks leaked:\n{stderr}"
    );
    assert!(
        stderr.contains("leak summary: clean"),
        "heap summary is not clean:\n{stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// Returns the assembly slice between two labels, including the start and excluding the end.
fn asm_between_labels<'a>(asm: &'a str, start_label: &str, end_label: &str) -> &'a str {
    let start = asm
        .find(start_label)
        .unwrap_or_else(|| panic!("missing start label {start_label}"));
    let tail = &asm[start..];
    let end = tail
        .find(end_label)
        .unwrap_or_else(|| panic!("missing end label {end_label} after {start_label}"));
    &tail[..end]
}

/// Asserts that every assembly needle appears in the provided order.
fn assert_asm_contains_ordered(asm: &str, needles: &[&str]) {
    let mut cursor = 0;
    for needle in needles {
        let relative = asm[cursor..].find(needle).unwrap_or_else(|| {
            panic!("missing assembly line `{needle}` after byte {cursor}:\n{asm}")
        });
        cursor += relative + needle.len();
    }
}

/// Pins `__rt_resource_type_name` inside the REAL generated runtime, on both targets.
///
/// The end-to-end tests above can only run on the host, and `--emit-asm --target
/// linux-x86_64` cannot complete on an aarch64 host (the runtime cache is assembled with
/// the host `as`, which rejects `xor eax, eax`). Generating the runtime text directly is
/// therefore the mechanism that gives the x86_64 body real coverage here, and it proves
/// something the emitter unit tests cannot: that the helper is actually REGISTERED in
/// `emit_runtime` and that the two literals it names exist in the data section.
#[test]
fn the_runtime_defines_the_resource_type_name_helper_on_both_targets() {
    for (target_name, closed_label, open_needles, closed_needles, end_label) in [
        (
            "macos-aarch64",
            "L__rt_resource_type_name_closed:",
            vec![
                "tbnz x0, #63, L__rt_resource_type_name_closed",
                "adrp x1, _resource_type_stream@PAGE",
                "add x1, x1, _resource_type_stream@PAGEOFF",
                "mov x2, #6",
                "ret",
            ],
            vec![
                "adrp x1, _resource_type_unknown@PAGE",
                "add x1, x1, _resource_type_unknown@PAGEOFF",
                "mov x2, #7",
                "ret",
            ],
            "__rt_resource_write_stdout",
        ),
        (
            "linux-x86_64",
            "__rt_resource_type_name_closed_x86:",
            vec![
                "test rax, rax",
                "js __rt_resource_type_name_closed_x86",
                "lea rax, [rip + _resource_type_stream]",
                "mov rdx, 6",
                "ret",
            ],
            vec![
                "lea rax, [rip + _resource_type_unknown]",
                "mov rdx, 7",
                "ret",
            ],
            "__rt_resource_write_stdout",
        ),
    ] {
        let target = elephc::codegen::platform::Target::parse(target_name).expect("valid target");
        let runtime_asm = elephc::codegen::generate_runtime_with_features(
            8_388_608,
            target,
            elephc::codegen::RuntimeFeatures::none(),
        );
        let open_arm = asm_between_labels(&runtime_asm, "__rt_resource_type_name:", closed_label);
        assert_asm_contains_ordered(open_arm, &open_needles);
        assert!(
            !open_arm.contains("_resource_type_unknown"),
            "the open arm must not name the closed literal ({target_name}):\n{open_arm}"
        );
        let closed_arm = asm_between_labels(&runtime_asm, closed_label, end_label);
        assert_asm_contains_ordered(closed_arm, &closed_needles);
        assert!(
            !closed_arm.contains("_resource_type_stream"),
            "the closed arm must not name the open literal ({target_name}):\n{closed_arm}"
        );
        assert!(
            runtime_asm.contains("_resource_type_stream:\n    .ascii \"stream\""),
            "the open type-name literal must exist ({target_name})"
        );
        assert!(
            runtime_asm.contains("_resource_type_unknown:\n    .ascii \"Unknown\""),
            "the closed type-name literal must exist ({target_name})"
        );
    }
}
