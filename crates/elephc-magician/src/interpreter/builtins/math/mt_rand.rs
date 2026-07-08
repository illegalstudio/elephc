//! Purpose:
//! Declarative eval registry entry for `mt_rand`.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the random-number hook.

eval_builtin! {
    name: "mt_rand",
    area: Math,
    params: [min, max],
    direct: Random,
    values: Random,
}
