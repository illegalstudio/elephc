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
mod native_host;
mod plan;

pub use live::{Container, LiveFailure, LiveHost, LiveValue, convert_live};
pub(crate) use native_host::{
    elephc_mbstring_variable_array_count_v1, elephc_mbstring_variable_heap_shape_v1, hash_child, indexed_child,
};
pub use plan::VariablePlan;
