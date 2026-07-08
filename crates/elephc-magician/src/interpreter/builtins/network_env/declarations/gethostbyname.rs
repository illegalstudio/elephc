//! Purpose:
//! Declarative eval registry entry for `gethostbyname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the host lookup hook.

eval_builtin! {
    name: "gethostbyname",
    area: NetworkEnv,
    params: [hostname],
    direct: NetworkEnv,
    values: NetworkEnv,
}
