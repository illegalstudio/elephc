//! Purpose:
//! Declarative eval registry entry for `stream_socket_enable_crypto`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the conservative TLS status helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "stream_socket_enable_crypto",
    area: Filesystem,
    params: [
        stream,
        enable,
        crypto_method = EvalBuiltinDefaultValue::Null,
        session_stream = EvalBuiltinDefaultValue::Null
    ],
    direct: Filesystem,
    values: Filesystem,
}
