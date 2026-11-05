//! Purpose:
//! Declares the eval binding for `tidy_parse_file`.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled tidy prelude.

eval_builtin! { contract: "tidy_parse_file", area: Tidy, direct: Tidy, values: Tidy }
