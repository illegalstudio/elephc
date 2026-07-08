//! Purpose:
//! Declarative eval registry entry for `pfsockopen`.
//!
//! Called from:
//! - `crate::interpreter::builtins::filesystem::declarations`.
//!
//! Key details:
//! - Direct calls keep their source-sensitive by-reference error-output path.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "pfsockopen",
    area: Filesystem,
    params: [
        hostname,
        port,
        error_code: by_ref = EvalBuiltinDefaultValue::Null,
        error_message: by_ref = EvalBuiltinDefaultValue::Null,
        timeout = EvalBuiltinDefaultValue::Null
    ],
    by_ref: [error_code, error_message],
    direct: none,
    values: Filesystem,
}
