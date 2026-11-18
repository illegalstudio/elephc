//! Purpose:
//! Collects memory lifetime operations for callable descriptors.
//!
//! Called from:
//! - Compiler runtime orchestration and focused memory integration tests.
//!
//! Key details:
//! - Runs inside the compiler and emits target assembly only.

pub mod descriptor_release;
pub use descriptor_release::emit_callable_descriptor_release;

pub mod argument_owners;

pub mod storage;
