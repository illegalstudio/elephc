//! Purpose:
//! Declarative eval registry entry for `stream_get_transports`.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the static stream-transport list helper.

eval_builtin! {
    name: "stream_get_transports",
    area: String,
    params: [],
    direct: StreamIntrospection,
    values: StreamIntrospection,
}
