//! Purpose:
//! End-to-end heap tests for a store that a loop's BACK EDGE re-executes while the store itself
//! is lowered OUTSIDE the loop body: a `while`/`for` condition, a `do while` condition, and a
//! `for` update clause.
//!
//! THE BUG THESE PIN. `store_local` (`src/ir_lower/context.rs`) releases the previous occupant of
//! a slot before overwriting it. For a slot with no straight-line predecessor store it can only
//! know the occupant exists because a back edge brought it, so those arms were guarded by
//! `!loop_stack.is_empty()`. The loop stack holds a `LoopFrame` only while the loop BODY is
//! lowered — `break`/`continue` cannot appear in a condition, so no frame is pushed there — and
//! the four clauses above are all lowered with an empty stack. Every store in them therefore
//! overwrote a live value without releasing it:
//!
//! ```text
//! while (($c = fgetc($h)) !== false) { $n++; }   // 900 000 bytes
//! Fatal error: heap memory exhausted             // before
//! 900000                                         // after, php-identical
//! ```
//!
//! `while (($x = f()) !== null)` is THE idiomatic PHP read loop, so this leaked one allocation
//! per iteration in the most ordinary shape there is. It was invisible because every suite runs
//! short loops: five iterations leak five blocks and still exit successfully.
//!
//! Called from:
//! - `cargo test --test loop_edge_store_leak_tests` through Rust's test harness.
//!
//! Key details:
//! - The instrument is `--heap-debug`, which reports on the program's STDERR after `main`
//!   returns. The assertion compares a SHORT run against a LONG one: a leak is per-iteration, so
//!   only the difference between two iteration counts distinguishes it from the constant blocks
//!   a program legitimately still holds at exit (the loop variable's final value is one).
//! - Each program is also compared against its own expected stdout. A program that stopped
//!   producing output would otherwise "leak nothing" and pass.
//! - Host-target only: the change is in EIR lowering, with no architecture-specific assembly.
//!   Reference values captured from PHP 8.5.6 with `php -n`.

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

/// Compiles `source` with `--heap-debug` and returns the executable's path.
fn compile_with_heap_debug(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    cmd.current_dir(dir);
    cmd.arg("--heap-debug");
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
    assert!(
        output.status.success(),
        "elephc compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    dir.join(stem)
}

/// Compiles and runs `source`, returning its stdout and the live block count at exit.
fn run_and_count_live_blocks(prefix: &str, source: &str) -> (String, usize) {
    let dir = make_test_dir(prefix);
    let bin = compile_with_heap_debug(&dir, source, prefix);
    let output = Command::new(&bin)
        .current_dir(&dir)
        .output()
        .expect("failed to run compiled binary");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    assert!(
        output.status.success(),
        "{prefix}: compiled binary exited non-zero ({:?}):\n{stdout}{stderr}",
        output.status.code()
    );
    let live = stderr
        .lines()
        .find_map(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix("live_blocks="))
        })
        .unwrap_or_else(|| panic!("{prefix}: no live_blocks in heap debug output:\n{stderr}"))
        .parse::<usize>()
        .expect("live_blocks is a number");
    let _ = fs::remove_dir_all(&dir);
    (stdout, live)
}

/// Runs the same program at two iteration counts and asserts the heap does not grow with them.
///
/// `source` must contain `{N}` where the iteration count goes, and print the same text at both
/// counts. A per-iteration leak shows up as `live_blocks` differing by the iteration difference;
/// blocks a program legitimately still owns at exit are the same in both runs.
fn assert_iterations_do_not_grow_the_heap(prefix: &str, source: &str, expected_stdout: &str) {
    let (short_stdout, short_live) =
        run_and_count_live_blocks(prefix, &source.replace("{N}", "10"));
    let (long_stdout, long_live) =
        run_and_count_live_blocks(prefix, &source.replace("{N}", "2000"));
    assert_eq!(short_stdout, expected_stdout, "{prefix}: stdout diverged");
    assert_eq!(long_stdout, expected_stdout, "{prefix}: stdout diverged");
    assert_eq!(
        short_live, long_live,
        "{prefix}: 2000 iterations left {long_live} live blocks where 10 left {short_live} — the \
         loop leaks one allocation per iteration"
    );
}

/// Verifies an assignment inside a `while` condition releases the value it overwrites.
#[test]
fn assignment_in_a_while_condition_does_not_grow_the_heap() {
    let source = r#"<?php
function mk(int $i): string
{
    return str_repeat("ab", 1 + ($i % 3));
}
$n = 0;
while (($s = mk($n)) !== "zzz") {
    $n++;
    if ($n >= {N}) {
        break;
    }
}
echo "while\n";
"#;
    assert_iterations_do_not_grow_the_heap("elephc_leak_while_cond", source, "while\n");
}

