//! Purpose:
//! Declarative eval registry entry for `getprotobynumber`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the protocol lookup hook.

eval_builtin! {
    name: "getprotobynumber",
    area: NetworkEnv,
    params: [protocol],
    direct: NetworkEnv,
    values: NetworkEnv,
}
