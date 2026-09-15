//! Purpose:
//! Registers the eval implementation of PHP `user_error()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "user_error", area: Core, direct: Core, values: Core }
