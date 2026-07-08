//! Purpose:
//! Declarative eval registry entry for `getservbyport`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the service lookup hook.

eval_builtin! {
    name: "getservbyport",
    area: NetworkEnv,
    params: [port, protocol],
    direct: NetworkEnv,
    values: NetworkEnv,
}
