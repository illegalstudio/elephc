//! Purpose:
//! Declarative eval registry entry for `stream_socket_recvfrom`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference address path.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_recvfrom",
    area: Filesystem,
    params: [
        socket,
        length,
        flags = EvalBuiltinDefaultValue::Int(0),
        address: by_ref = EvalBuiltinDefaultValue::String("")
    ],
    by_ref: [address],
    direct: none,
    values: Filesystem,
}
