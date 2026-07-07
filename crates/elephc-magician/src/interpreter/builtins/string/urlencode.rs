//! Purpose:
//! Declarative eval registry entry for `urlencode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing URL encode hook.

eval_builtin! {
    name: "urlencode",
    area: String,
    params: [string],
    direct: UrlEncode,
    values: UrlEncode,
}
