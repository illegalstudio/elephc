//! Purpose:
//! Declarative eval registry entry for `is_int`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Runtime behavior stays delegated to the existing type-predicate hook.

eval_builtin! {
    name: "is_int",
    area: Types,
    params: [value],
    direct: TypePredicate,
    values: TypePredicate,
}
