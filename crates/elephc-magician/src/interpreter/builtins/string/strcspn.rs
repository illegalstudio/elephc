//! Purpose:
//! Declarative eval registry entry for `strcspn`.
//!
//! Called from:
//! - `crate::interpreter::builtins::string`.
//!
//! Key details:
//! - Shares `eval_span_count` with `strspn`, counting bytes NOT in the mask.

eval_builtin! {
    contract: "strcspn",
    area: String,
    direct: Strcspn,
    values: Strcspn,
}
