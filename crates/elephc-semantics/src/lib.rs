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
pub mod indexing;
pub mod numeric;
pub mod numeric_cast;
pub mod gmp_integer_cast;
pub mod settype_target;
pub mod integer_cast_base;
pub mod numeric_domains;
pub mod numeric_unary;
pub mod integer_power;
pub mod numeric_string;
pub mod integer_literals;
pub mod conversions;
pub mod weak_scalar_binding;
pub mod comparisons;
pub mod scalar_value;
pub mod scalar_compare;
pub mod constant_numeric;
pub mod native_local_contract;
pub mod native_callable_sources;
pub mod initialization;
pub mod nullability;

pub use catalog::{FeatureSpec, StrictFeature, FEATURES, INTEGRATED_FEATURE_BITS};
pub use profile::{ProfileDecodeError, SemanticAbiError, SemanticProfile, StrictRequests, SEMANTICS_ABI_VERSION};
