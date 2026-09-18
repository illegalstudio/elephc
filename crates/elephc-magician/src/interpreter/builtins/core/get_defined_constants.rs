//! Purpose:
//! Registers the eval implementation of PHP `get_defined_constants()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "get_defined_constants", area: Core, direct: Core, values: Core }
