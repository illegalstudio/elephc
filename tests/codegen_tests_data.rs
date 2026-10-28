//! Purpose:
//! End-to-end codegen test binary for scalars, strings, arrays, numeric and type builtins.
//!
//! Called from:
//! - `cargo test --test codegen_tests_data` through Rust's test harness.
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
    mod echo_vars;
    mod scalar_strings;
    mod array_basics;
    mod float_sort;
    mod numeric_scalars;
    mod type_builtins;
    mod casts_and_constants;
    mod magic_constants;
    mod strings;
    mod arrays;
    mod math;
    mod types;
}
