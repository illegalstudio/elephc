//! Purpose:
//! Binds mb_split to the same protected runtime operation used by compiled PHP.
//!
//! Called from:
//! - The Magician builtin registry when the managed Oniguruma provider is available.
//!
//! Key details:
//! - Eval has no independent regex splitter or argument-conversion implementation.

eval_builtin! {
    contract: "mb_split",
    area: String,
    direct: none,
    values: none,
}
