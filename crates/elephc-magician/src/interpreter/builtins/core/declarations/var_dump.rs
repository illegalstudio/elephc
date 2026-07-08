//! Purpose:
//! Declarative eval registry entry for `var_dump`.
//!
//! Called from:
//! - `crate::interpreter::builtins::core::declarations`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the debug-output hook.

eval_builtin! {
    name: "var_dump",
    area: Core,
    params: [value],
    variadic: values,
    direct: Core,
    values: Core,
}
