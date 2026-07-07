//! Purpose:
//! Per-builtin declarations for numeric functions migrated to the eval builtin
//! registry.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!`.

mod abs;
