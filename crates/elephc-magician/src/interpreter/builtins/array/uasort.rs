//! Purpose:
//! Declarative eval registry entry for `uasort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

eval_builtin! {
    name: "uasort",
    area: Array,
    params: [array: by_ref, callback],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
