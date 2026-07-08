//! Purpose:
//! Declarative eval registry entry for `stream_socket_sendto`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the connected-socket write helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_sendto",
    area: Filesystem,
    params: [
        socket,
        data,
        flags = EvalBuiltinDefaultValue::Int(0),
        address = EvalBuiltinDefaultValue::String("")
    ],
    direct: Filesystem,
    values: Filesystem,
}
