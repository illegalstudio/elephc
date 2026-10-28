//! Purpose:
//! End-to-end codegen test binary for classes, objects, generics, SPL, iterators, generators, exceptions and fibers.
//!
//! Called from:
//! - `cargo test --test codegen_tests_oop` through Rust's test harness.
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
    mod exceptions;
    mod fibers;
    mod generics;
    mod generic_native_storage;
    mod generics_review;
    mod generics_constructors;
    mod generics_named_spreads;
    mod generics_docblock_methods;
    mod generics_docblock_enums;
    mod namespaces;
    mod case_insensitive_symbols;
    mod symbol_catalog;
    mod core_classes;
    mod objects;
    mod oop;
    mod static_class_features;
    mod iterators;
    mod spl;
    mod generators;
}
