//! Purpose:
//! Groups static property assignment lowering for direct writes, array mutations, and late-bound receivers.
//! Separates receiver resolution from symbol storage and array update paths.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments`
//!
//! Key details:
//! - Static property access must respect visibility, inheritance, and symbol naming conventions.

mod arrays;
mod assign;
mod late_bound;
mod resolve;

pub(crate) use arrays::{
    emit_static_property_array_assign_stmt,
    emit_static_property_array_push_stmt,
};
pub(crate) use assign::emit_static_property_assign_stmt;
