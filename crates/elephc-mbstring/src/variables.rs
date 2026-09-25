//! Purpose:
//! Exposes the live PHP variable conversion engine for mb_convert_variables.
//!
//! Called from:
//! - Native and eval reference adapters through the mbstring invocation coordinator.
//!
//! Key details:
//! - Conversion traverses caller storage after one shared source-encoding choice.
//! - Host adapters preserve array COW, object identity, and PHP reference replacement.

mod live;
pub mod host;

pub use live::{Container, LiveFailure, LiveHost, LiveValue, convert_live};
