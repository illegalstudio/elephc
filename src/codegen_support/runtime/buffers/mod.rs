//! Purpose:
//! Collects runtime emitters for the compiler buffer extension.
//! The module owns re-export wiring for allocation, length, bounds, and use-after-free helper labels.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` during the buffer runtime section.
//!
//! Key details:
//! - Buffer helpers enforce extension ownership rules, including live headers, bounds checks, and fatal paths before unsafe access.

mod bounds_fail;
mod buffer_len;
mod buffer_new;
mod use_after_free;

pub use bounds_fail::emit_buffer_bounds_fail;
pub use buffer_len::emit_buffer_len;
pub use buffer_new::emit_buffer_new;
pub use use_after_free::emit_buffer_use_after_free;
