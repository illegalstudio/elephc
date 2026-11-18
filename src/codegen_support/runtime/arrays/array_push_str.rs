//! Purpose:
//! Exposes the native storage emitter to the compiler runtime.
//!
//! Called from:
//! - The compiler's managed runtime emitter inventory.
//!
//! Key details:
//! - Assembly and symbol selection enter through the established compiler adapter.

pub use elephc_memoir::values::array_push_str::emit_array_push_str;
