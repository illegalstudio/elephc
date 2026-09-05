//! Purpose:
//! Registers the eval implementation of PHP `set_exception_handler()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "set_exception_handler", area: Core, direct: Core, values: Core }
