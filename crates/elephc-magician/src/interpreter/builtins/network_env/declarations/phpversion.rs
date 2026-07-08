//! Purpose:
//! Declarative eval registry entry for `phpversion`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the system-information hook.

eval_builtin! {
    name: "phpversion",
    area: NetworkEnv,
    params: [],
    direct: NetworkEnv,
    values: NetworkEnv,
}
