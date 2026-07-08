//! Purpose:
//! Declarative eval registry entry for `gethostname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the host lookup hook.

eval_builtin! {
    name: "gethostname",
    area: NetworkEnv,
    params: [],
    direct: NetworkEnv,
    values: NetworkEnv,
}
