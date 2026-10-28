//! Purpose:
//! End-to-end codegen test binary for the compiler pipeline: control flow, operators, optimizer, callables, CLI/REPL and regressions.
//!
//! Called from:
//! - `cargo test --test codegen_tests_core` through Rust's test harness.
//!
//! Key details:
//! - The codegen suite is split into six `codegen_tests_*` binaries by domain so a library edit
//!   recompiles them in parallel; test names keep the `codegen::<module>::` path, and CI selects
//!   the family with `binary(/^codegen_tests_/)`.
//! - Each binary mounts the shared runner helpers; the ones a domain never calls are expected.

#[allow(dead_code)]
#[path = "support/managed_pcre2.rs"]
mod managed_pcre2_support;

#[allow(dead_code, unused_imports, unused_macros)]
#[path = "codegen/support/mod.rs"]
mod support;

#[allow(dead_code)]
#[path = "support/locals_retype_fixtures.rs"]
mod locals_retype_fixtures;

mod codegen {
    mod preprocessor;
    mod cli;
    mod repl;
    mod strict_php;
    mod testing;
    mod testing_command;
    mod testing_generic_functions;
    mod testing_generics;
    mod strict_php_source_guards;
    mod lfc;
    mod benchmarks;
    mod operators;
    mod control_flow;
    mod callables;
    mod regressions;
    mod misc;
    mod optimizer;
}
