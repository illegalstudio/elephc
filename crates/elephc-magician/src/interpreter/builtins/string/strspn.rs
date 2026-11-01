//! Purpose:
//! Declarative eval registry entry for PHP `strspn()`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Execution shares the php-src-compatible byte-span implementation owned by `strcspn`.

eval_builtin! {
    contract: "strspn",
    area: String,
    direct: StringSpan,
    values: StringSpan,
}
