//! Purpose:
//! Collects runtime emitters for the compiler buffer extension.
//! The module owns re-export wiring for allocation, validation, and fatal helper labels.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` during the buffer runtime section.
//!
//! Key details:
//! - Buffer helpers enforce generation-safe ownership, bounds checks, and distinct fatal paths before unsafe access.

/// Maximum simultaneously addressable generation-safe buffer descriptors.
///
/// Handle index zero remains invalid; each live descriptor carries its own
/// generation so a freed slot can be reused without reviving stale aliases.
pub(crate) use elephc_parallel_contract::BUFFER_REGISTRY_CAPACITY;
/// Bytes occupied by one static buffer descriptor.
///
/// Layout: payload pointer, logical length, element stride, generation, active
/// marker, and free-list successor. The public PHP buffer value remains a scalar
/// `generation:u32 | index:u32` handle.
pub(crate) use elephc_parallel_contract::BUFFER_DESCRIPTOR_SIZE;

mod bounds_fail;
mod buffer_free;
mod buffer_len;
mod buffer_new;
mod drain;
mod registry_fail;
mod resolve;
mod use_after_free;

pub use bounds_fail::emit_buffer_bounds_fail;
pub use buffer_free::emit_buffer_free;
pub use buffer_len::emit_buffer_len;
pub use buffer_new::emit_buffer_new;
pub use drain::emit_buffer_registry_drain;
pub use registry_fail::emit_buffer_registry_fail;
pub use resolve::emit_buffer_resolve;
pub use use_after_free::emit_buffer_use_after_free;
