//! Purpose:
//! End-to-end parity tests for PHP OBJECT HANDLES ACROSS AN `eval()` BOUNDARY: the
//! `object(C)#N` handle `var_dump()` prints, the `spl_object_id()` that must agree
//! with it, object IDENTITY (`===`) and write-through, LIFO handle reuse, and
//! allocator cleanliness for a loop that evals repeatedly.
//!
//! Called from:
//! - `cargo test --test eval_object_handle_tests` through Rust's test harness.
//!
//! Key details:
//! - IDENTITY IS ASSERTED BEFORE HANDLES, DELIBERATELY. The question these tests
//!   exist to answer is whether the AOT eval bridge COPIES objects across the
//!   boundary or passes the same heap object. If it copied, `===` would report
//!   `false` and a write through one alias would be invisible through the other —
//!   a far more serious failure than a wrong `#N`, and one a handle-only test would
//!   never distinguish from bookkeeping drift. It does not copy: the same object
//!   crosses, `===` holds, writes propagate, and two aliases share one `#N`.
//!   `eval_preserves_object_identity_across_the_boundary` is that assertion.
//! - THESE REPLACE A MISDIAGNOSIS. `eval_in_scope_remints_object_handles_divergence`
//!   in `var_dump_object_tests` pinned `#5` and blamed the bridge for
//!   "re-materializing live objects" into "freshly allocated storage that draws
//!   freshly minted handles". Probing disproved every clause: the shift was visible
//!   on an object dumped BEFORE the `eval()` ever ran, which no staging behaviour
//!   can explain. The real cause was eager enum-case materialization — `eval` makes
//!   `codegen::seed_eval_visible_enum_singleton_names` treat every enum in the
//!   module as reachable, so the four prelude cases (`PropertyHookType::{Get,Set}`,
//!   `SortDirection::{Ascending,Descending}`) consumed handles 1..4 in the `_main`
//!   prologue and shifted every user object by exactly +4. Nothing in
//!   `eval_bridge.rs` was ever at fault.
//! - EXPECTATIONS ARE REFERENCE PHP'S OUTPUT, BYTE FOR BYTE, WITH NO EXCEPTION
//!   CARVED OUT. Every expectation here was taken from `php -d xdebug.mode=off` on
//!   the same program against PHP 8.5.6. The `-d xdebug.mode=off` matters: the host
//!   `php` loads Xdebug, which OVERLOADS `var_dump` and would silently change the
//!   reference. `eval_can_name_enum_cases_matches_php_case_numbering` is the test
//!   that shows this discipline paying off — PHP prints `#4` there, not the `#1` a
//!   plausible reading of "lazy" would predict, and elephc matches the real answer.
//! - Tests invoke the elephc CLI (CARGO_BIN_EXE_elephc) as a subprocess in an
//!   isolated temp dir, compile a plain executable, run it, and assert stdout — the
//!   same harness style as `var_dump_object_tests` / `opcache_ini_tests`.
//!   Host-target only (macOS aarch64 local).
//! - Compile STDERR is filtered to elephc's OWN diagnostics: on Linux, GNU `ld`
//!   adds static-glibc and `.note.GNU-stack` warnings that Apple's linker never
//!   emits, so an unfiltered assertion would be non-portable.

#[path = "support/managed_pcre2.rs"]
mod managed_pcre2_support;

