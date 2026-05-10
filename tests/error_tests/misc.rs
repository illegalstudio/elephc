//! Purpose:
//! Groups the misc integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for miscellaneous syntax diagnostics, classes, system builtin diagnostics, pointers, functions, and related suites.

use super::*;

#[path = "misc/syntax_misc.rs"]
mod syntax_misc;
#[path = "misc/classes.rs"]
mod classes;
#[path = "misc/system_builtins.rs"]
mod system_builtins;
#[path = "misc/pointers.rs"]
mod pointers;
#[path = "misc/functions.rs"]
mod functions;
#[path = "misc/string_and_type_builtins.rs"]
mod string_and_type_builtins;
#[path = "misc/math_more.rs"]
mod math_more;
