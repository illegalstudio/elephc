mod arrays;
mod assign;
mod magic_set;
mod references;
mod storage;
mod target;

pub(crate) use arrays::{
    emit_property_array_assign_stmt,
    emit_property_array_push_stmt,
};
pub(crate) use assign::emit_property_assign_stmt;
