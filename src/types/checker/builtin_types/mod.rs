//! Purpose:
//! Wires synthetic builtin class and interface declarations into checker setup.
//! Exposes patching and validation hooks for PHP runtime types such as Throwable, Exception, and Fiber.
//!
//! Called from:
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Injected declarations must be present before schema validation and method signature checks run.

mod declarations;
mod exception;
mod fiber;
mod magic_methods;
mod reflection;

pub(crate) use declarations::{InterfaceDeclInfo, inject_builtin_throwables};
pub(crate) use exception::patch_builtin_exception_signatures;
pub(crate) use fiber::patch_builtin_fiber_signatures;
pub(crate) use magic_methods::{patch_magic_method_signatures, validate_magic_method_contracts};
pub(crate) use reflection::{inject_builtin_reflection, patch_builtin_reflection_signatures};
