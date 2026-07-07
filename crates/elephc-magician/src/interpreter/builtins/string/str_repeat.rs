//! Purpose:
//! Declarative eval registry entry for `str_repeat`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing repeat hook.

eval_builtin! {
    name: "str_repeat",
    area: String,
    params: [string, times],
    direct: StrRepeat,
    values: StrRepeat,
}
