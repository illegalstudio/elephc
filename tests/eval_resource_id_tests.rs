//! Purpose:
//! End-to-end tests for PHP resource NUMBERING across the `eval()` boundary.
//!
//! PHP keeps ONE resource-id counter per request, and code running inside `eval()`
//! draws from it exactly like the code around it. Measured against PHP 8.5.6:
//!
//! ```text
//! $h = fopen(...);  eval('$a = fopen(...); $b = fopen(...);');  $i = fopen(...);
//!   ->  5                     6              7                      8
//! ```
//!
//! elephc runs `eval()` through the `elephc-magician` interpreter, which owns its
//! resources in `EvalStreamResources` — a dense zero-based table entirely unrelated
//! to the file descriptors and allocator handles the compiled program produces. Both
//! sides nevertheless box their resources through `__rt_mixed_from_value`, whose
//! tag-9 arm binds a PHP id in the runtime registry
//! (`src/codegen_support/runtime/resource_ids.rs`), so both sides were keying ONE
//! table with two incompatible numbering schemes. Two defects followed:
//!
//! - Eval's first three keys were 0, 1 and 2, which the registry answers directly as
//!   `payload + 1` because those are the standard stream descriptors. So the first
//!   three eval resources reported 1, 2, 3 and the fourth jumped to the counter's 5:
//!   `get_resource_id()` inside `eval()` returned 1,2,3,5,6,7 where PHP returns
//!   5,6,7,8,9,10. Those three also consumed nothing from the shared counter, so
//!   every host resource created after an `eval()` reported an id too low.
//! - Past the third, eval key `n` and host descriptor `n` are the same registry key.
//!   An eval stream would find the binding of an unrelated host stream and report ITS
//!   id — two live resources, one number.
//!
//! The fix gives eval payloads their own key namespace
//! (`elephc_magician::stream_resources::EVAL_RESOURCE_PAYLOAD_BASE`) while leaving
//! the ID space shared, which is what PHP does.
//!
//! Called from:
//! - `cargo test --test eval_resource_id_tests` through Rust's test harness.
//!
//! Key details:
//! - EVERY expected string here was captured from reference PHP 8.5.6 with
//!   `php -d xdebug.mode=off` — mandatory, because the host `php` loads Xdebug, which
//!   overloads `var_dump`. The exact program each capture came from is the `source`
//!   literal in the test that asserts it.
//! - Harness style mirrors `tests/resource_id_and_hash_context_tests.rs`: the elephc
//!   CLI (`CARGO_BIN_EXE_elephc`) runs as a subprocess in an isolated temp dir,
//!   compiles to a plain executable, runs it, and its stdout is asserted. Host-target
//!   only.
//! - THE COVERAGE IS DELIBERATELY THREE-SIDED: eval-only, host-only, and mixed. The
//!   host-only test is not redundant with `resource_id_and_hash_context_tests.rs`; it
//!   pins that the eval namespace base did not leak into host numbering, which is the
//!   way a future change to the base would most plausibly go wrong.
//! - Programs use `php://memory` rather than files wherever the identity of the
//!   underlying stream does not matter, so the ids do not depend on which descriptor
//!   numbers the host happens to have free. The one test that DOES need real
//!   descriptors (`eval_streams_never_alias_a_host_descriptor`) opens fixture files
//!   written from Rust, because PHP's `file_put_contents()` consumes a resource id and
//!   elephc's does not — creating fixtures from PHP would put an unrelated divergence
//!   inside an id assertion.

#[path = "support/managed_pcre2.rs"]
mod managed_pcre2_support;

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

