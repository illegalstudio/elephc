//! Purpose:
//! Models PHP array graphs shared by mbstring recursive validation and conversion.
//!
//! Called from:
//! - Array-capable bridge adapters and focused PHP compatibility tests.
//!
//! Key details:
//! - Array identities preserve cycles and repeated references without Rust reference cycles.
//! - String keys remain distinct from integer keys, including converted numeric-looking keys.
//! - Object/resource values are explicitly unsupported by these PHP mbstring operations.

mod check;
mod convert;

pub use check::{check_encoding, ArrayCheck};
pub use convert::{convert_encoding, ArrayConversion, ConversionFailure};
pub use elephc_builtin_contract::mbstring_abi::array::{Array, ArrayGraph, Key, Value};
