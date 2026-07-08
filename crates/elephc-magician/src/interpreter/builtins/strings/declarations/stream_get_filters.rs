//! Purpose:
//! Declarative eval registry entry for `stream_get_filters`.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the static stream-filter list helper.

eval_builtin! {
    name: "stream_get_filters",
    area: String,
    params: [],
    direct: StreamIntrospection,
    values: StreamIntrospection,
}
