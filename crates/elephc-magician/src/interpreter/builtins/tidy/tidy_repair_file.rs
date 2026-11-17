//! Purpose:
//! Declares the eval binding for `tidy_repair_file`.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled tidy prelude.

eval_builtin! { contract: "tidy_repair_file", area: Tidy, direct: Tidy, values: Tidy }
