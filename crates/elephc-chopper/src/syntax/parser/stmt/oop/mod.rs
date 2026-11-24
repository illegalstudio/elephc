//! Purpose:
//! Groups parsers for PHP class-like declarations and members.
//! Re-exports class, interface, trait, enum, packed, body, modifier, and trait-use parsers.
//!
//! Called from:
//! - `crate::syntax::parser::stmt::parse_stmt()`.
//!
//! Key details:
//! - OOP parsing is split by declaration surface while producing shared AST member records.

mod declarations;
mod body;
pub(crate) use body::consume_set_marker;
mod method_params;
mod property_hook_storage;
mod traits;

pub(super) use declarations::*;
pub use declarations::parse_anonymous_class;
pub(super) use body::*;
// traits is used directly by body.rs via super::traits::parse_trait_use
