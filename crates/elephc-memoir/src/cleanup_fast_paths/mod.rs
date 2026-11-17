//! Purpose:
//! Collects memory proofs for release paths without child cleanup.
//!
//! Called from:
//! - Compiler exception cleanup adapters.
//!
//! Key details:
//! - No exception handlers or PHP type definitions are emitted here.

pub mod scalar_mixed;
pub mod empty_array;
