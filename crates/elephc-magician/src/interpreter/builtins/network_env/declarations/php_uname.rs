//! Purpose:
//! Declarative eval registry entry for `php_uname`.
//!
//! Called from:
//! - `crate::interpreter::builtins::network_env::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the system-information hook.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "php_uname",
    area: NetworkEnv,
    params: [mode = EvalBuiltinDefaultValue::String("a")],
    direct: NetworkEnv,
    values: NetworkEnv,
}
