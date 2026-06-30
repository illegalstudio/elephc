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

pub mod addslashes;
pub mod chop;
pub mod explode;
pub mod implode;
pub mod lcfirst;
pub mod ltrim;
pub mod nl2br;
pub mod ord;
pub mod rtrim;
pub mod str_contains;
pub mod str_ends_with;
pub mod str_repeat;
pub mod str_split;
pub mod str_starts_with;
pub mod strcasecmp;
pub mod strcmp;
pub mod stripslashes;
pub mod strpos;
pub mod strrev;
pub mod strrpos;
pub mod strstr;
pub mod strtolower;
pub mod strtoupper;
pub mod substr;
pub mod trim;
pub mod ucfirst;
pub mod ucwords;
