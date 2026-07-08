//! Purpose:
//! Declarative eval registry entry for `stream_socket_pair`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the local socket-pair helper.

eval_builtin! {
    name: "stream_socket_pair",
    area: Filesystem,
    params: [domain, r#type, protocol],
    direct: Filesystem,
    values: Filesystem,
}
