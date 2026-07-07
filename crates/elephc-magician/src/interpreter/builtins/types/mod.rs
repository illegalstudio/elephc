//! Purpose:
//! Per-builtin declarations for scalar type and conversion functions migrated
//! to the eval builtin registry.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!`.

mod boolval;
