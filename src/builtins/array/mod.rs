//! Purpose:
//! Groups all `array`-area builtin homes into this module so the registry can
//! collect them in one place. Each submodule declares exactly one builtin via
//! `builtin!` and provides its type-check and lowering hooks.
//!
//! Called from:
//! - `crate::builtins` (`mod array;` in `src/builtins/mod.rs`).
//!
//! Key details:
//! - Add `pub mod <name>;` here for every new array builtin home.
//! - Most array builtins need a `check` hook because their return type depends
//!   on the inferred argument type (e.g. `array_keys` over an indexed vs assoc
//!   array). The `builtin!` `returns:` field is only consulted when no hook is
//!   present.

pub mod array_chunk;
pub mod array_column;
pub mod array_combine;
pub mod array_diff;
pub mod array_diff_assoc;
pub mod array_diff_key;
pub mod array_fill;
pub mod array_fill_keys;
pub mod array_flip;
pub mod array_intersect;
pub mod array_intersect_assoc;
pub mod array_intersect_key;
pub mod array_is_list;
pub mod array_key_exists;
pub mod array_key_first;
pub mod array_key_last;
pub mod array_keys;
pub mod array_merge;
pub mod array_merge_recursive;
pub mod array_pad;
pub mod array_pop;
pub mod array_product;
pub mod array_push;
pub mod array_rand;
pub mod array_replace;
pub mod array_replace_recursive;
pub mod array_reverse;
pub mod array_search;
pub mod array_shift;
pub mod array_slice;
pub mod array_splice;
pub mod array_sum;
pub mod array_unique;
pub mod array_unshift;
pub mod array_values;
pub mod in_array;
pub mod range;
