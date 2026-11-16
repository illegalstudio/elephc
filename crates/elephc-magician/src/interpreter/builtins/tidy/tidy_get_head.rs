//! Purpose:
//! Declares the eval binding for `tidy_get_head`.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled tidy prelude.

eval_builtin! { contract: "tidy_get_head", area: Tidy, direct: Tidy, values: Tidy }
