//! Purpose:
//! Declares the eval binding for `filter_var_array`.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled filter prelude.

eval_builtin! { contract: "filter_var_array", area: Filter, direct: Filter, values: Filter }
