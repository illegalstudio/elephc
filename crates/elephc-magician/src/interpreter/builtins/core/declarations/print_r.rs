//! Purpose:
//! Declarative eval registry entry for `print_r`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the debug-output hook.

use super::super::super::spec::EvalBuiltinDefaultValue;

eval_builtin! {
    name: "print_r",
    area: Core,
    params: [value, r#return = EvalBuiltinDefaultValue::Bool(false)],
    direct: Core,
    values: Core,
}