use std::fs;
use std::path::PathBuf;
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
/// than anything elephc emitted: GNU `ld` reports static-glibc notes and the
/// `.note.GNU-stack` deprecation, while Apple's linker stays silent. Anchoring on
/// elephc's own line starts isolates its diagnostics — and still surfaces an
/// UNEXPECTED elephc warning, which an allow-list of known messages would hide.
fn elephc_diagnostics(stderr: &str) -> String {
    stderr
        .lines()
        .filter(|line| {
            line.starts_with("Warning: ")
                || line.starts_with("warning:")
                || line.starts_with("warning[")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compiles `source` with the given extra CLI flags and returns the compile dir and stem path.
///
/// Asserts a clean compile with no elephc diagnostics: a compile that emitted a
/// warning is not a parity result, and an over-widened local surfaces here as a hard
/// error rather than as wrong output.
fn compile_php(stem: &str, source: &str, flags: &[&str]) -> PathBuf {
    let dir = make_test_dir("elephc_eval_object_handle");
    let php = dir.join(format!("{}.php", stem));
    fs::write(&php, source).unwrap();

    let mut cmd = Command::new(elephc_bin());
    cmd.env("XDG_CACHE_HOME", dir.join("cache-root"));
    managed_pcre2_support::configure_host_managed_pcre2(&mut cmd, &dir);
    cmd.current_dir(&dir);
    cmd.arg(&php);
    for flag in flags {
        cmd.arg(flag);
    }
    let compile = cmd.output().expect("failed to spawn elephc");
    let raw_stderr = String::from_utf8_lossy(&compile.stderr).into_owned();
    assert!(
        compile.status.success(),
        "elephc compile failed:\n{raw_stderr}"
    );
    let diagnostics = elephc_diagnostics(&raw_stderr);
    assert!(
        diagnostics.is_empty(),
        "unexpected elephc diagnostics:\n{diagnostics}"
    );
    dir.join(stem)
}

/// Compiles `source`, runs the executable and returns its STDOUT.
fn run_php(stem: &str, source: &str) -> String {
    let bin = compile_php(stem, source, &[]);
    let output = Command::new(&bin)
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

/// Compiles `source` with `--heap-debug`, runs it, and returns the run's STDERR.
///
/// `--heap-debug` is the AUTHORITATIVE allocator check here — `--gc-stats`
/// under-reports — and it writes its summary to STDERR, hence the STDERR return.
fn run_php_heap_debug(stem: &str, source: &str) -> String {
    let bin = compile_php(stem, source, &["--heap-debug"]);
    let output = Command::new(&bin)
        .output()
        .expect("failed to run compiled binary");
    assert!(
        output.status.success(),
        "compiled binary exited non-zero ({:?})",
        output.status.code()
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// PARITY — THE OBJECT SURVIVING AN `eval()` IS THE SAME OBJECT, NOT A COPY.
///
/// The load-bearing test of this file, and the one that answers the question the
/// handle numbering only hinted at. Three independent facts each fail if the bridge
/// round-trips objects through a serialize/rebuild: `===` reports `true`, a write
/// made through `$o` is visible through `$before`, and both aliases print the SAME
/// `#N`. A copy would necessarily carry a different handle, so the shared `#N` is
/// evidence of shared storage independent of `===`.
#[test]
fn eval_preserves_object_identity_across_the_boundary() {
    let out = run_php(
        "handle_eval_identity",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$o = new P();\n",
            "$before = $o;\n",
            "eval('$q = 1;');\n",
            "var_dump($before === $o);\n",
            "$o->x = 42;\n",
            "var_dump($before->x);\n",
            "var_dump($o);\n",
            "var_dump($before);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "bool(true)\n",
            "int(42)\n",
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(42)\n}\n",
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(42)\n}\n",
        )
    );
}

// NOTE: the single-object case that `eval_in_scope_remints_object_handles_divergence`
// pinned at `#5` is NOT duplicated here. It now lives in `var_dump_object_tests` as
// `eval_in_scope_preserves_object_handles`, asserting PHP's real answer of `#1`,
// alongside the lazy-enum tests that explain it. Everything in this file covers a
// case that one does not.

/// PARITY — `spl_object_id()` AGREES WITH THE PRINTED `#N` ACROSS AN `eval()`.
///
/// The two must never be able to contradict each other: both read
/// `__rt_object_handle_of`, and an `eval()` between them must not decouple them.
///
/// THE `instanceof` GUARD IS LOAD-BEARING, NOT DECORATIVE. Every eval-synchronized
/// local is reloaded through a boxed Mixed cell, so after an `eval()` the local's
/// static type is `Mixed` — soundly, since the eval'd code really may rebind it —
/// and a bare `spl_object_id($o)` is REJECTED AT COMPILE TIME with
/// "spl_object_id() argument must be an object". Re-narrowing with `instanceof` is
/// the supported way to recover the object type. That residual checker gap is
/// reported separately; it costs expressiveness only, never correctness, as the
/// `1` and the `#1` agreeing here show.
#[test]
fn eval_boundary_keeps_spl_object_id_and_printed_handle_in_agreement() {
    let out = run_php(
        "handle_eval_spl_agreement",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$o = new P();\n",
            "eval('$q = 1;');\n",
            "if ($o instanceof P) { echo spl_object_id($o), \"\\n\"; }\n",
            "var_dump($o);\n",
        ),
    );
    assert_eq!(out, "1\nobject(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n");
}

/// PARITY — AN OBJECT CREATED INSIDE `eval()` AND RETURNED OUT TAKES THE NEXT
/// HANDLE, AND IS DISTINCT FROM THE ONE THAT WENT IN.
#[test]
fn object_created_inside_eval_takes_the_next_handle() {
    let out = run_php(
        "handle_eval_inside",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$a = new P();\n",
            "$b = eval('return new P();');\n",
            "var_dump($a);\n",
            "var_dump($b);\n",
            "var_dump($a === $b);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "bool(false)\n",
        )
    );
}

/// PARITY — A MUTATION MADE INSIDE `eval()` LANDS ON THE ORIGINAL OBJECT.
///
/// The write must reach the enclosing scope AND leave the handle alone: a bridge
/// that rebuilt the object in order to carry the write back out would satisfy the
/// first half and fail the second, so the pair discriminates between them.
#[test]
fn eval_writes_propagate_without_renumbering() {
    let out = run_php(
        "handle_eval_mutate",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$o = new P();\n",
            "eval('$o->x = 99;');\n",
            "var_dump($o->x);\n",
            "var_dump($o);\n",
        ),
    );
    assert_eq!(
        out,
        "int(99)\nobject(P)#1 (1) {\n  [\"x\"]=>\n  int(99)\n}\n"
    );
}

