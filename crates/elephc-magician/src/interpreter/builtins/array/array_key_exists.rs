//! Purpose:
//! Declarative eval registry entry for `array_key_exists`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the runtime key-existence hook.

eval_builtin! {
    name: "array_key_exists",
    area: Array,
    params: [key, array],
    direct: ArrayKeyExists,
    values: ArrayKeyExists,
}
