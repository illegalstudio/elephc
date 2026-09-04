//! Purpose:
//! Wires object-specific runtime helpers for stdClass, Mixed property access, and Mixed array/object indexing.
//! Keeps object helper emitters re-exported for the top-level runtime emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Helper names are consumed directly by codegen paths for `stdClass` and JSON-decoded `Mixed` values.

mod call_destructor;
mod enum_debug;
mod export_props;
mod handles;
mod mixed_array_append;
mod mixed_array_fetch_for_write;
mod mixed_array_get;
mod object_not_array;
mod mixed_array_set;
mod mixed_cell_autovivify;
mod new_by_name;
mod object_vars;
mod print_r_object;
mod stdclass;
mod string_offset_warning;

pub(crate) use call_destructor::emit_call_object_destructor;
pub(crate) use enum_debug::{
    emit_obj_enum_case_name, emit_obj_enum_kind, emit_obj_enum_name_offset,
    emit_var_dump_emit_enum_line,
};
pub(crate) use export_props::{emit_obj_prop_count, emit_obj_prop_name, emit_obj_prop_value};
pub(crate) use print_r_object::{emit_pr_obj_desc, emit_print_r_object};
pub(crate) use handles::{
    emit_acquire_object_handle, emit_object_handles, object_handle_free_slots,
    object_handle_index_slots,
};
pub(crate) use mixed_array_append::emit_mixed_array_append;
pub(crate) use mixed_array_fetch_for_write::emit_mixed_array_fetch_for_write;
pub(crate) use mixed_array_get::emit_mixed_array_get;
pub(crate) use object_not_array::emit_throw_object_not_array;
pub(crate) use mixed_array_set::emit_mixed_array_set;
pub(crate) use mixed_cell_autovivify::emit_mixed_cell_autovivify_array;
pub(crate) use new_by_name::emit_new_by_name;
pub(crate) use string_offset_warning::{
    emit_string_offset_warning, STRING_OFFSET_MSG_CAPACITY, STRING_OFFSET_PREFIX,
};
pub(crate) use object_vars::emit_object_to_hash;
pub(crate) use stdclass::{
    emit_json_encode_stdclass, emit_mixed_property_get, emit_mixed_property_set,
    emit_stdclass_from_hash, emit_stdclass_get, emit_stdclass_new, emit_stdclass_set,
};
