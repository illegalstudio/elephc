//! Purpose:
//! Integration tests for SPL foundation codegen coverage.
//! Wires the SPL test submodules into the codegen test harness.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through Rust's test harness.
//!
//! Key details:
//! - Submodules cover AOT autoloading, builtin interfaces, SPL redirects, and SPL exceptions.

#[path = "spl/autoload.rs"]
mod autoload;
#[path = "spl/classes.rs"]
mod classes;
#[path = "spl/decorators.rs"]
mod decorators;
#[path = "spl/interfaces.rs"]
mod interfaces;
#[path = "spl/introspection.rs"]
mod introspection;
#[path = "spl/iterator_helpers.rs"]
mod iterator_helpers;
#[path = "spl/heaps.rs"]
mod heaps;
#[path = "spl/intrinsics.rs"]
mod intrinsics;
#[path = "spl/redirects.rs"]
mod redirects;
#[path = "spl/exceptions.rs"]
mod exceptions;
#[path = "spl/filesystem.rs"]
mod filesystem;
#[path = "spl/file_object_open.rs"]
mod file_object_open;
#[path = "spl/file_object_bounds.rs"]
mod file_object_bounds;
#[path = "spl/dynamic_method_name.rs"]
mod dynamic_method_name;
#[path = "spl/exception_location.rs"]
mod exception_location;
#[path = "spl/exception_trace.rs"]
mod exception_trace;
#[path = "spl/array_object_missing_key.rs"]
mod array_object_missing_key;
#[path = "spl/file_object_read_ahead.rs"]
mod file_object_read_ahead;
#[path = "spl/file_object_lines.rs"]
mod file_object_lines;
#[path = "spl/recursive.rs"]
mod recursive;
#[path = "spl/regex.rs"]
mod regex;
#[path = "spl/storage.rs"]
mod storage;
