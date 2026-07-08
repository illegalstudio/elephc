//! Purpose:
//! Declarative eval registry entry for `stream_socket_accept`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference peer-name path.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_accept",
    area: Filesystem,
    params: [
        socket,
        timeout = EvalBuiltinDefaultValue::Null,
        peer_name: by_ref = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [peer_name],
    direct: none,
    values: Filesystem,
}
