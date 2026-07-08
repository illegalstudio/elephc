//! Purpose:
//! Declarative eval registry entry for `putenv`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the environment hook.

eval_builtin! {
    name: "putenv",
    area: NetworkEnv,
    params: [assignment],
    direct: NetworkEnv,
    values: NetworkEnv,
}
