//! Purpose:
//! Declarative eval registry entry for `getprotobyname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the protocol lookup hook.

eval_builtin! {
    name: "getprotobyname",
    area: NetworkEnv,
    params: [protocol],
    direct: NetworkEnv,
    values: NetworkEnv,
}
