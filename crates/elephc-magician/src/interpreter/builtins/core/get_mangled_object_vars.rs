//! Purpose:
//! Registers the eval implementation of PHP `get_mangled_object_vars()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "get_mangled_object_vars", area: Core, direct: Core, values: Core }
