//! Purpose:
//! Locks the parallel body-emission pass (`src/codegen/block_emit.rs`): a program sent through
//! it must assemble, run like PHP, and emit the same assembly whatever the thread count.
//!
//! Called from:
//! - `cargo test --test parallel_codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - The pass only engages at 256 bodies, which a small fixture never reaches.
//!   `ELEPHC_CODEGEN_PARALLEL_MIN=0` sends it through anyway, so each compilation runs in its own
//!   process with the knobs set on the child instead of on this test process.
//! - Each worker emits against caches of its own, so a shared helper reached by two workers is
//!   emitted twice under the same key-derived symbols and the merge drops the copy. Anything
//!   emitted AFTER the pass from the compiling thread's caches (main) needs the same treatment.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Returns the path to the cargo-built `elephc` binary.
fn elephc_cli_bin() -> String {
    std::env::var("CARGO_BIN_EXE_elephc").unwrap_or_else(|_| {
        let mut path = std::env::current_exe().expect("failed to resolve current test binary");
        path.pop();
        if path.ends_with("deps") {
            path.pop();
        }
        path.join("elephc").to_string_lossy().into_owned()
    })
}

/// Creates a process-unique fixture directory holding `main.php`.
fn fixture_dir(name: &str, source: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "elephc_parallel_codegen_{}_{}_{}",
        name,
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("failed to create parallel codegen fixture directory");
    fs::write(dir.join("main.php"), source).expect("failed to write parallel codegen fixture");
    dir
}

/// Runs elephc on the fixture with every body sent through a parallel pass of `jobs` threads.
fn compile_parallel(dir: &Path, jobs: usize, extra: &[&str]) {
    let output = Command::new(elephc_cli_bin())
        .env("XDG_CACHE_HOME", dir.join("cache-root"))
        .env("ELEPHC_CODEGEN_PARALLEL_MIN", "0")
        .env("ELEPHC_CODEGEN_JOBS", jobs.to_string())
        .current_dir(dir)
        .arg("--quiet")
        .args(extra)
        .arg(dir.join("main.php"))
        .output()
        .expect("failed to run elephc CLI");
    assert!(
        output.status.success(),
        "elephc failed at {jobs} jobs: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A closure invoker is a keyed shared helper. `twice` reaches it inside a worker; main then
/// reaches invokers of the same shape from the compiling thread, whose caches never saw the
/// worker's copy.
const SHARED_INVOKER_SOURCE: &str = r#"<?php
function run(callable $f, int $v): int {
    return $f($v);
}
function twice(int $v): int {
    return run(function (int $x): int { return $x * 2; }, $v);
}
echo twice(3), "|";
echo run(function (int $x): int { return $x + 1; }, 3), "|";
$m = [function (int $x): int { return $x - 1; }];
echo call_user_func($m[0], 3), "\n";
"#;

/// Regression: main re-emitted a callable invoker a worker had already emitted, and the
/// assembler refused the second definition of `_eir_shared_callable_invoker_<key>_0`. Every
/// curl callback program hit it, because the curl prelude pushes those modules past 256 bodies.
#[test]
fn main_reaching_a_helper_a_worker_emitted_still_assembles() {
    let dir = fixture_dir("main_helper", SHARED_INVOKER_SOURCE);
    compile_parallel(&dir, 4, &[]);
    let run = Command::new(dir.join("main"))
        .output()
        .expect("failed to run the compiled fixture");
    let _ = fs::remove_dir_all(&dir);
    assert!(run.status.success(), "fixture exited with {:?}", run.status);
    assert_eq!(String::from_utf8_lossy(&run.stdout), "6|4|2\n");
}

/// The chunking is fixed and the merge runs in chunk order, so the thread count must not reach
/// the output.
#[test]
fn parallel_output_does_not_depend_on_the_thread_count() {
    let dir = fixture_dir("jobs", SHARED_INVOKER_SOURCE);
    let mut outputs = Vec::new();
    for jobs in [2, 5] {
        compile_parallel(&dir, jobs, &["--emit-asm"]);
        outputs.push(fs::read_to_string(dir.join("main.s")).expect("failed to read main.s"));
    }
    let _ = fs::remove_dir_all(&dir);
    assert!(
        outputs[0] == outputs[1],
        "2 and 5 codegen threads emitted different assembly ({} vs {} lines)",
        outputs[0].lines().count(),
        outputs[1].lines().count()
    );
}
