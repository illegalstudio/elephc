//! Purpose:
//! Integration or regression tests for end-to-end codegen coverage of benchmarks, including benchmark sum loop fixture, benchmark array sum fixture, and benchmark string concat fixture.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Inline PHP fixtures are compiled to native binaries and assertions compare stdout or expected failures.

use crate::support::compile_and_run;

#[test]
fn test_benchmark_sum_loop_fixture() {
    let source = std::fs::read_to_string("benchmarks/cases/sum_loop/main.php")
        .expect("failed to read sum_loop benchmark fixture");
    let out = compile_and_run(&source);
    assert_eq!(out, "20000100000\n");
}

#[test]
fn test_benchmark_array_sum_fixture() {
    let source = std::fs::read_to_string("benchmarks/cases/array_sum/main.php")
        .expect("failed to read array_sum benchmark fixture");
    let out = compile_and_run(&source);
    assert_eq!(out, "2398830\n");
}

#[test]
fn test_benchmark_string_concat_fixture() {
    let source = std::fs::read_to_string("benchmarks/cases/string_concat/main.php")
        .expect("failed to read string_concat benchmark fixture");
    let out = compile_and_run(&source);
    assert_eq!(out, "15000\n");
}
