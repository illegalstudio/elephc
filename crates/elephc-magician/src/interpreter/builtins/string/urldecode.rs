//! Purpose:
//! Declarative eval registry entry for `urldecode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing URL decode hook.

eval_builtin! {
    name: "urldecode",
    area: String,
    params: [string],
    direct: UrlDecode,
    values: UrlDecode,
}