/// PARITY — OBJECTS REACHABLE ONLY THROUGH AN ARRAY OR ANOTHER OBJECT'S PROPERTY
/// KEEP THEIR HANDLES WHEN THE BRIDGE STAGES SCOPE.
///
/// These are the objects the bridge never names, so a staging step that walked
/// reachable values would renumber them while leaving directly-named locals alone —
/// the failure mode a locals-only test cannot see. The `#1` / `#3` pair also pins
/// the ORDERING: the `Q` takes `#2` between the two `P`s.
#[test]
fn container_reachable_objects_keep_handles_across_eval() {
    let out = run_php(
        "handle_eval_reachable",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "class Q { public $inner; }\n",
            "$arr = [new P()];\n",
            "$q = new Q();\n",
            "$q->inner = new P();\n",
            "eval('$k = 1;');\n",
            "var_dump($arr[0]);\n",
            "var_dump($q->inner);\n",
            "$arr[0]->x = 7;\n",
            "var_dump($arr[0]->x);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#3 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "int(7)\n",
        )
    );
}

/// PARITY — LIFO HANDLE REUSE STILL HOLDS IN A PROGRAM CONTAINING `eval()`.
///
/// Guards the POOL INVARIANT rather than the bridge: two live objects must never
/// share a handle, and a released handle must return exactly once. `$c` and `$d`
/// reclaim `#2` then `#1` — had `eval` leaked a handle the reuse order would drift,
/// and had it released one twice the free stack would hand the same number to both.
#[test]
fn eval_preserves_lifo_handle_reuse() {
    let out = run_php(
        "handle_eval_lifo",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "eval('$k = 1;');\n",
            "$a = new P();\n",
            "$b = new P();\n",
            "var_dump($a);\n",
            "var_dump($b);\n",
            "unset($a);\n",
            "unset($b);\n",
            "$c = new P();\n",
            "$d = new P();\n",
            "var_dump($c);\n",
            "var_dump($d);\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// PARITY — NESTED AND REPEATED `eval()` ACCUMULATE NO HANDLE DRIFT.
///
/// Separates two failure modes the single-eval tests cannot tell apart: a PER-EVAL
/// renumbering bug makes the handle climb with each `eval()`, while a PER-PROGRAM
/// one shifts it once. Three sequential evals plus a nested one leave `$o` at `#1`
/// and the object created afterwards at `#2`.
#[test]
fn repeated_and_nested_eval_do_not_drift_handles() {
    let out = run_php(
        "handle_eval_repeat",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "$o = new P();\n",
            "var_dump($o);\n",
            "eval('$q = 1;');\n",
            "eval('$r = 2;');\n",
            "eval('eval(\"\\$z = 5;\"); $y = 1;');\n",
            "var_dump($o);\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#1 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
            "object(P)#2 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// PARITY — `eval()` CAN NAME ENUM CASES, AND THE HANDLE NUMBERING MATCHES PHP'S.
///
/// This constraint rules out the tempting "just stop seeding enum singletons when
/// the module contains `eval`" shortcut: `eval` really can reach any case by name
/// at runtime, prelude cases included, so they must stay reachable.
///
/// THE `#4` IS MEASURED, NOT REASONED. A plausible reading of "materialize lazily"
/// predicts `#1` here; reference PHP 8.5.6 prints `#4`, because naming one case
/// materializes its whole enum. elephc matches the measured answer. This is exactly
/// the assumption that has to be checked against the real interpreter rather than
/// derived — the first draft of this test asserted `#1` and was wrong.
#[test]
fn eval_can_name_enum_cases_matches_php_case_numbering() {
    let out = run_php(
        "handle_eval_enum_names",
        concat!(
            "<?php\n",
            "enum E: string { case A = 'a'; case B = 'b'; }\n",
            "class P { public int $x = 1; }\n",
            "$c = eval('return E::A;');\n",
            "var_dump($c->value);\n",
            "$d = eval('return PropertyHookType::Get;');\n",
            "var_dump($d->name);\n",
            "var_dump(new P());\n",
        ),
    );
    assert_eq!(
        out,
        concat!(
            "string(1) \"a\"\n",
            "string(3) \"Get\"\n",
            "object(P)#4 (1) {\n  [\"x\"]=>\n  int(1)\n}\n",
        )
    );
}

/// HEAP HEALTH — A LOOP THAT EVALS REPEATEDLY LEAVES NOTHING BEHIND.
///
/// `--heap-debug` is authoritative: `--gc-stats` under-reports, and from counters
/// alone a leak that happens to balance reads identically to a double free, so this
/// asserts the allocator's own verdict rather than an `allocs == frees` equality.
///
/// The `live_blocks=0` is a REGRESSION ANCHOR in its own right. While enum cases
/// were materialized eagerly this same program ended at `live_blocks=4
/// live_bytes=192` — the four never-freed prelude enum singletons, whose sizes
/// account for those bytes exactly (2 * (40 + 16) + 2 * (24 + 16) = 192). Their
/// disappearance is the same fix as the handle parity above, seen from the
/// allocator side rather than from `var_dump`.
#[test]
fn repeated_eval_loop_is_heap_clean() {
    let stderr = run_php_heap_debug(
        "handle_eval_heap",
        concat!(
            "<?php\n",
            "class P { public int $x = 1; }\n",
            "for ($i = 0; $i < 50; $i++) {\n",
            "    $o = new P();\n",
            "    eval('$k = 1;');\n",
            "    $o->x = $i;\n",
            "}\n",
            "echo \"done\\n\";\n",
        ),
    );
    assert!(
        stderr.contains("leak summary: clean"),
        "expected a clean heap-debug summary, got:\n{stderr}"
    );
    assert!(
        stderr.contains("live_blocks=0"),
        "expected no live blocks at exit, got:\n{stderr}"
    );
}
