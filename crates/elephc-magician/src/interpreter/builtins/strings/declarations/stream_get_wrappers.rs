//! Purpose:
//! Declarative eval registry entry for `stream_get_wrappers`.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the eval stream-wrapper registry helper.

eval_builtin! {
    name: "stream_get_wrappers",
    area: String,
    params: [],
    direct: StreamIntrospection,
    values: StreamIntrospection,
}
