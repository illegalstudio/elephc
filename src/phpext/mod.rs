//! Purpose:
//! Owns producer-side integration with a real PHP host, separately from extension hosting.
//!
//! Called from:
//! - PHP extension output integration and `tests/phpext_tests.rs`.
//!
//! Key details:
//! - Host headers and host Zend symbols remain authoritative.
//! - This module never links the replacement engine into a produced extension.

pub mod build;
pub mod classes;
pub mod class_helpers;
pub mod class_registration;
pub mod constants;
pub mod constant_values;
mod constant_types;
mod constant_arrays;
mod producer;
mod prune;
mod flags;
mod probe;
mod registration;
mod scratch;
mod sdk;
pub(crate) mod state;
pub(crate) mod signature;

pub use registration::render_registration;
pub use constants::ExportedConstant;
pub use sdk::{HostBuild, PhpSdk};