/// Verifies an assignment inside a `do while` condition releases the value it overwrites.
#[test]
fn assignment_in_a_do_while_condition_does_not_grow_the_heap() {
    let source = r#"<?php
function mk(int $i): string
{
    return str_repeat("ab", 1 + ($i % 3));
}
$m = 0;
do {
    $m++;
} while (($t = mk($m)) !== "zzz" && $m < {N});
echo "do-while\n";
"#;
    assert_iterations_do_not_grow_the_heap("elephc_leak_do_while_cond", source, "do-while\n");
}

/// Verifies an assignment inside a `for` condition releases the value it overwrites.
#[test]
fn assignment_in_a_for_condition_does_not_grow_the_heap() {
    let source = r#"<?php
function mkarr(int $i): array
{
    return [$i, $i + 1];
}
for ($k = 0; ($u = mkarr($k)) !== [-1]; $k++) {
    if ($k >= {N}) {
        break;
    }
}
echo "for-cond\n";
"#;
    assert_iterations_do_not_grow_the_heap("elephc_leak_for_cond", source, "for-cond\n");
}

/// Verifies an assignment inside a `for` update clause releases the value it overwrites.
///
/// `$w` is deliberately NOT assigned before the loop. A slot with a straight-line predecessor
/// store is released by an arm that never consulted the loop stack, so pre-initializing `$w`
/// would make this test pass with the defect still in place — measured.
#[test]
fn assignment_in_a_for_update_does_not_grow_the_heap() {
    let source = r#"<?php
function mk(int $i): string
{
    return str_repeat("ab", 1 + ($i % 3));
}
for ($j = 0; $j < {N}; $w = mk($j++)) {
}
echo "for-update\n";
"#;
    assert_iterations_do_not_grow_the_heap("elephc_leak_for_update", source, "for-update\n");
}

/// Verifies an object built in a `while` condition is freed on each iteration.
///
/// The string cases above all release through the string path; an object carries a refcount and
/// a destructor, so it exercises the other half of `release_stored_local_value_before_overwrite`.
#[test]
fn object_assigned_in_a_while_condition_does_not_grow_the_heap() {
    let source = r#"<?php
class Box
{
    public int $v;
    public function __construct(int $v)
    {
        $this->v = $v;
    }
}
$p = 0;
while (($o = new Box($p)) !== null) {
    $p++;
    if ($p >= {N}) {
        break;
    }
}
echo "object\n";
"#;
    assert_iterations_do_not_grow_the_heap("elephc_leak_while_new", source, "object\n");
}

/// Verifies the byte-at-a-time read loop finishes a file larger than the default heap.
///
/// This is the program that exposed the defect. `fgetc()` returns a fresh one-byte string, the
/// condition stores it, and 900 000 unreleased one-byte strings exhausted the 8 MiB heap. The
/// file is written by the program so the test needs no fixture, and the byte count is checked
/// against `php -n`, which prints 900000.
#[test]
fn a_byte_at_a_time_fgetc_loop_reads_a_900k_file() {
    let source = r#"<?php
$path = "bytes.txt";
file_put_contents($path, str_repeat("abcdefgh\n", 100000));
$h = fopen($path, "r");
$n = 0;
while (($c = fgetc($h)) !== false) {
    $n++;
}
fclose($h);
unlink($path);
echo $n, "\n";
"#;
    let (stdout, _live) = run_and_count_live_blocks("elephc_leak_fgetc_loop", source);
    assert_eq!(stdout, "900000\n", "the read loop did not reach EOF");
}

/// Verifies the value assigned in a condition is still readable after the loop.
///
/// The release added for the overwrite must drop the PREVIOUS occupant, never the one being
/// stored: a fix that released the new value instead would leave `$last` pointing at freed
/// memory, which prints as whatever the allocator hands out next.
#[test]
fn the_value_assigned_in_a_condition_survives_the_loop() {
    let source = r#"<?php
function mk(int $i): string
{
    return str_repeat("ab", 1 + ($i % 3));
}
$q = 0;
$last = "";
while (($last = mk($q)) !== "zzz") {
    $q++;
    if ($q >= 5) {
        break;
    }
}
echo $last, "|", strlen($last), "\n";
"#;
    let (stdout, _live) = run_and_count_live_blocks("elephc_leak_cond_value_survives", source);
    assert_eq!(stdout, "abab|4\n", "the last assigned value did not survive");
}
