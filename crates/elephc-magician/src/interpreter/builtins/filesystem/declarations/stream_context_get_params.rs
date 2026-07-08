//! Purpose:
//! Declarative eval registry entry for `stream_context_get_params`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Eval mirrors the main backend's current empty-params behavior.

eval_builtin! {
    name: "stream_context_get_params",
    area: Filesystem,
    params: [context],
    direct: Filesystem,
    values: Filesystem,
}
