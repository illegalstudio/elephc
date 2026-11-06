//! Purpose:
//! Reserves ordinary Throwable construction metadata outside PHP property storage.
//!
//! Called from:
//! - Generic object allocation and runtime class-data emission.
//!
//! Key details:
//! - The optional word follows declared properties and the dynamic-property hash.
//! - Property, GC, and dynamic-hash offsets keep their existing layout.

use std::collections::HashMap;

use crate::types::ClassInfo;

/// Returns the dedicated creation-line offset for an ordinary Throwable layout.
pub(crate) fn ordinary_creation_line_offset(
    class_name: &str,
    classes: &HashMap<String, ClassInfo>,
) -> Option<usize> {
    let class = classes.get(class_name)?;
    let mut current = Some(class_name);
    while let Some(name) = current {
        let ancestor = classes.get(name)?;
        if ancestor.is_internal && matches!(name, "Exception" | "Error") {
            return Some(8 + class.properties.len() * 16 + usize::from(class.has_property_hash_storage()) * 8);
        }
        current = ancestor.parent.as_deref();
    }
    None
}

/// Resolves ordinary allocation metadata when an emitter carries a numeric class id.
pub(crate) fn ordinary_creation_line_offset_by_id(
    class_id: u64,
    classes: &HashMap<String, ClassInfo>,
) -> Option<usize> {
    let (name, _) = classes.iter().find(|(_, class)| class.class_id == class_id)?;
    ordinary_creation_line_offset(name, classes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extra metadata follows the complete inherited layout without reusing a scalar high word.
    #[test]
    fn ordinary_throwable_metadata_follows_properties_and_dynamic_hash() {
        let tokens = crate::lexer::tokenize("<?php class LocationChild extends Exception { public int $extra = 1; } class PlainLocation { public int $code = 1; }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let mut checked = crate::types::check(&program).unwrap();
        let class = &checked.classes["LocationChild"];
        let property_end = 8 + class.properties.len() * 16;
        assert_eq!(ordinary_creation_line_offset("LocationChild", &checked.classes), Some(property_end));
        assert_eq!(ordinary_creation_line_offset_by_id(class.class_id, &checked.classes), Some(property_end));
        checked.classes.get_mut("LocationChild").unwrap().allow_dynamic_properties = true;
        assert_eq!(ordinary_creation_line_offset("LocationChild", &checked.classes), Some(property_end + 8));
        assert_eq!(ordinary_creation_line_offset("PlainLocation", &checked.classes), None);
    }
}
