//! Purpose:
//! Owns Elephc's emitted memory-management helpers and Magician's eval buffer storage.
//!
//! Called from:
//! - Elephc enables `codegen` and supplies an emitter and target ABI.
//! - Magician enables `runtime` and supplies PHP argument and diagnostic handling.
//!
//! Key details:
//! - Cargo can build each consumer with just its own feature.
//! - The codegen module forbids unsafe code. The eval allocator contains its libc calls.

#![deny(unsafe_code)]

#[cfg(any(feature = "codegen", feature = "runtime"))]
pub mod owner_layout;

#[cfg(feature = "codegen")]
#[path = "codegen_root.rs"]
mod codegen_root;

#[cfg(feature = "codegen")]
pub use codegen_root::*;

#[cfg(feature = "runtime")]
pub mod cell_retirement;
#[cfg(feature = "runtime")]
pub mod eval_buffers;
#[cfg(feature = "runtime")]
pub mod object_owners;
#[cfg(feature = "runtime")]
pub mod output_handlers;
#[cfg(feature = "runtime")]
pub mod runtime_bytes;
#[cfg(feature = "runtime")]
pub mod runtime_owners;
