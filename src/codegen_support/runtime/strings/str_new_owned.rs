//! Purpose:
//! Preserves compiler imports and tests for Memoir string storage.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::strings::str_new_owned::*;
