//! Purpose:
//! Declarative eval registry entry for `stream_socket_server`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the TCP listener helper.

eval_builtin! {
    name: "stream_socket_server",
    area: Filesystem,
    params: [address],
    direct: Filesystem,
    values: Filesystem,
}
