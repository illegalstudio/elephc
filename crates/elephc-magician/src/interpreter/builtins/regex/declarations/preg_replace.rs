//! Purpose:
//! Declarative eval registry entry for `preg_replace`.
//!
//! Called from:
//! - `crate::interpreter::builtins::regex::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the regex replacement hook.

eval_builtin! {
    name: "preg_replace",
    area: Regex,
    params: [pattern, replacement, subject],
    direct: Regex,
    values: Regex,
}
