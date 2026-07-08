//! Purpose:
//! Declarative eval registry entry for `getenv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the environment hook.

eval_builtin! {
    name: "getenv",
    area: NetworkEnv,
    params: [name],
    direct: NetworkEnv,
    values: NetworkEnv,
}
