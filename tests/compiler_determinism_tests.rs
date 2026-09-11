//! Purpose:
//! Locks compiler output reproducibility: compiling the same source twice must
//! produce byte-identical assembly.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Each compilation runs in its OWN process on purpose. `HashMap` seeds its
//!   hasher from a per-process random state, so a single in-process pair of
//!   compilations would not exercise the ordering difference these tests exist
//!   to catch.
//! - `--emit-asm` stops before the assembler and the linker, so the fixtures stay
//!   cheap while still covering every phase that can bake an ordering decision
//!   into the output.

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

/// Returns a process-unique suffix so parallel tests never share a directory.
fn unique_test_id() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Compiles `source` to assembly `runs` times, each in a fresh process, and
/// returns the emitted assembly of every run.
///
/// Every run compiles the SAME file in the SAME directory: the compiler bakes the
/// canonicalized source path into the output (`__FILE__`, `Throwable::getFile()`),
/// so compiling copies in sibling directories would report a path difference as a
/// reproducibility failure. The isolated cache root ensures a regression cannot
/// populate the user's runtime-object cache; the assembly is regenerated each run.
fn emit_asm_repeatedly(name: &str, source: &str, runs: usize) -> Vec<String> {
    let dir = std::env::temp_dir().join(format!(
        "elephc_determinism_{}_{}_{}",
        name,
        std::process::id(),
        unique_test_id()
    ));
    fs::create_dir_all(&dir).expect("failed to create determinism fixture directory");
    let php_path: PathBuf = dir.join("main.php");
    fs::write(&php_path, source).expect("failed to write determinism PHP fixture");
    let cache_root = dir.join("cache-root");

    let outputs = (0..runs)
        .map(|run| emit_asm_once(&dir, &cache_root, &php_path, run, name))
        .collect();

    let _ = fs::remove_dir_all(&dir);
    outputs
}

/// Runs one `elephc --emit-asm` invocation and returns the assembly it wrote.
fn emit_asm_once(
    dir: &Path,
    cache_root: &Path,
    php_path: &Path,
    run: usize,
    name: &str,
) -> String {
    let compile = Command::new(elephc_cli_bin())
        .env("XDG_CACHE_HOME", cache_root)
        .current_dir(dir)
        .arg("--quiet")
        .arg("--emit-asm")
        .arg(php_path)
        .output()
        .expect("failed to run elephc CLI");
    assert!(
        compile.status.success(),
        "elephc failed for {name} run {run}: stderr={}",
        String::from_utf8_lossy(&compile.stderr)
    );

    fs::read_to_string(dir.join("main.s")).expect("failed to read emitted assembly")
}

/// Asserts every run produced the same assembly, reporting the divergence size
/// rather than dumping megabytes of assembly into the failure output.
fn assert_all_identical(name: &str, outputs: &[String]) {
    let first = &outputs[0];
    for (run, other) in outputs.iter().enumerate().skip(1) {
        if first == other {
            continue;
        }
        let differing = first
            .lines()
            .zip(other.lines())
            .filter(|(a, b)| a != b)
            .count();
        panic!(
            "{name}: compiling the same source twice produced different assembly \
             (run 0 vs run {run}: {} lines differ, {} vs {} lines total). \
             Compiler output must not depend on per-process hash ordering.",
            differing,
            first.lines().count(),
            other.lines().count()
        );
    }
}

/// Builds a fixture with enough sibling classes that an unordered walk over the
/// class table is astronomically unlikely to match a stable one by chance.
fn many_classes_source() -> String {
    let mut source = String::from("<?php\n");
    for index in 0..12 {
        source.push_str(&format!(
            "class Shape{index} {{\n    private int $n = {index};\n    \
             public function area(int $side): int {{ return $side * {}; }}\n}}\n",
            index + 1
        ));
    }
    source.push_str("$total = 0;\n");
    for index in 0..12 {
        source.push_str(&format!(
            "$s{index} = new Shape{index}(); $total = $total + $s{index}->area($argc);\n"
        ));
    }
    source.push_str("echo $total;\n");
    source
}

/// Two compilations of the same class-heavy program must emit identical assembly.
///
/// Regression: class ids were handed out while walking a `HashMap`, whose
/// iteration order is randomized per process. Those ids reach the output as
/// symbol names, immediates, and `.quad` values, so two runs of an unchanged
/// source disagreed on roughly a quarter of the emitted lines.
#[test]
fn class_heavy_program_emits_identical_assembly_across_processes() {
    let outputs = emit_asm_repeatedly("classes", &many_classes_source(), 3);
    assert_all_identical("classes", &outputs);
}

/// The same guarantee for interfaces, whose ids are assigned by a separate walk.
#[test]
fn interface_heavy_program_emits_identical_assembly_across_processes() {
    let mut source = String::from("<?php\n");
    for index in 0..10 {
        source.push_str(&format!(
            "interface Contract{index} {{ public function tag{index}(): int; }}\n"
        ));
    }
    for index in 0..10 {
        source.push_str(&format!(
            "class Impl{index} implements Contract{index} {{\n    \
             public function tag{index}(): int {{ return {index}; }}\n}}\n"
        ));
    }
    source.push_str("$total = 0;\n");
    for index in 0..10 {
        source.push_str(&format!(
            "$i{index} = new Impl{index}(); $total = $total + $i{index}->tag{index}();\n"
        ));
    }
    source.push_str("echo $total;\n");

    let outputs = emit_asm_repeatedly("interfaces", &source, 3);
    assert_all_identical("interfaces", &outputs);
}

/// Enums share the class-id counter, so they need the same guarantee.
#[test]
fn enum_heavy_program_emits_identical_assembly_across_processes() {
    let mut source = String::from("<?php\n");
    for index in 0..8 {
        source.push_str(&format!(
            "enum Level{index}: int {{ case Low = {}; case High = {}; }}\n",
            index,
            index + 100
        ));
    }
    source.push_str("$total = 0;\n");
    for index in 0..8 {
        source.push_str(&format!("$total = $total + Level{index}::High->value;\n"));
    }
    source.push_str("echo $total;\n");

    let outputs = emit_asm_repeatedly("enums", &source, 3);
    assert_all_identical("enums", &outputs);
}
