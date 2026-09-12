//! Purpose:
//! Registers the eval implementation of PHP `error_reporting()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "error_reporting", area: Core, direct: Core, values: Core }
