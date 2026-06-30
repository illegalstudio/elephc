//! Purpose:
//! Groups all `string`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its type-check and lowering hooks.
//!
//! Called from:
//! - `crate::builtins` (`mod string;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Add `pub mod <name>;` here for every new string builtin home.
//! - Pure-data builtins (no check hook) only need a `lower` fn; the `builtin!`
//!   `returns:` field provides the declared return type.

pub mod ord;
pub mod substr;
