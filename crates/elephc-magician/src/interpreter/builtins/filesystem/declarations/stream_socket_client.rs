//! Purpose:
//! Declarative eval registry entry for `stream_socket_client`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the TCP client stream helper.

eval_builtin! {
    name: "stream_socket_client",
    area: Filesystem,
    params: [address],
    direct: Filesystem,
    values: Filesystem,
}
