//! Purpose:
//! Eval registry entry for `hexdec`: renders a hexadecimal numeral as an integer.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - The conversion lives in `base_digits`, shared with its five siblings.

eval_builtin! {
    contract: "hexdec",
    area: Math,
    direct: BaseDigits,
    values: BaseDigits,
}
