//! Purpose:
//! Registers the eval implementation of PHP `restore_error_handler()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "restore_error_handler", area: Core, direct: Core, values: Core }
