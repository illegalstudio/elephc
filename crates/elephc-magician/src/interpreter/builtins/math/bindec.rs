//! Purpose:
//! Eval registry entry for `bindec`: renders a binary numeral as an integer.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - The conversion lives in `base_digits`, shared with its five siblings.

eval_builtin! {
    contract: "bindec",
    area: Math,
    direct: BaseDigits,
    values: BaseDigits,
}
