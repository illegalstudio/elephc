//! Purpose:
//! Binds mb_ereg_replace to the same protected shared operation as compiled PHP.
//!
//! Called from:
//! - The Magician builtin registry when the managed Oniguruma provider is available.
//!
//! Key details:
//! - Eval has no separate replacement scanner or argument-conversion rules.

eval_builtin! {
    contract: "mb_ereg_replace",
    area: String,
    direct: none,
    values: none,
}
