//! Purpose:
//! Declarative eval registry entry for `rawurldecode`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing URL decode hook.

eval_builtin! {
    name: "rawurldecode",
    area: String,
    params: [string],
    direct: UrlDecode,
    values: UrlDecode,
}
