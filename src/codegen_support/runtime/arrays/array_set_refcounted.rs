//! Purpose:
//! Exposes Memoir's container emitter through the compiler runtime.
//!
//! Called from:
//! - Runtime assembly generation.
//!
//! Key details:
//! - The compiler controls emitter selection and validates the generated code.

pub use elephc_memoir::values::array_set_refcounted::emit_array_set_refcounted;
