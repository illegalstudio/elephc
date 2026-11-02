//! Purpose:
//! Eval registry entry for `dechex`: renders an integer as a hexadecimal numeral.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - The conversion lives in `base_digits`, shared with its five siblings.

eval_builtin! {
    contract: "dechex",
    area: Math,
    direct: BaseDigits,
    values: BaseDigits,
}
