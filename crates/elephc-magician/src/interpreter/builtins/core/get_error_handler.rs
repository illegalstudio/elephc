//! Purpose:
//! Registers the eval implementation of PHP `get_error_handler()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.
//! - Hosted eval reads the process-wide native handler; pure eval reads its local stack.

eval_builtin! { contract: "get_error_handler", area: Core, direct: Core, values: Core }
