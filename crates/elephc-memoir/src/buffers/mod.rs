//! Purpose:
//! Collects runtime emitters for the compiler buffer extension.
//! The module owns re-export wiring for allocation, validation, and fatal helper labels.
//!
//! Called from:
//! - `elephc::codegen_support::runtime::emitters::emit_runtime()` during the buffer runtime section.
//!
//! Key details:
//! - Buffer helpers enforce generation-safe ownership, bounds checks, and distinct fatal paths before unsafe access.

/// Maximum simultaneously addressable generation-safe buffer descriptors.
///
/// Handle index zero remains invalid; each live descriptor carries its own
/// generation so a freed slot can be reused without reviving stale aliases.
pub const BUFFER_REGISTRY_CAPACITY: usize = 4096;
/// Bytes occupied by one static buffer descriptor.
///
/// Layout: payload pointer, logical length, element stride, generation, active
/// marker, and free-list successor. The public PHP buffer value remains a scalar
/// `generation:u32 | index:u32` handle.
pub const BUFFER_DESCRIPTOR_SIZE: usize = 48;

mod bounds_fail;
mod buffer_free;
mod buffer_len;
mod buffer_new;
mod registry_fail;
mod resolve;
mod use_after_free;

pub use bounds_fail::emit_buffer_bounds_fail;
pub use buffer_free::emit_buffer_free;
pub use buffer_len::emit_buffer_len;
pub use buffer_new::emit_buffer_new;
pub use registry_fail::emit_buffer_registry_fail;
pub use resolve::{emit_buffer_resolve, emit_buffer_try_resolve};
pub use use_after_free::emit_buffer_use_after_free;
pub mod output;
pub mod offset_pairs;
pub mod diagnostic_buffer;
pub mod stream_scratch;
pub mod stream_state;
pub mod stream_tables;

pub mod native_words;

pub use resolve::{emit_element_address, emit_load_payload};

pub mod stream_context_owners;

pub mod io_scratch;
