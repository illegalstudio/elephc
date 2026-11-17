//! Purpose:
//! Connects Memoir's reusable C-string destinations to the I/O runtime.
//!
//! Called from:
//! - Compiler platform runtime emission.
//!
//! Key details:
//! - Callers continue to use the two distinct borrowed destinations.

pub use elephc_memoir::interop::cstr_scratch::emit_cstr;
