//! Purpose:
//! Groups the FFI integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for extern calls, memory, nullable `?string` parameters and returns, return normalization, SDL FFI and extern method calls, extern class struct layouts, syntax and callbacks.

use crate::support::*;

mod extern_calls;
mod memory;
mod nullable_strings;
mod return_normalization;
mod sdl_and_methods;
mod struct_layout;
mod syntax_and_callbacks;
