//! Purpose:
//! Collects and validates declaration schemas before body type checking.
//! Builds function, class, interface, enum, trait, and FFI metadata consumed by later checker phases.
//!
//! Called from:
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Schema checks run before expression inference so all declarations are available for recursive references.

pub(crate) mod validation;
mod interfaces;
mod classes;
mod enums;

pub(crate) use interfaces::*;
pub(crate) use classes::*;
pub(crate) use enums::*;
