//! Purpose:
//! Declarative eval registry entry for `long2ip`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the IP conversion hook.

eval_builtin! {
    name: "long2ip",
    area: NetworkEnv,
    params: [ip],
    direct: NetworkEnv,
    values: NetworkEnv,
}
