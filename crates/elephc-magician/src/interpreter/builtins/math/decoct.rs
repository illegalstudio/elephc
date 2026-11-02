//! Purpose:
//! Eval registry entry for `decoct`: renders an integer as a octal numeral.
//!
//! Called from:
//! - `crate::interpreter::builtins::math`.
//!
//! Key details:
//! - The conversion lives in `base_digits`, shared with its five siblings.

eval_builtin! {
    contract: "decoct",
    area: Math,
    direct: BaseDigits,
    values: BaseDigits,
}
