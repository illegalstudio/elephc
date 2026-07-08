//! Purpose:
//! Declarative eval registry entry for `stream_context_set_params`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the accepted no-op helper.

eval_builtin! {
    name: "stream_context_set_params",
    area: Filesystem,
    params: [context, params],
    direct: Filesystem,
    values: Filesystem,
}
