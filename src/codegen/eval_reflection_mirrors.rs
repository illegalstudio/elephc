//! Purpose:
//! Resolves the compiled storage representation of public Reflection properties.
//!
//! Called from:
//! - The compiler's eval Reflection object constructors.
//!
//! Key details:
//! - A property the program ever binds with `=&` owns a reference cell.

use crate::types::ClassInfo;

/// Returns whether the slot of `property` holds an object-owned reference cell.
pub(super) fn reflection_property_owns_reference(info: &ClassInfo, property: &str) -> bool {
    info.property_offsets
        .get(property)
        .is_some_and(|offset| info.property_slot_owns_reference((offset - 8) / 16))
}
