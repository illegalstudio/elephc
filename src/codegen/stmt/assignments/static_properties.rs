mod arrays;
mod assign;
mod late_bound;
mod resolve;

pub(crate) use arrays::{
    emit_static_property_array_assign_stmt,
    emit_static_property_array_push_stmt,
};
pub(crate) use assign::emit_static_property_assign_stmt;
