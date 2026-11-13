//! Purpose:
//! Shares semantic profiles and policy decisions between the compiler and eval.
//!
//! Called from:
//! - Compiler option resolution, semantic checking, and runtime integration.
//!
//! Key details:
//! - This crate has no compiler, target, runtime-cell, or assembly dependencies.

mod catalog;
mod profile;
pub mod arithmetic_policy;
pub mod array_keys;
pub mod numeric;
pub mod numeric_string;
pub mod integer_literals;
pub mod conversions;

pub use catalog::{FeatureSpec, StrictFeature, FEATURES, INTEGRATED_FEATURE_BITS};
pub use profile::{ProfileDecodeError, SemanticAbiError, SemanticProfile, StrictRequests, SEMANTICS_ABI_VERSION};
