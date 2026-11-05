//! Purpose:
//! Declares the eval binding for `tidy_clean_repair`.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled tidy prelude.

eval_builtin! { contract: "tidy_clean_repair", area: Tidy, direct: Tidy, values: Tidy }
