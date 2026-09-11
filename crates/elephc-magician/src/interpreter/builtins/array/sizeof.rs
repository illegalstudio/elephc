//! Purpose:
//! Declarative eval registry entry for `sizeof`, PHP's alias of `count`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Shares `Count` direct/values hooks so dispatch reuses count's adapters.

eval_builtin! {
    contract: "sizeof",
    area: Array,
    direct: Count,
    values: Count,
}
