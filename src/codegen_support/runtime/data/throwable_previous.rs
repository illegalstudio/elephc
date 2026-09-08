//! Purpose:
//! Describes physical previous-exception slots for non-compact Throwable objects.
//!
//! Called from:
//! - `super::user::emit_runtime_data_user()`.
//!
//! Key details:
//! - Offset low bits encode boxed storage and a property-reference indirection.
//! - Physical slot order selects the inherited field before any private shadow.

use std::collections::HashMap;

use crate::types::{ClassInfo, PhpType};

/// Emits a bounded class-id table, preserving holes left by class reachability pruning.
pub(super) fn emit_previous_slots(
    out: &mut String,
    max_class_id: Option<u64>,
    classes: &HashMap<u64, &ClassInfo>,
) {
    let count = max_class_id.map_or(0, |id| id + 1);
    out.push_str(".p2align 3\n.globl _class_previous_slot_count\n_class_previous_slot_count:\n");
    out.push_str(&format!("    .quad {count}\n"));
    out.push_str(".globl _class_previous_slots\n_class_previous_slots:\n");
    for id in 0..count {
        let descriptor = classes.get(&id).map_or(0, |class| previous_slot_descriptor(class));
        out.push_str(&format!("    .quad {descriptor}\n"));
    }
}

/// Encodes one physical slot; zero means this class has no previous-exception field.
fn previous_slot_descriptor(class: &ClassInfo) -> u64 {
    let Some((index, (name, ty))) = class.properties.iter().enumerate()
        .find(|(_, (name, _))| name == "previous") else {
        return 0;
    };
    let boxed = u64::from(matches!(ty.codegen_repr(), PhpType::Mixed));
    let reference = u64::from(class.property_slot_is_reference(index, name));
    (8 + index as u64 * 16) | boxed | (reference << 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies physical slot selection, boxed/reference flags, and sparse class-id tables.
    #[test]
    fn previous_descriptors_preserve_storage_and_class_id_holes() {
        let tokens = crate::lexer::tokenize("<?php class PreviousLayout { public int $prefix = 1; public ?Throwable $previous = null; }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check(&program).unwrap();
        let mut class = checked.classes["PreviousLayout"].clone();
        assert_eq!(previous_slot_descriptor(&class), 25);
        class.properties.push(("previous".into(), PhpType::Object("Throwable".into())));
        assert_eq!(previous_slot_descriptor(&class), 25, "a shadow must not replace the base field");
        class.property_reference_slots = vec![false, true, false];
        assert_eq!(previous_slot_descriptor(&class), 27);
        class.properties[1].1 = PhpType::Object("Throwable".into());
        assert_eq!(previous_slot_descriptor(&class), 26);
        let mut out = String::new();
        emit_previous_slots(&mut out, Some(2), &HashMap::from([(2, &class)]));
        assert!(out.ends_with("_class_previous_slots:\n    .quad 0\n    .quad 0\n    .quad 26\n"));
        class.properties.clear();
        assert_eq!(previous_slot_descriptor(&class), 0);
    }
}
