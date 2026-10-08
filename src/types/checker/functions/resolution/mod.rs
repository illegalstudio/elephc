//! Purpose:
//! Organizes named-function resolution, signature publication, and specialization.
//!
//! Called from:
//! - `crate::types::checker::functions`.
//!
//! Key details:
//! - Direct and pre-normalized calls converge on the same resolved-signature validator.

mod call;
mod instantiate;
mod resolved;
mod signature;
mod specialization;
