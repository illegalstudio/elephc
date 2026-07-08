//! Purpose:
//! Declarative eval registry entry for `stream_supports_lock`.
//!
//! Called from:
//! - `crate::interpreter::builtins::strings::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the stream boolean predicate helper.

eval_builtin! {
    name: "stream_supports_lock",
    area: String,
    params: [stream],
    direct: StreamBoolPredicate,
    values: StreamBoolPredicate,
}
