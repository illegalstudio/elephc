//! Purpose:
//! Integration tests for benchmark fixture correctness: integer sums, array reads,
//! string concatenation, and equivalent buffer/indexed/associative storage updates.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::compile_and_run;

/// Compiles the native sum-loop fixture and asserts stdout is "20000100000".
///
/// The fixture uses a while-loop to sum integers 1..200000 (Gauss formula result).
#[test]
fn test_benchmark_sum_loop_fixture() {
    let source = std::fs::read_to_string("benchmark/cases/native/sum_loop/main.php")
        .expect("failed to read sum_loop benchmark fixture");
    let out = compile_and_run(&source);
    assert_eq!(out, "20000100000\n");
}

/// Compiles the native array-sum fixture and asserts stdout is "2398830".
///
/// The fixture sums a hardcoded array of integers.
#[test]
fn test_benchmark_array_sum_fixture() {
    let source = std::fs::read_to_string("benchmark/cases/native/array_sum/main.php")
        .expect("failed to read array_sum benchmark fixture");
    let out = compile_and_run(&source);
    assert_eq!(out, "2398830\n");
}

/// Compiles the native string-concat fixture and asserts stdout is "15000".
///
/// The fixture concatenates strings in a loop to produce a repeated character count.
#[test]
fn test_benchmark_string_concat_fixture() {
    let source = std::fs::read_to_string("benchmark/cases/native/string_concat/main.php")
        .expect("failed to read string_concat benchmark fixture");
    let out = compile_and_run(&source);
    assert_eq!(out, "15000\n");
}

/// Checks equivalent position updates through all three hot-path storage representations.
#[test]
fn test_benchmark_hot_path_fixtures() {
    for variant in ["buffer", "indexed", "assoc"] {
        let source = std::fs::read_to_string(format!(
            "benchmark/cases/hot-path/buffer-vs-arrays/{variant}.php"
        )).expect("failed to read hot-path benchmark fixture");
        assert_eq!(compile_and_run(&source), "160000\n", "{variant}");
    }
}
