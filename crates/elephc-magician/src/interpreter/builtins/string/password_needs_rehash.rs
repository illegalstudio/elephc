//! Purpose:
//! Eval registry entry for `password_needs_rehash`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Implemented with the rest of the password API in `password_hash.rs`, which owns the shared
//!   bcrypt algorithm and cost rules.

eval_builtin! {
    contract: "password_needs_rehash",
    area: String,
    direct: PasswordApi,
    values: PasswordApi,
}
