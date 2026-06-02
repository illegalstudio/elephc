//! Purpose:
//! Wires object-specific runtime helpers for stdClass, Mixed property access, and Mixed array/object indexing.
//! Keeps object helper emitters re-exported for the top-level runtime emitter.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()`.
//!
//! Key details:
//! - Helper names are consumed directly by codegen paths for `stdClass` and JSON-decoded `Mixed` values.

mod mixed_array_get;
mod mixed_array_set;
mod new_by_name;
mod stdclass;

pub(crate) use mixed_array_get::emit_mixed_array_get;
pub(crate) use mixed_array_set::emit_mixed_array_set;
pub(crate) use new_by_name::emit_new_by_name;
pub(crate) use stdclass::{
    emit_json_encode_stdclass, emit_mixed_property_get, emit_mixed_property_set,
    emit_stdclass_from_hash, emit_stdclass_get, emit_stdclass_new, emit_stdclass_set,
};
