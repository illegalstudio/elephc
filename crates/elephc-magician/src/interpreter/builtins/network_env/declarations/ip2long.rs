//! Purpose:
//! Declarative eval registry entry for `ip2long`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the IP conversion hook.

eval_builtin! {
    name: "ip2long",
    area: NetworkEnv,
    params: [ip],
    direct: NetworkEnv,
    values: NetworkEnv,
}
