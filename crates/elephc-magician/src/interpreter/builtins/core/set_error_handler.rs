//! Purpose:
//! Registers the eval implementation of PHP `set_error_handler()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "set_error_handler", area: Core, direct: Core, values: Core }
