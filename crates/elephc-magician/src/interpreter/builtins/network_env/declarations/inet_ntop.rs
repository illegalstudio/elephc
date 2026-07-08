//! Purpose:
//! Declarative eval registry entry for `inet_ntop`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the IP conversion hook.

eval_builtin! {
    name: "inet_ntop",
    area: NetworkEnv,
    params: [ip],
    direct: NetworkEnv,
    values: NetworkEnv,
}
