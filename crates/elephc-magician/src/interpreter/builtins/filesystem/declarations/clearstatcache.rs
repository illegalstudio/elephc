//! Purpose:
//! Declarative eval registry entry for `clearstatcache`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to eval's ordered no-op stat-cache helper.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "clearstatcache",
    area: Filesystem,
    params: [
        clear_realpath_cache = EvalBuiltinDefaultValue::Bool(false),
        filename = EvalBuiltinDefaultValue::String("")
    ],
    direct: Filesystem,
    values: Filesystem,
}
