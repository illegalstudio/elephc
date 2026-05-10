//! Purpose:
//! Groups the FFI integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for extern calls, memory, SDL FFI and extern method calls, syntax and callbacks.

use crate::support::*;

mod extern_calls;
mod memory;
mod sdl_and_methods;
mod syntax_and_callbacks;
