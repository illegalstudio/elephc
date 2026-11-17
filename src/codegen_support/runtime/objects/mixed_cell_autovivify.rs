//! Purpose:
//! Emits the shared runtime helper that turns an existing boxed `Mixed(null)`
//! or null-container cell into a fresh empty indexed array.
//!
//! Called from:
//! - Mixed array append, set, and fetch-for-write runtime helpers.
//!
//! Key details:
//! - The caller must only pass a non-null cell whose old payload owns no heap
//!   reference (tag 8, zero, or the in-band null-container sentinel).
//! - The fresh array's initial reference is transferred directly into the cell.

pub(crate) use elephc_memoir::values::boxed_array::emit_mixed_cell_autovivify_array;
