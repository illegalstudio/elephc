//! Purpose:
//! Declares the eval binding for `filter_input`.
//!
//! Called from:
//! - The declarative eval registry.
//!
//! Key details:
//! - Execution forwards to the host's compiled filter prelude.

eval_builtin! { contract: "filter_input", area: Filter, direct: Filter, values: Filter }
