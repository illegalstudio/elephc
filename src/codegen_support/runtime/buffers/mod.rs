//! Purpose:
//! Re-exports generation-safe buffer memory emission from Memoir.
//!
//! Called from:
//! - Compiler memory emission and its integration tests.
//!
//! Key details:
//! - Runs only in the compiler and preserves the generated target assembly.

pub use elephc_memoir::buffers::{emit_buffer_bounds_fail, emit_buffer_free, emit_buffer_len, emit_buffer_new, emit_buffer_registry_fail, emit_buffer_resolve, emit_buffer_try_resolve, emit_buffer_use_after_free};
