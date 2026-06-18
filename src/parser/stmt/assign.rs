//! Purpose:
//! Groups assignment-statement parsers and re-exports their shared lowering helpers.
//! Separates compound, list, local, postfix, and simple assignment surfaces.
//!
//! Called from:
//! - `crate::parser::stmt::parse_stmt()`.
//!
//! Key details:
//! - Assignment statement parsing is split to keep PHP l-value and evaluation-order rules localized.

mod compound;
mod list;
mod locals;
mod postfix;
mod simple;

pub(super) use list::{
    parse_list_construct_unpack,
    parse_list_unpack,
};
pub(crate) use list::parse_and_lower_foreach_destructure;
pub(super) use locals::{
    looks_like_typed_assign,
    parse_global,
    parse_incdec_stmt,
    parse_static_var,
    parse_typed_assign,
};
pub(crate) use postfix::{
    can_replay_assignment_target,
};
pub(super) use postfix::{
    try_parse_postfix_assignment,
    try_parse_scoped_property_assignment,
};
pub(super) use simple::parse_variable_stmt;
