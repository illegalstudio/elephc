//! Purpose:
//! Declarative eval registry entry for `settype`.
//!
//! Called from:
//! - `crate::interpreter::builtins::types`.
//!
//! Key details:
//! - Direct calls stay on the source-sensitive by-reference path because they
//!   need writable `EvalCallArg` targets.

eval_builtin! {
    name: "settype",
    area: Types,
    params: [var: by_ref, r#type],
    by_ref: [var],
    direct: none,
    values: Settype,
}
