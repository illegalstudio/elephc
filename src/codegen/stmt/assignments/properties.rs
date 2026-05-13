//! Purpose:
//! Groups object property assignment lowering for direct writes, references, magic setters, and array mutations.
//! Keeps property target resolution separate from value storage mechanics.
//!
//! Called from:
//! - `crate::codegen::stmt::assignments`
//!
//! Key details:
//! - Property writes must honor visibility, nullable receivers, magic methods, and declared property types.

mod arrays;
mod assign;
mod dynamic_props;
mod magic_set;
mod references;
mod storage;
mod target;

pub(crate) use dynamic_props::emit_dynamic_property_get;

pub(crate) use arrays::{
    emit_property_array_assign_stmt,
    emit_property_array_push_stmt,
};
pub(crate) use assign::emit_property_assign_stmt;
