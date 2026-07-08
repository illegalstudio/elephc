//! Purpose:
//! Declarative eval registry entry for `stream_socket_shutdown`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the socket shutdown helper.

eval_builtin! {
    name: "stream_socket_shutdown",
    area: Filesystem,
    params: [stream, mode],
    direct: Filesystem,
    values: Filesystem,
}
