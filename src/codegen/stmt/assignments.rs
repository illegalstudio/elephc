mod locals;
mod properties;
mod static_properties;

pub(super) use locals::emit_assign_stmt;
pub(super) use properties::{
    emit_property_array_assign_stmt,
    emit_property_array_push_stmt,
    emit_property_assign_stmt,
};
pub(super) use static_properties::{
    emit_static_property_array_assign_stmt,
    emit_static_property_array_push_stmt,
    emit_static_property_assign_stmt,
};
