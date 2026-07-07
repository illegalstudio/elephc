//! Purpose:
//! Per-builtin declarations for array and collection functions migrated to the
//! eval builtin registry.
//!
//! Called from:
//! - `crate::interpreter::builtins` module loading.
//!
//! Key details:
//! - Leaf files register metadata through `eval_builtin!`.

mod array_flip;
mod array_key_exists;
mod array_keys;
mod array_pad;
mod array_product;
mod array_rand;
mod array_reverse;
mod array_search;
mod array_slice;
mod array_sum;
mod array_unique;
mod array_values;
mod count;
mod in_array;
mod range;
