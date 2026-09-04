//! Purpose:
//! Registers the eval implementation of PHP `debug_print_backtrace()`.
//!
//! Called from:
//! - The declarative eval builtin registry.
//!
//! Key details:
//! - Runtime behavior is shared by `core::runtime_introspection`.

eval_builtin! { contract: "debug_print_backtrace", area: Core, direct: Core, values: Core }
