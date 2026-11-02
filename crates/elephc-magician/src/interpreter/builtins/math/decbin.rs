//! Purpose:
//! Eval registry entry for `decbin`: renders an integer as a binary numeral.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - The conversion lives in `base_digits`, shared with its five siblings.

eval_builtin! {
    contract: "decbin",
    area: Math,
    direct: BaseDigits,
    values: BaseDigits,
}
