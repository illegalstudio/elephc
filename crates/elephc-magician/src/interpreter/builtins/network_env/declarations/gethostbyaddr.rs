//! Purpose:
//! Declarative eval registry entry for `gethostbyaddr`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the host lookup hook.

eval_builtin! {
    name: "gethostbyaddr",
    area: NetworkEnv,
    params: [ip],
    direct: NetworkEnv,
    values: NetworkEnv,
}
