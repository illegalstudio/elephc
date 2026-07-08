//! Purpose:
//! Declarative eval registry entry for `stream_socket_get_name`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the socket-name lookup helper.

eval_builtin! {
    name: "stream_socket_get_name",
    area: Filesystem,
    params: [socket, remote],
    direct: Filesystem,
    values: Filesystem,
}
