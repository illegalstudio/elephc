//! Purpose:
//! Re-exports Memoir's opaque runtime cells through Magician's existing value API.
//!
//! Called from:
//! - The interpreter, runtime hooks and the eval FFI adapters.
//!
//! Key details:
//! - The native ABI still carries raw pointers.

pub use elephc_memoir::runtime_owners::{RuntimeCell, RuntimeCellHandle};
