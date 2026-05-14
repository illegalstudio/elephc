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
#[path = "spl/interfaces.rs"]
mod interfaces;
#[path = "spl/redirects.rs"]
mod redirects;
#[path = "spl/exceptions.rs"]
mod exceptions;
