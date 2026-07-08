//! Purpose:
//! Declarative eval registry entry for `random_int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the random-number hook.

eval_builtin! {
    name: "random_int",
    area: Math,
    params: [min, max],
    direct: Random,
    values: Random,
}
