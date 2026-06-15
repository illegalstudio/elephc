//! Purpose:
//! Groups parsers for PHP class-like declarations and members.
//! Re-exports class, interface, trait, enum, packed, body, modifier, and trait-use parsers.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()`.
//!
//! Key details:
//! - OOP parsing is split by declaration surface while producing shared AST member records.

mod declarations;
mod body;
mod method_params;
mod traits;

pub(super) use declarations::*;
pub(crate) use declarations::parse_anonymous_class;
pub(super) use body::*;
// traits is used directly by body.rs via super::traits::parse_trait_use