/// Keeps only elephc's own diagnostics from a compile's stderr.
///
/// Linking also surfaces the HOST linker's warnings, which are environmental rather
/// than anything elephc emitted: GNU `ld` on Linux reports the static-`getaddrinfo`
/// glibc notes and the `.note.GNU-stack` deprecation, while Apple's linker stays silent.
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
fn compile(dir: &Path, source: &str, stem: &str) -> PathBuf {
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();
    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    managed_pcre2_support::configure_host_managed_pcre2(&mut cmd, dir);
    cmd.current_dir(dir);
    cmd.arg(&php);
    let output = cmd.output().expect("failed to spawn elephc");
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

/// Runs a compiled executable IN ITS OWN TEMP DIR and returns its stdout.
///
/// Setting the working directory is not cosmetic: `Command::new(bin)` would otherwise
/// inherit the harness's cwd (the repository root), so a program opening `"first.txt"`
/// would read a file next to `Cargo.toml`.
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

/// Writes the fixture files the descriptor-aliasing program opens.
///
/// Written from RUST, not from PHP: `file_put_contents()` consumes a resource id under
/// PHP and does not under elephc, so creating fixtures from the program under test would
/// fold an unrelated divergence into every id assertion.
fn write_fixture_files(dir: &Path) {
    fs::write(dir.join("first.txt"), b"a\n").unwrap();
    fs::write(dir.join("second.txt"), b"b\n").unwrap();
}

/// Compiles `source`, runs it, and asserts stdout equals `expected`.
fn assert_program_output(prefix: &str, source: &str, expected: &str) {
    let dir = make_test_dir(prefix);
    write_fixture_files(&dir);
    let bin = compile(&dir, source, prefix);
    assert_eq!(run_binary_in(&dir, &bin), expected);
    let _ = fs::remove_dir_all(&dir);
}

/// Verifies resources created INSIDE `eval()` are numbered 5, 6, 7, ... like PHP's.
///
/// This is the regression the file exists for. Before the eval payload namespace, this
/// exact program printed `1,2,3,5,6,7` and `resource(0) of type (stream)`.
#[test]
fn eval_resources_are_numbered_consecutively_from_five() {
    let source = r#"<?php
eval('
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
$c = fopen("php://memory", "r");
$d = fopen("php://memory", "r");
$e = fopen("php://memory", "r");
$f = fopen("php://memory", "r");
echo get_resource_id($a), ",", get_resource_id($b), ",", get_resource_id($c), ",";
echo get_resource_id($d), ",", get_resource_id($e), ",", get_resource_id($f), "\n";
var_dump($a);
');
"#;
    assert_program_output(
        "elephc_eval_rid_eval_only",
        source,
        "5,6,7,8,9,10\nresource(5) of type (stream)\n",
    );
}

/// Verifies the SAME program without `eval()` keeps the numbering it always had.
///
/// Guards the direction a change to the eval namespace base would most plausibly break:
/// host resources are keyed by descriptor number and must stay untouched by anything
/// done for eval.
#[test]
fn host_resources_are_numbered_consecutively_from_five() {
    let source = r#"<?php
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
$c = fopen("php://memory", "r");
$d = fopen("php://memory", "r");
$e = fopen("php://memory", "r");
$f = fopen("php://memory", "r");
echo get_resource_id($a), ",", get_resource_id($b), ",", get_resource_id($c), ",";
echo get_resource_id($d), ",", get_resource_id($e), ",", get_resource_id($f), "\n";
var_dump($a);
"#;
    assert_program_output(
        "elephc_eval_rid_host_only",
        source,
        "5,6,7,8,9,10\nresource(5) of type (stream)\n",
    );
}

/// Verifies host and eval resources share ONE counter and interleave in creation order.
///
/// The mixed shape is what proves the id space is shared rather than merely
/// self-consistent on each side: a design that gave eval its own counter would print
/// `5 / 5,6 / 6` here and pass both single-sided tests above.
#[test]
fn host_and_eval_resources_draw_from_one_shared_counter() {
    let source = r#"<?php
$h1 = fopen("php://memory", "r");
echo "h1=", get_resource_id($h1), "\n";
eval('
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
echo "e=", get_resource_id($a), ",", get_resource_id($b), "\n";
');
$h2 = fopen("php://memory", "r");
echo "h2=", get_resource_id($h2), "\n";
"#;
    assert_program_output(
        "elephc_eval_rid_mixed",
        source,
        "h1=5\ne=6,7\nh2=8\n",
    );
}

/// Verifies an eval stream never inherits the id bound to a live host DESCRIPTOR.
///
/// The fourth eval stream is the one that matters: its eval key used to be 3, the same
/// registry key as the host's first real file descriptor, so it looked that host stream
/// up and reported ITS id. `h1again` re-reads the host handle afterwards to show the
/// binding was not merely overwritten in the other direction — every id in this program
/// is distinct and creation-ordered.
#[test]
fn eval_streams_never_alias_a_host_descriptor() {
    let source = r#"<?php
$h1 = fopen("first.txt", "r");
echo "h1=", get_resource_id($h1), "\n";
eval('
$e1 = fopen("php://memory", "r");
$e2 = fopen("php://memory", "r");
$e3 = fopen("php://memory", "r");
$e4 = fopen("php://memory", "r");
echo "e=", get_resource_id($e1), ",", get_resource_id($e2), ",", get_resource_id($e3), ",", get_resource_id($e4), "\n";
');
$h2 = fopen("second.txt", "r");
echo "h2=", get_resource_id($h2), "\n";
echo "h1again=", get_resource_id($h1), "\n";
"#;
    assert_program_output(
        "elephc_eval_rid_alias",
        source,
        "h1=5\ne=6,7,8,9\nh2=10\nh1again=5\n",
    );
}

/// Verifies a closed eval stream's id is burned, not recycled, as in php-src.
///
/// `zend_resource` handles come from a monotonically increasing list index, so the
/// stream opened after an `fclose()` gets the NEXT id and the closed one's is never
/// seen again — PHP 8.5.6 prints `5,7` for this program, not `5,6`.
#[test]
fn a_closed_eval_stream_does_not_release_its_id() {
    let source = r#"<?php
eval('
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
fclose($b);
$c = fopen("php://memory", "r");
echo get_resource_id($a), ",", get_resource_id($c), "\n";
');
"#;
    assert_program_output("elephc_eval_rid_reopen", source, "5,7\n");
}

/// Verifies eval resource ids are STABLE across two runs of the same binary.
///
/// This is the shape that separates "deterministic" from "happens to look right". The
/// defect that created the id registry was an id derived from a malloc'd address, which
/// is identical within a run and differs between runs under ASLR, so only a cross-run
/// comparison can see it. Eval payloads are allocator-independent by construction, and
/// this test is what keeps them that way.
#[test]
fn eval_resource_ids_are_stable_across_runs_of_one_binary() {
    let source = r#"<?php
eval('
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
$d = opendir(".");
$c = fopen("php://memory", "r");
echo get_resource_id($a), ",", get_resource_id($b), ",", get_resource_id($c), "\n";
');
"#;
    let dir = make_test_dir("elephc_eval_rid_stable");
    write_fixture_files(&dir);
    let bin = compile(&dir, source, "elephc_eval_rid_stable");
    let first = run_binary_in(&dir, &bin);
    let second = run_binary_in(&dir, &bin);
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(first, "5,6,8\n", "first run diverged from PHP 8.5.6");
    assert_eq!(second, first, "eval resource ids changed between two runs");
}
