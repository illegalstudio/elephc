//! Purpose:
//! Declarative eval registry entry for `strrev`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing string-reversal hook.

eval_builtin! {
    name: "strrev",
    area: String,
    params: [string],
    direct: Strrev,
    values: Strrev,
}
