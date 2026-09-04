//! Purpose:
//! Registers the eval implementation of PHP `get_required_files()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "get_required_files", area: Core, direct: Core, values: Core }
