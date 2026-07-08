//! Purpose:
//! Declarative eval registry entry for `uksort`.
//!
//! Called from:
//! - `crate::interpreter::builtins::array`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path.

eval_builtin! {
    name: "uksort",
    area: Array,
    params: [array: by_ref, callback],
    by_ref: [array],
    direct: none,
    values: ArrayMutating,
}
