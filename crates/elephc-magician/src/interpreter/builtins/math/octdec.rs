//! Purpose:
//! Eval registry entry for `octdec`: renders a octal numeral as an integer.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - The conversion lives in `base_digits`, shared with its five siblings.

eval_builtin! {
    contract: "octdec",
    area: Math,
    direct: BaseDigits,
    values: BaseDigits,
}
