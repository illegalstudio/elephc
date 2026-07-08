//! Purpose:
//! Declarative eval registry entry for `getservbyname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the service lookup hook.

eval_builtin! {
    name: "getservbyname",
    area: NetworkEnv,
    params: [service, protocol],
    direct: NetworkEnv,
    values: NetworkEnv,
}
