//! Purpose:
//! Declarative eval registry entry for `exec`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the process-command hook.

eval_builtin! {
    name: "exec",
    area: NetworkEnv,
    params: [command],
    direct: NetworkEnv,
    values: NetworkEnv,
}
