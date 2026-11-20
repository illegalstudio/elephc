//! Purpose:
//! Declares the eval binding for `filter_has_var`.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled filter prelude.

eval_builtin! { contract: "filter_has_var", area: Filter, direct: Filter, values: Filter }
