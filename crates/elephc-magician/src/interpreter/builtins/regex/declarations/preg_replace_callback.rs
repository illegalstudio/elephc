//! Purpose:
//! Declarative eval registry entry for `preg_replace_callback`.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::declarations`.
//!
//! Key details:
//! - Direct calls keep lexical scope for callback names; evaluated dynamic
//!   dispatch uses the same scope-free behavior as the legacy dispatcher.

eval_builtin! {
    name: "preg_replace_callback",
    area: Regex,
    params: [pattern, callback, subject],
    direct: Regex,
    values: Regex,
}
