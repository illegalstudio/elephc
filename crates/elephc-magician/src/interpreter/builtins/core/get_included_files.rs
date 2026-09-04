//! Purpose:
//! Registers the eval implementation of PHP `get_included_files()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "get_included_files", area: Core, direct: Core, values: Core }
